# TASK061 — One-click pre-filled GitHub issue

**Status:** completed  
**Dependencies:** TASK056

## Objective

Let an operator open a new GitHub issue with the complete redacted local
diagnostic report already filled into the issue body.

## Acceptance criteria

- About and the crash-recovery screen offer a direct **Report to GitHub**
  action.
- The action creates the local redacted report, copies it as a backup, and
  opens the repository's new-issue page with the full report in its body.
- The app never submits an issue automatically; the operator reviews and
  submits it in GitHub.

## Evidence

- The report is supplied as the GitHub issue body's query parameter, and the
  same report remains in the local clipboard. The action uses the system
  browser in Tauri and a new tab in browser preview.
- Local frontend tests and browser visual QA are recorded after the final
  change. No diagnostic data is sent until the operator submits in GitHub.
