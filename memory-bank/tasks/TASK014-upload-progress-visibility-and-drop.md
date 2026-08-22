# TASK014 — Upload progress, visibility, and drag-and-drop intake

## Status

completed

## Outcome

Operators can drop one or more supported videos into the local workspace, choose a private (default), unlisted, or public visibility before queueing each item, and see truthful current-item and remaining-batch progress and ETA while uploads run.

## Acceptance criteria

- Current upload has byte progress, elapsed transfer estimate, and an ETA derived only from measured acknowledged upload progress.
- Remaining batch has aggregate byte progress and ETA based on the current measured upload rate; no ETA is claimed before a rate is known.
- Visibility is explicit per upload item, defaults to private, persists locally, is sent to YouTube when the resumable session is created, and is recorded in the local audit log.
- Drag-and-drop accepts supported video files, imports them into the managed device-local workspace, and never bypasses import validation.
- Browser-side and native tests cover the changed contracts; frontend build and focused Rust checks pass.

## Boundaries

- No application backend, cloud storage, telemetry, or webview access to OAuth credentials.
- Watched-folder automation remains private-only and does not inherit manual visibility choices.
- Live YouTube upload verification remains separately gated by operator-owned credentials and a safe channel.

## Evidence

- Per-item visibility now defaults to `private`, is persisted locally and audited, and is sent as YouTube `privacyStatus` when a resumable session is created. Watched-folder dispatch explicitly remains private.
- Native upload acknowledgements persist a measured transfer rate and attempt start time. The UI refreshes active uploads once per second and shows current-item and aggregate remaining progress/ETA only once a rate is known.
- The native window drag-drop listener imports supported paths through the same managed-local import command as the multi-file picker; native input validation also rejects unsupported extensions.
- Verified locally: `npm run build`; `npm test -- --run` (12 tests); `cargo fmt --check`; `cargo test` (14 tests); and `git diff --check`.
- Browser preview at `http://localhost:1420/` rendered the queue intake surface with no console warnings/errors and confirmed the choose-videos fallback message. Browser preview cannot exercise native file drag-drop, secure local import, a real provider upload, or a live ETA.
