//! Previews come from the JPEG every camera embeds in its RAW files. That's far
//! faster than decoding the RAW and is what the camera showed on its screen.

use crate::exiftool::ExifTool;
use crate::project::Project;
use anyhow::{bail, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::DynamicImage;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique temp-file prefix per extraction, so concurrent jobs never collide.
fn unique(prefix: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!("{prefix}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed))
}

pub const PREVIEW_MAX: u32 = 2560;
pub const THUMB_MAX: u32 = 768;

/// Extracts all embedded JPEGs into `dir` and returns the largest one's path.
pub fn extract_largest(exif: &ExifTool, src: &Path, dir: &Path, prefix: &str) -> Result<Option<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let pattern = dir.join(format!("{prefix}_%t.%s"));
    exif.run(&[
        "-b".into(),
        "-m".into(),
        "-JpgFromRaw".into(),
        "-PreviewImage".into(),
        "-OtherImage".into(),
        "-W!".into(),
        pattern.to_string_lossy().into_owned(),
        src.to_string_lossy().into_owned(),
    ])?;
    let mut found: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("{prefix}_")))
        .map(|e| (e.metadata().map(|m| m.len()).unwrap_or(0), e.path()))
        .collect();
    found.sort();
    let best = found.pop().map(|(_, p)| p);
    for (_, p) in found {
        let _ = std::fs::remove_file(p);
    }
    Ok(best)
}

pub fn orient(img: DynamicImage, orientation: Option<i64>) -> DynamicImage {
    match orientation.unwrap_or(1) {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

fn save_jpeg(img: &DynamicImage, path: &Path, quality: u8) -> Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let f = std::fs::File::create(&tmp)?;
        let mut w = std::io::BufWriter::new(f);
        let enc = JpegEncoder::new_with_quality(&mut w, quality);
        img.to_rgb8().write_with_encoder(enc)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn fit(img: DynamicImage, max: u32, filter: FilterType) -> DynamicImage {
    if img.width().max(img.height()) <= max {
        img
    } else {
        img.resize(max, max, filter)
    }
}

fn is_plain_jpeg(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg"))
        .unwrap_or(false)
}

fn is_heif(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "heic" | "heif" | "hif"))
        .unwrap_or(false)
}

fn decode(p: &Path) -> Result<DynamicImage> {
    Ok(image::ImageReader::open(p)?.with_guessed_format()?.decode()?)
}

/// Builds `<id>_p.jpg` (≤2560 px) and `<id>_t.jpg` (≤768 px), both upright.
pub fn build(exif: &ExifTool, project: &Project, id: i64, src: &Path, orientation: Option<i64>) -> Result<()> {
    let img = decode_for_preview(exif, project, id, src)?;
    save_previews(project, id, orient(img, orientation))
}

/// The best quick source for a preview: the file itself for JPEG, else the
/// camera's embedded JPEG, else decoding the file (TIFF), else a converter (HEIF).
fn decode_for_preview(exif: &ExifTool, project: &Project, id: i64, src: &Path) -> Result<DynamicImage> {
    if is_plain_jpeg(src) {
        return decode(src).with_context(|| format!("decoding {}", src.display()));
    }
    let tmp = project.cache_dir().join("tmp");
    let prefix = unique(&id.to_string());
    if let Some(p) = extract_largest(exif, src, &tmp, &prefix)? {
        let img = decode(&p);
        let _ = std::fs::remove_file(&p);
        if let Ok(img) = img {
            return Ok(img);
        }
    }
    if let Ok(img) = decode(src) {
        return Ok(img);
    }
    if is_heif(src) {
        let out = tmp.join(format!("{prefix}_heif.jpg"));
        let converted = convert_heif(src, &out).and_then(|()| decode(&out));
        let _ = std::fs::remove_file(&out);
        return converted.with_context(|| format!("converting {}", src.display()));
    }
    bail!("no preview could be made for {}", src.display())
}

/// HEIF has no pure-Rust decoder: use the system's image engine on macOS, ffmpeg elsewhere.
fn convert_heif(src: &Path, out: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        let st = std::process::Command::new("sips")
            .args(["-s", "format", "jpeg"])
            .arg(src)
            .arg("--out")
            .arg(out)
            .output()?
            .status;
        anyhow::ensure!(st.success() && out.exists(), "sips couldn't read the file");
        Ok(())
    } else {
        crate::video::still(src, out)
    }
}

