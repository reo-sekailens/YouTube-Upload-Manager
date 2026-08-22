# TASK052: Nightly pre-release builds

## Status

completed

## Scope

Build and publish a replaceable nightly GitHub pre-release from every commit to `main`.

## Evidence

- The workflow builds Windows x64 NSIS plus a portable ZIP, Linux x64 DEB/AppImage plus a consistently named portable AppImage, macOS Apple Silicon/Intel app/DMG plus portable app-bundle ZIPs, and a universal Android debug APK. It then replaces the `nightly` GitHub pre-release with that commit's assets.
- Desktop assets are intentionally unsigned and unnotarized. The Android APK uses Android's disposable debug signing so it can be installed without a project keystore or signing secret. iOS is excluded pending a mobile project and Apple provisioning.
- Workflow syntax is based on Tauri's official GitHub Actions desktop matrix guidance. It has not run remotely yet.
