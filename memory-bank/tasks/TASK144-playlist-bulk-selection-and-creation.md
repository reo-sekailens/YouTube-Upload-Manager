# TASK144 — Playlist selection, additions and creation

## Status

`completed` for local source, native mocks and browser fixture scope

## Objective

Improve the in-app workflow for selecting many videos, adding them to a playlist,
and creating a playlist. The operator requested all three shortcuts: filtered
matching selection, current-page selection, and Shift range selection.

## Acceptance criteria

- Search/filter, select page, select matching and Shift range selection preserve
  explicit selections across pages/search; the 200-video limit is visible.
- Show selected/hidden counts and review destination/count before addition.
- Native additions preserve order, skip existing membership, report actual
  progress and partial outcomes, and retain unresolved items for safe retry.
- Creation uses a focused accessible dialog with name validation, clear privacy
  choices, inline feedback, and automatic destination selection on success.
  The user's preference is stepped dialogs: Name → Visibility → Review/create,
  with preserved Back navigation and final-step submission only.
- Playlist creation uses the existing separately protected management consent
  and temporary mode. Upload/read credentials do not have playlist-write scope;
  ordinary upload consent remains unchanged. The review step exposes permission
  status, explicit authorization/refresh and management-mode entry.
- Shared creation works in Playlists and upload intake; channel scope, existing
  upload choices and temporary playlist-management permission remain enforced.
- Focused tests, build/budget checks, and desktop/mobile visual interactions
  pass; screenshots are delivered. No real account writes are part of QA.

## Evidence

- `npm run test`: 29 files, 167 tests passed, including selection/receipt
  partition checks, scoped addition progress, native command argument binding,
  creation privacy/validation/permission and staged-provider boundaries.
- `cargo test --manifest-path src-tauri/Cargo.toml --offline --locked
  --features performance-harness --lib playlist_bulk::tests -- --test-threads=1`
  passed 15 focused cases. The same command without the filter passed 176 tests,
  zero failed, and five intentionally ignored release benchmarks. Native check
  and formatting passed with seven pre-existing warnings and none added.
- Native mocks prove normal permitted writes, review-order preservation,
  skip-existing, partial/ambiguous write recovery, remaining-only retry,
  ownership/live selected-channel checks, expiration and credential-kind
  enforcement. Creation uses management authorization before token access;
  [YouTube's playlist insert requirements](https://developers.google.com/youtube/v3/docs/playlists/insert)
  do not permit ordinary upload/read scope.
- Strict TypeScript, production build with a task-local external output folder,
  Tailwind lazy-style guard and Git whitespace checks pass. Existing startup
  bundle budgets are verified separately against the final output.
- Playwright CLI/Chrome at `http://127.0.0.1:1420/playlist-qa.html`, 1440x1000
  and 390x844, verifies page identity/content, no overlays/console errors,
  page/matching selection, Shift-click across pages, Shift-Space, retained hidden
  selections, the 200 cap and partial retry that resends only unresolved IDs.
- Browser checks also verify Name → Visibility → Review, private defaults,
  Back preservation, final-step-only writes, pending double-submit guard,
  confirmed failure retry, uncertain outcome locking, explicit permission/mode
  handling and parent permission handoff. Intake preserves all upload choices,
  selects the new playlist, and isolates its parent during nested creation.
  Channel changes suppress late results. Mobile/dark layouts and primary action
  contrast were inspected; screenshots/scripts/receipts are outside the checkout.
- Browser plugin was unavailable, so Playwright CLI supplied rendered QA. Native
  checks use the existing isolated Windows toolchain/Cargo directories, offline
  locked dependencies and process-only Tauri overrides excluding unavailable
  package sidecars/resources. No dependency, schema, lockfile or operator profile
  was changed; earlier TASK143 work remains intact.

## Remaining certification

- Live Google consent and YouTube playlist creation/additions are unverified;
  provider mocks and browser bridge fixtures do not establish real account writes.
- Packaged Windows launch, non-Windows/mobile native behavior and signing are
  unverified by this task.
