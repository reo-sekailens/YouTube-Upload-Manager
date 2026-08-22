# TASK090 — Watched-source final integrity gate

## Status

completed

## Objective

Prevent changed watched-folder content from being automatically uploaded after
its initial stability check.

## Acceptance criteria

- Dispatch revalidates the persisted source identity/signature immediately.
- A changed source is withheld or failed before provider transfer.
- Restart and background hashing preserve the same safety invariant.

## Evidence

- Watched-folder dispatch re-reads the persisted size and modified-time
  signature before requesting an access token or creating a provider session.
- A mismatch cancels the item, records a safe local audit event, and leaves the
  source untouched. Existing restart/background verification continues to
  reject a source that changes while its deep hash is running.
- Focused Rust regression verifies a stale watched-source signature never
  reaches provider dispatch.
