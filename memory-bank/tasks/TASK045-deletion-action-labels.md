# TASK045 — Deletion action labels

## Status

Completed

## Objective

Use direct deletion language for single and bulk actions in the Video deletion
tab while retaining the existing local-request and typed-confirmation safety
workflow.

## Evidence

- `Review one` is now **Delete one** and `Review selected` is now **Delete
  selected**. Both still open the existing confirmation flow; only the final,
  separately labelled action performs a YouTube deletion.
- The single-video card action is now the shorter **Delete** label; its existing
  confirmation and authorization flow is unchanged.
- `npm run check`, `npm test` (29 passed), `npm run build`, and `git diff
  --check` passed. Desktop browser preview rendered the Video deletion tab with
  no framework overlay or console errors; populated actions require signed-app
  inventory data.
