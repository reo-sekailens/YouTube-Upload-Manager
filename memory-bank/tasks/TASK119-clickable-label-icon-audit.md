# TASK119: Clickable label and icon audit

## Status

completed

## Objective

Audit every rendered workspace for overly long clickable labels, replace only
unambiguous compact secondary actions with Lucide icons, and keep the action
text available on hover and to assistive technology.

## Decision boundary

- Icon-only controls use a matching `aria-label` and `title`, retain native
  button semantics and focus styling, and keep their disabled state.
- Keep visible text for destructive, consent, OAuth, data transfer, recovery,
  batch, and other multi-step or state-changing actions.
- Do not convert tabs, form labels, disclosure summaries, support cards, or
  status text into icons.

## Scope

- Audit `App.tsx` and every `src/components/*.tsx` workspace surface.
- Convert the context-bound comparison close control to an accessible Lucide
  `X` button.
- Replace the remaining bespoke SVG comparison transport glyphs with Lucide
  icons so icon-only controls use one library.

## Acceptance criteria

- All icon-only buttons have matching accessible and hover text.
- Long labels remain visible where the action's consequence is not immediately
  obvious from its location and icon.
- Type checks, tests, Tailwind checks, bundle budget, and browser interaction
  checks pass without overflow.

## Completion evidence (2026-08-23)

- Audited Batch, Monitor, Dedupe, Transfer, Rename, deletion-review,
  connection, recovery, and diagnostics surfaces. The clear/refresh/search
  controls from TASK118 remain the only globally safe compact actions.
- Converted the Dedupe comparison's context-bound Close comparison control to
  a Lucide `X` with matching `aria-label` and `title`.
- Replaced the comparison's custom play/pause/seek SVGs with Lucide icons;
  their existing accessible names and hover titles remain intact.
- Retained visible labels for upload/import, queue and monitor lifecycle,
  deletion, OAuth/temporary permissions, export/import, recovery, and support
  actions because a tooltip is insufficient to convey their consequence.
- Passed `npm run check`, `npm test` (87 tests), `npm run check:tailwind-ui`,
  `npm run performance:frontend:check` (223.71 KiB raw / 69.97 KiB gzip JS;
  35.24 KiB CSS), and `git diff --check`.
- The local 10,000-item browser interaction fixture passed with search p95
  89 ms, clear p95 81 ms, and no interaction Long Tasks. Its rendered
  screenshot is `output/performance/clickable-icon-audit-browser-interactions.png`.

## Follow-up (2026-08-23)

- Replaced the clipped uploaded-row `Delete original…` label with a compact
  Lucide `Trash2` control. Its matching `aria-label` and `title` say
  `Delete original file`; the explicit typed-filename confirmation remains
  fully text-labelled before deletion occurs.
