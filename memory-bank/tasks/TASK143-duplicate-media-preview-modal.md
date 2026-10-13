# TASK143 — In-app duplicate video preview

## Status

`completed` for local source and browser scope

## Objective

Let the operator compare suspected local duplicate files and the affected
YouTube videos inside a modal before deciding whether to upload or skip.

## Boundaries

- Resolve duplicate evidence from native-owned records in the current channel.
- Stream original files through revocable opaque tokens; no media copies,
  filesystem paths in the webview, uploads, or deletion during preview.
- YouTube playback uses its official embedded player and existing session.
- Missing/changed originals, unsupported codecs, and YouTube playback limits
  must remain visible and distinct from duplicate decisions.

## Acceptance criteria

- Upload title holds, pre-ingest results, and duplicate candidates offer
  Compare videos with a local player and matched YouTube embed where applicable.
- The modal supports multiple sources/matches, keyboard dismissal, focus
  containment/restoration, small screens, and dark mode.
- Media is acquired only when the modal opens; close and stale results revoke
  native grants and unload players.
- Native range/scope/revocation checks, frontend tests/type/build checks, and
  rendered player interactions pass; screenshots show the new UI.

## Evidence

- `npm run test`: 25 files, 134 tests passed. Modal tests cover native video and
  YouTube frame markup, local/online-only pairs, unavailable-source states, and
  active/stale acquisition cleanup. Bridge checks verify opaque target IDs,
  reviewed channel binding, token-only URLs, and revocation.
- `npm run build -- --outDir <task-local external folder>` passed TypeScript and
  production bundling; default `dist` points to unavailable storage. Tailwind
  guard and unchanged frontend bundle budgets passed (223.38 KiB initial JS raw,
  69.85 KiB JS gzip, 38.93 KiB CSS raw). Preview code/styles are lazy-loaded.
- Native checks used the existing isolated Windows toolchain/Cargo directories,
  offline locked dependencies, and a process-only Tauri config excluding
  unavailable package sidecars/resources. No configuration files or lockfiles
  were changed to work around this workstation boundary.
- `cargo test --manifest-path src-tauri/Cargo.toml --offline --locked
  --features performance-harness --lib media_preview::tests -- --test-threads=1`
  passed 7 tests. The same command without the filter passed 161 tests, zero
  failed, and 5 intentionally ignored release benchmarks. Ordinary `cargo
  check`, Rust formatting and Git whitespace checks passed. Seven pre-existing
  dead-code warnings remain; this change adds none.
- Native fixtures verify authoritative channel/duplicate evidence, original
  GET/HEAD/suffix/invalid/bounded ranges, missing/changed/replaced files,
  expiry/revocation, no copies/path disclosure, and unavailable URI sources.
  A Windows case-insensitive fixture collision was corrected. The existing
  requeue test now holds the source reader during admission assertions and
  accepts pending/running hashing, avoiding its background-worker timing race;
  production requeue behavior was unchanged and the exact test passed.
- Playwright CLI/Chrome at `http://127.0.0.1:1420/duplicate-preview-qa.html`,
  1440x1000 and 390x844: meaningful page/title, no framework overlay or app
  console errors, all three modal entry points, actual fixture video
  decoding/play/seek, local/YouTube selectors, Escape/focus restore, player
  removal/token release, failed preparation/retry, narrow player containment,
  and light/dark screenshots passed. The real public YouTube sample player
  loaded. Only a YouTube/Chrome graphics warning remained.
- Browser plugin was unavailable, so Playwright CLI supplied rendered QA.
  The fixture mocks the native bridge; it does not certify packaged native
  protocol playback. Screenshots, scripts and receipts are outside the checkout.

## Remaining certification

- Owner-private YouTube playback and signed-in embed behavior are unverified.
- Packaged WebView streaming, supported local codecs and non-Windows runtime
  behavior require platform evidence. Non-path mobile picker URIs report local
  playback unavailable; no fallback copies are made.
- Read-only preview validates size and available saved modification evidence;
  it never rehashes whole videos or authorizes duplicate deletion.
