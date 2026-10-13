# TASK145 — OAuth JSON replacement and distinct upload limits

**Status:** completed

**Dependencies:** TASK020, TASK024, TASK087, TASK142

## Objective

Provide a manual stepped OAuth JSON replacement for the connected account when
uploads encounter a limit. Give Google Cloud API quota precedence in diagnosis,
while preserving the separate channel upload limit and the existing queue.

## Acceptance criteria

- Choose JSON → Review → Reconnect stages the candidate natively, with no JSON
  contents, tokens, or client secrets exposed to the webview or documentation.
- The existing connection remains intact until Google verifies the same channel.
  Cancellation, wrong-channel consent, and failed authorization preserve it.
- Queue identities, original files, and encrypted resumable checkpoints remain
  intact. Native admission fences prevent credential replacement during active
  uploads or conflicting consent/mutations.
- Exact structured provider reasons distinguish Cloud API quota from channel
  upload limits, including documented HTTP 400 uploadLimitExceeded; Cloud quota
  wins if both reasons appear. Unrelated/malformed responses do not create a pause.
- Replacement never claims to reset provider quota. A channel pause survives;
  only an established different Cloud project uses a different API quota scope.
  Same/unknown project identity carries the active API pause to its new scope.
- Focused native/frontend checks and rendered stepped-flow screenshots are
  recorded. Live Google consent, uploads, and packaged playback remain separate.

## Evidence

- Existing TASK143/144 changes are preserved.
- 182 frontend tests passed; the 12 focused modal tests pass after final wording.
- 198 integrated native tests passed, zero failed, five release benchmarks ignored.
  Includes 22 new cases: 13 replacement/pause tests, eight reason-classification
  tests and a version-4 migration preservation test. Scope/pointer rollback,
  cancellation/stale callback fencing, empty optional secret, expiry, mixed quota
  persistence, same/unknown project retention and unchanged checkpoints covered.
- Strict TypeScript, native offline/locked check, Rust formatting, production
  build, Tailwind ownership, whitespace and startup budgets passed.
- Playwright CLI/Chrome controlled bridge checks verify the real stepped dialog
  at 1440×1000/390×844 in light/dark, including permitted completion and all
  meaningful cancel/retry/validation/focus/late-account cases. The browser plugin
  was unavailable. Screenshots, scripts, native source hashes and logs are in
  the external `oauth-replacement-20261013` evidence folder; no provider writes.

## Compatibility and follow-ups

- Schema 5 migration adds nullable credential/project metadata and API pauses in
  one transaction. Existing source and checkpoint evidence remains unchanged.
  Older apps reject a migrated database; deployed rollback requires restoring
  the pre-migration database backup. No live profile was migrated for these tests.
- Google browser consent, actual protected-store replacement, YouTube quota
  behavior/uploads and packaged/non-Windows flows remain unverified gates.

## Provider boundary

YouTube documents the daily upload cap as channel-wide across apps and the API.
Cloud API quota is separate. Credential replacement is an explicit account
configuration action, with no automatic cycling or quota-reset promise.
