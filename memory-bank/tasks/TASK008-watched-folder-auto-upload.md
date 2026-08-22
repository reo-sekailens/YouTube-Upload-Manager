# TASK008 — Opt-in watched-folder private uploads

## Status

`completed`

## Objective

Allow an operator to bind one device-local folder to the currently connected
YouTube channel. While the desktop app is running, completed video files newly
appearing in that folder are ingested into the managed workspace, checked
against the local SHA-256 ledger, and uploaded privately without another click.

Enabling the monitor is the operator's explicit recurring upload approval. It
does not authorize public publishing, deletion, telemetry, or any non-YouTube
network destination.

## Boundaries

- The monitor is disabled by default and must be enabled for a selected folder
  while a YouTube channel is connected.
- The authorization is bound to that channel scope. A disconnected or changed
  channel pauses processing rather than redirecting files to another account.
- Scan only direct child files with supported video extensions; ignore
  directories, symlinks, hidden/temp files, and unsupported extensions.
- Require an unchanged size and modification time across consecutive scans
  before ingesting, so files still being copied are not uploaded.
- Every accepted source is copied to the managed local workspace and SHA-256
  verified before it becomes upload-eligible.
- A digest already represented by a canonical local upload record is skipped;
  retries reuse that record rather than creating another YouTube upload.
- A title matching the last synchronized active-channel inventory, including a
  trailing `(2)` or higher variant, is skipped for review rather than uploaded
  automatically. This is conservative metadata evidence, not media identity.
- Automatic uploads always use the existing private, resumable, crash-safe
  YouTube upload path. Ambiguous outcomes enter reconciliation.
- Monitoring runs only while the native app is running. No cloud service,
  daemon installation, telemetry, or background OS service is introduced.

## Work items

### TASK008-A — Native monitor and idempotent ingestion

- **Paths:** `src-tauri/src/lib.rs`
- **Depends on:** none
- Persist monitor configuration and file observation receipts locally.
- Add a polling worker, stable-file gate, supported-file validation, channel
  binding, digest-based skip/reuse behavior, managed import, queueing, and
  private resumable upload dispatch.
- Expose narrow commands for reading, enabling, disabling, and manually
  requesting a scan.
- Add focused Rust tests without calling Google or YouTube.

### TASK008-B — Operator controls and status

- **Paths:** `src/App.tsx`, `src/components/FolderMonitorPanel.tsx`,
  `src/lib/local.ts`, `src/lib/local.test.ts`, `src/lib/types.ts`,
  `src/lib/types.test.ts`, `src/styles.css`
- **Depends on:** none
- Add a dashboard panel that selects a folder, clearly authorizes recurring
  private uploads, shows channel binding/status, allows disabling, and refreshes
  queue state after an operator-requested scan.
- Keep browser-preview behavior safe and accessible.

### TASK008-C — Integration, documentation, and QA

- **Paths:** `memory-bank/tasks/TASK008-watched-folder-auto-upload.md`,
  `memory-bank/tasks/_index.md`, `memory-bank/progress.md`,
  `memory-bank/architecture.md`, `memory-bank/technical-notes.md`
- **Depends on:** TASK008-A, TASK008-B
- Record the opt-in safety boundary, implemented behavior, validation evidence,
  and live-provider limitation.

## Acceptance criteria

- No folder is monitored until the operator explicitly enables one.
- Enabling fails without a connected channel or a valid directory.
- A supported file is ignored until unchanged across consecutive scans.
- Once stable, it is imported to managed storage and queued for private upload.
- The same source fingerprint or SHA-256 digest cannot create a second upload.
- A title already represented in the last synchronized channel inventory is
  not automatically uploaded.
- Channel disconnect or mismatch pauses the monitor without uploading.
- Disable stops new discovery without deleting queued files or source media.
- UI exposes folder, channel, state, last result, and irreversible-network
  implications in clear language.
- Rust tests, TypeScript checks, web tests, production build, and rendered
  desktop/mobile QA pass.

## Evidence

- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml -j 1` passed all 9 Rust
  tests. Folder-monitor coverage proves baseline exclusion, two-scan stability,
  supported-file filtering, SHA-256 reuse, synchronized-title withholding,
  channel mismatch pause, enable validation, and disable behavior without
  provider calls.
- `npm run check`, `npm run test` (8/8), and `npm run build` passed.
- In-app Browser QA passed at 1280×900 and 390×844. The rendered workflow
  showed the disabled state, explicit recurring-private-upload approval,
  enabled folder/channel facts, scan status, disable control, and no mobile
  horizontal overflow or console warnings/errors.
- Browser evidence used deterministic local fixture state. No folder contents,
  Google credentials, or YouTube account were accessed during visual QA.

## Follow-ups

- Live YouTube verification requires an authorized test channel and a deliberate
  private video canary placed into a temporary watched folder.
- Mobile background behavior remains uncertified; this implementation promises
  monitoring only while the native app is running and can access the folder.
