# TASK122: v1 GitHub release

## Status

in-progress

## Objective

Publish the first stable GitHub release as `v1.0.0`, with the current Windows
x64 NSIS installer and a matching source tag.

## Scope and safety boundary

- Synchronize the application version in the JavaScript package, lockfile,
  Rust package, and Tauri bundle configuration.
- Publish only the locally built Windows x64 NSIS installer for this release.
- The installer remains unsigned. Do not add signing credentials or describe
  the artifact as signed, notarized, cross-platform, or live-provider verified.
- Existing device-local queues, audit records, hashes, originals, and OAuth
  credentials are not release assets and must not be included in the build.

## Acceptance criteria

- `main` contains the exact `1.0.0` application version.
- The release commit is tagged `v1.0.0` and pushed to GitHub.
- GitHub has a non-prerelease `v1.0.0` release with the Windows x64 NSIS
  installer, its SHA-256, and its unsigned status stated in the notes.
- Frontend, native, and installer build checks are recorded as local evidence;
  installation and live YouTube verification remain explicitly unverified.

## Validation

- `npm test` passed: 87 frontend tests.
- `cargo test` passed: 134 native tests, with 5 explicitly ignored local
  release-only benchmarks.
- `npm run check`, `npm run check:tailwind-ui`, `cargo fmt --check`, and
  `npm run performance:frontend:check` passed.
- `npm run tauri -- build --bundles nsis` produced
  `YouTube Upload Manager_1.0.0_x64-setup.exe` (26,721,098 bytes; SHA-256
  `5564B0ABA17681101C6B1F3176435ADA62C72694E52162BD1E71B71C51E2FD69`).
  Authenticode status: `NotSigned`.
- GitHub publication remains pending.
