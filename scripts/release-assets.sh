#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Build the release assets of the Linux x86_64 binary into one directory:
#   lodwright-x86_64-unknown-linux-gnu   the binary
#   lodwright-universal-apple-darwin     the macOS binary (arm64 and x86_64), built on macOS and passed in [macos-dir]
#   lodwright-<document>.v<N>.json       the contract schemas (copied from schemas/, checked against the binary)
#   SHA256SUMS                           the SHA-256 of every file above
# The attestation is made by the release workflow over this directory.
# Usage: scripts/release-assets.sh <out-dir> [macos-dir]
set -euo pipefail

out="${1:?usage: scripts/release-assets.sh <out-dir> [macos-dir]}"
macos="${2:-}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

if [ -e "$out" ] && [ -n "$(ls -A "$out")" ]; then
  echo "release-assets: $out exists and is not empty" >&2
  exit 1
fi
mkdir -p "$out"

cargo build --release --locked -p lodwright-cli
binary="target/release/lodwright"
cp "$binary" "$out/lodwright-x86_64-unknown-linux-gnu"

# The published schemas are the committed ones, and the binary must print exactly the same text.
shopt -s nullglob
schemas=(schemas/lodwright-*.v*.json)
if [ "${#schemas[@]}" -eq 0 ]; then
  echo "release-assets: no schema under schemas/" >&2
  exit 1
fi
for file in "${schemas[@]}"; do
  name="$(basename "$file")"
  kind="${name#lodwright-}"; kind="${kind%%.*}"
  version="${name#*.v}"; version="${version%.json}"
  if ! "$binary" schema "$kind" --schema-version "$version" | cmp -s - "$file"; then
    echo "release-assets: $file differs from 'lodwright schema $kind --schema-version $version'" >&2
    exit 1
  fi
  cp "$file" "$out/$name"
done

if [ -n "$macos" ]; then
  mac="$macos/lodwright-universal-apple-darwin"
  [ -f "$mac" ] || { echo "release-assets: $mac is missing" >&2; exit 1; }
  install -m 0755 "$mac" "$out/lodwright-universal-apple-darwin"
fi

(cd "$out" && sha256sum -- * > ../SHA256SUMS.tmp)
mv "$out/../SHA256SUMS.tmp" "$out/SHA256SUMS"
ls -l "$out"
