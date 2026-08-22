# TASK095: Source-volume upload concurrency

## Status

completed

## Scope

Dispatch queued manual, batch, folder, and dropped uploads according to the
safe concurrency currently available to each physical source volume.

## Acceptance criteria

- A watched folder begins an eligible upload automatically without claiming its
  entire queue as active work.
- Queued files from a source volume wait when that volume is already at its
  cached safe capacity, then are reconsidered when a worker finishes.
- Disk capability is measured only when a source volume is first encountered
  or its cached result is older than three days.
- Connection capability is recorded from confirmed YouTube transfer throughput
  no more often than every three days; no third-party speed-test service is
  contacted.
- Scheduling applies to every persisted upload type and preserves atomic
  queue claiming.

## Evidence

- `start_queued_uploads_impl` considers every queued `upload_items` row,
  groups active and candidate jobs by source-volume identifier, and claims only
  candidates below that volume's current limit.
- `upload_disk_capabilities` and `upload_connection_capability` persist the
  device-local, three-day measurements. The disk sample reads at most 8 MiB;
  the connection result is learned from YouTube-confirmed progress rather than
  exposing upload data to a speed-test provider.
- A full source volume returns a successful zero-dispatch result, so recurring
  folder scans keep the file honestly queued instead of reporting a false
  failure. Completion and reconciliation hand the scheduler forward.
- Focused scheduler-capacity and atomic-claim native tests passed, followed by
  `cargo check` and whitespace-diff validation.
