# TASK110: Media probe and build pipeline

## Status

proposed

## Objective

Remove redundant FFprobe, hash, copy, and provisioning work from development,
builds, startup, and intake while keeping media validation and packaged sidecar
integrity explicit.

## Scope

- Stream FFprobe checksum verification and persist a local provenance receipt
  keyed by binary identity, checksum, license, target, and preparation version.
- Make a valid cached preparation network-free, skip mobile targets entirely,
  and select one desktop architecture unless universal packaging is explicit.
- Bound all runtime FFprobe processes and output readers through the shared
  native resource scheduler.
- Add a duration-only probe for upload-limit validation; reuse richer metadata
  only for the same stable file signature.
- Measure single-pass copy-plus-hash and safe platform clone/offload options
  without weakening managed-workspace or watched-source guarantees.
- Include the correct sidecar and license in installer and portable artifacts
  and validate their target architecture.

## Acceptance criteria

- Repeated preparation of a verified cache reads no network resource and avoids
  a full binary hash unless provenance inputs changed.
- Mobile development/build performs zero desktop FFprobe provisioning work;
  ordinary desktop preparation produces only the selected architecture.
- Runtime FFprobe/hash/copy concurrency is bounded, cancellable, and lower
  priority than an active upload where the source volume is shared.
- Duration and supported-format decisions remain identical on the media fixture
  corpus, including malformed and oversized provider output.
- Copy/hash throughput and responsiveness meet TASK103 budgets without asking an
  operator to reselect an already queued asset after interruption.
- Fresh Windows installer and portable artifacts contain the verified FFprobe
  binary and license; unavailable platform packaging is recorded separately.

## Dependencies

TASK103.

## Affected areas

scripts/prepare-ffprobe.mjs, Tauri build hooks/configuration, native media probe,
ingest/hash/copy scheduler, packaging workflow, and media fixtures.
