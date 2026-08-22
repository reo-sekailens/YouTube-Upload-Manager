# TASK093 — Link-safe local source deletion

## Status

completed

## Objective

Prevent duplicate-review and post-upload cleanup from following a local
symlink or Windows reparse point to delete a file other than the operator's
reviewed directory entry.

## Acceptance criteria

- Deletion targets reject symlinks and reparse points before review and at the
  final destructive boundary.
- A reviewed link can never cause its resolved target to be staged or removed.
- Regression coverage proves a link target is retained.

## Evidence

- Every destructive local path now rejects symlinks and Windows reparse points
  before canonicalization, staging, or deletion. Post-upload cleanup records a
  retained outcome instead of following such a path.
- The Windows regression `confirmed_source_cleanup_retains_a_linked_source_and_its_target`
  proves the reviewed link and its target remain present after cleanup.
