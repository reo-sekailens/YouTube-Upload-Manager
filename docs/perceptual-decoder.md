# Local perceptual decoder packaging

The desktop duplicate-review worker uses a separately bundled FFmpeg executable
to read small grayscale frames from the approved original. It looks beside the
application executable for `ffmpeg.exe` on Windows and `ffmpeg` on macOS/Linux.
The application does not download a decoder at runtime or search `PATH` for a
substitute. Android and iOS skip this desktop preparation step.

`scripts/prepare-perceptual-decoder.mjs` prepares the same four desktop target
triples as the existing FFprobe preparation. It streams downloads with a
120-second deadline, an exact pinned length and SHA-256, a 128 MiB binary limit
and a 1 MiB per-notice limit. Non-regular cache files are rejected. A pre-existing
output junction is accepted only in read-only mode when all selected decoder
bytes, raw target notices, generated notices and canonical provenance receipt
already match their exact pins. This path performs no downloads, directory
creation, permissions changes, repairs, temporary writes or receipt rewrites.
Missing, corrupt or unverified redirected caches fail. Redirected raw-notice
subdirectories are rejected. Temporary downloads into ordinary output folders replace a prior artifact
only after verification; incomplete, oversized, truncated and mismatched
downloads do not receive a provenance receipt. Every build rehashes cached
artifacts. The receipt records the inputs and generated notice digests.

An existing build-cache junction can be populated separately at its explicitly
resolved target by the integration owner. Stage the prepared decoder,
`ffmpeg-license.txt`, `ffmpeg-source-notice.txt`,
`.perceptual-decoder-provenance.json` and the selected files in
`.perceptual-decoder-notices/`. Preparation itself never populates or repairs a
junction; a successful read-only result includes `readOnly: true`.

## Build integration

The integration owner must make these shared configuration changes:

- Add package script `prepare:perceptual-decoder` with value
  `node scripts/prepare-perceptual-decoder.mjs`.
- Add `node --test scripts/prepare-perceptual-decoder.tests.mjs` to the
  operations test command or run it separately.
- Set `build.beforeDevCommand` to
  `npm run prepare:ffprobe && npm run prepare:perceptual-decoder && npm run dev`.
- Set `build.beforeBuildCommand` to
  `npm run build && npm run prepare:ffprobe && npm run prepare:perceptual-decoder`.
- Add `binaries/ffmpeg` to `bundle.externalBin`.
- Add `binaries/ffmpeg-license.txt` and `binaries/ffmpeg-source-notice.txt` to
  `bundle.resources`.
- Extend portable-archive inspection to require the decoder and both notices,
  verify its matching target digest/architecture, and compare the archived
  bytes with the prepared artifact. Existing FFprobe checks remain required.

Tauri strips the target triple from the packaged sidecar name. Preparation
creates `ffmpeg-<target-triple>[.exe]`; a macOS universal build prepares both
architecture artifacts when `TAURI_ENV_TARGET_TRIPLE=universal-apple-darwin`
or `--universal` is supplied. It does not create a combined executable itself.
Unsupported explicit desktop targets fail rather than selecting another
architecture. Unsupported hosts and explicit mobile targets skip provisioning.

## Provenance and notices

Artifacts are pinned to the exact bytes published under
[eugeneware/ffmpeg-static release b6.1.1](https://github.com/eugeneware/ffmpeg-static/releases/tag/b6.1.1).
The SHA-256 digests and exact lengths were read from GitHub's release-asset
metadata on 2026-10-13. All eight target LICENSE/README downloads were also
checked against those digests locally.

| Target | Decoder bytes | Decoder SHA-256 |
| --- | ---: | --- |
| Windows x64 | 82,797,568 | `04e1307997530f9cf2fe35cba2ca7e8875ca91da02f89d6c7243df819c94ad00` |
| Linux x64 | 79,826,272 | `e7e7fb30477f717e6f55f9180a70386c62677ef8a4d4d1a5d948f4098aa3eb99` |
| macOS arm64 | 45,568,216 | `a90e3db6a3fd35f6074b013f948b1aa45b31c6375489d39e572bea3f18336584` |
| macOS x64 | 78,862,176 | `ebdddc936f61e14049a2d4b549a412b8a40deeff6540e58a9f2a2da9e6b18894` |

The upstream [build-source description](https://github.com/eugeneware/ffmpeg-static/blob/b6.1.1/README.md)
identifies Gyan for Windows, John Van Sickle for Linux, OSXExperts for macOS
arm64 and evermeet for macOS x64. The artifact release tag does not establish a
common FFmpeg version: the pinned Windows README says 6.1.1, Linux README says
7.0.2 and macOS x64 README says 8.0. The macOS arm64 README does not identify a
build version. The source notice preserves the selected target README verbatim,
including its version, source/configuration/dependency information, its download
URL and digest. No single FFmpeg source tag is presented as corresponding to
every platform build.

The generated license notice preserves each selected target's supplied LICENSE
and includes the complete GNU GPL version 3 text provided with the upstream
Windows/Linux artifacts. The Windows and Linux build notices identify GPLv3;
macOS supplies FFmpeg's license overview. FFmpeg's
[license explanation](https://ffmpeg.org/legal.html) states that enabled GPL
components change the licensing of the FFmpeg build. These source and license
notices are provenance evidence; they do not establish that a complete set of
corresponding dependency sources has been archived or certify distribution
compliance. A release must retain the matching distributor's source/build
material for the exact binary it ships.

## Verification boundary

Local injected-stream tests cover successful preparation, warm-cache
revalidation, independent universal notices, mobile/unsupported skips, checksum
and length failures, oversized output, stalled headers/body, input bounds and
non-regular cache destinations. The Unix symlink case is skipped on Windows.
The final Windows run passed 18 tests with that one skipped case, including
read-only acceptance of a fully prepared Windows directory junction and
unchanged-cache rejection for missing/corrupt binaries, notices and provenance.
Script syntax and whitespace checks
passed.
The Windows artifact was downloaded, its SHA-256/length verified, and executed
against generated MP4 media. The native worker's `mov` demuxer whitelist accepted
MP4 and produced the expected 72-byte `9 x 8` grayscale frame. Its PE machine
was checked as `0x8664` (x64). A separate generated-media run decoded 49 frames
from seven clips with the native arguments in 18,396 ms; the original generated
source digests remained unchanged. The aligned grayscale hashes retained the
re-encoded and color-shifted versions as candidates, rejected different motion
and a shared-intro clip, and marked still/short fixtures as limited under an
equivalent threshold calculation. Color-shift invariance demonstrates why a
candidate remains subject to human playback review.

This is decoder/build-script evidence. Native perceptual comparison accuracy,
packaged installation/runtime behavior and execution on Linux/macOS/mobile
require their separate verification. Generated fixture paths and receipts are
kept outside the checkout under the task's artifact directory.
