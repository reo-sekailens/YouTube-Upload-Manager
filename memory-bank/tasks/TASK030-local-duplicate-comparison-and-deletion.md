# TASK030 — Local duplicate comparison and deletion

## Status

`completed`

## Outcome

Desktop pre-ingest duplicate results now provide a local-versus-saved-file and
local-versus-YouTube-title comparison card. An operator can permanently delete
only the just-dropped desktop source when it has an exact SHA-256 match.

## Safety boundary

- A short-lived opaque native token identifies the reviewed source; its path
  never enters the webview.
- The exact filename must be typed before deletion.
- Rust re-hashes the source immediately before deleting it and rejects changed,
  missing, expired, or app-managed media paths.
- A local delete does not alter the saved managed copy or any YouTube video.

## Evidence

- The focused Rust test proves a wrong confirmation does not delete the source,
  an exact confirmation does, and managed media cannot become a target.
- Type check, web tests, production build, and diff check passed.
