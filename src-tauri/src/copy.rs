//! Verified copy: hash while reading the source, write to every destination,
//! flush to disk, re-read each destination from the disk itself (bypassing the
//! OS file cache) and compare, then rename the `.part` file into place. The
//! source is only ever opened read-only, and an existing file is never replaced.

use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use xxhash_rust::xxh3::Xxh3;

const CHUNK: usize = 8 * 1024 * 1024;
/// Unbuffered reads need a buffer aligned to the disk's sector size; 4 KiB covers them all in practice.
const ALIGN: usize = 4096;

pub fn hash_hex(h: u128) -> String {
    format!("{h:032x}")
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut h = Xxh3::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hash_hex(h.digest128()))
}

/// Opens `path` so reads come from the drive, not from pages still in the OS cache.
#[cfg(windows)]
fn open_uncached(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
    fs::OpenOptions::new().read(true).custom_flags(FILE_FLAG_NO_BUFFERING).open(path)
}

#[cfg(target_os = "macos")]
fn open_uncached(path: &Path) -> std::io::Result<File> {
    use std::os::unix::io::AsRawFd;
    let f = File::open(path)?;
    if unsafe { libc::fcntl(f.as_raw_fd(), libc::F_NOCACHE, 1) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(f)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_uncached(path: &Path) -> std::io::Result<File> {
    use std::os::unix::io::AsRawFd;
    let f = File::open(path)?;
    // The file was just synced, so its cached pages are clean and can be dropped.
    unsafe { libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
    Ok(f)
}

/// Like `hash_file`, but reading what is actually on the drive. Falls back to a
/// normal read where the file system refuses unbuffered access (some network shares).
pub fn hash_file_on_disk(path: &Path) -> Result<String> {
    let size = fs::metadata(path).with_context(|| format!("opening {}", path.display()))?.len();
    let Ok(mut f) = open_uncached(path) else { return hash_file(path) };
    let mut storage = vec![0u8; CHUNK + ALIGN];
    let offset = storage.as_ptr().align_offset(ALIGN);
    let buf = &mut storage[offset..offset + CHUNK];
    let mut h = Xxh3::new();
    let mut got = 0u64;
    // Stop at the known size: after a short read the position is no longer
    // sector-aligned, and Windows rejects unbuffered reads from there.
    while got < size {
        let n = match f.read(buf) {
            Ok(n) => n,
            Err(e) if got == 0 && e.kind() == std::io::ErrorKind::InvalidInput => return hash_file(path),
            Err(e) => return Err(anyhow::Error::from(e).context(format!("re-reading {}", path.display()))),
        };
        if n == 0 {
            break;
        }
        let take = n.min((size - got) as usize);
        h.update(&buf[..take]);
        got += take as u64;
    }
    anyhow::ensure!(got == size, "{} is shorter than what was written", path.display());
    Ok(hash_hex(h.digest128()))
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".safelight-part");
    dest.with_file_name(name)
}

/// Renames `from` to `to`, failing if `to` exists. Plain `fs::rename` silently
/// replaces it on both platforms, and checking first leaves a gap in between.
pub fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    match rename_excl(from, to) {
        Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
            // The file system can't do it atomically (exFAT on macOS): check, then rename.
            if to.symlink_metadata().is_ok() {
                return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{} already exists", to.display())));
            }
            fs::rename(from, to)
        }
        r => r,
    }
}

