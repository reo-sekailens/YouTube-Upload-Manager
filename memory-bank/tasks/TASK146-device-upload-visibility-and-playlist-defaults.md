# TASK146 — Device upload visibility and playlist defaults

**Status:** completed

**Dependencies:** TASK017, TASK144, TASK145

## Objective

Extend the existing device-wide Made for Kids default with visibility and a
playlist default. Prefill every manual upload review while keeping the choices
visible and editable before import.

## Acceptance criteria

- The existing panel offers audience, Private/Unlisted/Public visibility, and
  No playlist or an owned playlist. Persist preferences locally, without writes
  to YouTube, and preserve old Made for Kids settings.
- Audience/visibility are device-wide; a playlist follows the connected channel
  on that device. Channel switches never show or apply another channel's default.
- Native partial saves validate visibility, playlist ownership and final channel
  before one transaction. Failed saves preserve all preferences.
- Upload review uses valid saved defaults and retains explicit per-batch changes
  when asynchronous reads finish or playlists refresh. Missing/unavailable
  playlists are explained before import, with no hidden application.
- Existing queued uploads and saved Folder Monitor configurations stay intact.
- Focused persistence/UI checks, normal permitted review, pending/failure/channel
  races, responsive light/dark screenshots and build budgets are recorded.

## Evidence

- Existing TASK143–145 changes are preserved.
- 194 frontend tests passed, including seven panel tests, three intake tests and
  the partial/scoped command regressions. TypeScript and Tailwind ownership pass.
- 206 integrated native tests passed (zero failed, five ignored release benchmarks).
  Seven focused preference tests and one schema migration test cover defaults,
  partial updates, wrong ownership/channel, transaction rollback, scoped clear,
  device reopen and unchanged existing preference/OAuth/queue/checkpoint data.
  Native offline/locked check and formatting passed; source hashes match receipt.
- Production build and existing startup budgets pass after moving preference
  command helpers to the lazy manual-upload surface. Thresholds are unchanged.
- Playwright CLI/Chrome fixture checks pass at 1440×1000 and 390×844 in light/
  dark: save/reload; permitted prefilled review; overrides/late reads; failure/
  pending guards; channel restore/clear; missing playlist; retry and stale response.
  No console errors/warnings. The browser plugin was unavailable. Screenshot and
  native/browser receipts are external in `upload-defaults-20261013`.

## Compatibility and remaining gates

- Schema 6 migration is transactional and retains all prior device data. Older
  binaries reject a migrated database; rollback requires its pre-migration backup.
  No personal profile was migrated during these checks.
- Live owned-playlist reads, packaged native persistence, signed release and
  non-Windows/mobile runtime are separate unverified certification gates.
