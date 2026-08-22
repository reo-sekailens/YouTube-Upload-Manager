# TASK049: Local deletion for remote-title pre-ingest matches

## Status

in-progress

## Scope

Allow a reviewed desktop source whose filename matches the active channel's persisted YouTube title inventory to use the same guarded local-file deletion and bulk-selection flow as a saved local-copy match.

## Acceptance criteria

- Uploaded-title match cards show the local deletion selection and action.
- The native command rechecks either a saved local match or the active channel's normalized uploaded-title match before issuing a deletion token.
- The external-path protection, typed filename confirmation, short-lived token, and pre-delete re-hash remain unchanged.

## Evidence

Pending implementation verification.
