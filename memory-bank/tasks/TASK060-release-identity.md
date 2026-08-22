# TASK060 — Release identity in About and diagnostics

**Status:** completed  
**Dependencies:** TASK052, TASK056

## Objective

Make the installed release identity visible in About and preserve the exact
version and channel in every copied GitHub issue report.

## Acceptance criteria

- About displays the native app version and clearly labels regular versus
  nightly builds.
- Copied diagnostic reports include the same version and release channel.
- Nightly CI explicitly stamps its artifacts as `nightly`; non-nightly builds
  safely default to `regular`.
- No release metadata contains operator, account, credential, or filesystem
  data.

## Evidence

- The native release-identity command is the shared source for the About UI and
  copied report. The nightly workflow provides `APP_RELEASE_CHANNEL=nightly` to
  desktop and Android builds; all other builds receive the native `regular`
  default.
- `npm run test` (31 tests) and `npm run build` passed. Focused native tests
  compiled and passed; `cargo fmt --check` is currently blocked only by
  unrelated concurrent formatting in the pre-ingest implementation. No live
  provider operation is needed.
