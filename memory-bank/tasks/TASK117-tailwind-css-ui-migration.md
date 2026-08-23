# TASK117: Tailwind CSS UI migration

## Status

ready

## Owner

unassigned

## Objective

Migrate the complete React/Tauri presentation layer from the current large
semantic CSS surface to Tailwind CSS while preserving the rendered product,
accessibility, lazy workspace boundaries, responsive behavior, and fixed
frontend performance budgets.

## Audit snapshot (2026-08-23)

- The presentation layer is React 19 + Vite 6 and imports one global stylesheet
  from `src/main.tsx`.
- Twenty-two production or performance-harness TSX files contain 436
  `className` sites. Six inline styles are runtime progress widths and should
  remain data-driven rather than becoming generated class names.
- Six stylesheets contain 3,481 source lines / 79,132 bytes, 267 unique class
  selectors, 233 unique hexadecimal colors, 19 media queries, and 26
  `!important` declarations.
- `src/styles.css` owns the shell, shared controls, queue, setup, support,
  modal, and most cross-surface styling: 2,289 lines and 180 class selectors.
- Five feature stylesheets are lazy-loaded with their workspaces:
  `DeletionReview.lazy.css`, `DuplicateReview.lazy.css`,
  `FolderMonitorPanel.lazy.css`, `PreIngestDuplicatePanel.lazy.css`, and
  `VideoTitleRename.lazy.css`.
- The accepted TASK106 production baseline is 230,478 B initial JavaScript raw,
  71,657 B initial JavaScript gzip, and 38,470 B initial CSS raw. The fixed
  limits are 235 KiB JavaScript raw, 70 KiB JavaScript gzip, and 40 KiB CSS raw.
  The gzip JavaScript gate has only 23 B of headroom, so long utility recipes
  must not be copied throughout the initial shell.
- Dynamic semantic classes currently cover connection state, active workspace,
  drag state, dedupe phases/activity, deletion activity, monitor state, setup
  progress, upload status, duplicate evidence, pre-ingest verdicts, and rename
  activity. Tailwind utility variants must be complete static strings selected
  from typed maps; interpolated utility fragments are prohibited.
- Complex residual behavior includes mobile table labels through pseudo
  content, `:focus-visible`/`:focus-within`, `:not(...)`, `:nth-child(...)`,
  child/sibling selectors, modal stacking, progress widths, and responsive
  breakpoints at 640 px and 820 px.

## Complete UI migration inventory

### Entry points and shell

| Path | Current styling surface | Migration requirement |
| --- | --- | --- |
| `index.html` | Theme-color and root mount only | Keep document metadata/root behavior; update the theme color only if the equivalent Tailwind token remains exactly `#2463df`. |
| `performance-interactions.html` | Performance root mount only | Keep the dedicated no-index harness entry unchanged. |
| `src/main.tsx` | Imports `styles.css` | Retain one global style entry and startup ordering; Tailwind must not delay safe-shell rendering. |
| `src/App.tsx` | 59 class sites; startup shell, top bar, workspace tabs, drop zone, notices, exit/intake dialogs | Migrate after shared primitives; preserve `data-performance-*`, tab roles, focus flow, modal sibling backdrops, and active/drop state maps. |
| `src/performance-interactions.tsx` | 7 class sites; real 10,000-row Batch harness | Use the same production Tailwind primitives and keep measurement selectors/semantics unchanged. |

### Component checklist

