//! ONNX models, all run locally through ONNX Runtime:
//! * YuNet (OpenCV Zoo, MIT): face boxes + 5 landmarks
//! * MediaPipe Face Mesh V2 (Google, Apache-2.0; ONNX export on Hugging Face, MIT):
//!   478 landmarks, used for eyelid opening (blink check)
//! * SFace (OpenCV Zoo, Apache-2.0): 128-d face identity, for "same person" clusters
//! * CLIP ViT-B/32 vision (OpenAI, MIT; ONNX by Xenova): content tags and, through
//!   LAION's linear aesthetic head (MIT), a 1–10 "looks good" score.
//! Tag text embeddings and the aesthetic head are pre-computed by
//! `scripts/gen_clip_data.py` and compiled in (`clip_data.bin`).

use anyhow::{anyhow, Context, Result};
use image::imageops::FilterType;
use image::RgbImage;
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub struct ModelFile {
    pub name: &'static str,
    /// Pinned to a commit, so the file behind the URL can't change under us.
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub const FILES: &[ModelFile] = &[
    ModelFile {
        name: "face_detection_yunet_2023mar.onnx",
        url: "https://github.com/opencv/opencv_zoo/raw/47534e27c9851bb1128ccc0102f1145e27f23f98/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
        bytes: 232_589,
        sha256: "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4",
    },
    ModelFile {
        name: "face_mesh_v2.onnx",
        url: "https://huggingface.co/astaileyyoung/FaceMeshONNX/resolve/71a6de2112ae66337e098bff6cfcdb3905899207/mesh.onnx",
        bytes: 5_133_323,
        sha256: "c334c26b128dfc709da0f353c6157ac340b785f5f25cdab5dbea12bbea243172",
    },
    ModelFile {
        name: "face_recognition_sface_2021dec.onnx",
        url: "https://github.com/opencv/opencv_zoo/raw/47534e27c9851bb1128ccc0102f1145e27f23f98/models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
        bytes: 38_696_353,
        sha256: "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79",
    },
    ModelFile {
        name: "clip_vit_b32_vision_quantized.onnx",
        url: "https://huggingface.co/Xenova/clip-vit-base-patch32/resolve/d15189d7028b43f1d3e65039190477f6af591c2a/onnx/vision_model_quantized.onnx",
        bytes: 89_117_001,
        sha256: "583fd1110a514667812fee7d684952aaf82a99b959760c8d7dca7e0ab9839299",
    },
];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(hex(&h.finalize()))
}

/// Checks every model against its pinned hash before it's handed to ONNX Runtime.
/// A file that doesn't match is deleted, so Settings offers the download again.
fn verify(dir: &Path) -> Result<()> {
    for f in FILES {
        let p = dir.join(f.name);
        if sha256_file(&p)? != f.sha256 {
            let _ = std::fs::remove_file(&p);
            anyhow::bail!("{} is damaged or not the expected file; download the AI helpers again", f.name);
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub installed: bool,
    pub download_mb: u64,
}

pub fn available(dir: &Path) -> bool {
    FILES.iter().all(|f| dir.join(f.name).exists())
}

pub fn status(dir: &Path) -> Status {
    let missing: u64 = FILES.iter().filter(|f| !dir.join(f.name).exists()).map(|f| f.bytes).sum();
    Status { installed: available(dir), download_mb: (missing as f64 / 1e6).ceil() as u64 }
}

/// Downloads whatever is missing. `progress(done_bytes, total_bytes, file)`.
pub fn download(dir: &Path, mut progress: impl FnMut(u64, u64, &str)) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let todo: Vec<&ModelFile> = FILES.iter().filter(|f| !dir.join(f.name).exists()).collect();
    let total: u64 = todo.iter().map(|f| f.bytes).sum();
    let mut done = 0u64;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(30)))
        .timeout_recv_response(Some(std::time::Duration::from_secs(60)))
        .timeout_recv_body(Some(std::time::Duration::from_secs(30 * 60)))
        .build()
        .into();
    for f in todo {
        use sha2::{Digest, Sha256};
        let part = dir.join(format!("{}.part", f.name));
        let mut resp = agent
            .get(f.url)
            .call()
            .with_context(|| format!("downloading {}. Are you online?", f.name))?;
        let mut reader = resp.body_mut().with_config().limit(u64::MAX).reader();
        let mut out = std::io::BufWriter::new(std::fs::File::create(&part)?);
        let mut buf = vec![0u8; 256 * 1024];
        let mut got = 0u64;
        let mut sha = Sha256::new();
        let mut last = std::time::Instant::now();
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            sha.update(&buf[..n]);
            got += n as u64;
            if last.elapsed().as_millis() > 150 {
                last = std::time::Instant::now();
                progress(done + got, total, f.name);
            }
        }
        out.flush()?;
        drop(out);
        if got != f.bytes || hex(&sha.finalize()) != f.sha256 {
            let _ = std::fs::remove_file(&part);
            anyhow::bail!("{} didn't download correctly ({got} of {} bytes, or wrong contents). Please try again.", f.name, f.bytes);
        }
        std::fs::rename(&part, dir.join(f.name))?;
        done += f.bytes;
        progress(done, total, f.name);
    }
    Ok(())
}

