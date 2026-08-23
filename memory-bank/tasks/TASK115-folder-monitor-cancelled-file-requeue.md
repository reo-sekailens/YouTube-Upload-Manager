# TASK115: Folder-monitor cancelled-file requeue

## Status

completed

## Scope

Expose cancelled watched-folder uploads in their own bounded view and let the
operator safely requeue one eligible file or all eligible files without
overriding duplicate-safety cancellations or crossing the active channel.

## Acceptance criteria

- The Folder monitor tab identifies eligible cancelled watched-folder files.
- The operator can requeue one file or all eligible files.
- Native requeue remains channel-scoped, preserves resumable checkpoints, and
  resumes automatic dispatch.
- Duplicate and hash-safety cancellations cannot be requeued from this view.
- Source paths do not cross into the webview.

## Evidence

- The bounded native overview now exposes only an opaque local item identifier
  alongside the existing filename-only activity projection.
- `requeue_cancelled_folder_monitor_files` only restores cancelled jobs bound to
  the active watched-folder channel, even when discovery is later disabled. It
  excludes duplicate, rejected, and failed-integrity observations, retains
  resumable checkpoints, wakes pending BLAKE3 verification, and requests the
  automatic capacity-aware dispatcher.
- Operator-cancelled records that retain a stale `background_hash_status =
  failed` are no longer incorrectly treated as permanent duplicate failures.
  If their original local source can be safely reopened, requeue records a
  fresh size/modified signature, clears the old digest, and restarts BLAKE3 in
  the background; unavailable or unsafe sources remain cancelled.
- A requeue is durable before dispatch is attempted. A transient scheduler wake
  therefore leaves the file queued for automatic recovery rather than falsely
  reporting that it could not be added back.
- Native string rejections now reach the operator as their actionable message
  instead of the generic requeue failure notice.
- The Folder monitor provides a collapsed cancelled-files view with individual
  **Queue again** actions and a bulk **Queue all N again** action.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`
  passed: 131 passed, 0 failed, 5 ignored. `npm test`, `npm run build`, and
  `git diff --check` passed.
- Rendered fixture QA at `http://127.0.0.1:1420/` exercised the bulk requeue
  interaction: two cancelled records became the zero-item state and the notice
  confirmed automatic resume. The browser fixture is UI proof only; it does
  not represent a live YouTube operation.
