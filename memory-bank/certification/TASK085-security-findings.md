# TASK085 security findings — 2026-08-23

## Scope and method

Static production-readiness scan of the current repository, covering the Rust
native layer, Tauri capability/configuration boundary, React command callers,
and local artifact paths. This is source evidence only; it does not certify a
live Google/YouTube account, installed package, or physical mobile device.

## Findings requiring remediation

| ID | Severity | Finding | Primary evidence |
| --- | --- | --- |
| TASK086 | medium | Manual queued uploads are not immutably bound to their reviewed channel and can dispatch after an account switch. | `src-tauri/src/lib.rs:6293-6354`, `6487-6523`, `5284-5310`, `1853-1905` |
| TASK087 | low | The ordinary connection requests `youtube.force-ssl`, granting provider-side deletion capability before deletion-specific consent. | `src-tauri/src/lib.rs:5666-5682` |
| TASK088 | low | Saved remote inventory and deletion-request views are not scoped to the active immutable channel. | `src-tauri/src/lib.rs:626-739`, `6058-6126` |
| TASK089 | medium | FFprobe output is byte-limited only after `wait_with_output` has buffered it in memory. | `src-tauri/src/lib.rs:3794-3840` |
| TASK090 | medium | Reference-in-place watched sources can change after stability acceptance and before automatic upload. | `src-tauri/src/lib.rs:4381-4475`, `4780-4889`, `1853-1947` |
| TASK091 | low | Path-based local-source cleanup and duplicate deletion retain a replace-before-delete race. | `src-tauri/src/lib.rs:1563-1600`, `3333-3464` |

## Confirmed controls

- Desktop OAuth uses a loopback listener, exact state validation, PKCE S256,
  and HTTPS token/API endpoints; verifiers, refresh tokens, client secrets,
  and resumable-session URLs use the OS credential store.
- Provider deletion additionally requires local request/typed confirmation,
  temporary deletion mode, and fresh channel-ownership validation.
- Tauri filesystem permissions are not granted to the webview; normal managed
  import filenames are UUID-derived; portable archive import is bounded and
  parameterized; diagnostics redact sensitive values.

## Certification impact

The production-security row remains **blocked** until TASK086 through TASK091
are remediated and retested. No live-provider certification claim is made.