/// Loads the bundled ONNX Runtime library (once per process).
pub fn init_runtime(lib: Option<&Path>) -> Result<()> {
    static INIT: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    INIT.get_or_init(|| {
        let lib = lib.ok_or("ONNX Runtime library not bundled")?;
        ort::init_from(lib).map_err(|e| e.to_string())?.with_name("grabit").commit();
        Ok(())
    })
    .clone()
    .map_err(|e| anyhow!(e))
}

pub struct Face {
    /// Normalised box (0..1).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub eyes_closed: bool,
    pub embedding: Option<Vec<f32>>,
}

struct ClipData {
    labels: Vec<String>,
    text: Vec<Vec<f32>>,
    aes_w: Vec<f32>,
    aes_b: f32,
}

fn clip_data() -> &'static ClipData {
    static DATA: OnceLock<ClipData> = OnceLock::new();
    DATA.get_or_init(|| {
        let b: &[u8] = include_bytes!("clip_data.bin");
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let f32s = |o: usize, n: usize| -> Vec<f32> {
            b[o..o + n * 4].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect()
        };
        let n = u32_at(0) as usize;
        let mut o = 4;
        let mut labels = Vec::with_capacity(n);
        for _ in 0..n {
            let len = u16::from_le_bytes([b[o], b[o + 1]]) as usize;
            labels.push(String::from_utf8_lossy(&b[o + 2..o + 2 + len]).into_owned());
            o += 2 + len;
        }
        let text = (0..n).map(|i| f32s(o + i * 512 * 4, 512)).collect();
        o += n * 512 * 4;
        let aes_w = f32s(o, 512);
        let aes_b = f32s(o + 512 * 4, 1)[0];
        ClipData { labels, text, aes_w, aes_b }
    })
}

pub struct Models {
    yunet: Mutex<Session>,
    mesh: Mutex<Session>,
    sface: Mutex<Session>,
    clip: Mutex<Session>,
}

fn session(dir: &Path, name: &str, threads: usize) -> Result<Mutex<Session>> {
    let path: PathBuf = dir.join(name);
    let mut b = Session::builder()
        .map_err(|e| anyhow!("{e}"))?
        .with_intra_threads(threads)
        .map_err(|e| anyhow!("{e}"))?;
    let s = b.commit_from_file(&path).map_err(|e| anyhow!("loading {name}: {e}"))?;
    Ok(Mutex::new(s))
}

/// Runs a single-input model and returns each requested output as a flat Vec.
fn run(s: &Mutex<Session>, input: &str, shape: [usize; 4], data: Vec<f32>, outputs: &[&str]) -> Result<Vec<Vec<f32>>> {
    let t = Tensor::from_array((shape, data)).map_err(|e| anyhow!("{e}"))?;
    let mut s = s.lock();
    let out = s.run(ort::inputs![input => t]).map_err(|e| anyhow!("{e}"))?;
    outputs
        .iter()
        .map(|name| {
            let (_, v) = out
                .get(*name)
                .ok_or_else(|| anyhow!("model output {name} missing"))?
                .try_extract_tensor::<f32>()
                .map_err(|e| anyhow!("{e}"))?;
            Ok(v.to_vec())
        })
        .collect()
}

