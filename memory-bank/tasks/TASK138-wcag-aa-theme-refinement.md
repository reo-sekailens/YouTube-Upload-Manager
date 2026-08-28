# TASK138 — WCAG AA light and dark theme refinement

## Status

completed

## Scope

Refine the Tailwind CSS theme tokens and shared accessibility rules so the
existing device-local UI remains legible and keyboard-operable in both light
and dark appearance modes. This task deliberately preserves workflow,
account-scope, and native-operation behavior.

## Acceptance criteria

- Semantic foreground, surface, border, control, status, and focus tokens have
  WCAG AA text contrast in both themes.
- Existing literal utility colours used by lazy workspaces cannot expose a
  light surface or low-contrast foreground in dark mode.
- Controls retain an obvious keyboard focus indicator, native fields follow
  the chosen colour scheme, and motion is reduced when requested.
- The Tailwind policy gate, TypeScript check, test suite, production build, and
  rendered browser checks complete successfully.

## Evidence

- `src/styles.css` now supplies semantic tokens for warning, selected,
  secondary-control, and focus states in both appearances; native controls use
  the matching `color-scheme`.
- The shared theme bridge now normalizes all literal arbitrary foreground
  utilities and, in dark appearance, their background and border companions
  used by mounted lazy workspaces; verified brand, destructive, success, and
  link/status accents remain intentional exceptions.
- Calculated token-pair contrast ratios are 4.72:1 or greater for normal text
  and 5.16:1 or greater for white action text: light ink 15.31, muted 4.72,
  brand 5.35, danger 6.18, warning 8.75; dark ink 13.84, muted 9.23, brand
  5.28, danger 5.16, warning 11.44, and focus 9.02.
- Passed: `npm run check`, `npm run check:tailwind-ui`, `npm run build`, and
  `git diff --check`. Browser preview verified the rendered dark interface has
  no horizontal overflow. The complete Vitest run has one unrelated existing
  failure: `workspace-isolation.test.tsx` still expects 8 tabs while the
  current user-modified `App.tsx` renders 9.

## Follow-ups

- Keep any visual provider or package evidence separate from local browser
  evidence; no YouTube operation is part of this task.
