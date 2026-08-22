# TASK091 — Race-safe local source deletion

## Status

completed

## Objective

Make reviewed local-source cleanup and duplicate deletion bind to the validated
file object rather than a replaceable pathname.

## Acceptance criteria

- Final destructive deletion includes identity-safe revalidation.
- Managed copies remain non-deletable through external-source flows.
- Replacement race regression coverage passes.

## Evidence

- External original-source cleanup moves the reviewed file to a unique sibling
  staging name before its final digest check and destructive removal. A
  replacement at the original path is therefore never deleted.
- Duplicate-review deletion persists the reviewed size/modified signature,
  rechecks it before and after the same staging move, and leaves changed or
  replacement files untouched. Managed workspace paths remain rejected.
- Focused Rust regressions cover changed/replaced duplicate-review sources and
  unchanged versus changed post-upload source cleanup.
