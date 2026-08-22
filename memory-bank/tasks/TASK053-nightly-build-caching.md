# TASK053: Nightly build caching

## Status

completed

## Scope

Cache public build dependencies for nightly GitHub Actions builds without storing credentials, release artifacts, or application data.

## Evidence

- `actions/setup-node` caches npm's package-download cache by `package-lock.json`.
- `swatinem/rust-cache` restores Cargo registries, Git dependencies, and per-runner Rust build output.
- Android caches its fixed NDK 27.0.12077973 directory and Gradle dependencies/wrapper. Gradle cache keys include the npm and Cargo lockfiles.
- The first cache population still requires downloads; cache eviction or key changes safely fall back to a clean install.
