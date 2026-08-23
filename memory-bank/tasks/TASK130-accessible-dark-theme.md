# TASK130: Accessible dark theme and deletion-review visual QA

## Status

completed

## Objective

Add a device-local light/dark appearance preference, use Tailwind-first theme
tokens for the shared application shell, and correct visual/accessibility
issues found while inspecting every workspace.

## Acceptance criteria

- The operator can switch between light and dark appearances; the choice stays
  on the device and follows the system preference before a choice is saved.
- The application shell and every lazy workspace remain readable with visible
  keyboard focus, sufficient contrast, and no clipped controls at desktop and
  narrow viewport widths.
- The destructive workflow keeps its two typed-ID confirmation stages and
  clear action hierarchy.
- No OAuth, account scope, inventory, or deletion behavior changes.

## Evidence

- `npm run check`, `npm run check:tailwind-ui`, and the focused theme test pass.
- A Vite browser-preview session at `http://127.0.0.1:1421/` passed desktop
  and 390 px rendered QA. The appearance button changed to the pressed
  “Light mode” state after selecting dark mode; the preference persisted over
  a reload. All eight workspace tabs mounted and showed their labeled content
  with no browser-console errors.
- `npm run build` passes. The pre-existing initial JavaScript gzip budget is
  still exceeded (72.14 KiB against the documented 70 KiB), so this task does
  not claim performance-budget certification.
- Corrected after QA: on narrow viewports, Workspace remains a vertical
  navigation sidebar rather than becoming a horizontally scrolling tab row.

## Follow-ups

- None.
