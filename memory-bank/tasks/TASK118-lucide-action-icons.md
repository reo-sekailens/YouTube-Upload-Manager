# TASK118: Lucide action icons

## Status

in-progress

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
