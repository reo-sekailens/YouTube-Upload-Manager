# TASK064: Diagnostic issue names in GitHub handoff

## Status

completed

## Scope

Include safe crash, error, and warning names in copied and pre-filled GitHub
reports, without exposing raw crash messages, local paths, credentials, or
provider payloads.

## Acceptance criteria

- Crash marker origin has a safe name in reports.
- Persisted warning/error event names are listed separately and de-duplicated.
- Copy and GitHub buttons use the same redacted report.

## Evidence

- Crash markers contribute their safe origin name (`Native panic` or `Webview
  error`) to the copied report.
- A separate de-duplicated issue-name section includes only safe persisted
  warning/error event identifiers; malformed names are replaced with a generic
  local label.
- Both copy and pre-filled GitHub issue buttons already use the native report
  verbatim, so they include this section automatically.
- Rust format, 41 native tests, TypeScript check, and diff check passed.
