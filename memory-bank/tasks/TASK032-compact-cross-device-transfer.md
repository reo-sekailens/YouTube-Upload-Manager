# TASK032 — Compact cross-device transfer

## Status

`completed`

## Outcome

The Export and import workspace transfers gzip-compressed duplicate metadata:
local SHA-256 records and synchronized YouTube inventory. Imports merge as
metadata-only records so they cannot queue or upload without the original media.

## Security and size boundaries

- Archives omit managed media, source/workspace paths, refresh tokens, OAuth
  client secrets, resumable session URLs, monitor configuration, and audit logs.
- Desktop OAuth JSON is imported through its own picker into protected local
  storage; the receiving device must authorize YouTube again.
- The native importer limits compressed and expanded archive sizes to 16 MiB.

## Evidence

- Native test exports a sub-2 KiB fixture, imports hash and remote-inventory
  records, and confirms imported items remain metadata-only.
- Type check, 26 web tests, production build, diff check, and browser-preview
  UI verification passed.
