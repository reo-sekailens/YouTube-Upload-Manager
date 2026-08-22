# TASK021 — Native dropdown runtime detection

## Status

`completed`

## Outcome

Use Tauri's public runtime capability detector for interactive controls so
dropdowns are enabled in the signed native application and remain safely
disabled in browser preview mode.

## Evidence

- `npm run check`, `npm run test` (16 tests), `npm run build`, and
  `git diff --check` passed locally.
