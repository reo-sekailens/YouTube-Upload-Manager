# TASK010 — Windows startup stack overflow

**Status:** completed  
**Owner:** unassigned  
**Dependencies:** TASK009

## Objective

Prevent the packaged Windows application from crashing during automatic startup queue reconciliation.

## Root cause

Windows Error Reporting recorded exception `0xc00000fd` two seconds after launch. The matching release dump and PDB resolve the fault to `reconcile_queue_impl`; release inlining pulled `digest_file`'s 1 MiB stack buffer into the main-thread frame and exhausted the Windows GUI thread stack before reconciliation could run.

## Work items

- [x] **TASK010-A — Heap-backed digest buffer:** Move the startup digest read buffer off the stack without weakening streaming SHA-256 verification.
- [x] **TASK010-B — Small-stack regression:** Exercise startup reconciliation on a constrained thread stack so future release-shape regressions fail locally.
- [x] **TASK010-C — Package and launch proof** *(depends on TASK010-A, TASK010-B)*: Run focused checks, rebuild the NSIS installer, install it, and verify the installed process remains running without a new Application Error event.

## Acceptance criteria

- Startup reconciliation no longer reserves an approximately 1 MiB native stack frame.
- Existing import, queue, duplicate, folder-monitor, and channel-safety tests remain green.
- The installed x64 application remains running after launch and no new `0xc00000fd` event is recorded.
- The replacement installer hash and actual installed-launch evidence are recorded.

## Evidence

- Windows Application Error events at 06:15:01 and 06:15:11 recorded `youtube-mass-uploader.exe` exception `0xc00000fd` with two-second process uptime.
- `cdb` loaded the matching release dump/PDB and resolved `__chkstk` to `youtube_mass_uploader_lib::reconcile_queue_impl`, with an approximately `0x100468`-byte stack probe.
- Both 1 MiB streaming SHA-256 buffers are heap-backed; `startup_reconciliation_resumes_local_copy_on_a_small_native_stack` passes on a 512 KiB thread stack.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` and `cargo test --manifest-path src-tauri/Cargo.toml -j 1` passed: 11 tests.
- `npm run check`, `npm run test`, and the Tauri production build passed: 9 web tests and a fresh x64 NSIS bundle.
- Replacement installer SHA-256: `811EFFC1E7EA929DB60B55F6AB1591980B2AE544CE37FDBA3BD2536EC53B58A0`.
- Silent replacement install exited 0. The installed process remained alive for 10 seconds and produced zero new Windows Application Error events before the verification process was stopped.
