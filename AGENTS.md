# Repository guidance for AI agents

## Source of truth

Before implementation, read `memory-bank/project-brief.md`, `memory-bank/architecture.md`, `memory-bank/technical-notes.md`, `memory-bank/progress.md`, and `memory-bank/tasks/_index.md`.

Create or update a scoped task file in `memory-bank/tasks/` for non-trivial work. Keep task status, acceptance criteria, evidence, and follow-ups current.

## Engineering rules

- Preserve account isolation on each device: every YouTube channel connection, upload batch, inventory record, and audit event must be explicitly scoped.
- Build a Tauri application without an application API, cloud worker, remote database, object store, or telemetry pipeline. Network calls are limited to explicit Google/YouTube operations and operator-selected updates.
- Use official Google/YouTube APIs and installed-app OAuth with PKCE. Access and refresh tokens belong in OS-protected secure storage, never in the webview, plain files, logs, or source control. A distributed OAuth client ID is not a secret.
- Prefer idempotent operations, resumable uploads, structured errors, rate-limit handling, and append-only audit events.
- A queued asset must survive a crash or forced close: ingest it into the app's device-local workspace by default, persist encrypted resumable-session checkpoints transactionally, and reconcile with YouTube before any retry. Never ask the operator to drag an existing queued item in again.
- Validate external inputs; do not trust filenames, metadata, OAuth callbacks, provider payloads, or webview state.
- Add tests with behavior changes and run the narrowest relevant check before handoff.
- Do not change generated files, lockfiles, or deployment configuration without a clear reason.

## Documentation expectations

Update the memory bank when a decision, boundary, integration, workflow, or verified result changes. Record what was actually tested; distinguish local validation from live-provider verification.

## Git commit messages

Every generated commit message must follow Git's conventional subject-and-body form:

- Write a concise, imperative subject line that identifies the affected area and the purpose of the commit.
- Include a blank line followed by a contextual body describing the material changes, additions, and removals, plus any relevant migration, compatibility, or verification context.
- Keep the message faithful to the staged diff; do not use generic subjects or omit the body for non-trivial changes.