| Path | Class sites / local CSS | Migration focus |
| --- | --- | --- |
| `src/components/ConnectionPanel.tsx` | 16 / global | Connected/idle state, status row, action spacing, and responsive account identity. |
| `src/components/CrashBoundary.tsx` | 1 / global | Minimal lazy fallback must remain visually safe before other chunks load. |
| `src/components/CrashRecoveryScreen.tsx` | 11 / global | Full-screen recovery gradient, readable contrast, support actions, and mobile layout. |
| `src/components/DedupeActivityPanel.tsx` | 9 / global; 1 dynamic width | Typed phase/activity variants and truthful accessible progress. |
| `src/components/DeletionReview.tsx` | 34 / `DeletionReview.lazy.css` | Inventory cards, selection toolbar, two destructive confirmation stages, and lazy CSS ownership. |
| `src/components/DiagnosticsPanel.tsx` | 15 / global | About/support grid, release labels, copy/report feedback, and fixed outbound actions. |
| `src/components/DuplicateReview.tsx` | 69 / `DuplicateReview.lazy.css`; 1 dynamic width | Comparison players, synchronized controls, evidence badges, bulk deletion review, logs, pagination, and lazy CSS ownership. |
| `src/components/FolderMonitorPanel.tsx` | 49 / `FolderMonitorPanel.lazy.css` | Monitor state, configuration, live activity, cancelled-file actions, responsive facts/logs, and lazy CSS ownership. |
| `src/components/GoogleSetupWizard.tsx` | 13 / global | Modal stacking, six-step state variants, mobile controls, and accessible current/completed states. |
| `src/components/ManualUploadDefaultsPanel.tsx` | 3 / global | Compact device-default control and saved-status feedback. |
| `src/components/PaginationControls.tsx` | 3 / global | Shared bounded-list controls and disabled/focus states. |
| `src/components/PreIngestDuplicatePanel.tsx` | 42 / `PreIngestDuplicatePanel.lazy.css` | Drop state, progress/activity, evidence verdicts, metadata comparison, bulk/local deletion dialogs, and lazy CSS ownership. |
| `src/components/QueueTable.tsx` | 39 / global; 1 dynamic width | Desktop table, 640 px card transformation, progress, status/actions, source-cleanup confirmation, and 32-row behavior. |
| `src/components/StatusPill.tsx` | 3 / global | Replace interpolated status class construction with a complete typed status-to-utility map. |
| `src/components/TransferPanel.tsx` | 10 / global | Responsive export/import cards and safe receipt/error states. |
| `src/components/UploadIntakeReview.tsx` | 10 / global | Required-review modal, playlist creation, audience/visibility, cleanup consent, and focus behavior. |
| `src/components/UploadProgressSummary.tsx` | 7 / global; 2 dynamic widths | Current and batch progress tracks, truthful ETA text, and accessible values. |
| `src/components/UploadTitleDuplicateReview.tsx` | 11 / global | Explicit upload/skip review, apply-to-all choice, evidence badge, and action hierarchy. |
| `src/components/VideoTitleRename.tsx` | 25 / `VideoTitleRename.lazy.css`; 1 dynamic width | Rule/review grids, before/after list, management mode, activity variants, progress, and lazy CSS ownership. |

### Style, configuration, and verification paths

- `src/styles.css`
- `src/components/DeletionReview.lazy.css`
- `src/components/DuplicateReview.lazy.css`
- `src/components/FolderMonitorPanel.lazy.css`
- `src/components/PreIngestDuplicatePanel.lazy.css`
- `src/components/VideoTitleRename.lazy.css`
- `package.json` and `package-lock.json`
- `vite.config.ts`
- `src/components/large-list-rendering.test.tsx`
- `src/components/workspace-isolation.test.tsx`
- `src/performance-harness.ts` and `src/performance-harness.test.ts`
- `scripts/performance/frontend-baseline.mjs`
- `scripts/performance/measure-browser-interactions.mjs`
- `tests/performance-baseline.test.ts`

## Target architecture

- Default to Tailwind CSS 4 with the official `@tailwindcss/vite` plugin. Add
  only `tailwindcss` and `@tailwindcss/vite` as build-time dependencies and
  preserve the existing React plugin, manifest, chunk naming, server, and Tauri
  environment configuration.
- Before locking Tailwind 4, record that supported WebViews meet its official
  minimums (Chrome 111, Safari 16.4, and Firefox 128). If the product must
  support an older mobile WebView, use Tailwind 3.4 and document that decision
  instead of silently narrowing platform support.
- Start coexistence without Tailwind Preflight. Import theme and utilities
  explicitly so the current heading, list, image, input, button, and border
  behavior cannot change globally. Enable Preflight only if a separate rendered
  regression proves parity and the required base styles are explicit.
