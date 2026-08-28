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

- Built [YouTube Upload Manager_1.0.2-nightly.1_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.1_x64-setup.exe)
  from the current local working tree with `npm run tauri -- build --bundles nsis`.
- Artifact size: 26,730,437 bytes. SHA-256:
  `210D9BCEF719F8D46AA914392E087803BD76EA560F984F63B52DE458B33CF82B`.
- Authenticode status is `NotSigned`; this is an unsigned local installer, not
  a production-signed release.
- Tauri reused the verified `ffprobe` sidecar and the configured sidecar binary
  and license files are present. Archive-level payload enumeration was not
  available locally without installing or adding an archive tool.
- Current local nightly: [YouTube Upload Manager_1.0.2-nightly.7_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.7_x64-setup.exe),
  26,778,205 bytes, SHA-256
  `F0E15C05350BE92A58D7B7B53C9E1AAE93E9A3CB7405AA34A239FDCB87178CFC`,
  Authenticode `NotSigned`.

## Rebuild evidence (2026-08-24)

- Rebuilt the current working tree with `npm run tauri -- build --bundles nsis`.
  The frontend type check and production build passed, and the build reused the
  verified bundled x64 FFprobe sidecar.
- Current artifact: [YouTube Upload Manager_1.0.2-nightly.7_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.7_x64-setup.exe)
  (26,772,899 bytes; SHA-256
  `D7E37AEEFFF8ABE3C29FF59958E6F122406037F7EB2EE4484D5DD7F8FEF1E3C9`).
  Authenticode status remains `NotSigned`; it is an unsigned local installer.

## Local-deletion rebuild (2026-08-24)

- Rebuilt after the faster deep-review deletion and readable bulk-progress
  changes with `npm run tauri -- build --bundles nsis`.
- Current artifact: [YouTube Upload Manager_1.0.2-nightly.7_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.7_x64-setup.exe)
  (26,782,809 bytes; SHA-256
  `784F485EC41C2F43943217962F538793562E80253775063D4FA9A79B53137032`).
  Authenticode status: `NotSigned`.

## Light-review deletion rebuild (2026-08-24)

- Rebuilt after making light-review deletion explicit: an accepted light match
  does not start a deep duplicate review.
- Current artifact: [YouTube Upload Manager_1.0.2-nightly.7_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.7_x64-setup.exe)
  (26,776,647 bytes; SHA-256
  `DCC66856E14CA7C5BF864397846335245C6C34BEAF3DC0B2E218681564FEF3CF`).
  Authenticode status: `NotSigned`.

## Fast light-deletion rebuild (2026-08-24)

- Rebuilt after removing full-file content hashing from operator-selected light
  deletion. Deep-review deletion retains its final staged-content comparison.
- Current artifact: [YouTube Upload Manager_1.0.2-nightly.7_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.7_x64-setup.exe)
  (26,782,375 bytes; SHA-256
  `FAA8A5ADD4D22CEB4985472B8BA37518905E83282629DC36E92B41267BD73703`).
  Authenticode status: `NotSigned`.

## Distinct light-delete upgrade (2026-08-24)

- Published as `1.0.2-nightly.8` so Windows can upgrade a running nightly.7
  install rather than leaving the prior same-version executable in place.
- Light bulk deletion visibly reports **no file scan** while it checks the
  saved match and safe path.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.8_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.8_x64-setup.exe)
  (26,777,124 bytes; SHA-256
  `ECB686BFA6306879D1BB9BC8714C49CE1CC7E12793416E17F0CB2664059ED6DA`).
  Authenticode status: `NotSigned`.

## Retained-file recovery upgrade (2026-08-24)

- Published as `1.0.2-nightly.9`. A guarded or unavailable selected source no
  longer aborts the whole bulk operation; valid later selections continue and
  retained filenames show their exact safe native error.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.9_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.9_x64-setup.exe)
  (26,781,715 bytes; SHA-256
  `E12A85A37492A5F680796E4B93D067599553F5ACB2C21485FE9491952482CBC6`).
  Authenticode status: `NotSigned`.

## Uploaded-only light-dedupe upgrade (2026-08-24)

- Published as `1.0.2-nightly.10`. Light duplicate detection now considers only
  videos already uploaded to the active YouTube channel; local queued, failed,
  draft, and cancelled records are excluded.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.10_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.10_x64-setup.exe)
  (26,761,961 bytes; SHA-256
  `D6F132B81A3E72D7E30C537B9B85EAA680CDA4C0D0837731ED71A472D64B0E75`).
  Authenticode status: `NotSigned`.

## Playlist-management upgrade (2026-08-26)

- Published as `1.0.2-nightly.11`, adding the Playlists workspace with title
  ordering, private playlist creation, and explicit selected-video additions.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.11_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.11_x64-setup.exe)
  (26,771,153 bytes; SHA-256
  `BB6F799DE84B8914DF865CC25B475BA6192EA492A1A3B763BA50A141D0E7EC95`).
  Authenticode status: `NotSigned`.

## Playlist custom-order upgrade (2026-08-26)

- Published as `1.0.2-nightly.12`, adding public/unlisted/private playlist
  creation, a YouTube Studio playlist-settings shortcut, and managed Manual
  A-Z/Z-A playlist item ordering.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.12_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.12_x64-setup.exe)
  (26,780,595 bytes; SHA-256
  `5F498068D6AD99690DF78E0B4B4AF2459881476A4506520EE863FE7F81A6E4F2`).
  Authenticode status: `NotSigned`.

## Playlist workflow split (2026-08-26)

- Published as `1.0.2-nightly.13`, separating playlist creation/video additions
  from existing-playlist settings and custom ordering.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.13_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.13_x64-setup.exe)
  (26,778,480 bytes; SHA-256
  `91317A6921FA3491E4BDBA17953F63D67CC214FF5F0153B06320D3A8A9141803`).
  Authenticode status: `NotSigned`.

## Verified playlist ordering (2026-08-26)

- Published as `1.0.2-nightly.14`, adding per-video ordering progress, an
  activity log, native provider failure details, and a final YouTube readback.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.14_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.14_x64-setup.exe)
  (26,786,895 bytes; SHA-256
  `C274B80136DF49FB95C0C1D0B61559B1E9360B60AF8CB0F6269B4F98C9BC6166`).
  Authenticode status: `NotSigned`.

## Current local rebuild (2026-08-28)

- Rebuilt commit `5a0ef19` with `npm run tauri -- build --bundles nsis`.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.16_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.16_x64-setup.exe)
  (26,785,398 bytes; SHA-256
  `20C3FAE071BC43C994870EED42BFAC5B4E9EDFB800BF50F4FE307BDEF43DDE1D`).
- Authenticode status: `NotSigned`. The NSIS build used the verified x64
  FFprobe sidecar and the configured license resource; this remains an unsigned
  local installer, not a production-signed release.

## Lazy-workspace CSS repair rebuild (2026-08-28)

- Built `1.0.2-nightly.17` after restoring emitted Tailwind CSS ownership for
  the Deletion Review, Playlists, and Pre-ingest Duplicate Review workspaces.
- Artifact: [YouTube Upload Manager_1.0.2-nightly.17_x64-setup.exe](../../src-tauri/target/release/bundle/nsis/YouTube%20Upload%20Manager_1.0.2-nightly.17_x64-setup.exe)
  (26,812,169 bytes; SHA-256
  `2079B9DFB0CB8DB6EA6F1A164113AA3925568A332779B656EA064B63C1B024E6`).
- Authenticode status: `NotSigned`. The x64 FFprobe sidecar and license
  resource were present in the package.
