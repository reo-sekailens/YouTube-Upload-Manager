# TASK164 — Synchronized duplicate comparison and remote streaming

**Status:** completed locally; live-provider and installed-runtime certification pending

**Owner:** Codex

**Date:** 2026-10-18, Asia/Singapore

**Dependencies:** TASK143, TASK163

## Requested outcome and decision

Review upload/preflight and dedupe evidence with timestamp-linked side-by-side
playback. Prefer a native yt-dlp stream when configured and playable; retain the
existing YouTube embed as fallback.

The explicit user request authorizes optional extraction as a scoped exception
to the repository's official-API-only guidance. Account, upload, inventory,
playlist and deletion operations keep their official API boundaries; AGENTS.md
is unchanged. No browser-cookie collection or OAuth handoff to yt-dlp occurs.
Research identified YouTube developer-policy restrictions on undocumented
access and audiovisual copying. Memory-only buffering is not a claimed policy
exemption or evidence of provider approval.

## Acceptance

- [x] One shared modal covers upload, preflight and dedupe, including local/local
  and online/online candidates.
- [x] Shared play/pause, timeline, five-second skips, signed alignment offset,
  independent mode and one audible side; bounded correction and buffering holds.
- [x] Choose tools → Review setup stays inside the comparison modal, with Back
  navigation and a final save. Select installed yt-dlp and optional Deno/Node.
- [x] Native duplicate evidence and current channel authorize each remote video.
  Opaque capabilities keep CDN URLs, headers and credentials out of the webview.
- [x] Bounded in-memory progressive MP4 ranges; no video download/cache file,
  automatic tool installation, ambient cookie access or app OAuth reuse.
- [x] Retry/fallback retain selected pair, time, alignment and audio; closing
  revokes pending extraction, acquired streams and late results.
- [x] Behavior tests, TypeScript, production build/checks and desktop/mobile
  rendered evidence pass locally, with screenshots and measured fixture drift.
- [ ] Successful live extraction/CDN reads and decoded playback in the installed
  WebView with accessible operator-approved media. Current public probes hit an
  unavailable historical fixture and a YouTube sign-in challenge.

## Implementation

### Shared playback

comparison-sync.ts adapts HTML5 originals, compatibility MediaSource playback and
the existing iframe using a left-master clock and offset-adjusted overlapping
duration. Positive offset means right time minus left time. Native play/pause
and scrubbing propagate, including Play followed by a buffering transition.
Coordinator commands are fenced so delayed state reports do not look like new
operator actions. Closing fences late play promises and iframe messages.

The iframe requires real API readiness and validates origin plus frame identity.
Commands wait until navigation leaves about:blank. Paused restored seeks cannot
silently start a cued video. Metadata-only HTML5 preload is allowed to request
its first play, avoiding a readiness/buffering deadlock. Buffering holds the
linked pair and resumes only retained play intent; explicit pause cancels it.
Synchronization remains approximate, without a frame-accuracy promise.

The modal preserves review state across adapter replacement. Alignment/audio
settings collapse by default to keep videos closer on narrow screens. Individual
player controls and an independent mode remain available.

### Native streaming

media_preview/remote.rs reuses native duplicate evidence resolution. Four
main-window commands read configuration, save selected tools, prepare a stream
and revoke it. A preallocated UUID permits cancellation before acquisition;
bounded tombstones handle release-before-dispatch. Grants expire after
20 minutes and recheck channel, expiry and revocation on every read.

Only tool paths are persisted. Canonical executable names/locations are
validated and their signatures/identities checked across extraction. Metadata
extraction disables ambient configs/plugins/cache, remote components, cookies
and default JavaScript runtimes. Only explicitly selected Deno or Node is
enabled. Output is capped at 2 MiB, extraction at 45 seconds and simultaneous
extractors at two. Raw stderr/provider output is not exposed or logged.

Direct mode accepts one progressive HTTPS MP4 with AVC video and AAC audio.
Adaptive separate tracks, live, DRM, unavailable formats and access failures
return to the embed. Native HTTP uses a lazy pooled client with no ambient
proxy, strict HTTPS Googlevideo host/path and redirect checks, exact 206 and
Content-Range/length validation, and bounded body reads. Each response is at
most 1 MiB and no-store; per-grant mutexes serialize reads. Transient URLs,
allowed headers, metadata and media remain native memory.

