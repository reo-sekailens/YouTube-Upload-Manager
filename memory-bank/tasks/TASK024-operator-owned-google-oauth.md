# TASK024 — Operator-owned Google OAuth client

## Status

`completed`

## Outcome

The application no longer distributes a Google OAuth client ID. Each operator
creates their own Google Cloud project and Desktop OAuth client, then imports
the downloaded JSON on their device before connecting a channel.

## Boundaries

- Only Google Desktop OAuth JSON is accepted; arbitrary client-ID input is not
  exposed to the webview.
- The JSON is parsed by Rust. Its optional client secret remains only in the
  OS-protected credential store; tokens and JSON contents never reach the
  webview or audit log.
- Existing connected devices retain their saved local configuration; a fresh
  installation requires an operator-imported client before it can connect.
- Channel-verification HTTP failures now retain a safe, actionable category in
  the local connection detail and diagnostics audit. In particular, a `403`
  identifies a Google Cloud project where YouTube Data API access is denied
  instead of the previous generic callback failure.

## Evidence

- No distributed client-ID constant remains in the Rust source.
- Connection UI and public setup documentation require operator-created Google
  Cloud project configuration and Desktop OAuth JSON import.
