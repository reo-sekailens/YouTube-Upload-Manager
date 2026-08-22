# TASK016 — Windows taskbar icon alignment

**Status:** completed  
**Dependencies:** TASK001

## Objective

Ensure the packaged Windows executable and taskbar use the current blue upload-arrow application icon.

## Acceptance criteria

- `icons/icon.ico` is generated from the canonical `icons/icon.png` artwork.
- A fresh Windows NSIS installer embeds that icon.
- The installed application launches with the updated taskbar icon.

## Evidence

- `npx tauri icon src-tauri/icons/icon.png` regenerated `icons/icon.ico` and all
  platform variants from the canonical upload-arrow artwork.
- `npm run tauri -- build --bundles nsis` produced
  `src-tauri/target/release/bundle/nsis/YouTube Mass Uploader_0.1.4_x64-setup.exe`.
- The NSIS installer completed silently with exit code 0. The refreshed Start-menu
  shortcut targets the installed 0.1.4 executable; extracting that executable's
  associated icon yields the blue upload-arrow artwork. The app launched from that
  installed path successfully.
