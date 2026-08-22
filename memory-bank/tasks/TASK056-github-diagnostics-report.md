# TASK056 — GitHub-ready diagnostics report

**Status:** completed  
**Dependencies:** TASK035, TASK040

## Objective

Give operators an About and support workspace tab that copies a complete,
Markdown-formatted GitHub issue report for local troubleshooting.

## Acceptance criteria

- The About tab explains the product and diagnostic privacy boundary.
- One button copies a GitHub issue-ready Markdown report to the local clipboard.
- The native report contains app/runtime/system metadata, safely captured crash
  markers, and bounded locally persisted warnings/errors.
- Reports never include OAuth credentials, tokens, account identifiers,
  filesystem paths, media data, or raw provider payloads.
- A native panic marker survives process termination sufficiently to appear in
  the next diagnostic report.

## Evidence

- Native Markdown report command includes app/build/OS/architecture metadata,
  connection booleans, queue status totals, a persisted panic timestamp marker,
  and up to 30 recent local audit events.
- Detail redaction removes credential terms, OAuth artifacts, URLs, account IDs,
  filesystem paths, and raw provider details before text reaches the clipboard.
- `npm run test` (31 tests), `npm run build`, `cargo fmt --check`, and `cargo
  test` (35 tests) passed. Browser preview confirms the tab, clipboard action,
  and copied-status feedback; live provider activity was not used.
- The same tab links to the live GitHub Wiki and the project website through
  the system browser, without sending diagnostics or app state.
