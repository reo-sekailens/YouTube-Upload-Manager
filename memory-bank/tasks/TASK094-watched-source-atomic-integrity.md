# TASK094 — Watched-source atomic integrity

## Status

completed

## Objective

Bind reference-in-place watched uploads to verified bytes so a mutable watched
folder path cannot be replaced after its stability check and before upload.

## Acceptance criteria

- Watched-source upload opens with no-follow semantics and validates stable
  file identity at the final boundary.
- The verified input remains the source streamed to YouTube, or an equivalent
  verified managed copy is used.
- Replacement and link-swap regressions prove provider dispatch is withheld.

## Evidence

- A stable watched source is opened with no-follow semantics and snapshotted
  from that handle into the managed local workspace. The upload reads the
  completed BLAKE3-verified snapshot rather than a mutable watched pathname.
- Windows regressions prove a link swap is rejected before opening and a later
  source replacement cannot alter the managed bytes queued for upload.
- Exact completed local duplicates are withheld during snapshot intake without
  creating a provider dispatch record.
