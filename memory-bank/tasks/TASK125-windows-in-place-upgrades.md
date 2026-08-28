# TASK125: Windows in-place upgrades

## Status

completed

## Scope

Keep the Windows NSIS install identity scoped to the current user so an update
replaces the installed application without requiring an operator uninstall.
Preserve the device-local app-data location and reject accidental downgrades.

## Evidence

- `bundle.windows.nsis.installMode` is explicitly `currentUser`, matching the
  prior installer default and stable product name/identifier.
- `bundle.windows.allowDowngrades` is `false`, so an older installer cannot
  replace a newer installed version.
- Local-only installers use an explicit SemVer prerelease track:
  `1.0.2-nightly.7`. This keeps them visibly separate from release artifacts
  while remaining newer than the preceding local `1.0.1` package for in-place
  Windows upgrades. Artifact: `YouTube Upload Manager_1.0.2-nightly.7_x64-setup.exe`,
  26,781,769 bytes, SHA-256
  `A80D7D6C8DB75DC829D05BF6FCFD21B01F28B388089F6B763AE8D9F5367005AD`.
