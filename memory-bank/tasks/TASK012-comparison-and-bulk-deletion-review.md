# TASK012 — Video comparison and multi-select deletion review

## Status

`completed`

## Objective

Improve connected-account readability, let an operator compare two duplicate
YouTube uploads in-app with synchronized playback controls, and streamline
creation of existing safe local deletion requests through a multi-select review
queue.

## Boundaries

- YouTube embeds are used only for owner-authorized inventory video IDs.
- Playback synchronization is an operator aid; it never changes provider data.
- Multi-select creates only local, per-video deletion requests after the
  existing typed-ID confirmation. It never bulk-deletes or bypasses fresh
  deletion authorization.
- All selections remain scoped to the active locally synchronized channel.

## Acceptance criteria

- Connected channel identity, status, explanatory copy, and actions are clearly
  separated on desktop and mobile.
- A duplicate candidate with two YouTube IDs offers two embedded players and
  one syncable play/pause and seek control.
- The removal review supports individual checkboxes, Select all, and a selected
  review queue while retaining per-video typed confirmation and execution
  safeguards.
- Type checks, tests, production build, and rendered desktop/mobile UI checks
  pass. Live provider playback/deletion remains unverified.

## Evidence

- `npm run check`, `npm run test` (12 tests), `npm run build`, and `git diff
  --check` passed locally.
- In-app Browser QA loaded the Vite preview at 127.0.0.1:1420 on desktop and
  found meaningful rendered application content with no console warnings or
  errors. The browser-preview data boundary cannot populate a connected channel,
  inventory, or duplicate candidates, so embedded-player and deletion-selection
  interactions were certified by source behavior and type checks only.

## Follow-ups

- Live embedded-player behavior and live deletion authorization require an
  operator-authorized test channel with deliberate duplicate inventory entries.
