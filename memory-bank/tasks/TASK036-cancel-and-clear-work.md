# TASK036 — Cancel and clear persisted work

## Status

completed

## Acceptance criteria

- Operators can cancel active pre-ingest dedupe jobs without deleting their files.
- Operators can clear locally queued upload work without deleting managed media or falsely changing confirmed remote uploads.
- Operators can clear pending/recoverable deletion requests without touching YouTube videos.
- Every cancellation is persisted and audited; work cannot resume after cancellation.

## Evidence

- Native commands cancel active pre-ingest jobs, clear cancellable upload work while retaining managed media, and clear pending/recoverable local deletion requests without contacting YouTube.
- Pre-ingest workers re-check persisted job state before each source and before inventory synchronization, so cancellation cannot restart at launch.
- Type check, 28 web tests, production build, cargo check, focused Rust preflight tests, and diff check passed.
