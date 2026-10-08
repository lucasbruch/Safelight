//! Video previews with ffmpeg. The app's web view only plays 8-bit H.264, so
//! camera formats like Canon's 10-bit 4:2:2 HEVC get a poster frame and a small
//! H.264 proxy for playback. Log footage (Canon Log / Sony S-Log3) is shown
//! through a generated viewing LUT so it looks normal while culling; the
//! original files are never changed.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

static FFMPEG: OnceLock<PathBuf> = OnceLock::new();

/// Bundled ffmpeg, else one on PATH.
pub fn set_ffmpeg(bundled: Option<PathBuf>) {
    let _ = FFMPEG.set(bundled.unwrap_or_else(|| PathBuf::from("ffmpeg")));
}

fn ffmpeg() -> Command {
    let mut c = Command::new(FFMPEG.get().cloned().unwrap_or_else(|| PathBuf::from("ffmpeg")));
    c.args(["-hide_banner", "-nostdin", "-y"]).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    CLog,
    CLog2,
    CLog3,
    SLog3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)] // Canon's own name for it
pub enum Gamut {
    CinemaGamut,
    Bt709,
    Bt2020,
    SGamut3Cine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogProfile {
    pub curve: Curve,
    pub gamut: Gamut,
}

impl LogProfile {
    pub fn label(&self) -> &'static str {
        match self.curve {
            Curve::CLog => "Canon Log",
            Curve::CLog2 => "Canon Log 2",
            Curve::CLog3 => "Canon Log 3",
            Curve::SLog3 => "S-Log3",
        }
    }
    pub fn gamut_name(&self) -> String {
        format!("{:?}", self.gamut)
    }

    /// Inverse of `label()` + `gamut_name()`, for profiles stored in the database.
    pub fn parse(label: &str, gamut: Option<&str>) -> Option<Self> {
        let curve = match label {
            "Canon Log 3" => Curve::CLog3,
            "Canon Log 2" => Curve::CLog2,
            "Canon Log" => Curve::CLog,
            "S-Log3" => Curve::SLog3,
            _ => return None,
        };
        let gamut = match gamut.unwrap_or("") {
            "Bt709" => Gamut::Bt709,
            "Bt2020" => Gamut::Bt2020,
            "SGamut3Cine" => Gamut::SGamut3Cine,
            _ if curve == Curve::SLog3 => Gamut::SGamut3Cine,
            _ => Gamut::CinemaGamut,
        };
        Some(Self { curve, gamut })
    }

    fn file_name(&self) -> String {
        format!("{:?}_{:?}_to_709.cube", self.curve, self.gamut).to_lowercase()
    }
}

/// Canon writes `CanonLogVersion` (CLogV1/2/3) and `ColorSpace2` into its clips
/// (exiftool gives either the names or, with `-n`, the codes 0–3 and 0–2);
/// Sony writes `CaptureGammaEquation` into the clip's XML sidecar.
pub fn detect_log(canon_log: Option<&str>, canon_gamut: Option<&str>, sony_xml: Option<&str>) -> Option<LogProfile> {
    if let Some(v) = canon_log {
        let curve = match v.to_ascii_uppercase().as_str() {
            "CLOGV3" | "CLOG3" | "3" => Curve::CLog3,
            "CLOGV2" | "CLOG2" | "2" => Curve::CLog2,
            "CLOGV1" | "CLOG" | "CLOG1" | "1" => Curve::CLog,
            _ => return None,
        };
        let gamut = match canon_gamut.unwrap_or("").to_ascii_uppercase().replace(['.', ' '], "").as_str() {
            "BT709" | "0" => Gamut::Bt709,
            "BT2020" | "1" => Gamut::Bt2020,
            _ => Gamut::CinemaGamut,
        };
        return Some(LogProfile { curve, gamut });
    }
    if let Some(xml) = sony_xml {
        let x = xml.to_ascii_lowercase();
        if x.contains("s-log3") {
            // S-Gamut3 and S-Gamut3.Cine are close enough for a viewing preview.
            return Some(LogProfile { curve: Curve::SLog3, gamut: Gamut::SGamut3Cine });
        }
    }
    None
}

