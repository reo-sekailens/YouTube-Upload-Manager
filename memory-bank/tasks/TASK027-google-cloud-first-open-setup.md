# TASK027 — First-open Google Cloud setup

## Status

`completed`

## Outcome

An unconfigured signed app opens a six-step setup guide before normal work.
It guides the operator through their own Google account, Cloud project, API,
Auth Platform, Desktop OAuth client, and local JSON import.

## Boundaries

- The guide does not create Google accounts, projects, credentials, or consent
  settings. It opens fixed Google account and Cloud Console URLs in separate,
  unprivileged windows for operator action.
- The guide can be dismissed for the current session and returns at next launch
  until a Desktop OAuth JSON has been imported.
- OAuth JSON is selected through the native file picker and parsed by Rust;
  no credentials or JSON contents enter the webview.

## Acceptance criteria and evidence

- [x] First-open detection uses the existing `oauthConfigured` safe status.
- [x] Six ordered, user-controlled Google setup steps and JSON import exist.
- [x] Fixed Google setup windows use no application capability.
- [x] Guidance-unit tests, TypeScript check, full frontend test suite, and
  browser-preview UI flow passed locally.
- [x] Backdrop is a sibling layer, so modal surfaces retain their intended
  brightness while the surrounding dashboard is dimmed.

## Follow-up

Live Google account/project creation and OAuth consent remain operator actions
and need an authorized test account before provider certification.
