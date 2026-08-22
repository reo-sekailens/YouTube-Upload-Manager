# TASK017 — Device-wide Made for Kids default

## Status

`completed`

## Outcome

The operator can save a device-wide yes/no Made for Kids default. Every manual
drop review remains visible and prefilled with that value, so the operator can
confirm or override it for the current batch.

## Boundaries

- The default is device-local and never affects watched-folder automation.
- It is only a review prefill; each manual batch still presents the declaration.
- The native layer persists the value and creates an audit event for each change.

## Evidence

- `npm run check`, `npm run test` (15 tests), `npm run build`, `cargo fmt
  --check`, `cargo test` (14 Rust tests), and `git diff --check` passed.
- Browser preview rendered the device-wide default control without console
  warnings or errors. Native persistence needs a signed-app test; no provider
  request is involved in saving the default.
