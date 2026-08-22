# TASK078: Watched-folder reference-in-place uploads

## Status

completed

## Scope

Make watched-folder intake hash and upload the stable source file in place
without creating a full managed-media copy. Keep the source-revalidation and
post-confirmation cleanup safeguards explicit.

## Acceptance criteria

- A stable watched file is SHA-256 hashed and signature-rechecked before a
  queue record is created.
- The queued item's source and upload path refer to the watched file, with no
  partial or managed-media copy created.
- The folder-monitor UI clearly states the in-place transfer mode and the
  requirement to keep the file available until YouTube confirms the upload.
- Existing legacy interrupted managed imports remain recoverable.

## Evidence

- The watched-folder queue record stores the watched source as both its source
  and transfer path, with no partial path or managed-media file.
- `folder_monitor_automatically_processes_existing_stable_files` now asserts
  the direct reference, empty managed-media directory, and source-reference
  audit event.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 51 tests; `npm test` passed: 35 tests; `npm run build` passed.
- Browser preview of the Folder monitor tab showed the direct-source disclosure
  and post-confirmation cleanup boundary with no browser console warnings or
  errors.