/// Log curve → scene linear (1.0 ≈ 100% white), from the manufacturers' published formulas.
/// `x` is the normalised legal-range code value (0 = black level 64, 1 = 940 in 10-bit).
fn to_linear(curve: Curve, x: f64) -> f64 {
    match curve {
        Curve::CLog3 => {
            let l = if x < 0.097465473 {
                -(10f64.powf((0.12783901 - x) / 0.36726845) - 1.0) / 14.98325
            } else if x <= 0.15277891 {
                (x - 0.12512219) / 1.9754798
            } else {
                (10f64.powf((x - 0.12240537) / 0.36726845) - 1.0) / 14.98325
            };
            l / 0.9
        }
        Curve::CLog2 => {
            let l = if x < 0.092864125 {
                -(10f64.powf((0.035388128 - x) / 0.281400451) - 1.0) / 87.09937546
            } else {
                (10f64.powf((x - 0.035388128) / 0.281400451) - 1.0) / 87.09937546
            };
            l / 0.9
        }
        Curve::CLog => {
            let l = if x < 0.0730597 {
                -(10f64.powf((0.0730597 - x) / 0.529136) - 1.0) / 10.1596
            } else {
                (10f64.powf((x - 0.0730597) / 0.529136) - 1.0) / 10.1596
            };
            l / 0.9
        }
        Curve::SLog3 => {
            // Sony's formula works on full-range 10-bit code values.
            let code = (x * 876.0 + 64.0) / 1023.0;
            if code >= 171.2102946929 / 1023.0 {
                10f64.powf((code * 1023.0 - 420.0) / 261.5) * (0.18 + 0.01) - 0.01
            } else {
                (code * 1023.0 - 95.0) * 0.01125 / (171.2102946929 - 95.0)
            }
        }
    }
}

type M3 = [[f64; 3]; 3];

fn primaries_to_xyz(p: [(f64, f64); 3]) -> M3 {
    let (wx, wy) = (0.3127, 0.3290); // D65
    let xyz = |x: f64, y: f64| [x / y, 1.0, (1.0 - x - y) / y];
    let cols = [xyz(p[0].0, p[0].1), xyz(p[1].0, p[1].1), xyz(p[2].0, p[2].1)];
    let m: M3 = [
        [cols[0][0], cols[1][0], cols[2][0]],
        [cols[0][1], cols[1][1], cols[2][1]],
        [cols[0][2], cols[1][2], cols[2][2]],
    ];
    let s = mul_v(&inv(&m), xyz(wx, wy));
    [
        [m[0][0] * s[0], m[0][1] * s[1], m[0][2] * s[2]],
        [m[1][0] * s[0], m[1][1] * s[1], m[1][2] * s[2]],
        [m[2][0] * s[0], m[2][1] * s[1], m[2][2] * s[2]],
    ]
}

fn mul(a: &M3, b: &M3) -> M3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn mul_v(a: &M3, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i][0] * v[0] + a[i][1] * v[1] + a[i][2] * v[2])
}

fn inv(m: &M3) -> M3 {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        [(e * i - f * h) / det, (c * h - b * i) / det, (b * f - c * e) / det],
        [(f * g - d * i) / det, (a * i - c * g) / det, (c * d - a * f) / det],
        [(d * h - e * g) / det, (b * g - a * h) / det, (a * e - b * d) / det],
    ]
}

fn gamut_to_709(g: Gamut) -> M3 {
    let rec709 = primaries_to_xyz([(0.64, 0.33), (0.30, 0.60), (0.15, 0.06)]);
    let src = match g {
        Gamut::Bt709 => return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        Gamut::CinemaGamut => primaries_to_xyz([(0.74, 0.27), (0.17, 1.14), (0.08, -0.10)]),
        Gamut::Bt2020 => primaries_to_xyz([(0.708, 0.292), (0.170, 0.797), (0.131, 0.046)]),
        Gamut::SGamut3Cine => primaries_to_xyz([(0.766, 0.275), (0.225, 0.800), (0.089, -0.087)]),
    };
    mul(&inv(&rec709), &src)
}

