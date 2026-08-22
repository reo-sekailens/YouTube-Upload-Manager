# TASK011 — OAuth token-exchange diagnostics

**Status:** in-progress  
**Owner:** unassigned  
**Dependencies:** TASK003

## Objective

Make failed Google OAuth token exchanges actionable without exposing authorization codes, PKCE verifiers, tokens, request headers, or arbitrary provider payloads.

## Root cause evidence

- The installed app completed the browser authorization callback but stored only `Google rejected the authorization response.` after Google's token endpoint returned a non-success status.
- OAuth clients are operator-owned: each operator creates a Google Cloud project and Desktop client, then imports its downloaded JSON locally.
- Google Auth Platform currently warns that the OAuth configuration is incomplete. Branding URLs and configured Data Access scopes are empty.
- The GitHub repository is private, so its existing Markdown privacy and terms files are not usable as public consent-screen URLs.

## Work items

- [x] **TASK011-A — Safe token-error mapping:** Parse non-success token responses, map documented OAuth error codes to actionable operator messages, and keep provider payloads and credentials out of displayed errors.
- [x] **TASK011-B — Per-operator Desktop OAuth fallback:** Accept only a selected Google Desktop OAuth JSON, retain only its client ID in SQLite, keep its optional client secret in OS-protected storage, and use it only in native token requests.
- [x] **TASK011-C — Connection status and versioned installer:** Poll the native receipt after browser launch so completed/error callbacks update the screen, then build and install a versioned Windows replacement package.
- [x] **TASK011-E — Callback polling lifetime:** Keep the connection poll active after the browser opener succeeds so native callback success/error is rendered without restarting the app.
- [x] **TASK011-F — Operator-owned OAuth configuration:** Remove the distributed client ID and manual client-ID command. The app requires a selected Desktop OAuth JSON from the operator's own Google Cloud project.
- [ ] **TASK011-D — Operator live connection:** A dedicated `YouTube Mass Uploader Local JSON` Desktop client and its local JSON file have been created. Select it in the app and complete one real authorization. This remains deliberately user-owned because it grants the selected Google account access.

## Acceptance criteria

- A non-success token exchange records a stable, actionable message containing only a known OAuth error code and safe guidance.
- Authorization codes, PKCE verifiers, access/refresh tokens, HTTP headers, and raw provider descriptions are never stored or shown.
- Existing native tests remain green.
- A real installed-app retry either connects the YouTube channel or records the exact safe provider error category.
- A fresh installation has no OAuth client configured until the operator imports a Desktop OAuth JSON.

## Evidence

- Google Auth Platform configuration was saved with public homepage/privacy/terms URLs, an authorized domain, Testing audience (two test users), and `youtube.upload`, `youtube.readonly`, and `youtube.force-ssl` scopes. The app was not published.
- Token errors map known OAuth categories without preserving provider descriptions or credential material; focused Rust tests prove redaction.
- Version `0.1.1` passed 13 Rust tests, TypeScript checking, 9 web tests, and an x64 NSIS production build. Installer SHA-256 `9B402B9B1C9DE4FB3DDA2D202A321BB7F9F6F330440F518F66CDC8EC76691A4B`; the installed executable reports File/Product version `0.1.1` and starts successfully.
- Browser visual QA captured the connection panel's **Import Desktop OAuth JSON** action. A live user-account authorization remains pending user selection/consent.
- A dedicated Desktop OAuth client was created in `youtube-mass-uploader-506218`, restricted to existing test users. Its JSON was saved outside the repository under the operator's Downloads directory; its secret was not logged or copied into project files.
- Native SQLite confirmed the completed authorization for `SekaiLens`; version 0.1.2 removes the premature `connecting` reset that had stopped the callback poll before the native receipt arrived. It passed 13 Rust tests, TypeScript checking, 9 web tests, and a fresh NSIS build/install. Installer SHA-256 `E0AC72DFC62831AF1755D422BFEA7BEE237C75530548F0AEA5333ED3D0572135`.
- The distributed client ID and manual ID-only configuration command were removed. The connection panel and public docs now direct every operator to create their own Google Cloud project, enable the YouTube Data API, configure consent, create a Desktop OAuth client, and import its downloaded JSON. Local source scans and focused tests provide implementation evidence; a live authorization with an operator-created project remains pending.
