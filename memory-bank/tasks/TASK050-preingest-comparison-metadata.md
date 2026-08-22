# TASK050: Pre-ingest comparison metadata

## Status

completed

## Scope

Expose title and video-length comparison values by default for local-to-YouTube pre-ingest title matches, with secondary remote metadata collapsed.

## Evidence

- Remote title-match results now carry duration, privacy status, and local inventory-sync timestamp instead of title strings alone.
- Both sides show title and length by default. Local duration is explicitly shown as unavailable before ingest because arbitrary files, including INSV and LRV, cannot be safely decoded without introducing a codec-dependent ingestion step.
- Remote privacy and inventory-sync metadata are contained in a collapsed disclosure.
- Rust format, 31 Rust tests, 31 web tests, TypeScript check, production build, cargo check, and diff check passed. Browser preview cannot create native pre-ingest results, so populated-card visual QA requires a signed-app fixture.
