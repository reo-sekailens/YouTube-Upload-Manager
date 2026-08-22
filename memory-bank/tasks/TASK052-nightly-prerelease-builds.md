# TASK052: Nightly pre-release builds

## Status

in-progress

## Scope

Build and publish an incrementing lettered nightly GitHub pre-release from every commit to `main`.

## Evidence

- The workflow allocates the next SemVer-aligned pre-release tag from GitHub before building: `v0.1.9-nightly.a` through `v0.1.9-nightly.z`, then `v0.1.9-nightly.aa`, and so on. That tag is compiled into the app's release-channel metadata and used to create a new immutable GitHub pre-release rather than replacing the previous one.
- The workflow builds Windows x64 NSIS plus a portable ZIP, Linux x64 DEB/AppImage plus a consistently named portable AppImage, macOS Apple Silicon/Intel app/DMG plus portable app-bundle ZIPs, and a universal Android debug APK. It downloads artifact groups separately, selects only installable/package files, and assigns each release upload a unique artifact-prefixed name.
- Desktop assets are intentionally unsigned and unnotarized. The Android APK uses Android's disposable debug signing so it can be installed without a project keystore or signing secret. iOS is excluded pending a mobile project and Apple provisioning.
- The 2026-08-22 nightly run first failed because the `release-assets/**` glob supplied the downloaded Android `universal` directory to `gh release create`; filtering now selects only package files. The subsequent run found duplicate bundled `youtube-upload-manager.png` names; separated artifact downloads and explicit unique labels now prevent duplicate assets. A remote rerun is pending.
