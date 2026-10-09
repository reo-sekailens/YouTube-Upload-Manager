# TASK141 — Local reliability and usability evidence

Verified 2026-10-09. Changes remain in the local working tree. This record covers
source tests, operational fixtures, and browser rendering; it does not assert a
packaged launch, signing, live YouTube results, or physical power-loss durability.

TASK142 subsequently restores original-file uploads without new media copies
under the user's explicit storage constraint. This record certifies the earlier
TASK141 snapshot; current copy behavior and current-source validation are in
TASK142. Other channel, UX, and operations changes are retained.

## Findings and resulting behavior

| Finding | Implemented result |
| --- | --- |
| Reference-only intake could lose queued work when an original moved; startup cleanup could remove retained media. | Persist importing rows before writes; stream and verify bounded resumable copies; flush/rename before admission; recover interrupted imports after the safe shell; retain every referenced media/checkpoint. |
| Earlier asynchronous reads could populate the newly selected channel. | Fence dashboard and library responses with immutable channel/generation, clear channel-owned projections, scope playlist events and rename activity, and check connection before sequential rename items. |
| Sub-gigabyte queue items displayed as 0.0 GB; progress used one worker despite parallel execution. | Appropriate size units, status counts/search, explicit eligible retry and reconciliation guidance, bounded progress, and ETA based on all measured active workers. |
| Large library/activity lists and weak error states made routine work harder. | 32-record pages, keyboard tab controls, actionable errors/retry, and picker/OAuth lifecycle guards. |
| A native gate could pass after Cargo ran no matching tests; a streamed dependency download could hang. | Require the exact named passing test plus a one-pass/zero-fail/zero-ignore receipt; deadline the complete download, safely cancel stalled bodies, preserve existing destinations, and remove partial downloads. |
| The baseline initial gzip budget was already exceeded. | Load ready-workspace presentation and panel CSS after the safe shell. Preserve all existing limits; avoid adding eager failure-formatting code. |

## Completed validation

| Check | Result |
| --- | --- |
| TypeScript | Passed |
| Vitest | 24 files, 119 passed, zero failed |
| Native library, performance-harness | 166 discovered; 161 passed, zero failed, five intentionally ignored release benchmarks |
| Exact native query/worker gates | Four executed and passed; each exact filter ran its named test |
| Rust check and formatting | Passed; existing dead-code warnings remain |
| Offline operations regressions | 14 passed, zero failed |
| Tailwind source-ownership guard | Passed |
| Production Vite webview build | Passed with local `output/task141-build` |
| Initial bundle budgets | 228,468 B JS raw / 71,443 B gzip / 39,649 B CSS raw; limits unchanged at 240,640 / 71,680 / 40,960 B |
| Browser workspaces | 36 checks passed: all nine tabs at 1440/390 px and light/dark themes; no root overflow, one visible workspace, no unnamed buttons |
| Combined queue search/status filter | No-match state and reset verified in browser |
| Keyboard tab navigation | Four browser checks passed: workspace ArrowDown/Home/End and playlist ArrowRight |

The HEAD source snapshot's initial bundle measured 230,768 B JS raw,
72,196 B gzip, and 40,888 B CSS raw. Its gzip exceeded the existing limit.
The final bundle passes with 237 B of remaining initial gzip margin.

## Local receipts

- `output/task141-native-validation.json`: source SHA-256 receipts, toolchain,
  process-only configuration, native and operations results.
- `output/task141-native-tests.log`: complete native test receipt.
- `output/task141-native-budget-check.log`: four exact gate receipts.
- `output/performance/task141-frontend.json`: final webview assets and budgets.
- `output/performance/task141-before.json`: HEAD snapshot bundle comparison.
- `output/playwright/task141-browser-check.log`: browser checks and exact fixture
  traversal. Initial failed fixture setup attempts are separate from this final
  successful traversal; the browser records use synthetic queue/channel data.
- `output/playwright/task141-keyboard-check.log`: final successful keyboard checks.
- `output/playwright/task141-queue-dark.png`: desktop queue detail.
- `output/playwright/task141-batch-uploads-light-390.png`: compact light queue.
- `output/playwright/task141-*-light-1440.png` and corresponding dark/390 files:
  batch, transfer, rename, playlist, and connection screenshots.

## Environment and remaining limits

The existing `dist`, Rust/Cargo cache, and sidecar junctions target an unavailable
drive. They were preserved. The Vite build used an alternate output directory.
Native checks used Rust 1.98.0 from an existing toolchain, isolated Cargo storage,
two build workers, and process-only Tauri overrides for the frontend path and
empty sidecar/resource lists. No lockfile, deployment configuration, global
toolchain configuration, or operator profile was changed. This environment
cannot establish an ordinary FFprobe-enabled package launch.

Managed copies consume additional disk space. Originals must remain unchanged
until import completes; an original already lost by an older no-copy release
cannot be reconstructed. Failed/cancelled copies and checkpoints are retained
for safe recovery, consistent with the queue-clear evidence boundary.

Next evidence levels are a prepared sidecar and packaged disposable-profile
forced-close smoke, signed/cross-platform packages where required, and explicitly
scoped live Google/YouTube journeys. No current local result substitutes for them.

Automatic approval review rejected removal of the temporary Rust installation
at `E:\Temp\yum-native-validation-20261009-ops\rustup` with reason
`blocked by policy`. Approximately 944 MiB remains outside the checkout.
