# TASK058: Bundled desktop FFprobe sidecar

## Status

completed

## Scope

Bundle a verified FFprobe sidecar with each supported desktop package so local
video-container metadata, including duration, does not depend on a separately
installed FFmpeg. Android and iOS must not package or execute this sidecar.

## Distribution and provenance

- Source the sidecars at build time from the pinned
  [`eugeneware/ffmpeg-static` b6.1.1 release](https://github.com/eugeneware/ffmpeg-static/releases/tag/b6.1.1).
- Package the matching `ffprobe` executable for Windows x64, Linux x64, and
  macOS x64/arm64 only.
- Validate the selected release artifact against its published SHA-256 before
  packaging.
- Carry the selected artifact's supplied license in each desktop bundle.
- Record the upstream FFmpeg provenance as
  [FFmpeg n6.1.1](https://github.com/FFmpeg/FFmpeg/tree/n6.1.1).

## License boundary

FFprobe is GPLv3-or-later. It is distributed alongside the application's
AGPL-3.0-or-later code under compatible terms. The repository's root `LICENSE`
stays the canonical, unaltered AGPL text for GitHub license detection; third
party provenance and notices belong in `NOTICE` and the bundled sidecar license
file.

## Acceptance criteria

- A clean desktop package can read FFprobe metadata without relying on PATH or
  a system FFmpeg installation.
- Each desktop target receives only its matching executable and license.
- Artifact checksums fail the package build before an unverified binary can be
  included.
- Android and iOS builds contain no FFprobe sidecar and never spawn one.
- Installer/package inspection and the relevant automated tests verify the
  build-time sidecar behavior.

## Evidence

- The Windows x64 preparation command downloaded `ffprobe-win32-x64` from the
  pinned release and verified SHA-256
  `3a7e2dc003dc2cd1472827e4c7c4f056ae1ae0ae7c5bbc580c99b49827351ba4`.
- `npm run tauri -- build --bundles nsis` completed locally. Its generated NSIS
  script includes `ffprobe.exe` at the installer root and
  `binaries\\ffprobe-license.txt` in the installed application resources.
- The resulting unsigned Windows x64 installer is
  `src-tauri/target/release/bundle/nsis/YouTube Upload Manager_0.1.9_x64-setup.exe`
  (26,364,318 bytes, SHA-256
  `54F7941CE49193709E321EFC29C819E55E26F592DE220E9986771DD8DD67CCCE`).
- Rust format and 37 native tests, TypeScript check, production build, and diff
  check passed locally. Linux and macOS bundle preparation is configured but
  has not been run on those operating systems; Android/iOS sidecar exclusion is
  configuration-based and has not been built locally.
