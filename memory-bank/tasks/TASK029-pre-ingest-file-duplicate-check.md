# TASK029 — Pre-ingest file duplicate check

## Status

`completed`

## Outcome

Operators can drag arbitrary files, including `.insv` and `.lrv`, into the
duplicate-review workspace before ingestion. The native app hashes each file
without creating an upload item, reports matching saved files and—when connected and
the inventory sync succeeds—matching uploaded YouTube titles.

## Boundaries

- No dropped file becomes an upload item, managed copy, or network upload.
- Every regular, non-empty file extension is eligible for comparison; the
  separate ingest flow continues to enforce its supported upload formats.
- File contents and full source paths never leave the native device layer.

## Acceptance criteria

- [x] Pre-ingest drag/drop accepts arbitrary regular file extensions on desktop;
  Android and iOS use the native document picker.
- [x] Exact local SHA-256, duplicate-within-drop, and connected YouTube-title
  matches are clearly distinguished.
- [x] Local duplicate evidence remains available if YouTube inventory sync
  cannot run.
- [x] Focused native/frontend tests and browser UI verification pass.

## Evidence

- `cargo check`, `cargo test proprietary_file_extensions_are_hashable_without_ingestion`,
  and the mobile-URI filename test passed in `src-tauri`.
- `npm run check`, `npm test` (22 tests), `npm run build`, and `git diff --check`
  passed at the repository root.
- Browser-preview QA verified the Duplicate review panel layout. Native mobile
  picker/device behavior remains a platform-device integration check.
- A newer drag/drop request supersedes an earlier in-flight result, and iOS
  security-scoped picker access is released immediately after local hashing.