#[cfg(windows)]
fn rename_excl(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_COPY_ALLOWED};
    let wide = |p: &Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (f, t) = (wide(from), wide(to));
    // Without MOVEFILE_REPLACE_EXISTING this fails when `to` exists.
    if unsafe { MoveFileExW(f.as_ptr(), t.as_ptr(), MOVEFILE_COPY_ALLOWED) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn rename_excl(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = |p: &Path| CString::new(p.as_os_str().as_bytes()).map_err(|_| std::io::ErrorKind::InvalidInput);
    let (f, t) = (c(from)?, c(to)?);
    if unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) } == 0 {
        return Ok(());
    }
    let e = std::io::Error::last_os_error();
    match e.raw_os_error() {
        Some(libc::ENOTSUP) | Some(libc::EINVAL) => Err(std::io::ErrorKind::Unsupported.into()),
        _ => Err(e),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn rename_excl(from: &Path, to: &Path) -> std::io::Result<()> {
    // A hard link refuses to replace an existing file; the old name then goes.
    match fs::hard_link(from, to) {
        Ok(()) => fs::remove_file(from),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
        Err(_) => Err(std::io::ErrorKind::Unsupported.into()),
    }
}

pub struct Outcome {
    pub hash: String,
    /// One entry per destination, in the order given.
    pub results: Vec<Result<()>>,
}

/// Copies `src` to each of `dests`. Fails only if the source can't be read;
/// per-destination failures are reported in `Outcome::results`.
pub fn copy_verified(
    src: &Path,
    dests: &[PathBuf],
    cancel: &AtomicBool,
    mut on_bytes: impl FnMut(u64),
) -> Result<Outcome> {
    let mut input = File::open(src).with_context(|| format!("reading {}", src.display()))?;
    let src_mtime = fs::metadata(src).ok().map(|m| filetime::FileTime::from_last_modification_time(&m));

    let mut results: Vec<Result<()>> = Vec::with_capacity(dests.len());
    let mut writers: Vec<Option<File>> = Vec::with_capacity(dests.len());
    for d in dests {
        let open = (|| -> Result<File> {
            if let Some(p) = d.parent() {
                fs::create_dir_all(p).with_context(|| format!("creating {}", p.display()))?;
            }
            File::create(part_path(d)).with_context(|| format!("writing {}", d.display()))
        })();
        match open {
            Ok(f) => {
                writers.push(Some(f));
                results.push(Ok(()));
            }
            Err(e) => {
                writers.push(None);
                results.push(Err(e));
            }
        }
    }

    let mut hasher = Xxh3::new();
    let mut buf = vec![0u8; CHUNK];
    let streamed = (|| -> Result<()> {
        loop {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            let n = input.read(&mut buf).with_context(|| format!("reading {}", src.display()))?;
            if n == 0 {
                return Ok(());
            }
            hasher.update(&buf[..n]);
            for (i, w) in writers.iter_mut().enumerate() {
                if let Some(f) = w {
                    if let Err(e) = f.write_all(&buf[..n]) {
                        results[i] = Err(anyhow::Error::from(e).context(format!("writing {}", dests[i].display())));
                        *w = None;
                        let _ = fs::remove_file(part_path(&dests[i]));
                    }
                }
            }
            on_bytes(n as u64);
        }
    })();
    if let Err(e) = streamed {
        // Cancelled, or the card went away mid-file: leave no `.part` files behind.
        drop(writers);
        cleanup(dests);
        return Err(e);
    }
    let hash = hash_hex(hasher.digest128());

    for (i, w) in writers.into_iter().enumerate() {
        let Some(f) = w else { continue };
        let finish = (|| -> Result<()> {
            f.sync_all()?;
            drop(f);
            let part = part_path(&dests[i]);
            let written = hash_file_on_disk(&part)?;
            if written != hash {
                let _ = fs::remove_file(&part);
                bail!("checksum mismatch after copying to {}", dests[i].display());
            }
            if let Some(t) = src_mtime {
                let _ = filetime::set_file_mtime(&part, t);
            }
            match rename_no_replace(&part, &dests[i]) {
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    bail!("{} already exists; it was left untouched", dests[i].display())
                }
                r => r.with_context(|| format!("finalising {}", dests[i].display())),
            }
        })();
        if let Err(e) = finish {
            let _ = fs::remove_file(part_path(&dests[i]));
            results[i] = Err(e);
        }
    }
    Ok(Outcome { hash, results })
}

