# Architecture

## Current state

The repository is newly initialized; no application code has been written yet. The selected shape is a local-first Tauri 2 application using React + Vite for the interface and Rust for privileged local operations. It targets Windows, macOS, Linux, Android, and iOS from one codebase. All application-controlled state stays on the device; Google/YouTube is the only required external service. This document describes required boundaries, not implemented components.

## Intended ownership boundaries

| Area | Owns | Must not own |
| --- | --- | --- |
| React webview UI | File selection, metadata entry, review, progress display | OAuth tokens, unrestricted filesystem access, direct privileged API calls |
| Rust local command layer | OAuth exchange, validation, scoped filesystem access, batch orchestration, audit events | Rendering untrusted UI or exposing token material |
| Local job engine | Crash-safe queue, resumable upload execution, provider reconciliation, retries, quota/rate-limit handling | User interface state as its source of truth |
| OS secure store | Refresh tokens, encryption keys, session secrets | Webview-readable or plaintext secret storage |
| Local SQLite database | Canonical batches, items, idempotency keys, lifecycle, cache, and audit records | Raw OAuth tokens or unnecessary source-video copies |
| Managed local media workspace | Device-local immutable copies, source locators, digests, and resume-safe access | Automatic copying to an app-controlled cloud store |
| YouTube APIs | Account authorization and video upload/publish outcomes | Local operational state |

## Canonical entities

- **Account connection:** the authorized YouTube identity and its credential reference; scoped to an operator or tenant when multi-user support exists.
- **Upload batch:** operator-owned group of proposed uploads with a review and execution state.
- **Upload item:** one source asset plus its requested metadata, idempotency identity, lifecycle state, and provider video reference once known.
- **Local asset:** a device-local, immutable imported copy with byte size, SHA-256 digest, import state, and optional original source locator. It remains available when the app restarts or an external source moves.
- **Execution attempt:** an append-only record of a specific provider interaction, retry decision, response reference, redacted error details, and durable resume checkpoint.
- **Resume checkpoint:** encrypted resumable session URI, total byte length, provider-confirmed byte offset, request metadata fingerprint, and checkpoint time. It is updated transactionally after each acknowledged range.
- **Inventory video:** an account-scoped projection of an operator-owned YouTube video and its last synchronization state.
- **Duplicate candidate:** an explainable comparison between records with an evidence tier and an operator decision; never an automatic deletion instruction.
- **Deletion request:** immutable selected video IDs, confirmation context, execution results, and audit receipt.

## Lifecycle baseline

`draft → reviewed → queued → uploading → uploaded | failed | cancelled`

On launch, any `uploading` item is reconciled before it can send another byte: verify the managed local asset, refresh authorization if necessary, query the persisted provider session, then continue only from the provider-confirmed range. A completed response records its returned video ID; an expired session or ambiguous completion is marked **needs reconciliation**, never silently retried as a new upload.

Publishing visibility (private, unlisted, public, scheduled) should be explicit metadata, independently validated before execution, and included in the audit trail.

## Decisions pending

- Single-operator versus tenant-aware data model.
- Local database encryption-at-rest approach and backup/export semantics.
- Platform-specific OAuth client registrations and redirect handlers.
- Code signing, mobile-store distribution, desktop update, and release pipeline.
