# TASK113: No-copy reference intake and legacy media cleanup

## Status

completed

## Objective

Eliminate newly created app-managed video copies while retaining safe resumable
uploads from the original operator-selected source, and reclaim residual legacy
managed-media storage on the next installed launch without breaking live work.

## Scope

- Manual picker, drag/drop, and watched-folder intake persist a reference to
  the original source path with stable signature/digest evidence; they do not
  create `.media` or `.partial` files.
- There is no full-copy, hard-link, reflink, or clone fallback. An unavailable
  or changed original source fails safely or enters reconciliation.
- Startup classifies queue safety before any destructive legacy cleanup. It
  migrates each owned legacy record to its surviving original source, or marks
  an unavailable-source item failed before removing its `.media` and `.partial`.

## Acceptance criteria

- New manual and watched intake records upload directly from their original
  source reference and leave no app-managed media copy or partial file.
- Retries and recovery verify the source reference before resuming from the
  provider-confirmed range; moved, missing, or changed source bytes cannot be
  silently substituted.
- Startup cleanup occurs only after database-only queue classification. Live
  legacy work continues from a surviving original source; unavailable sources
  are made non-dispatchable before their managed bytes are removed.
- Legacy UUID-named `.media`/`.partial` files, including orphans, are removed
  on the next installed launch.
- Narrow native tests cover watched no-copy intake, source-integrity failure,
  and safe legacy reference migration/cleanup.

## Evidence

Coordinator validation passed:

- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 125 passed, 0 failed, 5 ignored.
- `git diff --check` passed.
- `npm run tauri -- build --bundles nsis` produced an unsigned Windows x64
  NSIS installer: `YouTube Upload Manager_0.1.9_x64-setup.exe` (26,668,208
  bytes; SHA-256 `A5D3A1CA2E9025F81D3F22342D555BE85EC21885AB9D610B0BEC2E72ADD2D116`).

This is local native and repository-hygiene evidence. It does not certify an
installed-launch migration or a live YouTube provider upload.

## Follow-up

Record any installed-launch cleanup observation separately from the local test
suite, and do not conflate either with a live YouTube upload canary.