- Define the app palette, type scale, radii, shadows, spacing, and the 640 px /
  820 px responsive boundaries as CSS-first `@theme` tokens. Consolidate
  repeated colors, but retain exact outlier values until visual parity is
  proven; this task is not a redesign.
- Keep `src/styles.css` as the global theme/base/shared-primitive entry. Use
  `source(none)` plus explicit `@source` ownership so utility generation does
  not pull every lazy workspace into the initial CSS.
- Keep each of the five existing lazy stylesheet imports as a feature Tailwind
  entry during migration. Use explicit per-feature sources and `@reference` to
  the global theme where required, then confirm the Vite manifest still keeps
  feature CSS out of the initial entry.
- Prefer utilities in JSX for one-off layout/state styling. Keep a small
  `@layer components`/custom-CSS surface for repeated primitives and selectors
  that utilities would make less readable, including mobile table labels,
  pseudo content, and tightly coupled complex descendants. Do not recreate all
  267 legacy selectors one-for-one with `@apply`.
- Reuse short semantic recipes for initial-shell primitives such as panels,
  buttons, fields, notices, backdrops, and visually-hidden content. This avoids
  duplicating long class strings in JavaScript and protects the 70 KiB gzip
  budget.
- Replace every dynamic class fragment with a finite typed map containing full
  utility strings. Runtime progress widths may remain inline styles or move to
  a CSS custom property; they must not generate arbitrary class names.

## Dependency-ordered migration plan

### Phase 0 — Freeze parity evidence

- Run the existing frontend tests, production build, asset-budget check, and
  10,000-row browser interaction harness before dependency changes.
- Record the current manifest's initial/lazy CSS ownership.
- Capture populated baseline screenshots at desktop, 820 px, 640 px, and 390 px
  widths for the safe shell, Batch, monitor, duplicate/pre-ingest review,
  transfer, rename, deletion, account, About, setup, recovery, and destructive
  dialogs. Keep fixture/browser evidence separate from native/provider proof.

### Phase 1 — Integrate Tailwind without changing the UI

- Add the selected Tailwind packages and intentional lockfile change.
- Add the official Vite plugin without overwriting the existing Rollup output
  configuration.
- Add the global CSS-first theme, explicit source ownership, and utilities-only
  import. Verify that Tailwind utilities compile in both the normal entry and
  the performance harness while the legacy selectors still render unchanged.
- Confirm the modern WebView compatibility decision and record it in technical
  notes before continuing.

### Phase 2 — Shared primitives and application shell

- Migrate base typography/focus rules, buttons, fields, panels, headings,
  notices, pagination, badges, progress tracks, screen-reader content, and
  modal backdrops first.
- Migrate `App.tsx`, `CrashBoundary.tsx`, `CrashRecoveryScreen.tsx`, and
  `GoogleSetupWizard.tsx` against those primitives.
- Preserve the two-frame safe-shell milestone, lazy workspace mounting,
  `hidden`/tab behavior, sibling backdrop stacking, and confirmed-exit flow.

### Phase 3 — Batch and shared operational surfaces

- Migrate `QueueTable.tsx`, `StatusPill.tsx`, `UploadProgressSummary.tsx`,
  `UploadIntakeReview.tsx`, `UploadTitleDuplicateReview.tsx`,
  `ManualUploadDefaultsPanel.tsx`, and `PaginationControls.tsx`.
- Preserve the desktop table-to-mobile-card transformation, `data-label`
  pseudo content, private default, separate progress/ETA, drag-and-drop review,
  source-cleanup safety copy, 32-row page bound, and all action disabled states.
- Update the performance interaction entry to consume the same migrated Batch
  styles rather than a parallel fixture-only presentation.

### Phase 4 — Independent lazy workspace waves

After the global theme/primitives are stable, migrate each pair independently
with exclusive ownership of its component and lazy stylesheet:

1. `DeletionReview.tsx` + `DeletionReview.lazy.css`
2. `DuplicateReview.tsx` + `DuplicateReview.lazy.css`
3. `PreIngestDuplicatePanel.tsx` + `PreIngestDuplicatePanel.lazy.css`
4. `FolderMonitorPanel.tsx` + `FolderMonitorPanel.lazy.css`
5. `VideoTitleRename.tsx` + `VideoTitleRename.lazy.css`

