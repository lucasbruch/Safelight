#!/usr/bin/env bash
# Downloads the helper programs Safelight bundles (not kept in git):
#   ExifTool (metadata, previews, XMP) · ONNX Runtime (AI helper) · LibRaw (Windows RAW decode for 100% zoom)
#   ffmpeg (video posters and playback copies)
# Every download is a pinned version checked against its SHA-256 before use, so
# a changed or tampered file upstream fails the build instead of being shipped.
# Used by CI too. Run once after cloning: bash scripts/fetch-helpers.sh
set -euo pipefail
EXIFTOOL_VERSION=13.59 ORT_VERSION=1.30.0 LIBRAW_VERSION=0.21.4 FFMPEG_VERSION=9.0.2
R="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/resources"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT

# fetch <url> <file in $T> <sha256>
fetch() {
  curl -fsSL --retry 3 -o "$T/$2" "$1"
  local got
  got="$( (sha256sum "$T/$2" 2>/dev/null || shasum -a 256 "$T/$2") | cut -d' ' -f1)"
  if [ "$got" != "$3" ]; then
    echo "Checksum mismatch for $2 from $1" >&2
    echo "  expected $3" >&2
    echo "  got      $got" >&2
    exit 1
  fi
}

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    fetch "https://sourceforge.net/projects/exiftool/files/exiftool-${EXIFTOOL_VERSION}_64.zip/download" et.zip \
      44b512b25af500724ba579d0a53c8fc5851628b692dd5e5d94ae4a15c2cba9ec
    unzip -q "$T/et.zip" -d "$T/et"; mkdir -p "$R/exiftool/windows"
    cp -r "$T"/et/*/exiftool_files "$R/exiftool/windows/"; cp "$T"/et/*/"exiftool(-k).exe" "$R/exiftool/windows/exiftool.exe"

    fetch "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-win-x64-${ORT_VERSION}.zip" ort.zip \
      c6ba983baf5681af108599675d2a89c2d145512d02de28aed0bff177cd0ba949
    unzip -q "$T/ort.zip" -d "$T/ort"; mkdir -p "$R/onnxruntime/windows"
    cp "$T"/ort/*/lib/onnxruntime.dll "$T"/ort/*/LICENSE "$R/onnxruntime/windows/"

    fetch "https://www.libraw.org/data/LibRaw-${LIBRAW_VERSION}-Win64.zip" lr.zip \
      400e288c9e2c0878483750a8cd8086f4123a05dadfad72e54ec003016b6f50a3
    unzip -q "$T/lr.zip" -d "$T/lr"; mkdir -p "$R/libraw/windows"
    cp "$T"/lr/*/bin/dcraw_emu.exe "$T"/lr/*/bin/libraw.dll "$T"/lr/*/COPYRIGHT "$T"/lr/*/LICENSE.* "$R/libraw/windows/"

    fetch "https://github.com/GyanD/codexffmpeg/releases/download/${FFMPEG_VERSION}/ffmpeg-${FFMPEG_VERSION}-essentials_build.zip" ff.zip \
      60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba
    unzip -q "$T/ff.zip" -d "$T/ff"; mkdir -p "$R/ffmpeg/windows"
    cp "$T"/ff/*/bin/ffmpeg.exe "$T"/ff/*/LICENSE "$R/ffmpeg/windows/"

    # ExifTool ships read-only files, which break re-copying during builds.
    cmd //c "attrib -R /S /D $(cygpath -w "$R")\\*" >/dev/null ;;
  Darwin)
    fetch "https://sourceforge.net/projects/exiftool/files/Image-ExifTool-${EXIFTOOL_VERSION}.tar.gz/download" et.tgz \
      668ea3acececb7235fbd0f4900e72d5f12c9b07e5c778fd36cb1e9b5828fd65a
    mkdir -p "$T/et" "$R/exiftool/unix"; tar xzf "$T/et.tgz" -C "$T/et"
    cp -R "$T"/et/*/exiftool "$T"/et/*/lib "$R/exiftool/unix/"

    # ONNX Runtime stopped publishing Intel/universal macOS builds after 1.23, and the
    # `ort` crate needs API 27+. Intel Macs therefore run without the AI models (the
    # sharpness/exposure/horizon checks still work).
    fetch "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-osx-arm64-${ORT_VERSION}.tgz" ort.tgz \
      6ebb5062a934537c352937821f9fe9718e7de1a2db1122a93dd363ffd53a7012
    mkdir -p "$T/ort" "$R/onnxruntime/macos"; tar xzf "$T/ort.tgz" -C "$T/ort"
    cp "$T"/ort/*/lib/libonnxruntime.${ORT_VERSION}.dylib "$R/onnxruntime/macos/libonnxruntime.dylib"
    cp "$T"/ort/*/LICENSE "$R/onnxruntime/macos/"

    # Native builds for both architectures, joined into one universal binary like the
    # app itself. An Intel-only ffmpeg runs under Rosetta and makes macOS warn about it.
    FF="https://ffmpeg.martin-riedl.de/download/macos"
    fetch "$FF/arm64/1789931890_${FFMPEG_VERSION}/ffmpeg.zip" ff-arm64.zip \
      c8ed4c4e6978a03c485edbfe4e0a5dc2380f8a30bba5150531b31b094492d924
    fetch "$FF/amd64/1789931006_${FFMPEG_VERSION}/ffmpeg.zip" ff-x86_64.zip \
      7c6b4125b191cbf773832dc51f424cf2b6bb7da43007d1e066f95909e47cacd4
    unzip -oq "$T/ff-arm64.zip" -d "$T/ff-arm64"; unzip -oq "$T/ff-x86_64.zip" -d "$T/ff-x86_64"
    mkdir -p "$R/ffmpeg/macos"
    lipo -create "$T/ff-arm64/ffmpeg" "$T/ff-x86_64/ffmpeg" -output "$R/ffmpeg/macos/ffmpeg"
    chmod +x "$R/ffmpeg/macos/ffmpeg" ;;
  *) echo "Unsupported OS"; exit 1 ;;
esac
echo "Helpers ready in $R"
