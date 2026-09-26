//! Long-lived ExifTool processes (`-stay_open`), so each call costs milliseconds
//! instead of a ~150 ms process start.

use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Proc {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    seq: u64,
}

impl Proc {
    fn spawn(exe: &Path) -> Result<Self> {
        let mut cmd = Command::new(exe);
        cmd.args(["-stay_open", "True", "-@", "-", "-common_args", "-charset", "filename=utf8"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .with_context(|| format!("could not start exiftool at {}", exe.display()))?;
        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = BufReader::new(child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?);
        Ok(Self { child, stdin, stdout, seq: 0 })
    }

    fn execute(&mut self, args: &[String]) -> Result<String> {
        self.seq += 1;
        let mut buf = String::new();
        for a in args {
            if a.contains('\n') {
                bail!("exiftool argument contains a newline");
            }
            buf.push_str(a);
            buf.push('\n');
        }
        buf.push_str(&format!("-execute{}\n", self.seq));
        self.stdin.write_all(buf.as_bytes())?;
        self.stdin.flush()?;

        let marker = format!("{{ready{}}}", self.seq);
        let mut out = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line)? == 0 {
                bail!("exiftool exited unexpectedly");
            }
            if line.trim_end() == marker {
                return Ok(out);
            }
            out.push_str(&line);
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(b"-stay_open\nFalse\n");
        let _ = self.stdin.flush();
        let _ = self.child.wait();
    }
}

/// A small pool of ExifTool processes, spawned lazily and respawned if one dies.
pub struct ExifTool {
    exe: PathBuf,
    procs: Vec<Mutex<Option<Proc>>>,
    next: AtomicUsize,
}

impl ExifTool {
    pub fn new(exe: PathBuf, size: usize) -> Self {
        Self {
            exe,
            procs: (0..size.max(1)).map(|_| Mutex::new(None)).collect(),
            next: AtomicUsize::new(0),
        }
    }

    pub fn run(&self, args: &[String]) -> Result<String> {
        // Prefer an idle process; otherwise queue on the next one round-robin.
        let mut guard = self
            .procs
            .iter()
            .find_map(|p| p.try_lock())
            .unwrap_or_else(|| {
                let i = self.next.fetch_add(1, Ordering::Relaxed) % self.procs.len();
                self.procs[i].lock()
            });
        for attempt in 0..2 {
            if guard.is_none() {
                *guard = Some(Proc::spawn(&self.exe)?);
            }
            match guard.as_mut().unwrap().execute(args) {
                Ok(out) => return Ok(out),
                Err(e) if attempt == 0 => {
                    eprintln!("[safelight] exiftool failed, restarting: {e:#}");
                    *guard = None;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    /// Reads metadata as JSON objects, one per file, in input order. Skips
    /// maker notes for speed; use `read_json_full` for maker-note tags.
    pub fn read_json(&self, files: &[PathBuf], tags: &[&str]) -> Result<Vec<serde_json::Value>> {
        self.read(files, tags, true)
    }

    /// Like `read_json` but including maker notes (e.g. Canon's log settings).
    pub fn read_json_full(&self, files: &[PathBuf], tags: &[&str]) -> Result<Vec<serde_json::Value>> {
        self.read(files, tags, false)
    }

    fn read(&self, files: &[PathBuf], tags: &[&str], fast: bool) -> Result<Vec<serde_json::Value>> {
        if files.is_empty() {
            return Ok(vec![]);
        }
        let mut args: Vec<String> = vec!["-json".into(), "-n".into(), "-api".into(), "QuickTimeUTC=1".into()];
        if fast {
            args.push("-fast2".into());
        }
        args.extend(tags.iter().map(|t| format!("-{t}")));
        args.extend(files.iter().map(|f| f.to_string_lossy().into_owned()));
        let out = self.run(&args)?;
        if out.trim().is_empty() {
            return Ok(vec![]);
        }
        let v: Vec<serde_json::Value> = serde_json::from_str(&out).context("parsing exiftool json")?;
        Ok(v)
    }
}

/// Finds a bundled helper: in the installed app's resources, or the source tree in dev.
pub fn locate_resource(resource_dir: Option<&Path>, rel: &str) -> Option<PathBuf> {
    let mut candidates = vec![];
    if let Some(r) = resource_dir {
        candidates.push(r.join(rel));
    }
    // The source tree only exists on the developer's machine; release builds must
    // never go looking for helpers at a path baked in at compile time.
    if cfg!(debug_assertions) {
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel));
    }
    candidates.into_iter().find(|p| p.exists())
}

/// Finds the bundled exiftool, falling back to one on PATH.
pub fn locate(resource_dir: Option<PathBuf>) -> PathBuf {
    let rel = if cfg!(windows) {
        "resources/exiftool/windows/exiftool.exe"
    } else {
        "resources/exiftool/unix/exiftool"
    };
    locate_resource(resource_dir.as_deref(), rel).unwrap_or_else(|| PathBuf::from("exiftool"))
}
