# TASK165 — Reuse a signed-in browser for duplicate streaming

**Status:** completed (local implementation and fixture verification)
**Owner:** Codex
**Date:** 2026-10-18, Asia/Singapore
**Dependencies:** TASK164

## Request and boundary

Add an explicit, remembered browser/profile option for yt-dlp to review private
YouTube videos using the operator's existing browser login. Keep synchronized
comparison, retry and embed fallback. No repeated application login per pair.

This request authorizes an opt-in browser-session feature. Implementation and
fixture QA do not authorize silently reading an unspecified personal profile.
Testing uses synthetic preferences/CLI captures; live private playback is a
separate evidence gate. App OAuth tokens remain independent and protected.

Use yt-dlp's documented cookies-from-browser option only after a reviewed
selection. Save browser/profile configuration per immutable channel, never
cookie values. Do not export a cookie jar, copy cookies into app preferences,
log metadata/stderr, or send cookies through the webview. yt-dlp may create
temporary browser-database copies internally; do not claim this third-party
operation is entirely memory-only. Media playback still creates no video copy.

## Work and acceptance

- [x] T165-A: Native opt-in browser/profile settings, channel fencing, safe
  invocation, disable/revocation, backward compatibility and behavior tests.
  Own remote.rs and media_preview.rs; coordinator owns command registration.
- [x] T165-B: Stepped browser-session setup and remembered state in the existing
  comparison modal; browser/profile selection, review, disable, recovery copy.
  Own UI components and their tests; coordinator owns the bridge contract.
- [x] T165-C: Scoped bridge/command integration, production/type/native checks,
  desktop/mobile interaction screenshots, documentation and Git handoff.
  Depends on A and B.

Configured profile use must preserve provider access controls and the existing
native duplicate evidence authority. Private access remains subject to the
chosen browser session's permissions, cookie expiry/decryption and available
formats. Do not promise CAPTCHA-free access or verified browser-account identity.
Disabling/changing the session invalidates affected in-flight native streams.

## Primary sources checked

- [yt-dlp FAQ: browser session selection](https://github.com/yt-dlp/yt-dlp/wiki/FAQ#how-do-i-pass-cookies-to-yt-dlp)
- [YouTube extractor: account cookies and rotation](https://github.com/yt-dlp/yt-dlp/wiki/Extractors#exporting-youtube-cookies)
- [Native browser-cookie reader and temporary database behavior](https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/cookies.py)

## Evidence

- Added **Use browser session** beside streaming setup, then browser/profile
  selection and a separate access review inside the existing comparison modal.
  Approval starts unchecked. Back/cancel preserve the review boundary. A profile
  folder picker runs only after a click; Firefox requires an explicit profile.
  Enabled settings show the remembered choice and a disable action.
- Native preferences save only browser/profile metadata per immutable channel.
  Main-window commands fence the current channel, record a scoped audit receipt
  and revoke active/pending streams when the setting changes or is disabled.
  Snapshot admission and final process start share the mutation lock so an old
  approved snapshot cannot start after disable returns.
- Extraction retains `--no-cookies` (no exported jar), disables browser cookies
  by default and supplies one validated `--cookies-from-browser browser:profile`
  argument only for the approved channel. App OAuth remains independent. Cookie
  values, extractor diagnostics and signed URLs do not cross the webview bridge.
- Authenticated extraction isolates third-party temporary work under a unique
  private directory beside the native database. The child is killed/waited for
  before cleanup on ordinary completion, errors, timeout, cancellation or unwind.
  Cleanup validates the direct canonical parent, UUID name and link/reparse
  boundary. Forced process termination or power loss cannot run this cleanup;
  stale-directory recovery is a follow-up, not a verified guarantee.
- Complete suites: **562 frontend tests** across 72 files, **444 native tests**,
  with nine ignored native integration cases. Production frontend build/strict
  TypeScript and production native check pass. Native formatting, Tailwind UI
  guard and diff whitespace checks pass. Existing native compiler warnings are
  unchanged. No dependency, lockfile or release configuration changes.
- Initial bundle guard passes: **223.25 KiB raw JS, 69.99 KiB gzip JS,
  39.40 KiB raw CSS**. The session UI remains in the lazy comparison surface.
- **29 rendered checks** pass with generated H264 video and synthetic IPC:
  selection/review/approval, mobile overflow, exact channel save, remembered
  settings on another pair/reopen, disable, failed-save retry, late channel
  rejection, no autoplay and retained 7/9-second positions plus two-second
  alignment. No application console errors or warnings.
- Inspected desktop/mobile screenshots, generated-media fixture, command
  receipts, build/budget and native/frontend logs are under the thread artifact
  root's `task165-browser-session-20261018/` directory. Screenshots use a
  synthetic browser profile and simulated provider authentication.
- No personal browser profile, cookies, keyring or private video was accessed.
  Installed WebView/browser decryption and successful live private-video
  extraction/CDN playback remain separate acceptance gates. The selected account
  must already have video access; expiry, browser protection, challenges and
  unsupported progressive formats may still require retry or embed fallback.

## Follow-ups

- Certify an operator-selected signed-in profile and authorized private video in
  the installed app without capturing cookies, URLs or raw tool diagnostics.
- Add narrowly owned stale temporary-directory recovery after abrupt app exit;
  do not scan or delete arbitrary browser/application paths.
