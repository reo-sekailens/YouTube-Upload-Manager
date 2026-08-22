# TASK087 — Separate deletion OAuth scope

## Status

completed

## Objective

Keep the destructive YouTube scope out of ordinary upload/read connection
consent and authorize it only through the dedicated deletion flow.

## Acceptance criteria

- Ordinary OAuth URL omits `youtube.force-ssl`.
- Deletion authorization explicitly requests it and remains locally gated.
- Tests protect both authorization URL contracts.

## Evidence

- The ordinary authorization scope now omits `youtube.force-ssl`; the separate
  deletion scope retains it and a focused regression test passes.
- Deletion authorization now stores and refreshes a distinct OS-protected
  credential, requires the already connected immutable channel, and clears on
  explicit disable, disconnect, new ordinary connection, or expiry.
- Focused scope-separation and expiry-removal regressions pass.
