#!/usr/bin/env bash
# Download the pinned ONNX Runtime shared library (needed for embeddings and
# reranking, ADR 0009) into ./models/onnxruntime/. The Docker image already has it.
# Usage: ./scripts/install-onnxruntime.sh [dest-dir]
# Then: AKASHA_ORT_DYLIB_PATH=<dest-dir>/libonnxruntime.so (or .dylib on macOS).
set -euo pipefail

VERSION=1.28.0 # keep >= the ONNX Runtime version the `ort` crate targets
DEST=${1:-./models/onnxruntime}

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) pkg=linux-x64; lib=libonnxruntime.so.$VERSION; out=libonnxruntime.so
    sum=a3e1b79d7bb1bf09696ce675f49e4064e6c81f6202b8225624fff0e93f8d6407 ;;
  Linux-aarch64 | Linux-arm64) pkg=linux-aarch64; lib=libonnxruntime.so.$VERSION; out=libonnxruntime.so
    sum=e15ff8b5d85afe6c144d97c6fd432254bf76a219daaf17658087d6ecb3e8f0bb ;;
  Darwin-arm64) pkg=osx-arm64; lib=libonnxruntime.$VERSION.dylib; out=libonnxruntime.dylib
    sum=1268b359718099bde2cedb55787f182a130067bc4f31e8c88478c445b850d3d8 ;;
  *) echo "unsupported platform $(uname -s)-$(uname -m); install ONNX Runtime >= 1.24 yourself" >&2
     exit 1 ;;
esac

if [ -f "$DEST/$out" ]; then
  echo "$DEST/$out already exists"
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/ort.tgz" \
  "https://github.com/microsoft/onnxruntime/releases/download/v$VERSION/onnxruntime-$pkg-$VERSION.tgz"
if command -v sha256sum >/dev/null; then
  echo "$sum  $tmp/ort.tgz" | sha256sum -c - >/dev/null
else
  echo "$sum  $tmp/ort.tgz" | shasum -a 256 -c - >/dev/null
fi
tar -xzf "$tmp/ort.tgz" -C "$tmp"
mkdir -p "$DEST"
cp "$tmp/onnxruntime-$pkg-$VERSION/lib/$lib" "$DEST/$out"
echo "Installed ONNX Runtime $VERSION: $DEST/$out"
echo "Set AKASHA_ORT_DYLIB_PATH=$DEST/$out"
