# TASK089 — Bounded FFprobe output

## Status

completed

## Objective

Enforce the FFprobe output byte limit while reading, before memory accumulation
or JSON parsing.

## Acceptance criteria

- Oversized output is terminated and handled safely.
- Metadata probing retains its timeout and bounded normal result.
- Regression coverage proves the byte cap is pre-buffer.

## Evidence

- FFprobe stdout is now read through a 2 MiB bounded reader in a worker thread;
  an over-limit result terminates the child before JSON parsing.
- The existing 15-second process deadline and non-success exit handling remain.
- `tests::ffprobe_output_limit_is_enforced_while_reading` passes.