Each wave must pass its rendered responsive and interaction checks before its
legacy selectors are removed. Shared theme/primitives remain coordinator-owned
to prevent concurrent stylesheet conflicts.

### Phase 5 — Account, support, and transfer surfaces

- Migrate `ConnectionPanel.tsx`, `DiagnosticsPanel.tsx`, `TransferPanel.tsx`,
  and `DedupeActivityPanel.tsx`.
- Preserve connected-account identity spacing, fixed outbound-link behavior,
  local diagnostic redaction boundaries, truthful dedupe phases, and receipt/
  error feedback.

### Phase 6 — Remove legacy CSS and certify the result

- Delete a legacy selector only after its owning JSX has migrated and its
  screenshot/interaction check passes. Remove orphan selectors, duplicate
  colors, obsolete media queries, and unnecessary `!important` declarations.
- Retain only documented residual custom CSS and the Tailwind theme/entry
  directives. Do not commit generated `dist` output.
- Re-run the complete validation matrix and compare before/after screenshots.
  Provide final screenshots because this migration changes UI implementation,
  even when the intended appearance is parity.

## Acceptance criteria

- Every production and performance-harness UI file in the inventory uses
  Tailwind utilities/tokens or a documented residual custom selector; there is
  no unowned legacy styling.
- No UI text, workflow, native command, OAuth scope, account/channel boundary,
  queue behavior, duplicate/deletion safeguard, or local-only architecture is
  changed by the migration.
- All finite visual states use statically detectable complete Tailwind class
  strings. No `bg-${...}`, `text-${...}`, or equivalent generated fragment is
  present.
- The safe shell and active Batch workspace retain their existing performance
  markers and ordering. Inactive workspaces remain unmounted and their feature
  CSS remains outside the initial manifest entry.
- Initial assets remain at or below 235 KiB JavaScript raw, 70 KiB JavaScript
  gzip, and 40 KiB CSS raw. The migration must reduce duplication or change its
  composition; raising the fixed budgets is not an acceptance path.
- The 10,000-row Batch harness retains fewer than 100 mounted records, search
  and clear p95 below 100 ms, and zero interaction Long Tasks above 50 ms.
- Keyboard workspace navigation, visible focus, labels, roles, live regions,
  modal focus/stacking, progress semantics, and disabled states remain intact.
- Desktop and responsive screenshots show no unintended layout, overflow,
  contrast, spacing, or stacking regression at 820 px, 640 px, and 390 px.
- Local browser/fixture, unsigned packaged Windows, non-Windows package, and
  live-provider evidence are reported separately. No Google/YouTube canary is
  required for a CSS-framework migration.

## Validation commands

- `npm run check`
- `npm test`
- `npm run build`
- `npm run performance:frontend:check`
- `node scripts/performance/measure-browser-interactions.mjs --output <absolute-json-path>`
- `git diff --check`

Rendered browser QA and an available unsigned Windows packaged smoke check are
also required. Provider uploads, OAuth consent, and destructive live operations
remain out of scope.

## Rollback and sequencing

- Keep legacy selectors until the owning surface passes parity checks; each
  component wave can then be reverted independently.
- Do not combine the migration with a redesign, content rewrite, React state
  refactor, dependency upgrade unrelated to Tailwind, or native/Rust change.
- If per-feature Tailwind source partitioning cannot preserve lazy CSS, stop
  and record the manifest/budget evidence before choosing a different bundle
  architecture. Do not silently fold every workspace into initial CSS.

## Planning evidence and references

- This task file and index entry are the only changes in the planning pass;
  Tailwind is not installed and the rendered UI is unchanged.
- Official Vite integration: https://tailwindcss.com/docs/installation/using-vite
- Official source detection and multiple-stylesheet ownership:
  https://tailwindcss.com/docs/detecting-classes-in-source-files
- Official CSS-first theme variables: https://tailwindcss.com/docs/theme
- Official Preflight behavior/opt-out: https://tailwindcss.com/docs/preflight
- Official browser compatibility: https://tailwindcss.com/docs/compatibility

## Dependencies

TASK106
