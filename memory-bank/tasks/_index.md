# Task Index

This is the source of truth for planned work. Keep one row per task and link the task file when created.

| ID | Title | Status | Owner | Dependencies | Notes |
| --- | --- | --- | --- | --- | --- |
| [TASK001](TASK001-cross-platform-foundation.md) | Cross-platform local-first foundation | in-progress | unassigned | — | Tauri 2 application foundation, device-local queue, and dashboard implementation underway; Google OAuth and live provider work remain pending. |
| TASK002 | Upload domain contract | proposed | unassigned | TASK001 | Define batch/item/attempt models, lifecycle transitions, idempotency, validation, and audit requirements. |
| TASK003 | Secure YouTube authorization | proposed | unassigned | TASK001 | Implement least-privilege OAuth, encrypted token handling, connection management, and revocation behavior. |
| TASK004 | Resumable batch execution | proposed | unassigned | TASK002, TASK003 | Implement validated queueing, rate-limit handling, retry policy, and operator-visible outcomes. |
| TASK005 | Channel inventory and duplicate candidates | proposed | unassigned | TASK002, TASK003 | Sync owner-authorized uploads and produce explainable, confidence-labeled duplicate candidates. |
| TASK006 | Safe video removal | proposed | unassigned | TASK003, TASK005 | Add reversible privacy controls and explicit, audited permanent deletion. |
| TASK007 | Cross-platform quality and release | proposed | unassigned | TASK004, TASK005, TASK006 | Certify responsive accessibility, supported browsers/devices, provider compliance, operations, and release safety. |

## Status values

`proposed`, `ready`, `in-progress`, `blocked`, `completed`, `superseded`
