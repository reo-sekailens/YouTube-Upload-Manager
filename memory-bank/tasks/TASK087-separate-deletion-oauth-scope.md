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
  disconnect or a new ordinary connection. Ending or expiring the local
  15-minute mode retains that protected credential, so re-entry does not
  require another Google consent screen.
- Focused scope-separation and expiry-session regressions pass.
