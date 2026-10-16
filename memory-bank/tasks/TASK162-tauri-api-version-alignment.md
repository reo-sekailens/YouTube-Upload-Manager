# TASK162 — Align Tauri JavaScript and Rust versions

**Status:** completed (local repair and verification)

**Owner:** Codex

**Date:** 2026-10-16, Asia/Singapore

**Dependencies:** TASK148, TASK158

## Failure and scope

The desktop build's Tauri version gate rejects Rust `tauri` 2.12.2 paired with
locked `@tauri-apps/api` 2.11.1. TASK158's notification dependency required the
Rust upgrade, but the npm lockfile was not advanced with it. Ordinary frontend
builds and direct Cargo checks did not exercise this cross-language CLI gate.

Align the JavaScript API and existing Rust core on minor 2.12, bound both
declarations to that minor, and guard declared/locked versions before frontend
production builds. Preserve independently versioned CLI/plugins and the existing
Rust dependency graph. No application UI, account, queue, source or checkpoint
behavior changes are intended.

## Acceptance and evidence

- [x] Reproduce the reported failure with the actual installed Tauri CLI.
- [x] Update the npm API package and lockfile; constrain both core manifests to
  minor 2.12 without disabling Tauri's compatibility validation.
- [x] Dependency guard rejects the original mismatch and declaration/lock drift,
  while allowing compatible patch differences.
- [x] A clean npm install, TypeScript, frontend tests/build and unchanged bundle
  budgets pass. Rust lockfile remains unchanged and locked Cargo checks pass.
- [x] The real Tauri CLI passes its version gate after the fix. Distinguish that
  proof from a complete installer build.
- [x] Correct the two additional confirmed verification failures from the same
  integration commit without raising performance budgets, and validate the
  portable path fixture and production-only worker count.

## Ownership and current result

Root owns manifests, lockfiles, build integration, documentation and final
verification; the parallel packet owns the two dependency guard script files.

The before-fix CLI run stops at the exact `2.12.2 : 2.11.1` mismatch before the
controlled build hook. Evidence directory:
`C:/Users/Workstation/.codex/visualizations/2026/10/12/01a12e98-b299-7c70-9152-bb18feb05abf/tauri-alignment-20261016/`.

The npm API now resolves to 2.12.4 (`~2.12.4`); Rust remains locked at 2.12.2
(`~2.12.2`). Compatible patch differences are permitted by Tauri's documented
major/minor rule. CLI 2.11.4 and independently versioned plugins are unchanged;
`Cargo.lock` is unchanged. The dependency guard runs before production frontend
builds, in the operations suite, and has its own CI test step. Native verification
now prepares FFmpeg and its notices, already required by `tauri.conf.json`, as
well as FFprobe before Cargo builds.

Local evidence so far:

- The actual Tauri CLI reproduces the original mismatch in `cli-before.log`.
  After alignment, `cli-after.log` reaches `TAURI_VERSION_GATE_PASSED` and a
  deliberately stopped hook (exit 73). No mismatch-ignore flag is used. This
  checks the real compatibility gate, not installer completion.
- Fresh `npm ci` installs 121 packages and resolves API 2.12.4 in an isolated
  fixture (`clean-install-result.json`). An initial attempt failed because the
  configured E: npm cache volume was full; using a fresh C: task-local cache
  passes. The failed fixture was retained when automatic cleanup review blocked
  its removal; no existing caches or user files were deleted.
- 15 version-guard cases pass, including the reported failure, compatible
  patch differences, broad range rejection, ambiguous records, malformed inputs
  and actual CLI exit codes. Operations: 55 pass, one platform skip.
- 485 frontend tests, strict TypeScript and production Vite build pass. Initial
  payload is 228,604 raw JS bytes / 71,680 gzip bytes / 40,346 raw CSS bytes;
  unchanged budgets pass. No UI source changes are included.
- `cargo check --offline --locked` and Rust formatting pass through the existing
  native harness, preserving the Rust dependency graph. Existing native warnings
  remain. Local resource overrides bypass the unavailable F: sidecar junction;
  that junction is untouched. This is not packaged application verification.

The prior main verification run also proves an Ubuntu path-fixture failure and
a false production-thread count from a named `#[cfg(test)]` module. Both repairs
are integrated:

- Cargo target fixtures now derive native absolute paths and preserve default,
  relative-to-working-directory and absolute semantics. A separate Windows-only
  case retains cross-drive coverage. Targeted suite: nine pass on Windows;
  actual Ubuntu execution remains CI evidence.
- The worker guard removes balanced test-only modules regardless of name,
  masks Rust literals/comments, and retains later production declarations.
  Four regression cases prove named test exclusion, later production thread
  rejection, literal/comment handling and unclosed-module fallback. Eight
  focused cases and five Cargo receipt cases pass. The real configuration gate
  passes unchanged upload/resident/direct-spawn limits: 4/4, 1/1 and 1/1.
- Final integrated frontend run: 485/485 pass. Whitespace checks pass. Runtime
  source and application UI are unchanged by these verification repairs.

Main handoff uses the user's existing commit/push authorization. Hosted
verification and multi-platform nightly packaging must be assessed on that
new revision; local compatibility success is not a completed hosted release.
No credentials, personal media or installed application state is involved.

Reference: [Tauri dependency update guidance](https://v2.tauri.app/develop/updating-dependencies/).
