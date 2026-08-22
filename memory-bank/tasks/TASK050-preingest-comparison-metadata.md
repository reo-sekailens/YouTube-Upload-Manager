# TASK050: Pre-ingest comparison metadata

## Status

completed

## Scope

Expose title and video-length comparison values by default for local-to-YouTube pre-ingest title matches, with secondary remote metadata collapsed.

## Evidence

- Remote title-match results now carry duration, privacy status, and local inventory-sync timestamp instead of title strings alone.
- Both sides show title and length by default. Local source facts now include
  file type, byte size, modified time, and a non-ingesting ISO-BMFF container
  duration for MP4, MOV, M4V, INSV, and LRV when the source exposes one.
  Unsupported or unreadable containers stay clearly marked as unavailable.
- Remote privacy and inventory-sync metadata are contained in a collapsed disclosure.
- Rust format, 31 Rust tests, 31 web tests, TypeScript check, production build, cargo check, and diff check passed. Browser preview cannot create native pre-ingest results, so populated-card visual QA requires a signed-app fixture.
- Local metadata is derived in the native layer and returns no source path to the
  webview. A fixture test verifies an MP4 `mvhd` duration is read without
  ingesting or decoding media.
- When the on-device `ffprobe` utility is available, the panel now exposes the
  complete format fields, tags, and stream fields in a collapsed disclosure for
  any container it recognizes; basic file facts and ISO-BMFF duration remain
  available without it.
