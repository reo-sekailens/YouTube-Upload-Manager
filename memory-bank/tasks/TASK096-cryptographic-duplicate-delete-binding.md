# TASK096 — Cryptographic duplicate-delete binding

## Status

completed

## Objective

Bind a pre-ingest duplicate-delete review token to its reviewed bytes, not just
its size and modification timestamp.

## Acceptance criteria

- Final local duplicate deletion verifies a cryptographic digest from the
  staged file against the review token.
- A same-size, timestamp-preserving replacement cannot be deleted.
- Existing link/reparse and managed-workspace guards remain effective.

## Evidence

- The review token now stores a BLAKE3 digest and final deletion compares it
  against the staged file. The focused regression simulates a replacement with
  an attacker-forged matching size/modification signature and proves it is
  retained.
