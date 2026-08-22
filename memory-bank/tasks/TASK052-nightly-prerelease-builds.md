# TASK052: Nightly pre-release builds

## Status

in-progress

## Scope

Build and publish a replaceable nightly GitHub pre-release from every commit to `main`.

## Evidence

- The workflow builds Windows x64 NSIS plus a portable ZIP, Linux x64 DEB/AppImage plus a consistently named portable AppImage, macOS Apple Silicon/Intel app/DMG plus portable app-bundle ZIPs, and a universal Android debug APK. It replaces the `nightly` GitHub pre-release with the downloaded asset files only.
- Desktop assets are intentionally unsigned and unnotarized. The Android APK uses Android's disposable debug signing so it can be installed without a project keystore or signing secret. iOS is excluded pending a mobile project and Apple provisioning.
- The 2026-08-22 nightly run reached publication but failed because the `release-assets/**` glob supplied the downloaded Android `universal` directory to `gh release create`. The publish step now gathers only regular files with `find -type f`; a remote rerun is pending.
