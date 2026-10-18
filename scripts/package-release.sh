#!/usr/bin/env bash
# Packages a release build of `trove` for one platform into <out>/trove-<version>-<platform>.<tar.gz|zip>, with the
# runtime libraries it loads from its own folder, and writes the archive's SHA-256 beside it as <archive>.sha256.
#
# Usage: scripts/package-release.sh <linux-x86_64|macos-arm64|windows-x86_64> <version> <build folder> <out folder>
set -euo pipefail
shopt -s nullglob

if [ $# -ne 4 ]; then
  echo "usage: $0 <platform> <version> <build folder> <out folder>" >&2
  exit 2
fi
platform=$1
version=$2
build=$3
out=$4

require() {
  for file in "$@"; do
    if [ ! -f "$file" ]; then
      echo "missing $file" >&2
      exit 1
    fi
  done
}

# ggml loads its CPU and Vulkan backends from the executable's folder.
require_backends() {
  if [ $# -eq 0 ]; then
    echo "no ggml CPU backends in $build" >&2
    exit 1
  fi
}

case "$platform" in
  # `trove` and libllama find the other libraries through their `$ORIGIN` rpath.
  linux-x86_64)
    require "$build/trove" "$build/libllama.so.0" "$build/libggml.so.0" "$build/libggml-base.so.0" \
      "$build/libggml-vulkan.so"
    require_backends "$build"/libggml-cpu-*.so
    files=("$build/trove" "$build"/libllama.so* "$build"/libggml*.so*)
    extension=tar.gz
    ;;
  # Static llama.cpp with the Metal library embedded: the binary is all there is.
  macos-arm64)
    require "$build/trove"
    files=("$build/trove")
    extension=tar.gz
    ;;
  windows-x86_64)
    require "$build/trove.exe" "$build/llama.dll" "$build/ggml.dll" "$build/ggml-base.dll" "$build/ggml-vulkan.dll"
    require_backends "$build"/ggml-cpu-*.dll
    files=("$build/trove.exe" "$build"/llama.dll "$build"/ggml*.dll)
    # The llama.cpp DLLs need the VC++ runtime, which a fresh Windows may not have.
    vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
    runtime=()
    while IFS= read -r dll; do
      runtime+=("$(cygpath --unix "$dll")")
    done < <("$vswhere" -latest -find 'VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT/*.dll' | tr -d '\r')
    for dll in msvcp140.dll vcruntime140.dll; do
      if ! printf '%s\n' "${runtime[@]}" | grep -qi "/$dll$"; then
        echo "$dll not found in the Visual Studio redistributables" >&2
        exit 1
      fi
    done
    files+=("${runtime[@]}")
    extension=zip
    ;;
  *)
    echo "unknown platform $platform" >&2
    exit 2
    ;;
esac

name="trove-$version-$platform"
archive="$name.$extension"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp "${files[@]}" "$stage/"

mkdir -p "$out"
out=$(cd "$out" && pwd)
rm -f "$out/$archive"
if [ "$extension" = zip ]; then
  (cd "$stage" && 7z a -tzip -bso0 "$out/$archive" .)
else
  (cd "$stage" && tar -czf "$out/$archive" -- *)
fi

hash=$(openssl dgst -sha256 -r "$out/$archive" | cut -d ' ' -f 1)
printf '%s  %s\n' "$hash" "$archive" > "$out/$archive.sha256"
echo "$out/$archive"
