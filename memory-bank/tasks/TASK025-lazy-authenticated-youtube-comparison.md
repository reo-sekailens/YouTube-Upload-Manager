# TASK025 — Lazy authenticated YouTube comparison

## Status

`completed`

## Outcome

Duplicate-comparison frames are not created until the operator presses the
shared Play control. The app also provides a separate, unprivileged YouTube
account window where the operator can sign in directly with YouTube before
starting a comparison.

## Boundaries

- The account window navigates directly to `https://www.youtube.com/`; the app
  never renders, receives, stores, or logs Google credentials or browser data.
- It has no Tauri capability because the existing capability applies only to
  the `main` window. The main window alone may create it.
- Comparisons use YouTube's standard embed origin so the WebView session may be
  available to the frames. Video-level embedding rules and third-party-cookie
  policy can still prevent playback.
- Loading, sign-in, and playback remain explicit user actions; no frame is
  fetched merely by viewing a duplicate candidate.

## Evidence

- Comparison-control tests prove the standard origin and encoded embed URL.
- Type checking and native `cargo check` passed locally.
- Browser-preview QA rendered the existing app with no console errors; it
  cannot populate native channel candidates or verify an operator's sign-in.
