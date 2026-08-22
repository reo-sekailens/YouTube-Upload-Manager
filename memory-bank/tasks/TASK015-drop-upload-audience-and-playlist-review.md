# TASK015 — Drop upload audience and playlist review

## Status

`completed`

## Outcome

Every manual drag/drop or picker intake pauses before device-local import for a
batch review that requires the operator to declare Made for Kids, choose video
visibility, and select an owner-authorized YouTube playlist or no playlist.

## Boundaries

- The review occurs before any selected source is copied into the managed local
  workspace.
- Choices are stored only with the device-local upload item and are sent only
  from the native upload worker.
- Watched-folder automation remains private, has no playlist assignment, and
  is never covered by the manual drop review.
- A playlist insertion failure cannot erase or misreport a successfully
  uploaded video; it is recorded as a separate local audit outcome.

## Acceptance criteria

- Manual intake cannot continue until Made for Kids has an explicit answer.
- The review visibly requests visibility and playlist for every manual batch.
- The native upload request sends the made-for-kids declaration and, when a
  playlist was selected, records its insertion outcome after upload.
- Local types, Rust schema/migrations, focused tests, build, and UI QA pass.

## Evidence

- `npm run check`, `npm run test` (14 tests), `npm run build`, `cargo fmt
  --check`, `cargo test` (14 Rust tests), and `git diff --check` passed.
- The webview sends the review choices through the native import command; Rust
  persists them locally, sends `selfDeclaredMadeForKids` in the resumable
  upload metadata, and records playlist insertion success or failure after an
  upload returns a video ID.
- Browser preview cannot provide native drag/drop paths, a connected channel,
  or an authorized playlist list. The live playlist API result remains
  unverified.