/// A 33³ .cube viewing LUT: log → linear → Rec.709 primaries → soft highlight
/// roll-off → gentle contrast → sRGB display gamma.
pub fn cube(profile: LogProfile) -> String {
    const N: usize = 33;
    let m = gamut_to_709(profile.gamut);
    let mut s = String::with_capacity(N * N * N * 30);
    s.push_str(&format!("TITLE \"Safelight {} preview\"\nLUT_3D_SIZE {N}\n", profile.label()));
    for bi in 0..N {
        for gi in 0..N {
            for ri in 0..N {
                let f = |i: usize| i as f64 / (N - 1) as f64;
                let lin = [f(ri), f(gi), f(bi)].map(|x| to_linear(profile.curve, x));
                let rgb = mul_v(&m, lin).map(|v| v.max(0.0) * 0.85);
                // Hue-preserving extended Reinhard on the brightest channel.
                let mx = rgb.iter().cloned().fold(1e-6, f64::max);
                const W: f64 = 6.0;
                let t = mx * (1.0 + mx / (W * W)) / (1.0 + mx);
                let out = rgb.map(|v| {
                    let v = (v * t / mx).clamp(0.0, 1.0);
                    let g = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
                    // Soft S-curve around mid grey for a little snap.
                    let c = g + 0.12 * (g - 0.5) * (1.0 - (2.0 * g - 1.0).abs());
                    c.clamp(0.0, 1.0)
                });
                s.push_str(&format!("{:.5} {:.5} {:.5}\n", out[0], out[1], out[2]));
            }
        }
    }
    s
}

/// Writes the LUT for `profile` into `dir` (once) and returns its file name.
fn ensure_lut(dir: &Path, profile: LogProfile) -> Result<String> {
    std::fs::create_dir_all(dir)?;
    let name = profile.file_name();
    let p = dir.join(&name);
    if !p.exists() {
        // Several preview workers can get here at once: write a private copy and
        // rename it into place, so ffmpeg never reads a half-written LUT.
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = dir.join(format!("{name}.{}-{n}.tmp", std::process::id()));
        std::fs::write(&tmp, cube(profile))?;
        if let Err(e) = std::fs::rename(&tmp, &p) {
            let _ = std::fs::remove_file(&tmp);
            // Another worker won the race (and ffmpeg may hold its copy open): theirs is identical.
            anyhow::ensure!(p.exists(), e);
        }
    }
    Ok(name)
}

/// What ffmpeg says about the first video stream, e.g. "hevc (Rext) ... yuv422p10le".
fn probe_line(src: &Path) -> Result<String> {
    let out = ffmpeg().arg("-i").arg(src).stdout(Stdio::null()).stderr(Stdio::piped()).output()
        .context("running ffmpeg. Is it installed?")?;
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    Ok(text.lines().find(|l| l.contains("Video:")).unwrap_or("").to_string())
}

/// True if the web view can play the original file as-is (8-bit 4:2:0 H.264).
pub fn directly_playable(src: &Path) -> Result<bool> {
    let l = probe_line(src)?;
    let ext_ok = src.extension().map(|e| matches!(e.to_ascii_lowercase().to_str(), Some("mp4" | "mov" | "m4v"))).unwrap_or(false);
    Ok(ext_ok && l.contains("h264") && l.contains("yuv420p") && !l.contains("yuv420p10"))
}

/// Video filter: scale (legal-range BT.709 in) and, for log footage, the viewing LUT.
fn filter(scale: &str, lut: Option<&str>, for_jpeg: bool) -> String {
    let mut f = format!("scale={scale}:in_color_matrix=bt709:in_range=tv:out_range=pc:flags=bicubic");
    if let Some(l) = lut {
        f.push_str(&format!(",format=gbrp16le,lut3d={l}"));
    }
    if for_jpeg {
        f.push_str(",format=rgb24");
    } else {
        f.push_str(",scale=out_color_matrix=bt709:out_range=tv,format=yuv420p");
    }
    f
}

