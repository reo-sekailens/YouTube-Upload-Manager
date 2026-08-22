# TASK084 — Upload queue row layout

## Status

completed

## Objective

Make persisted-upload rows easier to scan by aligning column headers with their data, grouping state and actions, and reducing sparse table whitespace.

## Implementation

- Corrected the visibility and local-identity column order.
- Moved actions into the status column so completion receipts, delete controls, and in-progress cancellation belong to the relevant state.
- Styled desktop queue entries as compact bordered cards with explicit column proportions, concise digest treatment, and responsive retained mobile labels.

## Verification

- `npm test` — 35 passed.
- `npm run build` — passed; Vite retained its pre-existing mixed static/dynamic `@tauri-apps/plugin-opener` chunk warning.
- Browser preview loaded without console warnings or errors after the layout update. Native-only queue data is unavailable in browser-preview mode, so uploaded-row rendering was validated from the typed component structure and responsive CSS rather than fabricated preview data.
- `git diff --check` — passed.
