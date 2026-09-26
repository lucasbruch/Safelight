//! XMP sidecars (`IMG_0001.xmp` next to `IMG_0001.CR3`), the format Lightroom
//! Classic reads on import. Written by a background thread so rating stays instant.

use crate::exiftool::ExifTool;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

pub const PICK_KEYWORD: &str = "Safelight Pick";

#[derive(Debug, Clone, Default)]
pub struct XmpState {
    pub rating: i64,
    pub flag: i64,
    pub artist: String,
    pub copyright: String,
    pub tags: Vec<String>,
}

pub fn sidecar_for(media: &Path) -> PathBuf {
    media.with_extension("xmp")
}

/// Only proprietary RAWs get a sidecar. Lightroom ignores sidecars for JPEG/HEIC/TIFF/DNG,
/// and in a RAW+JPEG pair both files would otherwise share (and fight over) one `.xmp`.
pub fn wants_sidecar(media: &Path) -> bool {
    crate::meta::is_raw(media)
}

pub fn args(xmp: &Path, st: &XmpState) -> Vec<String> {
    let mut a: Vec<String> = vec!["-overwrite_original".into(), "-m".into()];
    // Lightroom has no reject flag in XMP; -1 is the Bridge/XMP convention for "rejected".
    let rating = if st.flag < 0 { -1 } else { st.rating.clamp(0, 5) };
    a.push(format!("-XMP-xmp:Rating={rating}"));
    a.push(if st.flag > 0 { "-XMP-xmp:Label=Green".into() } else { "-XMP-xmp:Label=".into() });
    a.push("-XMP-dc:Subject=".into());
    let mut kw: Vec<&str> = st.tags.iter().map(String::as_str).collect();
    if st.flag > 0 {
        kw.push(PICK_KEYWORD);
    }
    for k in kw {
        a.push(format!("-XMP-dc:Subject={k}"));
    }
    if !st.artist.trim().is_empty() {
        a.push(format!("-XMP-dc:Creator={}", st.artist.trim()));
    }
    if !st.copyright.trim().is_empty() {
        a.push(format!("-XMP-dc:Rights={}", st.copyright.trim()));
        a.push("-XMP-xmpRights:Marked=True".into());
    }
    a.push(xmp.to_string_lossy().into_owned());
    a
}

enum Msg {
    Write(PathBuf, XmpState),
    Flush(Sender<()>),
}

pub struct Writer {
    tx: Sender<Msg>,
}

impl Writer {
    pub fn start(exif: Arc<ExifTool>) -> Self {
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("xmp-writer".into())
            .spawn(move || run(exif, rx))
            .expect("spawn xmp writer");
        Self { tx }
    }

    pub fn write(&self, xmp: PathBuf, st: XmpState) {
        let _ = self.tx.send(Msg::Write(xmp, st));
    }

    /// Blocks until every queued write is on disk.
    pub fn flush(&self) {
        let (tx, rx) = channel();
        if self.tx.send(Msg::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(30));
        }
    }
}

fn run(exif: Arc<ExifTool>, rx: Receiver<Msg>) {
    while let Ok(first) = rx.recv() {
        // Coalesce bursts (e.g. rating many photos at once): the last state per file wins.
        let mut pending: HashMap<PathBuf, XmpState> = HashMap::new();
        let mut waiters = vec![];
        let mut take = |m: Msg| match m {
            Msg::Write(p, s) => {
                pending.insert(p, s);
            }
            Msg::Flush(w) => waiters.push(w),
        };
        take(first);
        while let Ok(m) = rx.recv_timeout(Duration::from_millis(40)) {
            take(m);
        }
        for (p, s) in pending {
            if let Err(e) = exif.run(&args(&p, &s)) {
                eprintln!("[safelight] xmp write failed for {}: {e:#}", p.display());
            }
        }
        for w in waiters {
            let _ = w.send(());
        }
    }
}