fn run(mut cmd: Command, what: &str) -> Result<()> {
    let out = cmd.stdout(Stdio::null()).stderr(Stdio::piped()).output().with_context(|| format!("running ffmpeg for {what}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("");
        bail!("ffmpeg couldn't make the {what}: {last}");
    }
    Ok(())
}

/// A still frame at 2560 px (1 s in, or the start for very short clips).
pub fn poster(src: &Path, out: &Path, lut_dir: &Path, log: Option<LogProfile>, duration: Option<f64>) -> Result<()> {
    std::fs::create_dir_all(lut_dir)?; // also ffmpeg's working folder
    let at = duration.map(|d| (d * 0.25).min(1.0)).unwrap_or(1.0);
    let lut = log.map(|p| ensure_lut(lut_dir, p)).transpose()?;
    let mut c = ffmpeg();
    // Run inside the LUT folder so the filter can use a bare file name
    // (Windows paths are a quoting nightmare in ffmpeg filter strings).
    c.current_dir(lut_dir)
        .args(["-loglevel", "error", "-ss", &format!("{at:.2}")])
        .arg("-i")
        .arg(src)
        .args(["-frames:v", "1", "-vf", &filter("2560:-2", lut.as_deref(), true), "-q:v", "3"])
        .arg(out);
    run(c, "preview frame")
}

/// Decodes a still image ffmpeg understands (e.g. HEIF) to a JPEG.
pub fn still(src: &Path, out: &Path) -> Result<()> {
    let mut c = ffmpeg();
    c.args(["-loglevel", "error"]).arg("-i").arg(src).args(["-frames:v", "1", "-q:v", "2"]).arg(out);
    run(c, "preview")
}

/// A 720p H.264 proxy for smooth playback in the app.
pub fn proxy(src: &Path, out: &Path, lut_dir: &Path, log: Option<LogProfile>) -> Result<()> {
    std::fs::create_dir_all(lut_dir)?; // also ffmpeg's working folder
    let lut = log.map(|p| ensure_lut(lut_dir, p)).transpose()?;
    let tmp = out.with_extension("part.mp4");
    let mut c = ffmpeg();
    c.current_dir(lut_dir)
        .args(["-loglevel", "error"])
        .arg("-i")
        .arg(src)
        .args(["-map", "0:v:0", "-map", "0:a:0?", "-vf", &filter("-2:720", lut.as_deref(), false)])
        .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "24", "-c:a", "aac", "-b:a", "128k", "-movflags", "+faststart"])
        .arg(&tmp);
    run(c, "playback copy")?;
    std::fs::rename(&tmp, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_canon_and_sony_log() {
        let p = detect_log(Some("CLogV3"), Some("CinemaGamut"), None).unwrap();
        assert_eq!((p.curve, p.gamut), (Curve::CLog3, Gamut::CinemaGamut));
        assert_eq!(detect_log(Some("CLogV2"), Some("BT.709"), None).unwrap().gamut, Gamut::Bt709);
        assert_eq!(detect_log(None, None, Some(r#"<CaptureGammaEquation value="s-log3-cine"/>"#)).unwrap().curve, Curve::SLog3);
        assert!(detect_log(None, None, Some("<Device/>")).is_none());
        assert!(detect_log(Some("Off"), None, None).is_none());
        // exiftool -n gives codes instead of names.
        let p = detect_log(Some("3"), Some("2"), None).unwrap();
        assert_eq!((p.curve, p.gamut), (Curve::CLog3, Gamut::CinemaGamut));
        assert_eq!(detect_log(Some("1"), Some("0"), None).unwrap().gamut, Gamut::Bt709);
        assert!(detect_log(Some("0"), Some("0"), None).is_none());
    }

    #[test]
    fn clog3_mid_grey_lands_near_18_percent() {
        // Canon: 18% grey records at ~34.3% in Canon Log 3.
        let l = to_linear(Curve::CLog3, 0.343) * 0.9;
        assert!((l - 0.18).abs() < 0.03, "{l}");
        // Sony: 18% grey at code 420 (full range 10-bit).
        let x = (420.0 - 64.0) / 876.0;
        assert!((to_linear(Curve::SLog3, x) - 0.18).abs() < 0.01);
    }

    #[test]
    fn cube_is_well_formed() {
        let c = cube(LogProfile { curve: Curve::CLog3, gamut: Gamut::CinemaGamut });
        let rows = c.lines().filter(|l| l.chars().next().map(|ch| ch.is_ascii_digit()).unwrap_or(false)).count();
        assert_eq!(rows, 33 * 33 * 33);
        assert!(c.lines().last().unwrap().starts_with("1.00000"), "white stays white-ish");
    }
}
