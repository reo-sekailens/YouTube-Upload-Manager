# TASK077: Windows installer for upload-limit and light-dedupe release

## Status

completed

## Scope

Build and hash-verify the current Windows x64 NSIS and MSI installers.

## Acceptance criteria

- Tauri release build completes.
- NSIS and MSI installers exist under the release bundle directories.
- SHA-256 is recorded for handoff.

## Evidence

- `npm run tauri -- build --bundles nsis,msi` completed successfully on
  2026-08-23. Both x64 installers are unsigned:
  - `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,439,659 bytes;
    SHA-256 `4B0C9F384D4C868652CBF9562369DFA3C4E2A77490637FF9C0CE24CB5D92B31A`.
  - `YouTube Upload Manager_0.1.9_x64_en-US.msi` — 36,286,464 bytes;
    SHA-256 `6786EA4D9D9C258B70B5BED213A59EFEDCCF6D965D3E78E53DED61546CEDB4CE`.
- The repaired watched-folder inventory-error handling, actionable diagnostic
  categories, OAuth channel-verification reporting, serialized inventory
  refreshes, and stale-staging cleanup were packaged as a new unsigned x64
  NSIS installer on 2026-08-23:
  `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,441,771 bytes;
  SHA-256 `1D7157C251ACB51551F2E4C1328CC770AFD1E42299D3DCEA2C32E75DD3439699`.
- The WAL-backed local library commit repair was packaged as a new unsigned
  x64 NSIS installer on 2026-08-23:
  `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,457,565 bytes;
  SHA-256 `357258F2D8F7604C118248B51324E2E684E2C06CB9ECFCEA3C4173FBDF5A5F6B`.
- The fresh-connection, channel-scoped inventory-commit repair was packaged as
  a new unsigned x64 NSIS installer on 2026-08-23:
  `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,456,324 bytes;
  SHA-256 `D6E9CE2A327D78A088F48CEB37506FF181618A99AF601346BEFEC035DAB4DB48`.
- The mandatory legacy inventory-schema upgrade and native error-display repair
  were packaged as a new unsigned x64 NSIS installer on 2026-08-23:
  `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,477,322 bytes;
  SHA-256 `E127331D8CD965D314FC62DD1374F543FF3799C995276E2EA4093FB49F257E46`.
- The duplicate YouTube playlist-entry inventory repair was packaged as a new
  unsigned x64 NSIS installer on 2026-08-23:
  `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,474,987 bytes;
  SHA-256 `A5657214A4C9443812670BC707FFE401BCB0E84AE8C3B1D00F80DA4B4003E6A4`.
- The automatic watched-folder handoff and three-day source-volume capacity
  scheduler were packaged as unsigned x64 installers on 2026-08-23:
  - NSIS `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,482,990 bytes;
    SHA-256 `568BFD293EC6CEAA716DDA0855085B1A4A68B034E567FE141FFC548E69D04078`.
  - MSI `YouTube Upload Manager_0.1.9_x64_en-US.msi` — 36,323,328 bytes;
    SHA-256 `11F3B0A882F67000379D810B075F90C50809FFA0BFF061EC726EEE15CB06D794`.
- The automatic-queue-dispatch release, which removes the global manual start
  button, was packaged as unsigned x64 installers on 2026-08-23:
  - NSIS `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,485,890 bytes;
    SHA-256 `695C9BAF2F1714C3CD0A4BF94F29D169AEB8297B73BFA9D82956A1020CD5E27A`.
  - MSI `YouTube Upload Manager_0.1.9_x64_en-US.msi` — 36,323,328 bytes;
    SHA-256 `ACD8D9AD630A6F72EAD0B5A5424C59B11678033908773C370825792EC8232C87`.
- The responsive pre-ingest duplicate-review release was packaged as unsigned
  x64 installers on 2026-08-23:
  - NSIS `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,474,492 bytes;
    SHA-256 `78643691FB317758A3452D5BA340376226940385A22F733FA375106CFCC4CF06`.
  - MSI `YouTube Upload Manager_0.1.9_x64_en-US.msi` — 36,323,328 bytes;
    SHA-256 `2098AFA676947527D802741CE6D79C1B92081CFD2E8FE9A13C56A4CF9E62E99A`.
- The first-open Connected account handoff was packaged as unsigned x64
  installers on 2026-08-23:
  - NSIS `YouTube Upload Manager_0.1.9_x64-setup.exe` — 26,474,639 bytes;
    SHA-256 `1308E03E337F0AD66100197CB2996AC6A79630C0B1DA48C287A9A94EA16C3C29`.
  - MSI `YouTube Upload Manager_0.1.9_x64_en-US.msi` — 36,323,328 bytes;
    SHA-256 `ACF1AA8F406DD15FFE8EFE7A5AA2669C19D92BF885DFE3DAF4EBA6BED5FF0FC7`.
