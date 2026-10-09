# Local operations and verification

Run these commands from the repository checkout. The verification workflow uses
Node.js 24 and stable Rust; install the platform's Tauri prerequisites before a
native build. Use `npm ci` to install the committed dependency versions.

## Checks for a change

```sh
npm run check
npm run test
npm run test:operations
npm run performance:frontend:check
npm run performance:native:check -- --configuration-only
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Frontend bundle checks build the production webview and record local byte
measurements under `output/performance`. The native configuration check reads
source only; it does not execute the queue, open an operator profile, or measure
runtime performance.

For native changes, prepare the desktop sidecar and run the native suite:

```sh
npm run prepare:ffprobe
cargo test --manifest-path src-tauri/Cargo.toml --features performance-harness
cargo check --manifest-path src-tauri/Cargo.toml
```

The focused query and queue gates are also available through
`npm run performance:native:check`. Each exact Cargo filter must produce its
named passing test and a receipt with exactly one passed, zero failed, and zero
ignored tests. A successful Cargo exit with no matching test fails this check.

The script regression tests use synthetic files and injected process/network
fixtures. They do not download FFprobe, launch Cargo, or contact Google/YouTube.

## FFprobe provisioning

Desktop preparation streams the pinned FFprobe binary and license, checks each
SHA-256 digest before replacement, and records provenance in
`src-tauri/binaries/.ffprobe-provenance.json`. An unchanged, verified receipt
avoids network access and full-file hashing. Changes to provenance or file
identity trigger revalidation; a missing or invalid artifact is downloaded
again. Mobile targets skip desktop sidecar provisioning.

Each binary or license download has a two-minute deadline covering response
headers and the streamed body. A timeout fails the build step, cancels the
stream, and removes its partial download; an existing destination is retained.
Retry the preparation command after resolving connectivity. Do not edit the
pinned checksums or create a verification receipt by hand.

Select a target explicitly when cross-building, for example:

```sh
npm run prepare:ffprobe -- x86_64-pc-windows-msvc
npm run prepare:ffprobe -- --universal
```

The second command prepares both supported macOS architectures and their
license notices. Cross-platform preparation alone does not certify a package
on that operating system.

## Evidence and release handoff

Record the exact checks and outcomes for the scoped task in the memory bank.
Keep TypeScript/Node/Rust fixtures, browser rendering, unsigned packaged
runtime, signed production, and live Google/YouTube results distinct. Local
passing tests do not establish that OAuth, uploads, playlist changes, or
deletions work against a real channel.

Build and timing procedures for the isolated Windows performance package live
in [the performance runner guide](../scripts/performance/README.md). That
runner requires a disposable empty profile and rejects an ordinary production
executable. It must never use the operator's real profile.

Intake saves a reference to each accepted original video and its integrity
evidence. It does not create another video copy, link, reflink, or clone. Keep
the original available and unchanged until upload and recovery complete.
Retry and crash recovery verify that file and reconcile the persisted upload
session; a missing or changed original fails safely. Existing referenced legacy
managed media is preserved when it is needed for recovery.

Keep the persisted reconciliation record when completion is ambiguous;
removing the application profile loses the queue and session evidence,
and a fresh import can create another upload. Preserve the queue and use the
application's recovery outcome and redacted diagnostics when investigating a
failure.

Standard Windows packaging remains unsigned unless a separate signing process
is configured. Build an installer with `npm run tauri -- build --bundles nsis`;
by default it is emitted beneath `src-tauri/target/release/bundle/nsis`.
`CARGO_TARGET_DIR`, when configured, changes that target location. Building
locally does not publish a release.
