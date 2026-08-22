# TASK019 — Product rename and trademark notice

## Status

`completed`

## Outcome

The product is named **YouTube Upload Manager**. The app, README, and public
documents state that it is independent of Google and YouTube, with trademarks
belonging to their respective owners. The linked GitHub repository is
`Satoshiii-DCS/YouTube-Upload-Manager`, and its local checkout is named
`YouTube-Upload-Manager` once Codex has released the active workspace process.

## Boundaries

- The existing Tauri identifier and secure-store service name remain unchanged
  so installed-device data and OAuth credentials are not orphaned.
- The remote is renamed and local `origin` uses its new HTTPS URL. GitHub CLI
  authentication remains invalid, but the rename was completed through the
  authenticated browser session.

## Evidence

- `npm run check`, `npm run test` (16 tests), `npm run build`, `cargo fmt
  --check`, `cargo test` (14 tests), and `git diff --check` passed locally.
- Browser preview confirms the new page title, UI heading, and visible notice
  with no console warnings or errors.
