# Architecture

## Current state

The repository is newly initialized; no application code has been written yet. The selected shape is a local-first Tauri 2 application using React + Vite for the interface and Rust for privileged local operations. It targets Windows, macOS, Linux, Android, and iOS from one codebase. All application-controlled state stays on the device; Google/YouTube is the only required external service. This document describes required boundaries, not implemented components.

The product is named **YouTube Upload Manager**. It is an independent project,
not an official Google or YouTube product. Google and YouTube are trademarks of
Google LLC; other marks belong to their respective owners.

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
- **Watched-folder authorization:** one operator-selected device-local directory, the channel scope approved to receive its new files, enabled state, scan receipts, and last result. Existing files form a baseline; only later additions or changed replacements can become candidates.
- **Folder observation:** channel-scoped path, size, modification key, processing state, optional SHA-256 digest, and canonical upload-item link used to make polling idempotent across restarts.

## Lifecycle baseline

`draft → reviewed → queued → uploading → uploaded | failed | cancelled`

On launch, any `uploading` item is reconciled before it can send another byte: verify the managed local asset, refresh authorization if necessary, query the persisted provider session, then continue only from the provider-confirmed range. A completed response records its returned video ID; an expired session or ambiguous completion is marked **needs reconciliation**, never silently retried as a new upload.

Queue recovery is a native startup responsibility and completes before the
webview session is created. Interrupted local imports resume or become a
repairable failed record, pre-network dispatch claims return to `queued`, and
interrupted uploads enter `needs_reconciliation` with their secure resumable
checkpoint preserved. The dashboard therefore exposes no manual recovery
control.

Publishing visibility (private, unlisted, public, scheduled) should be explicit metadata, independently validated before execution, and included in the audit trail.

Watched-folder automation is a deliberate recurring approval rather than an
unreviewed publishing path. The operator chooses private or unlisted visibility
when enabling the monitor; public visibility is unavailable. The local worker
scans direct child files only while the app runs, requires an unchanged
signature across consecutive scans, imports accepted media into managed storage,
withholds local-digest and last-synchronized-title matches for review, and
dispatches only the persisted private or unlisted resumable uploader. A
connection mismatch pauses before network use.

Completed manual intake batches are queued and dispatch automatically whenever
their approved channel is connected. If YouTube reports its daily upload limit,
the local job engine records a conservative device-local 24-hour dispatch pause,
keeps affected work queued, and resumes it after that pause while the app runs
or on the next launch. The pause stores no provider response body or credential.

Duplicate detection remains an explicit operator action. **Run dedupe** first
synchronizes the active channel's owner-authorized upload inventory, then
rebuilds local SHA-256 and uploaded-title candidates for review. It never
creates or executes a deletion request.

The product display name is **YouTube Upload Manager**. Its Tauri identifier
and OS credential-store service name deliberately retain the established
`youtube-mass-uploader` namespace so a product rename cannot orphan existing
local queues or OAuth credentials.

## Decisions pending

- Single-operator versus tenant-aware data model.
- Local database encryption-at-rest approach and backup/export semantics.
- Platform-specific OAuth client registrations and redirect handlers.
- Code signing, mobile-store distribution, desktop update, and release pipeline.
