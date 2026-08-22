# Task Index

This is the source of truth for planned work. Keep one row per task and link the task file when created.

| ID | Title | Status | Owner | Dependencies | Notes |
| --- | --- | --- | --- | --- | --- |
| [TASK001](TASK001-cross-platform-foundation.md) | Cross-platform local-first foundation | in-progress | unassigned | — | Tauri 2 application foundation, device-local queue, and dashboard implementation underway; Google OAuth and live provider work remain pending. |
| TASK002 | Upload domain contract | proposed | unassigned | TASK001 | Define batch/item/attempt models, lifecycle transitions, idempotency, validation, and audit requirements. |
| TASK003 | Secure YouTube authorization | proposed | unassigned | TASK001 | Implement least-privilege OAuth, encrypted token handling, connection management, and revocation behavior. |
| TASK004 | Resumable batch execution | proposed | unassigned | TASK002, TASK003 | Implement validated queueing, rate-limit handling, retry policy, and operator-visible outcomes. |
| [TASK005](TASK005-uploaded-title-duplicates.md) | Channel inventory and uploaded-title duplicate candidates | completed | unassigned | TASK002, TASK003 | Active-channel inventory now flags exact normalized titles and trailing `(2)` or higher variants for human review. |
| TASK006 | Safe video removal | proposed | unassigned | TASK003, TASK005 | Add reversible privacy controls and explicit, audited permanent deletion. |
| TASK007 | Cross-platform quality and release | proposed | unassigned | TASK004, TASK005, TASK006 | Certify responsive accessibility, supported browsers/devices, provider compliance, operations, and release safety. |
| [TASK008](TASK008-watched-folder-auto-upload.md) | Opt-in watched-folder private uploads | completed | unassigned | TASK003, TASK004 | Channel-bound local polling ingests newly added stable videos, withholds known digest/title matches, and dispatches the existing private resumable uploader. |
| [TASK009](TASK009-dedupe-trigger-startup-queue-recovery.md) | Dedupe trigger and automatic startup queue recovery | completed | unassigned | TASK004, TASK005 | Channel-gated duplicate scan implemented; crash-safe queue recovery now runs automatically during native startup. |
| [TASK010](TASK010-windows-startup-stack-overflow.md) | Windows startup stack overflow | completed | unassigned | TASK009 | Heap-backed streaming buffers remove the release startup overflow; real installed launch proof passed. |
| [TASK011](TASK011-oauth-token-diagnostics.md) | OAuth token-exchange diagnostics | in-progress | unassigned | TASK003 | Preserve safe Google OAuth failure categories so live connection errors are actionable. |
| [TASK012](TASK012-comparison-and-bulk-deletion-review.md) | Video comparison and multi-select deletion review | completed | unassigned | TASK005, TASK006 | In-app duplicate comparison plus safe multi-select local deletion request queue. |
| [TASK013](TASK013-dedupe-activity-log.md) | Dedupe activity log | completed | unassigned | TASK005, TASK009 | Manual dedupe now records truthful synchronization, local rebuild, completion, and error activity. |
| [TASK014](TASK014-upload-progress-visibility-and-drop.md) | Upload progress, visibility, and drag-and-drop intake | completed | unassigned | TASK004 | Per-item and batch transfer progress/ETA, manual visibility controls, and managed local drag-and-drop import. |
| [TASK015](TASK015-drop-upload-audience-and-playlist-review.md) | Drop upload audience and playlist review | in-progress | unassigned | TASK014 | Require kids declaration, visibility, and playlist selection before manual intake. |
| [TASK015](TASK015-dedupe-phase-progress.md) | Dedupe phase progress | completed | unassigned | TASK013 | Accessible phase-progress bar maps to actual dedupe command boundaries. |
| [TASK016](TASK016-windows-taskbar-icon.md) | Windows taskbar icon alignment | completed | unassigned | TASK001 | Canonical upload-arrow icon regenerated, packaged, installed, and verified from the installed executable. |
| [TASK017](TASK017-manual-made-for-kids-default.md) | Device-wide Made for Kids default | completed | unassigned | TASK015 | Persist and use a visible per-device default in each manual intake review. |
| [TASK017](TASK017-youtube-embed-csp.md) | YouTube embed content-security policy | completed | unassigned | TASK012 | Privacy-enhanced YouTube embeds are permitted for duplicate comparison. |
| [TASK018](TASK018-comparison-playback-icons.md) | Comparison playback icons | completed | unassigned | TASK012, TASK017 | Accessible shared play/pause and ten-second seek icon controls. |
| [TASK019](TASK019-watched-folder-visibility-and-queue-layout.md) | Watched-folder visibility and queue layout | completed | unassigned | TASK008, TASK014 | Private/unlisted recurring visibility and clearer queue intake spacing. |
| [TASK019](TASK019-product-rename-and-trademark-notice.md) | Product rename and trademark notice | completed | unassigned | TASK001 | Product and repository are named YouTube Upload Manager; local `origin` now points to the renamed GitHub repository. |
| [TASK020](TASK020-full-width-auto-dispatch-and-quota-resume.md) | Full-width workspace, automatic dispatch, and quota recovery | completed | unassigned | TASK004, TASK008, TASK014 | Full-width dashboard cards, batch auto-dispatch, and persisted daily-limit recovery. |
| [TASK021](TASK021-native-dropdown-runtime-detection.md) | Native dropdown runtime detection | completed | unassigned | TASK001 | Use Tauri's public runtime capability detector for interactive controls. |

## Status values

`proposed`, `ready`, `in-progress`, `blocked`, `completed`, `superseded`
