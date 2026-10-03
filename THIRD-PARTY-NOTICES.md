# Third-party notices

Safelight itself is licensed under the GNU General Public License v3.0 (see `LICENSE`).
It ships with, or downloads, the components below. Each remains under its own license.

## Bundled with the app

### FFmpeg 9.0.2 (video posters and playback copies)

License: GNU GPL. These builds include GPL-licensed libraries such as x264 and x265, so the
binaries as a whole are under the GPL. The GPL v3 text is in `LICENSE`, and a copy ships next to
the `ffmpeg` program.

- Source code: https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz (FFmpeg, https://ffmpeg.org)
- Windows build: "essentials" build by Gyan Doshi, https://www.gyan.dev/ffmpeg/builds/
  (release files: https://github.com/GyanD/codexffmpeg/releases/tag/9.0.2)
- macOS build: by Martin Riedl, https://ffmpeg.martin-riedl.de; built with the scripts at
  https://git.martin-riedl.de/ffmpeg/build-script, which also list the source of every library
  included. Safelight joins the Apple silicon and Intel builds into one file, unmodified.

### ExifTool 13.59 (metadata, embedded previews, XMP sidecars)

By Phil Harvey, https://exiftool.org. Free software; you can redistribute it and/or modify it
under the same terms as Perl itself (the Artistic License or the GNU GPL).

### LibRaw 0.21.4 (Windows only: RAW decoding for 100% zoom)

https://www.libraw.org. Dual-licensed under the GNU LGPL v2.1 or the CDDL v1.0. Its license
files ship next to it.

### ONNX Runtime 1.30.0 (runs the optional AI models)

Copyright (c) Microsoft Corporation. MIT License. https://github.com/microsoft/onnxruntime.
Its license file ships next to it.

### Barlow and Barlow Semi Condensed fonts

By Jeremy Tribby. SIL Open Font License 1.1. https://github.com/jpt/barlow

### CLIP tag embeddings and LAION aesthetic predictor (`src-tauri/src/ai/clip_data.bin`)

Pre-computed with `scripts/gen_clip_data.py` from OpenAI's CLIP ViT-B/32 (MIT License,
https://github.com/openai/CLIP) and LAION's aesthetic predictor (MIT License,
https://github.com/LAION-AI/aesthetic-predictor).

### Rust crates and npm packages

Safelight is built with open-source libraries listed in `src-tauri/Cargo.lock` and
`package-lock.json`, among them Tauri (MIT/Apache-2.0) and React (MIT). Their licenses are
included in their packages.

## Downloaded on request (Settings → AI helper → Download)

These models aren't part of the installer. Safelight downloads them from pinned revisions only
when you ask it to.

| Model | License | Source |
| --- | --- | --- |
| YuNet face detection | MIT | https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet |
| SFace face recognition | Apache-2.0 | https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface |
| Face mesh (MediaPipe Face Mesh V2, ONNX conversion) | MIT (as labelled by the uploader); original model Apache-2.0 | https://huggingface.co/astaileyyoung/FaceMeshONNX |
| CLIP ViT-B/32 image encoder (ONNX) | MIT | https://huggingface.co/Xenova/clip-vit-base-patch32 |