fn cleanup(dests: &[PathBuf]) {
    for d in dests {
        let _ = fs::remove_file(part_path(d));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_to_two_destinations_and_verifies() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.bin");
        let data: Vec<u8> = (0..(CHUNK * 2 + 123)).map(|i| (i % 251) as u8).collect();
        fs::write(&src, &data).unwrap();
        let a = dir.path().join("a/x/src.bin");
        let b = dir.path().join("b/src.bin");
        let cancel = AtomicBool::new(false);
        let mut total = 0;
        let out = copy_verified(&src, &[a.clone(), b.clone()], &cancel, |n| total += n).unwrap();
        assert!(out.results.iter().all(|r| r.is_ok()));
        assert_eq!(total, data.len() as u64);
        assert_eq!(fs::read(&a).unwrap(), data);
        assert_eq!(hash_file(&b).unwrap(), out.hash);
        assert!(!part_path(&a).exists());
    }

    #[test]
    fn detects_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("s.bin");
        fs::write(&src, b"hello world").unwrap();
        let d = dir.path().join("d.bin");
        let out = copy_verified(&src, std::slice::from_ref(&d), &AtomicBool::new(false), |_| {}).unwrap();
        let mut bytes = fs::read(&d).unwrap();
        bytes[0] ^= 1;
        fs::write(&d, bytes).unwrap();
        assert_ne!(hash_file(&d).unwrap(), out.hash);
    }

    #[test]
    fn never_replaces_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("s.bin");
        fs::write(&src, b"new").unwrap();
        let d = dir.path().join("d.bin");
        fs::write(&d, b"old").unwrap();
        let out = copy_verified(&src, std::slice::from_ref(&d), &AtomicBool::new(false), |_| {}).unwrap();
        assert!(out.results[0].is_err());
        assert_eq!(fs::read(&d).unwrap(), b"old");
        assert!(!part_path(&d).exists());
    }

    #[test]
    fn rename_never_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b, c) = (dir.path().join("a"), dir.path().join("b"), dir.path().join("c"));
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();
        let e = rename_no_replace(&a, &b).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&b).unwrap(), b"b");
        rename_no_replace(&a, &c).unwrap();
        assert!(!a.exists() && fs::read(&c).unwrap() == b"a");
    }

    #[test]
    fn on_disk_hash_matches_for_odd_sizes() {
        let dir = tempfile::tempdir().unwrap();
        for len in [0usize, 1, 4095, 4097, CHUNK + 7] {
            let p = dir.path().join(format!("f{len}"));
            let data: Vec<u8> = (0..len).map(|i| (i % 253) as u8).collect();
            fs::write(&p, &data).unwrap();
            assert_eq!(hash_file_on_disk(&p).unwrap(), hash_file(&p).unwrap(), "len {len}");
        }
    }

    #[test]
    fn unbuffered_reads_work_on_this_file_system() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("u.bin");
        fs::write(&p, vec![7u8; 10_000]).unwrap();
        let mut f = open_uncached(&p).expect("unbuffered open");
        let mut storage = vec![0u8; 3 * ALIGN];
        let off = storage.as_ptr().align_offset(ALIGN);
        let n = f.read(&mut storage[off..off + 2 * ALIGN]).expect("aligned unbuffered read");
        assert_eq!(n, 2 * ALIGN);
    }

    #[test]
    fn cancel_leaves_no_part_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("s.bin");
        fs::write(&src, b"data").unwrap();
        let d = dir.path().join("out/s.bin");
        assert!(copy_verified(&src, std::slice::from_ref(&d), &AtomicBool::new(true), |_| {}).is_err());
        assert!(!part_path(&d).exists() && !d.exists());
    }

    #[test]
    fn a_bad_backup_does_not_fail_the_main_copy() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("s.bin");
        fs::write(&src, b"data").unwrap();
        let blocker = dir.path().join("blocker");
        fs::write(&blocker, b"i am a file").unwrap();
        let good = dir.path().join("good/s.bin");
        let bad = blocker.join("sub/s.bin"); // parent is a file → cannot create
        let out = copy_verified(&src, &[good.clone(), bad], &AtomicBool::new(false), |_| {}).unwrap();
        assert!(out.results[0].is_ok());
        assert!(out.results[1].is_err());
        assert!(good.exists());
    }
}