const YUNET: u32 = 640;
const FACE_SCORE: f32 = 0.75;
/// Below this width (preview pixels) eyelid landmarks are too unreliable to judge blinks.
const MIN_BLINK_FACE: f32 = 90.0;
/// Mean eye aspect ratio below which both eyes count as shut.
const EAR_CLOSED: f32 = 0.12;

impl Models {
    pub fn load(dir: &Path) -> Result<Self> {
        verify(dir)?;
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
        Ok(Self {
            yunet: session(dir, FILES[0].name, threads)?,
            mesh: session(dir, FILES[1].name, 2)?,
            sface: session(dir, FILES[2].name, threads)?,
            clip: session(dir, FILES[3].name, threads)?,
        })
    }

    pub fn faces(&self, img: &RgbImage) -> Result<Vec<Face>> {
        let (w, h) = img.dimensions();
        let s = YUNET as f32 / w.max(h) as f32;
        let (rw, rh) = (((w as f32 * s).round() as u32).max(1), ((h as f32 * s).round() as u32).max(1));
        let small = image::imageops::resize(img, rw, rh, FilterType::Triangle);
        // Letterbox into 640×640, BGR, 0–255, NCHW.
        let plane = (YUNET * YUNET) as usize;
        let mut data = vec![0f32; plane * 3];
        for (x, y, p) in small.enumerate_pixels() {
            let i = (y * YUNET + x) as usize;
            data[i] = p.0[2] as f32;
            data[plane + i] = p.0[1] as f32;
            data[2 * plane + i] = p.0[0] as f32;
        }
        let names = ["cls_8", "cls_16", "cls_32", "obj_8", "obj_16", "obj_32", "bbox_8", "bbox_16", "bbox_32", "kps_8", "kps_16", "kps_32"];
        let o = run(&self.yunet, "input", [1, 3, YUNET as usize, YUNET as usize], data, &names)?;

        // Decode (same as OpenCV's FaceDetectorYN).
        let mut dets: Vec<(f32, [f32; 4], [f32; 10])> = vec![];
        for (k, stride) in [8u32, 16, 32].iter().enumerate() {
            let cols = YUNET / stride;
            let (cls, obj, bbox, kps) = (&o[k], &o[3 + k], &o[6 + k], &o[9 + k]);
            for idx in 0..cls.len() {
                let score = (cls[idx].clamp(0.0, 1.0) * obj[idx].clamp(0.0, 1.0)).sqrt();
                if score < FACE_SCORE {
                    continue;
                }
                let (r, c) = ((idx as u32 / cols) as f32, (idx as u32 % cols) as f32);
                let st = *stride as f32;
                let cx = (c + bbox[idx * 4]) * st;
                let cy = (r + bbox[idx * 4 + 1]) * st;
                let bw = bbox[idx * 4 + 2].exp() * st;
                let bh = bbox[idx * 4 + 3].exp() * st;
                let mut lm = [0f32; 10];
                for n in 0..5 {
                    lm[2 * n] = (kps[idx * 10 + 2 * n] + c) * st;
                    lm[2 * n + 1] = (kps[idx * 10 + 2 * n + 1] + r) * st;
                }
                dets.push((score, [cx - bw / 2.0, cy - bh / 2.0, bw, bh], lm));
            }
        }
        dets.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut keep: Vec<(f32, [f32; 4], [f32; 10])> = vec![];
        for d in dets {
            if keep.iter().all(|k| iou(&k.1, &d.1) < 0.3) {
                keep.push(d);
            }
            if keep.len() >= 100 {
                break;
            }
        }

        let mut faces = vec![];
        for (_, b, lm) in keep {
            // Back to full-image pixels.
            let b = [b[0] / s, b[1] / s, b[2] / s, b[3] / s];
            let lm: Vec<(f32, f32)> = (0..5).map(|n| (lm[2 * n] / s, lm[2 * n + 1] / s)).collect();
            let eyes_closed = if b[2] >= MIN_BLINK_FACE {
                self.eye_openness(img, &b, &lm).ok().flatten().map(|ear| ear < EAR_CLOSED).unwrap_or(false)
            } else {
                false // too small to judge reliably; saying nothing beats a false alarm
            };
            let embedding = if b[2] >= 40.0 { self.identity(img, &lm).ok() } else { None };
            faces.push(Face {
                x: (b[0] / w as f32).clamp(0.0, 1.0),
                y: (b[1] / h as f32).clamp(0.0, 1.0),
                w: (b[2] / w as f32).clamp(0.0, 1.0),
                h: (b[3] / h as f32).clamp(0.0, 1.0),
                eyes_closed,
                embedding,
            });
        }
        Ok(faces)
    }

