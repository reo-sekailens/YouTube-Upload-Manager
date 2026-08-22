# TASK059: Hidden FFprobe and fast light dedupe

## Status

completed

## Scope

Prevent the bundled Windows FFprobe sidecar from opening or focusing console
windows. Keep light matching responsive while retaining FFprobe metadata: match
results are stored first, then each file is enriched and cached once on a
separate native worker.

## Evidence

- Windows FFprobe child processes use `CREATE_NO_WINDOW`.
- Per-scan metadata is persisted locally and resumes after an interruption.
- Light result reloads use cached metadata or basic file facts; they never
  launch FFprobe themselves.
- Rust format, 39 native tests, TypeScript check, and diff check passed.
