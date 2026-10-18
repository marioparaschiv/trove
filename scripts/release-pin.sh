#!/usr/bin/env bash
# Prints the release pin the Obsidian plugin is bundled with (its TROVE_RELEASE_PIN): the version, the GitHub
# repository, and each platform's archive with its SHA-256, read from the .sha256 files package-release.sh wrote.
#
# Usage: scripts/release-pin.sh <version> <owner/repository> <folder with the archives>
set -euo pipefail
shopt -s nullglob

if [ $# -ne 3 ]; then
  echo "usage: $0 <version> <owner/repository> <folder>" >&2
  exit 2
fi
version=$1
repository=$2
folder=$3

hash_files=("$folder"/trove-"$version"-*.sha256)
if [ ${#hash_files[@]} -eq 0 ]; then
  echo "no trove-$version-*.sha256 files in $folder" >&2
  exit 1
fi

assets='{}'
for hash_file in "${hash_files[@]}"; do
  read -r sha256 file < "$hash_file"
  platform=${file#"trove-$version-"}
  platform=${platform%%.*}
  assets=$(jq --compact-output --arg platform "$platform" --arg file "$file" --arg sha256 "$sha256" \
    '. + {($platform): {file: $file, sha256: $sha256}}' <<< "$assets")
done

jq --null-input --compact-output --arg version "$version" --arg repository "$repository" --argjson assets "$assets" \
  '{version: $version, repository: $repository, assets: $assets}'
