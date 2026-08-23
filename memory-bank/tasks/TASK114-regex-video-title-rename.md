# TASK114 — Regex video title rename

## Status

completed

## Objective

Provide a dedicated, channel-scoped library tab that previews and applies
operator-selected YouTube video title edits using regular expressions and
normalization templates.

## Scope and safety boundary

- The preview operates only on the active channel's locally synchronized
  inventory; a title must match the expression before it can be selected.
- Operators can select individual matching videos or all matching videos, and
  a regex replacement changes only its matched portions.
- Applying a review rechecks the immutable active channel, every video's
  current provider title, title bounds, and ownership. It uses the separately
  authorized `youtube.force-ssl` credential in its temporary local mode.
- The protected management credential survives expiry of the temporary local
  mode; re-entering rename mode does not ask Google for consent again.
- Each successful change updates local title keys and leaves an append-only,
  channel-scoped audit event. A provider interruption requires a refresh
  before retrying; it never silently retries unknown provider state.

## Affected paths

- `src/components/VideoTitleRename.tsx` and its lazy stylesheet
- `src/lib/title-rename.ts`, `src/lib/local.ts`, and `src/lib/types.ts`
- `src/App.tsx`
- `src-tauri/src/lib.rs`

## Acceptance criteria

- Friendly regex inputs show parse errors, replacement help, normalization
  templates, and before/after previews.
- Individual matching titles and all matching titles can be selected.
- Partial replacements preserve unmatched title text.
- Applying selected changes is native-only, channel-bound, reviewed, audited,
  and refreshes the local inventory presentation.
- A visible per-title progress bar reports confirmed changes, the active title,
  and titles waiting. A detailed before/after activity log starts collapsed.

## Validation

- `npm test -- --run src/lib/title-rename.test.ts` passed (7 tests), including
  global and partial replacement, capture groups, invalid regex handling,
  matching-only selection, and normalization templates.
- `npm run build` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib title_rename --
  --test-threads=1` passed (1 focused native validation test); `cargo check
  --manifest-path src-tauri/Cargo.toml` passed.
- Browser preview: the Rename videos tab lazy-loaded, rendered its
  disconnected-state guard, and had no console warnings/errors. The actual
  title preview and provider update require a connected, approved YouTube
  canary and were not exercised against a live account.