    /// Mean eye aspect ratio (eyelid opening ÷ eye width) from Face Mesh, or
    /// None when the mesh isn't confident it sees a face (e.g. looking away).
    fn eye_openness(&self, img: &RgbImage, b: &[f32; 4], lm: &[(f32, f32)]) -> Result<Option<f32>> {
        // Square crop with 25% margin, rotated so the eyes are level (as MediaPipe expects).
        let (cx, cy) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
        let size = b[2].max(b[3]) * 1.5;
        let ang = (lm[1].1 - lm[0].1).atan2(lm[1].0 - lm[0].0);
        let (c, s) = (ang.cos(), ang.sin());
        const N: usize = 256;
        let mut data = vec![0f32; N * N * 3];
        for v in 0..N {
            for u in 0..N {
                let (dx, dy) = ((u as f32 + 0.5) / N as f32 - 0.5, (v as f32 + 0.5) / N as f32 - 0.5);
                let (dx, dy) = (dx * size, dy * size);
                let p = bilinear(img, cx + c * dx - s * dy, cy + s * dx + c * dy);
                let i = (v * N + u) * 3;
                data[i] = p[0] / 255.0;
                data[i + 1] = p[1] / 255.0;
                data[i + 2] = p[2] / 255.0;
            }
        }
        let t = Tensor::from_array(([1usize, N, N, 3], data)).map_err(|e| anyhow!("{e}"))?;
        let mut m = self.mesh.lock();
        let out = m.run(ort::inputs!["input_12" => t]).map_err(|e| anyhow!("{e}"))?;
        let (_, pts) = out["Identity"].try_extract_tensor::<f32>().map_err(|e| anyhow!("{e}"))?;
        let (_, presence) = out["Identity_1"].try_extract_tensor::<f32>().map_err(|e| anyhow!("{e}"))?;
        if 1.0 / (1.0 + (-presence[0]).exp()) < 0.5 {
            return Ok(None);
        }
        let p = |i: usize| (pts[i * 3], pts[i * 3 + 1]);
        let d = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        // [corner, upper, upper, corner, lower, lower] for each eye (MediaPipe mesh indices).
        let ear = |ix: [usize; 6]| {
            (d(p(ix[1]), p(ix[5])) + d(p(ix[2]), p(ix[4]))) / (2.0 * d(p(ix[0]), p(ix[3])).max(1e-3))
        };
        let right = ear([33, 160, 158, 133, 153, 144]);
        let left = ear([362, 385, 387, 263, 373, 380]);
        Ok(Some((right + left) / 2.0))
    }

    /// 128-d SFace embedding of a face aligned to the ArcFace template.
    fn identity(&self, img: &RgbImage, lm: &[(f32, f32)]) -> Result<Vec<f32>> {
        const TEMPLATE: [(f32, f32); 5] = [(38.2946, 51.6963), (73.5318, 51.5014), (56.0252, 71.7366), (41.5493, 92.3655), (70.7299, 92.2041)];
        let (a, b, tx, ty) = similarity(lm, &TEMPLATE);
        // Inverse map: for each output pixel, where in the source it comes from.
        let det = a * a + b * b;
        let plane = 112 * 112;
        let mut data = vec![0f32; plane * 3];
        for y in 0..112 {
            for x in 0..112 {
                let (u, v) = (x as f32 - tx, y as f32 - ty);
                let sx = (a * u + b * v) / det;
                let sy = (-b * u + a * v) / det;
                let p = bilinear(img, sx, sy);
                let i = y * 112 + x;
                // RGB, 0–255 (OpenCV's FaceRecognizerSF uses swapRB on BGR input).
                data[i] = p[0];
                data[plane + i] = p[1];
                data[2 * plane + i] = p[2];
            }
        }
        let o = run(&self.sface, "data", [1, 3, 112, 112], data, &["fc1"])?;
        Ok(normalize(&o[0]))
    }

