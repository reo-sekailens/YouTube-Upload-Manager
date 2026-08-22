# TASK072: Cross-platform app-icon packaging

## Status

in-progress

## Scope

Regenerate the Tauri platform icon set from the canonical upload-arrow asset,
verify Windows bundle configuration and the produced installer/executable icon,
and retain generated icons for desktop and mobile packaging.

## Acceptance criteria

- Windows bundle and executable use the canonical blue upload-arrow icon.
- Tauri icon variants are regenerated from one canonical source for supported
  desktop and mobile targets.
- A new Windows installer contains the corrected executable icon.
- Source and packaging checks record what was actually verified.

## Evidence

Pending regeneration, bundle inspection, and installer verification.
