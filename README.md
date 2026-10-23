# Pumpkin for Nerion on Android

Releases here are the official Android build of [Pumpkin](https://github.com/Pumpkin-MC/Pumpkin)
(`pumpkin-aarch64-android` from Pumpkin's `nightly` release), with debug symbols stripped
(`llvm-strip`) and nothing else changed. Nerion's Android app ships it as
`lib/arm64-v8a/libpumpkin.so`, because Android only lets an app run binaries installed with it,
and starts it as its own process for local servers.

Pumpkin's nightly link changes every day, so each release here pins one build: its tag names the
Pumpkin commit it was built from, and the release notes link that commit's source.

## License

Pumpkin is licensed under the GNU General Public License v3.0 ([LICENSE](LICENSE)). The complete
source of each binary is the Pumpkin commit named in its release, at
`https://github.com/Pumpkin-MC/Pumpkin/tree/<commit>`.

## Publishing a new build

`tools/release.sh` downloads the current official nightly, checks which commit it was built from,
strips it and publishes it as `pumpkin-android-<commit>`. Then update `pumpkin.properties` in
Nerion's launcher (`src-tauri/gen/android/app/`) with the new tag and SHA-256.