    /// L2-normalised CLIP image embedding.
    pub fn clip(&self, img: &RgbImage) -> Result<Vec<f32>> {
        let (w, h) = img.dimensions();
        let s = 224.0 / w.min(h) as f32;
        let (rw, rh) = (((w as f32 * s).round() as u32).max(224), ((h as f32 * s).round() as u32).max(224));
        let r = image::imageops::resize(img, rw, rh, FilterType::CatmullRom);
        let (ox, oy) = ((rw - 224) / 2, (rh - 224) / 2);
        const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
        const STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];
        let plane = 224 * 224;
        let mut data = vec![0f32; plane * 3];
        for y in 0..224u32 {
            for x in 0..224u32 {
                let p = r.get_pixel(ox + x, oy + y).0;
                let i = (y * 224 + x) as usize;
                for c in 0..3 {
                    data[c * plane + i] = (p[c] as f32 / 255.0 - MEAN[c]) / STD[c];
                }
            }
        }
        let o = run(&self.clip, "pixel_values", [1, 3, 224, 224], data, &["image_embeds"])?;
        Ok(normalize(&o[0]))
    }

    /// LAION aesthetic score (roughly 1–10; ~5 is average).
    pub fn aesthetic(&self, clip: &[f32]) -> Option<f64> {
        let d = clip_data();
        let s = dot(&d.aes_w, clip) + d.aes_b;
        Some(((s as f64) * 10.0).round() / 10.0)
    }

    /// Up to three content tags by zero-shot similarity.
    pub fn tags(&self, clip: &[f32]) -> Vec<String> {
        let d = clip_data();
        let logits: Vec<f32> = d.text.iter().map(|t| 100.0 * dot(t, clip)).collect();
        let max = logits.iter().cloned().fold(f32::MIN, f32::max);
        let exps: Vec<f32> = logits.iter().map(|l| (l - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        let mut ranked: Vec<(usize, f32)> = exps.iter().map(|e| e / sum).enumerate().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let top = ranked.first().map(|r| r.1).unwrap_or(0.0);
        ranked
            .into_iter()
            .take(3)
            .filter(|(_, p)| *p >= 0.08f32.max(top * 0.5))
            .map(|(i, _)| d.labels[i].clone())
            .collect()
    }
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = (a[0] + a[2]).min(b[0] + b[2]);
    let y2 = (a[1] + a[3]).min(b[1] + b[3]);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    inter / (a[2] * a[3] + b[2] * b[3] - inter).max(1e-6)
}

fn bilinear(img: &RgbImage, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = img.dimensions();
    if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
        return [0.0; 3];
    }
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let p = |x, y| img.get_pixel(x, y).0;
    let mut out = [0f32; 3];
    for c in 0..3 {
        let top = p(x0, y0)[c] as f32 * (1.0 - fx) + p(x1, y0)[c] as f32 * fx;
        let bot = p(x0, y1)[c] as f32 * (1.0 - fx) + p(x1, y1)[c] as f32 * fx;
        out[c] = top * (1.0 - fy) + bot * fy;
    }
    out
}

