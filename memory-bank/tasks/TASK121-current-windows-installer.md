# TASK121: Current Windows installer

## Status

completed

## Objective

Build the current local source state as an x64 NSIS installer without changing
signing, deployment, or release configuration.

## Acceptance criteria

- Produce a Windows NSIS installer from the present working tree.
- Record the exact artifact path, SHA-256, and Authenticode status.
- Verify the bundle inputs include the app executable, FFprobe sidecar, and
  sidecar license without installing it.

## Completion evidence (2026-08-23)

- Built `src-tauri/target/release/bundle/nsis/YouTube Upload Manager_0.1.9_x64-setup.exe` from the current local working tree with `npm run tauri -- build --bundles nsis`.
- Artifact size: 26,723,287 bytes. SHA-256:
  `F431F3409CE75B640EF6119018D34A0B3A14B8B27FEB82A5B480AF1AADB5BFCA`.
- Authenticode status is `NotSigned`; this is an unsigned local installer, not
  a production-signed release.
- Tauri reused the verified `ffprobe` sidecar and the configured sidecar binary
  and license files are present. Archive-level payload enumeration was not
  available locally without installing or adding an archive tool.
