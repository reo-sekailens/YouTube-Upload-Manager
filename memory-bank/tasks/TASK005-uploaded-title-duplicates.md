# TASK005 — Uploaded-video title duplicate candidates

## Status

`completed`

## Objective

Flag already-uploaded videos from the active channel when their titles are an
exact normalized match or differ only by a trailing duplicate-copy suffix such
as `(2)`. Keep the result review-only and route permanent removal through the
existing separately confirmed deletion workflow.

## Boundaries

- Compare only locally synchronized inventory records for the active channel.
- Treat case and repeated whitespace as insignificant for exact title matching.
- Strip only explicit trailing duplicate markers: `(2)` and higher, including
  whitespace before the marker. This original no-fuzzy boundary is superseded
  by TASK041's conservative, review-only capture-sequence matching.
- Keep title matches at metadata confidence; they are not proof of identical
  media and must never trigger automatic deletion.
- Preserve device-local storage and the existing YouTube deletion safeguards.

## Work items

### TASK005-A — Native candidate generation

- **Paths:** `src-tauri/src/lib.rs`
- **Depends on:** none
- Generate deterministic remote title candidates scoped to the active channel.
- Include both YouTube video IDs in each remote candidate.
- Add focused Rust tests for equal titles, case/whitespace normalization,
  `(2)`/higher suffixes, non-duplicate numeric parentheses, and channel scope.

### TASK005-B — Operator-visible evidence

- **Paths:** `src/lib/types.ts`, `src/components/DuplicateReview.tsx`
- **Depends on:** none
- Render remote title candidates as YouTube-library comparisons, show both
  video IDs, and direct removal through the separate review workflow.

### TASK005-C — Integration and documentation

- **Paths:** `memory-bank/tasks/TASK005-uploaded-title-duplicates.md`,
  `memory-bank/tasks/_index.md`, `memory-bank/progress.md`
- **Depends on:** TASK005-A, TASK005-B
- Record the implemented behavior, validation evidence, and live-provider
  verification boundary.

## Acceptance criteria

- Same-title uploaded videos are flagged without case or whitespace noise.
- `Title` and `Title (2)` (or a higher integer) are grouped as candidates.
- `Title (1)`, internal parentheses, partial titles, and fuzzy similarities are
  not treated as duplicates.
- Candidates never cross the active channel scope.
- The UI identifies both uploaded videos and does not imply automatic deletion.
- Rust tests, TypeScript checks, production build, and rendered desktop/mobile
  validation pass.

## Evidence

- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml` passed all 6 Rust tests,
  including title canonicalization, deterministic pairing, and channel scope.
- `npm run check`, `npm run test` (4/4), and `npm run build` passed.
- In-app Browser QA passed at 1280×900 and 390×844. The rendered flow moved
  from the empty duplicate state to two uploaded-title candidates after
  **Sync library**, displayed both YouTube IDs, had no console warnings/errors,
  and had no mobile horizontal overflow.
- Browser evidence used deterministic local fixture inventory. No Google or
  YouTube request was made during visual QA.

## Follow-ups

- Live YouTube inventory verification remains separate and requires an
  authorized test channel containing deliberate title duplicates.