Close kills extraction promptly and discards revoked read results. A blocking
network read can remain until its 15-second timeout, but cannot deliver bytes
after revocation. Failed/expired access offers explicit retry and embed fallback.
Mobile uses the existing embed. Playback changes no original, queue identity,
resumable checkpoint, duplicate decision or deletion authorization.

## Verification and evidence

Evidence directory:
C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/task164-stream-sync-20261018/

- Frontend controller tests cover intent, buffering, drift, offsets/overlap,
  pending seeks, native controls, late play, iframe origin/source/readiness,
  navigation and error handling. Bridge/session tests cover scoped requests
  and cancellation before or after acquisition.
- Native tests cover evidence ownership, channel/expiry/revocation, codec/URL
  restrictions, redirects, range/body limits, credential/header exclusion,
  explicit runtimes, preferences fallback and pending-release tombstones.
- Browser: 40 checks pass with actual generated H264/AAC pixels, actual React,
  fixture native transport and an explicitly simulated iframe API. Desktop
  1440×1280; mobile 390×844. Shared controls, alignment, one-side audio, hold/resume,
  independent controls, manual/automatic fallback, retries, match selection,
  setup steps/no early save and closing during extraction are exercised.
- A generated dual-HTML5 run measured maximum drift about 0.0013 seconds over
  twelve short samples. This is a fixture observation, not an installed WebView,
  live provider, codec-wide or frame-accuracy guarantee. Final browser checks
  report no app console errors or warnings.
- Screenshots: comparison-desktop.png, comparison-mobile.png,
  streaming-setup-desktop.png and fallback-desktop.png. Scripts and sanitized
  receipts: flow.log, recovery.log, provider-diagnostic.json.
- Final integrated suite: 553 frontend tests and 438 native tests pass (nine
  native ignores), with strict TypeScript, production native check, formatting,
  Tailwind ownership and whitespace checks passing.
- Initial budgets remain 223.25 KiB raw/70.00 KiB gzip JavaScript and 39.40 KiB
  CSS. Production output uses an isolated artifact directory and native checks
  use the existing resource-override harness. Unavailable F: build/sidecar
  junctions are preserved; these checks are not installer certification.

## Live-provider boundary and follow-ups

The installed yt-dlp 2026.03.17 probe against historical test video BaW_jenozKc
reported Video unavailable. A separate official 2026.08.19 executable was
verified against published SHA256 and kept in the test artifact directory only.
With selected Node 24.14.0, YouTube's documented IFrame example M7lc1UVf-VE
required sign-in. No user cookies, credentials or personal app profile were read.
No successful live extraction, signed-CDN transfer or packaged playback is claimed.

The ignored native smoke accepts YUM_TEST_YTDLP_PATH, optional
YUM_TEST_JS_RUNTIME_PATH and a selected eleven-character YUM_TEST_VIDEO_ID.
After successful extraction it probes one bounded CDN byte without printing
media, signed URLs, headers or raw diagnostics. It stays outside offline tests.
Authentication-dependent videos retain the embed, which itself depends on
provider access and embedding permission.

Before certifying shipped runtime support, test accessible authorized media with
maintained selected tools in the installed app, measure drift across seeks and
buffering, and confirm memory/cancellation behavior on each desktop WebView.
Broader adaptive formats and separate cookie authorization remain follow-ups.

## Primary sources checked 2026-10-18

- [yt-dlp README: URL output, stdout streaming and format selection](https://github.com/yt-dlp/yt-dlp#usage-and-options)
- [yt-dlp EJS and explicit runtime requirements](https://github.com/yt-dlp/yt-dlp/wiki/EJS)
- [yt-dlp PO token requirements](https://github.com/yt-dlp/yt-dlp/wiki/PO-Token-Guide)
- [YouTube IFrame Player API: controls, readiness and keyframe seeking](https://developers.google.com/youtube/iframe_api_reference)
- [YouTube API developer policies: III.D.7 and III.E.1](https://developers.google.com/youtube/terms/developer-policies)
- [Official yt-dlp 2026.08.19 release](https://github.com/yt-dlp/yt-dlp/releases/tag/2026.08.19)