/// Writes the 2560 px preview and 768 px thumbnail for an (upright) image.
pub fn save_previews(project: &Project, id: i64, img: DynamicImage) -> Result<()> {
    let preview = fit(img, PREVIEW_MAX, FilterType::Triangle);
    save_jpeg(&preview, &project.preview_path(id), 88)?;
    let thumb = fit(preview, THUMB_MAX, FilterType::Triangle);
    save_jpeg(&thumb, &project.thumb_path(id), 82)?;
    Ok(())
}

static RAW_DECODER: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

pub fn set_raw_decoder(p: Option<PathBuf>) {
    let _ = RAW_DECODER.set(p);
}

/// Develops a RAW to an upright JPEG with camera white balance (LibRaw's
/// `dcraw_emu` on Windows, the system RAW engine via `sips` on macOS).
fn decode_raw(src: &Path, out: &Path) -> Result<()> {
    let tmp = out.with_extension("decode");
    let status = if cfg!(target_os = "macos") {
        std::process::Command::new("sips")
            .args(["-s", "format", "jpeg", "-s", "formatOptions", "92"])
            .arg(src)
            .arg("--out")
            .arg(&tmp)
            .output()?
            .status
    } else {
        let exe = RAW_DECODER.get().cloned().flatten().ok_or_else(|| anyhow::anyhow!("no RAW decoder bundled"))?;
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["-w", "-o", "1", "-q", "3", "-Z"]).arg(&tmp).arg(src);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd.output()?.status
    };
    anyhow::ensure!(status.success() && tmp.exists(), "RAW decoder failed");
    if cfg!(target_os = "macos") {
        std::fs::rename(&tmp, out)?;
    } else {
        // dcraw_emu writes a PPM, already rotated upright.
        let img = image::ImageReader::open(&tmp)?.with_guessed_format()?.decode()?;
        let _ = std::fs::remove_file(&tmp);
        save_jpeg(&img, out, 92)?;
    }
    Ok(())
}

/// Full-resolution image for 100% zoom. Uses the embedded full-size JPEG when
/// the camera stores one (Canon CR3, most newer Sony); otherwise falls back to
/// the 2560 px preview.
pub fn full(exif: &ExifTool, project: &Project, id: i64, src: &Path, raw_width: Option<i64>, orientation: Option<i64>) -> Result<PathBuf> {
    let out = project.full_path(id);
    if out.exists() {
        return Ok(out);
    }
    if is_plain_jpeg(src) && orientation.unwrap_or(1) == 1 {
        return Ok(src.to_path_buf());
    }
    let tmp = project.cache_dir().join("tmp");
    let src_img = if is_plain_jpeg(src) {
        Some(src.to_path_buf())
    } else {
        extract_largest(exif, src, &tmp, &unique(&format!("{id}full")))?
    };
    let big_enough = |p: &Path| {
        let (w, h) = image::image_dimensions(p).unwrap_or((0, 0));
        raw_width.map(|rw| (w.max(h) as f64) >= rw as f64 * 0.9).unwrap_or(w.max(h) > PREVIEW_MAX)
    };
    let src_img = match src_img {
        Some(p) if big_enough(&p) => p,
        other => {
            if let Some(p) = other.filter(|p| p != src) {
                let _ = std::fs::remove_file(p);
            }
            // The camera didn't embed a full-size JPEG: develop the RAW itself.
            return match decode_raw(src, &out) {
                Ok(()) => Ok(out),
                Err(e) => {
                    log::warn!("raw decode failed for {}: {e:#}", src.display());
                    Ok(project.preview_path(id))
                }
            };
        }
    };
    if orientation.unwrap_or(1) == 1 && src_img != src {
        std::fs::rename(&src_img, &out)?;
    } else {
        let img = orient(image::ImageReader::open(&src_img)?.with_guessed_format()?.decode()?, orientation);
        save_jpeg(&img, &out, 92)?;
        if src_img != src {
            let _ = std::fs::remove_file(&src_img);
        }
    }
    Ok(out)
}
