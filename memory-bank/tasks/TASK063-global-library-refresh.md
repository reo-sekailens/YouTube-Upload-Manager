# TASK063: Global YouTube library refresh

## Status

completed

## Scope

Provide a global operator-triggered refresh for the active YouTube channel
library, and make sync failures actionable without exposing provider payloads.

## Acceptance criteria

- A global button runs the existing native, account-scoped inventory sync.
- All visible consumers reload the locally saved inventory after success.
- Network, authorization, permission/quota, and malformed-provider failures are
  distinguishable with safe messages.
- The operation remains native-worker-only and never uploads or deletes media.

## Evidence

- Header-level **Refresh library** calls the existing `sync_channel_inventory`
  native blocking command, then reloads the dashboard, connection receipt, and
  deletion-review inventory projection.
- Safe HTTP failure categories now tell the operator whether to reconnect,
  enable the API/confirm permission, wait for a rate limit, or retry a
  temporary YouTube outage. In every failed case the atomic prior library is
  retained.
- Rust format, 41 native tests, TypeScript check, and diff check passed.
- Browser smoke test confirmed the global control is visible, disabled until a
  channel is connected, and introduces no console warnings.
