# TASK141 — Integrated reliability and usability improvements

## Status

completed (local source and browser scope)

Later amendment: TASK142 restores original-file uploads without new media
copies at the user's explicit request. The copy-related implementation and test
receipts below describe TASK141's earlier snapshot; channel, UX, and operations
improvements remain current.

## Scope

Review and improve native queue durability, immutable channel isolation,
operator-facing queue information, library workflow races, and local operations
checks. Parallel agents own separate native, media, frontend, and script files;
the coordinator owns integration, documentation, and rendered verification.

## Acceptance criteria

- New queued assets have durable device-local copies, and interrupted intake
  recovers after the safe startup shell without requiring another file drop.
- Existing managed media referenced by live work survives startup cleanup.
- Channel changes discard stale dashboard/library responses and stop sequential
  rename work from crossing into another channel.
- Queue size, concurrent upload rate, filtering, retry, and empty states are
  understandable, accessible, and bounded to the existing page size.
- Native operations checks reject successful commands that ran no matching test.
- Focused behavior tests, integrated frontend/native checks, and rendered light,
  dark, and compact-width screenshots document the actual evidence level.

## Findings

- AGENTS.md requires managed-media durability, while TASK113 and the current
  architecture describe original-source references. The active repository
  instruction takes precedence; restoration is part of this task.
- Dashboard refreshes can accept old-channel asynchronous responses.
- Queue size formatting reports sub-gigabyte videos as 0.0 GB, and transfer
  summary currently uses one worker's speed despite parallel upload execution.
- The native budget gate can pass when an exact test filter runs zero tests.

## Evidence

- New manual/watched intake persists the importing record before writes, verifies
  resumed prefixes, flushes the complete copy before rename/admission, and
  recovers a rename that completed before the SQLite commit. Cleanup retains all
  referenced copies and checkpoints, including failed/cancelled work.
- Legacy reference-only migration retains identity, verified digest, confirmed
  offset, channel binding, and protected session. Dispatch releases an old source
  volume claim before scheduling the new managed-media volume. Retry and queue
  admission reject another channel's immutable binding.
- Dashboard, preflight, duplicate, library, rename, and playlist responses are
  scoped to their requested channel/generation. Playlist events validate both
  immutable channel and playlist IDs. Sequential rename checks the connection
  before each item. Queue filters, sizes, retry eligibility, and parallel ETA
  have behavior tests; library/activity pages are bounded to 32 records.
- Integrated frontend suite: 24 files, 119 passed, zero failed. TypeScript and
  Tailwind checks pass. Production Vite webview build succeeds using an alternate
  local output path. Existing initial budgets pass at 228,468 B JS raw,
  71,443 B JS gzip, and 39,649 B CSS raw.
- Native library suite: 161 passed, zero failed, five ignored release benchmarks.
  Four exact native budget gates, ordinary Rust check, and formatting pass.
  Operations fixtures: 14 passed, zero failed, including zero/ignored test
  receipts and stalled streamed FFprobe downloads.
- Browser checks: 36 passed across nine workspaces, two widths (1440/390 px), and
  both themes; no root horizontal overflow, exactly one visible workspace, and
  no unnamed buttons. Combined queue search/filter/reset works in the fixture.
  Screenshots show actual components with synthetic records, not a real account.
- Detailed receipt paths and environment limits are recorded in
  [the certification record](../certification/TASK141-reliability-and-usability.md).

## Decisions and evidence boundary

- Current AGENTS.md overrides the former no-copy boundary. Keeping verified
  device-local copies increases local disk use; originals must remain unchanged
  until import completes. Older records whose originals are already missing
  cannot be reconstructed. Cancellation/failure retains local evidence.
- No live Google/YouTube mutation, deployment, installed-package certification,
  signing, operator profile, or global toolchain/configuration change is included.
- Default output/toolchain/sidecar junctions point to an unavailable drive.
  Validation used an existing Rust toolchain, isolated Cargo cache/target,
  alternate Vite output, and process-only Tauri configuration excluding sidecars
  and bundled resources. These results establish native source behavior and
  webview rendering; they do not certify ordinary packaging or launch.

## Follow-ups

- Prepare a verified sidecar in a healthy packaging environment, then certify an
  ordinary packaged launch and a disposable-profile forced-close recovery path.
- With explicit provider-operation scope, verify real OAuth, upload/resume,
  rename, playlist ordering, and connection-switch workflows against a test
  channel. Synthetic and native unit results cannot establish provider behavior.
- Preserve the small remaining initial gzip budget margin (237 B); do not raise
  budgets to accommodate future eager UI additions.
- Automatic approval review blocked deletion of the temporary Rust installation
  at `E:\Temp\yum-native-validation-20261009-ops\rustup` with reason
  `blocked by policy`. It remains outside the checkout (approximately 944 MiB).
