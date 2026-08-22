# TASK044 — Remote video list alignment

## Status

Completed

## Objective

Correct the deletion review's remote-video rows so each title begins directly
after its selection control, retains available title width, and remains usable
on mobile.

## Evidence

- Replaced `justify-content: space-between` with a three-column grid: the
  checkbox keeps intrinsic width, metadata receives all remaining width, and
  the review action remains right-aligned. Long titles and metadata wrap
  safely.
- At 640 px and below, the row collapses to a single-column layout with an
  uncompressed checkbox and full-width action.
- `npm run check`, `npm test` (29 passed), `npm run build`, and `git diff
  --check` passed. Browser preview was checked at desktop and 390 × 844 mobile
  dimensions with no console errors; populated deletion rows require a signed
  app with saved channel inventory.
