# TASK093 — Link-safe local source deletion

## Status

in-progress

## Objective

Prevent duplicate-review and post-upload cleanup from following a local
symlink or Windows reparse point to delete a file other than the operator's
reviewed directory entry.

## Acceptance criteria

- Deletion targets reject symlinks and reparse points before review and at the
  final destructive boundary.
- A reviewed link can never cause its resolved target to be staged or removed.
- Regression coverage proves a link target is retained.

## Follow-up

- Use platform file identity where needed to bind the reviewed directory entry
  through staging and deletion.
