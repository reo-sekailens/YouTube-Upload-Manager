# TASK028 — Instant video-title search

## Status

completed

## Acceptance criteria

- Every list that renders videos has a title search control.
- Filtering is case-insensitive, local, immediate, and does not call YouTube.
- Empty searches and no-match states are clear and accessible.

## Evidence

- Queue, duplicate-candidate, locally saved YouTube inventory, and pending deletion-request lists filter immediately as the operator types.
- `npx tsc --noEmit` and `npm run test -- --run` passed (21 tests).
