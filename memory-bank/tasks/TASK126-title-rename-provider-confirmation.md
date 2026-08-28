# TASK126: Title rename provider confirmation

## Status

completed

## Objective

Verify every accepted YouTube title update with a second authenticated read of
the same active-channel video, and expose the native failure detail in the
rename activity log.

## Acceptance criteria

- A title is marked renamed only after YouTube's follow-up read returns the
  requested title and the active channel ID.
- A rejected or unconfirmed change records a safe local audit event and leaves
  later changes pending.
- The webview retains the actionable native error text rather than replacing a
  Tauri string rejection with a generic message.

## Evidence

- The rename worker uses the active management credential to re-read each
  updated video from YouTube before changing its local inventory projection.
- Focused native confirmation tests and title-job string-error tests pass;
  live provider confirmation will occur for each operator-selected title in
  the packaged app.
- Built unsigned installer `YouTube Upload Manager_1.0.2-nightly.4_x64-setup.exe`
  (SHA-256 `19A9AB50952E4BE3E9CADDDAE32A9D23012DD002C9F3B5A47E0BED689432E480`).
