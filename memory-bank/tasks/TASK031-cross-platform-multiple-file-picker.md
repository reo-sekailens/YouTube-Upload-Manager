# TASK031 — Cross-platform multiple-file picker

## Status

`completed`

## Outcome

Both manual intake and pre-ingest duplicate review request multi-file document
selection on desktop, Android, and iOS. Picker results preserve every returned
desktop path, Android content URI, or iOS file URI for one native operation.

## Evidence

- A focused unit test covers multi-file, single-file, and cancelled picker
  results.
- Type check, web tests, production build, and diff check passed.
