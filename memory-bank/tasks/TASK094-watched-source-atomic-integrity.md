# TASK094 — Watched-source atomic integrity

## Status

in-progress

## Objective

Bind reference-in-place watched uploads to verified bytes so a mutable watched
folder path cannot be replaced after its stability check and before upload.

## Acceptance criteria

- Watched-source upload opens with no-follow semantics and validates stable
  file identity at the final boundary.
- The verified input remains the source streamed to YouTube, or an equivalent
  verified managed copy is used.
- Replacement and link-swap regressions prove provider dispatch is withheld.

## Follow-up

- Preserve resumable-upload recovery and the device-local reference-in-place
  workflow while introducing the identity-safe handoff.
