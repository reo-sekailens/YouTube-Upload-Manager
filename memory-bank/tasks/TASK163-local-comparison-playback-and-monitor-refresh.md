# TASK163 — Local comparison playback and stable folder-monitor refresh

**Status:** completed (local implementation and verification)

**Owner:** Codex

**Date:** 2026-10-18, Asia/Singapore

**Dependencies:** TASK143, TASK147, TASK160

## Request and observed failures

The local comparison player shows a duration and timeline but no picture while
the matched YouTube player works. A metadata event currently dismisses loading
without proving a video frame exists; unsupported video tracks can therefore
leave a silent black player. The exact operator file/codec has not been inspected.

Folder-monitor events restart a loading effect, disable folder controls and
unmount the selected details. Frequent upload events can repeatedly collapse
the view, reset focus/scroll and prevent interaction.

## Acceptance

- [x] Detect local playback without video frames and offer an in-app
  compatibility player, including explicit loading/failure/retry states.
- [x] Compatibility playback decodes short bounded segments from the original
  through the pinned bundled decoder, in memory only; no original copy,
  transcoded disk file, arbitrary locator or external player is introduced.
- [x] Token/channel/source identity and expiry remain authoritative; close or
  source/channel changes revoke reads and cancel decoder work. Preserve audio,
  native video controls, seeking and bounded buffering where supported.
- [x] Background folder updates retain the mounted details, selected folder,
  scroll, focus and open dialogs; event storms coalesce without stale channel
  responses or refresh starvation. Initial/folder-switch loading remains clear.
- [x] Focused native/frontend tests, generated real video playback, production
  build/budgets, and desktop/mobile rendered interactions pass. Include visible
  screenshots and distinguish browser fixtures from the installed operator app.

## Ownership

- Native packet: `media_preview.rs`, its new compatibility module and the
  minimal bundled-decoder resolver visibility change.
- Playback UI packet: duplicate preview component/bridge and a scoped MSE
  controller/component/tests.
- Monitor packet: folder monitor panel and its scoped refresh queue/tests.
- Root: command registration, narrow media Blob CSP allowance, synthetic media,
  integration/rendered verification and documentation. Shared manifests and
  lockfiles are not worker-owned.

## Implementation and evidence

Evidence root:
`C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/task163-playback-monitor-20261018/`.

- A new picture-health observer detects metadata with no video dimensions and
  active playback with no decoded frames. Operators can also select
  **Use compatibility player**. Retry and original-player recovery stay inside
  the same comparison; opening/retrying never grants autoplay permission.
- Native raw IPC returns bounded six-second H264/AAC fragments from the
  original. Commands accept scoped tokens, never source paths. Decoder identity
  is pinned/verified and cached per grant, then identity/signature rechecked.
  Reads and process admission remain bounded; manual preview shares the
  two-process cap while ordinary upload/background policies remain intact.
- The MediaSource controller serializes work, discards stale seek results,
  evicts outside a bounded window and revokes Blob URLs during teardown. Real
  fragment QA caught AAC padding extending beyond the original duration;
  clipping the append window fixes the actual Chromium duration exception.
  This has both a regression test and browser no-error assertions.
- Monitor refreshes use a 250 ms first-event queue with one active read and one
  trailing refresh. Background updates never enter the initial loading state;
  stable rows and retained review/log wrappers preserve interaction state.
- Frontend: **517/517 tests**, TypeScript, production Vite build and unchanged
  initial budgets pass (223.25/70.00 KiB raw/gzip JavaScript, 39.40 KiB CSS).
  Tailwind ownership, worker configuration and whitespace checks pass.
- Native: **423 pass / 8 ignored**, locked
  production Cargo check and formatting pass. The separately opted-in real
  decoder case passes against an 18-second generated HEVC/AAC source, verifies
  three actual fragments including full video/audio decoding, unchanged source
  hash/signature and an empty app media directory. Synthetic fragment exports
  are test-only fixtures; runtime playback writes no segments.
- Non-ISO duration lookup uses only explicit bundled FFprobe, a 64 KiB output
  cap and a 20-second admission/child deadline. It shares foreground process
  and reader limits and inherits a held source descriptor on Unix. No PATH
  fallback or indefinite wait behind active uploads is introduced. Output and
  deadline fixtures pass; the real generated MP4 exercises ISO duration lookup.
- Browser: Chrome/Playwright with actual React components and fixture IPC,
  at **1440×1000** and **390×844**. **20 playback checks** pass for actual
  decoded pixels, segment transitions, forward/backward seek, duration,
  pause/no-autoplay, error-free status, teardown and responsive layout. **Six
  recovery checks** pass for simulated missing-picture metadata, failed decode,
  retry, no autoplay and Escape. Current runs have no app console errors or
  warnings. `playback-flow.log`, `playback-recovery.log`,
  `playback-desktop.png` and `playback-mobile.png` retain evidence.
- Monitor browser QA sends **410 invalidations at 10 ms intervals** with
  delayed reads; these coalesce into **five list/five overview reads, maximum
  one active read**. Detail DOM, selection, focus, partial typed confirmation,
  disclosure state, open setup dialog and scroll position (1241 → 1241) remain
  intact. Folder navigation, late folder/channel rejection, background-failure
  retry and mobile overflow checks pass. `monitor/results.json` and inspected
  desktop/mobile images contain receipts.

Browser plugin is unavailable, so rendered checks use the installed Playwright
CLI and isolated Chrome sessions. Local-network permission is granted only to
the disposable fixture origin so it can load generated media. The installed
application receives media bytes via native IPC; it adds no HTTP service.

## Verification boundary and handoff

The user's exact source and installed WebView were not inspected. Chrome could
decode the generated HEVC file directly, so the automatic missing-picture path
is exercised with a controlled dimension failure; the compatibility pipeline
itself decodes and displays real native-generated fragments. The matched
YouTube pane is an explicit fixture placeholder, not live-provider evidence.
Native checks use the existing resource-override harness, preserving the
unavailable F: sidecar junction. No full installer or packaged-runtime playback
is claimed. No personal video, credential, upload, folder configuration or
installed profile is modified. Hosted packaging and actual installed-app
playback require separate release evidence. Main handoff follows the existing
commit/push instruction for this upload-workflow release.

References: [MDN video dimensions](https://developer.mozilla.org/en-US/docs/Web/API/HTMLVideoElement/videoWidth),
[Tauri raw IPC](https://tauri.app/reference/javascript/api/namespacecore/).
