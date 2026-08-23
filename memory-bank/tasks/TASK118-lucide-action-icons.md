# TASK118: Lucide action icons

## Status

completed

## Objective

Use Lucide React icons for compact, recognizable UI controls where an icon is
clearer than repeated action text, while preserving visible labels for actions
whose meaning, risk, or workflow context requires text.

## Requirements

- Add `lucide-react` as the sole icon source; do not introduce bespoke SVGs or
  image icons for controls.
- Every icon-only control must have an accurate `aria-label` and a hover/focus
  tooltip via `title`; disabled controls must retain an accessible name.
- Keep text for destructive, irreversible, consent, OAuth, and multi-step
  workflow actions unless an adjacent visible label already conveys the full
  action.
- Preserve existing keyboard access, disabled behavior, test selectors, and
  responsive layout. Do not replace a labelled native input or status text with
  an icon.
- Prefer an icon beside text when it materially improves scanning without
  making the action ambiguous. Use `size={16}` and `aria-hidden` for decorative
  icon children.

## Scope

- `src/App.tsx`: library refresh and compact queue actions.
- `src/components/PaginationControls.tsx`: previous/next controls.
- `src/components/QueueTable.tsx`: compact search-clear control.
- `src/components/GoogleSetupWizard.tsx`: unambiguous step navigation.
- Tests and Tailwind source entries as required.

## Evidence

- `npm run check`
- `npm test`
- `npm run performance:frontend:check`
- Browser screenshot/DOM checks for tooltip names, keyboard access, and mobile
  overflow.

## Completion evidence (2026-08-23)

- Added `lucide-react` and used it as the sole control-icon source. Icon-only
  Refresh library, Clear upload queue, and Clear search buttons have matching
  `aria-label` and `title` text. Pagination and setup controls retain clear
  visible action text alongside their directional/progress icons.
- The action-icon module is lazy-loaded with the compact App controls, keeping
  the fixed initial JS budget intact: `npm run performance:frontend:check`
  passed at 223.68 KiB raw / 69.95 KiB gzip JS and 35.24 KiB CSS.
- Updated the 10,000-row interaction harness to locate the intentional
  icon-only Clear search button by accessible name, not removed visible text.
  It passed with search p95 77 ms, clear p95 74 ms, and no interaction Long
  Tasks.
- Passed `npm run check`, `npm test`, `npm run check:tailwind-ui`, and
  `git diff --check`. Browser fixture inspection confirmed the Refresh library
  button exposes its matching hover title and no horizontal overflow.
