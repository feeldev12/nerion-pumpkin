#!/usr/bin/env bash
# Republishes Pumpkin's official Android nightly, stripped, as a pinned release (see README).
# Needs gh and an NDK (ANDROID_NDK_HOME) for llvm-strip.
set -euo pipefail
ndk="${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME}"
strip=$(ls "$ndk"/toolchains/llvm/prebuilt/*/bin/llvm-strip* | head -1)
commit=$(gh api repos/Pumpkin-MC/Pumpkin/releases/tags/nightly --jq .body | sed -n 's/^From commit: \([0-9a-f]*\).*/\1/p')
[ -n "$commit" ] || { echo "can't tell the nightly's commit" >&2; exit 1; }
full=$(gh api "repos/Pumpkin-MC/Pumpkin/commits/$commit" --jq .sha)
tag="pumpkin-android-$commit"
out=$(mktemp -d)
curl -fsSL https://github.com/Pumpkin-MC/Pumpkin/releases/download/nightly/pumpkin-aarch64-android -o "$out/pumpkin"
"$strip" -o "$out/libpumpkin.so" "$out/pumpkin"
sha=$(sha256sum "$out/libpumpkin.so" | cut -d' ' -f1)
echo "$sha  libpumpkin.so" > "$out/libpumpkin.so.sha256"
gh release create "$tag" "$out/libpumpkin.so" "$out/libpumpkin.so.sha256" \
  --repo feeldev12/nerion-pumpkin --title "Pumpkin for Android ($commit)" \
  --notes "Official \`pumpkin-aarch64-android\` nightly built from Pumpkin-MC/Pumpkin@$full, debug symbols stripped. Source: https://github.com/Pumpkin-MC/Pumpkin/tree/$full (GPL-3.0). SHA-256: $sha"
echo "published $tag sha256=$sha"
