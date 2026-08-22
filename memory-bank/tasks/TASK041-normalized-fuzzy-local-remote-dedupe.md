# TASK041 — Normalized local-to-YouTube title dedupe

## Status

Completed

## Objective

Apply conservative filename/title normalization to every local-to-YouTube
duplicate comparison. Treat separators such as underscores as whitespace and
flag review-only fuzzy candidates when their ordered capture-number sequences
are identical.

## Acceptance criteria

- `VID_20251219_204823_00_014.mp4` matches `VID 20251219 204823 00 014`.
- A partial remote title with the same ordered multi-part numeric sequence is
  flagged for review, not treated as an exact media match.
- Unrelated titles with short or different number sequences are not matched.
- Upload, watched-folder, pre-ingest, and remote inventory dedupe use the same
  comparison predicate.

## Evidence

- Focused Rust title tests passed (3/3), including underscore/extension normalization, matching capture sequence `20251219 204823 00 014`, partial-title matching, remote-inventory candidates, and negative controls for short/different sequences.
- `npm run check`, `npm test` (28/28), `npm run build`, and `git diff --check` passed.
- `cargo fmt --check` remains repository-wide dirty because pre-existing unformatted Rust code outside this task would be rewritten; this task did not reformat unrelated code.
