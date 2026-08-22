# TASK054: Android nightly entry point

## Status

in-progress

## Scope

Make the Tauri native library export the Android mobile entry point required for APK packaging.

## Evidence

- The first Android nightly build compiled the native library but Gradle rejected it because the library lacked Tauri runtime symbols.
- `run()` now uses Tauri's conditional `mobile_entry_point` attribute, which has no effect on desktop builds and exports the required Android runtime symbols on mobile targets.
- A follow-up GitHub Actions nightly run is pending to exercise the Android SDK/NDK packaging path.
