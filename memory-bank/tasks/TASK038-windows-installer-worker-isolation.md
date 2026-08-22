# TASK038 — Windows installer after worker isolation

## Status

completed

## Acceptance criteria

- Build an x64 NSIS installer from the current source.
- Verify the output exists and record its cryptographic hash.
- Do not claim code signing or live provider validation without evidence.

## Evidence

- Rebuilt `src-tauri/target/release/bundle/nsis/YouTube Upload Manager_0.1.9_x64-setup.exe` from the current workspace on 2026-08-22.
- Size: 4,638,687 bytes.
- SHA-256: `25D6D9383A6D876833A9200628D811D3A67EB33F45174B5DE73C0059E7D61AC5`.
- This installer is unsigned; installation and live provider operations were not run for this packaging task.
