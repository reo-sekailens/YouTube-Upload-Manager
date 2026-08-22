# TASK074: Watched-folder automatic intake and post-upload cleanup

## Status

completed

## Scope

Remove the watched-folder starting baseline so all stable supported files are
automatically queued, while separating post-upload original-file deletion into
an after-upload confirmation and a distinct opt-in automatic-cleanup choice.

## Acceptance criteria

- Existing and newly added eligible files use the same two-scan stability gate,
  then queue and dispatch automatically without a Process existing files action.
- A manual original-file deletion prompt appears only after YouTube confirms an
  upload and requires the exact filename.
- Opting into automatic original cleanup remains available before processing and
  deletes only after the upload receipt and source revalidation.
- Native cleanup rejects anything not already uploaded.

## Evidence

- Enabling a monitor no longer records eligible files as a non-uploading
  baseline. Existing and new files first become observed, then move to native
  intake after the next unchanged scan.
- Existing legacy baseline observations migrate into that observed path during
  a scan, so no manual action is needed after upgrading.
- The queue shows a typed **Delete original** confirmation only for completed
  uploads without automatic cleanup. Native cleanup rejects non-uploaded items
  and wrong filenames, then reuses the guarded digest/path verification.
- Automatic cleanup remains an explicitly labelled pre-processing option and
  only runs after a confirmed YouTube receipt.
- Focused native auto-intake and cleanup tests, frontend native-boundary test,
  TypeScript check, formatting, and whitespace diff check passed.