/// Least-squares similarity transform (scale+rotation+translation) mapping
/// `src` onto `dst`: dst = [a -b; b a]·src + t. Returns (a, b, tx, ty).
fn similarity(src: &[(f32, f32)], dst: &[(f32, f32)]) -> (f32, f32, f32, f32) {
    let n = src.len() as f32;
    let (mut sx, mut sy, mut dx, mut dy) = (0f32, 0f32, 0f32, 0f32);
    for (s, d) in src.iter().zip(dst) {
        sx += s.0;
        sy += s.1;
        dx += d.0;
        dy += d.1;
    }
    let (sx, sy, dx, dy) = (sx / n, sy / n, dx / n, dy / n);
    let (mut num_a, mut num_b, mut den) = (0f32, 0f32, 0f32);
    for (s, d) in src.iter().zip(dst) {
        let (px, py, qx, qy) = (s.0 - sx, s.1 - sy, d.0 - dx, d.1 - dy);
        num_a += px * qx + py * qy;
        num_b += px * qy - py * qx;
        den += px * px + py * py;
    }
    let a = num_a / den.max(1e-6);
    let b = num_b / den.max(1e-6);
    (a, b, dx - (a * sx - b * sy), dy - (b * sx + a * sy))
}

/// Greedy clustering of face embeddings into people. Returns, per item id,
/// the person numbers: 1-based, in order of first appearance (the input is in
/// capture order), so a person keeps their number as more photos come in.
pub fn cluster_people(faces: &[(i64, Vec<Vec<f32>>)]) -> HashMap<i64, Vec<i64>> {
    // SFace's recommended cosine threshold for "same identity" is 0.363.
    const SAME_PERSON: f32 = 0.40;
    let mut centroids: Vec<(Vec<f32>, usize)> = vec![];
    let mut assign: Vec<(i64, usize)> = vec![];
    for (id, embs) in faces {
        for e in embs {
            let n = normalize(e);
            let best = centroids
                .iter()
                .enumerate()
                .map(|(i, (c, _))| (i, dot(c, &n)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let idx = match best {
                Some((i, s)) if s >= SAME_PERSON => {
                    let (c, k) = &mut centroids[i];
                    for (cv, nv) in c.iter_mut().zip(&n) {
                        *cv = (*cv * *k as f32 + nv) / (*k as f32 + 1.0);
                    }
                    *c = normalize(c);
                    *k += 1;
                    i
                }
                _ => {
                    centroids.push((n, 1));
                    centroids.len() - 1
                }
            };
            assign.push((*id, idx));
        }
    }
    // People seen only once aren't worth a filter.
    let order: Vec<usize> = (0..centroids.len()).filter(|&i| centroids[i].1 >= 2).collect();
    let rank: HashMap<usize, i64> = order.iter().enumerate().map(|(r, &i)| (i, r as i64 + 1)).collect();
    let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
    for (id, c) in assign {
        if let Some(&p) = rank.get(&c) {
            let v = out.entry(id).or_default();
            if !v.contains(&p) {
                v.push(p);
            }
        }
    }
    out
}

pub fn normalize(v: &[f32]) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    v.iter().map(|x| x / n).collect()
}

pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_recovers_known_transform() {
        // dst = 2·R(30°)·src + (5, -3)
        let (c, s) = (2.0 * 30f32.to_radians().cos(), 2.0 * 30f32.to_radians().sin());
        let src = [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (7.0, 3.0)];
        let dst: Vec<(f32, f32)> = src.iter().map(|&(x, y)| (c * x - s * y + 5.0, s * x + c * y - 3.0)).collect();
        let (a, b, tx, ty) = similarity(&src, &dst);
        assert!((a - c).abs() < 1e-3 && (b - s).abs() < 1e-3, "{a} {b}");
        assert!((tx - 5.0).abs() < 1e-3 && (ty + 3.0).abs() < 1e-3);
    }

    #[test]
    fn clip_data_parses() {
        let d = clip_data();
        assert!(d.labels.len() > 40);
        assert_eq!(d.text[0].len(), 512);
        assert_eq!(d.aes_w.len(), 512);
        assert!((dot(&d.text[0], &d.text[0]) - 1.0).abs() < 1e-3, "text embeddings are unit length");
    }

    #[test]
    fn people_cluster_by_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let a2 = vec![0.95, 0.1, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let p = cluster_people(&[(1, vec![a.clone()]), (2, vec![a2]), (3, vec![b.clone()]), (4, vec![b, a])]);
        assert_eq!(p[&1], p[&2]);
        assert_ne!(p[&1], p[&3]);
        assert_eq!(p[&4].len(), 2);
        assert_eq!(p[&1], vec![1], "numbered by first appearance");
        assert_eq!(p[&3], vec![2]);
    }
}
