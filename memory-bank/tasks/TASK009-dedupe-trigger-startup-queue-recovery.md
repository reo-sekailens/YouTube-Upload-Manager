# TASK009 — Dedupe trigger and automatic startup queue recovery

**Status:** completed  
**Owner:** unassigned  
**Dependencies:** TASK004, TASK005

## Objective

Give the operator an explicit duplicate-scan control while making device-local queue recovery an automatic app-start responsibility instead of a manual dashboard action.

## Boundaries

- Duplicate detection remains channel-scoped, explainable, and review-only; the control must not delete videos.
- A duplicate scan refreshes the connected channel's YouTube inventory before rebuilding exact normalized-title and trailing `(2+)` candidates.
- Startup recovery performs only the existing crash-safe local import and upload-state reconciliation. It must not silently start a fresh YouTube upload session.
- Existing managed media, resumable-session checkpoints, channel isolation, and audit receipts remain intact.

## Work items

- [x] **TASK009-A — Native startup recovery:** Extract the queue reconciliation implementation for reuse during Tauri setup, run it before the UI session starts, preserve the command for compatibility, and add focused restart recovery tests.
- [x] **TASK009-B — Dedupe UI:** Remove the manual Recover queue action, add a channel-gated Run dedupe action beside Duplicate candidates, refresh inventory and candidates through the existing native boundary, and cover the invocation contract.
- [x] **TASK009-C — Integration and evidence** *(depends on TASK009-A, TASK009-B)*: Review combined behavior, update memory-bank architecture/progress/technical notes, run Rust and web verification, then perform desktop and mobile browser QA with screenshots.

## Acceptance criteria

- Every app session reconciles interrupted `importing`, `dispatching`, and `uploading` records before the dashboard loads.
- Interrupted local imports resume when their saved source is available; interrupted uploads enter safe provider reconciliation without automatic duplicate retries.
- The dashboard no longer shows a Recover queue button.
- A visible Run dedupe button is enabled only for an active channel, synchronizes that channel's uploaded-video inventory, and refreshes rendered candidates.
- Empty, success, busy, and error states remain understandable and accessible on desktop and mobile.

## Evidence

- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml -j 1` passed: 11 tests.
- `npm run check`, `npm run test`, and `npm run build` passed: 9 web tests and a 40-module production build.
- `npm run tauri -- build --bundles nsis` produced the refreshed x64 Windows installer; SHA-256 `769F33C51C14647F1EA7F9A1F8C591B6F88F921C9F9A7B3B8EEE3E869E6511FD`.
- Deterministic Browser QA at 1280×900 and 390×844 verified the enabled **Run dedupe** action, successful inventory command path, rendered `Launch Video` / `Launch Video (2)` candidate, clean console, and no horizontal overflow.
- No live Google/YouTube synchronization or upload was performed; provider certification still requires an authorized test channel.
