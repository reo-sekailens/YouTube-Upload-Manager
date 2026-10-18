# TASK167 — Minimize YouTube reads before and between uploads

**Status:** completed
**Owner:** Codex
**Date:** 2026-10-19, Asia/Singapore
**Dependencies:** TASK108, TASK147, TASK166

## User objective

Aggressively reduce practical Google/YouTube query usage. The supplied Cloud
quota image shows 9,958 of 10,000 daily queries used with zero video uploads.
Do not substitute quota-clock repair alone for eliminating the request causes.

## Findings and acceptance

- [x] One canonical credential/channel-scoped inventory coordinator for
  preflight, plan preview/import, folder intake, queue checks and handoff.
  Persist explicit successful/empty snapshots; collapse concurrent work.
- [x] Warm automatic checks perform zero provider reads. Stale automatic checks
  read recent changes and merge only; complete/manual reads retain full
  reconciliation semantics. Bound cache age and error retries explicitly.
- [x] Saved Cloud quota holds prevent repeated list attempts. Failed automatic
  reads back off without renewing inventory freshness or flooding provider calls.
- [x] Reuse playlist display metadata across dialogs and immutable token/channel
  identity proofs; invalidate/fence account changes and playlist creation.
- [x] Plan preview/import share inventory evidence instead of separate full
  crawls. Preserve offline zero-network preview and source/plan fingerprints.
- [x] Automatic publication polling respects durable deadlines; asset jobs
  reuse proof/ownership/membership evidence only within a fenced run. Keep
  uncertain-write reconciliation and fresh mutation versions authoritative.
- [x] Targeted source cleanup verification never crawls the whole library.
- [x] Provider-injected request-count tests cover cold/warm/stale/empty/failure,
  quota, concurrency, channel isolation and normal upload admission. Native,
  frontend/build/budget and rendered regressions pass; record exact evidence.

## Ownership

Root owns lib.rs integration, quota gate, canonical cache policy, targeted
cleanup, playlist cache, frontend integration and task docs. Independent agents
own inventory_sync.rs, youtube_read_cache.rs, upload_plan.rs,
publication_schedule.rs and video_assets.rs. No personal queued upload, media,
tokens or live provider calls are used for test requests.

## Evidence

Full scan baseline is `1 + 2 * ceil(video_count/50)` list requests; 5,000 videos
costs 201 before a dispatch wave. Previous intake/scheduler/plan readers could
repeat that scan, while watched retries ignored an already recorded Cloud hold.
Local UI/SQLite snapshot reads are not YouTube quota usage.

Implemented one canonical coordinator with explicit persistent successful/empty
checkpoints, 15-minute warm reuse and 24-hour complete refresh. Recent checks
merge only; external old-title edits/deletions can remain stale until a complete
refresh. Manual Refresh library retains complete reconciliation. Binding hashes
include channel, client, project and credential bundle; concurrent consumers
recheck after locking. Failures preserve the old snapshot and back off 1–15 minutes.

Injected inventory tests prove: 5,000-video complete = 201 reads; unchanged recent
= one; new/changed head = two; 100 warm consumers = zero; 100 quota-blocked
consumers = zero; concurrent cold consumers share one three-read fixture refresh;
empty success persists across reopen. Playlist and token identity caches retain
owner/binding checks. Plans reuse the same evidence; no separate crawler remains.
Publication automatic work respects durable due times. Asset reuse is bounded
to item/permission-kind runs; uncertain reconciliation still reads fresh evidence.

Review caught and corrected warm-cache title safety: fully confirmed local upload
receipts immediately enter title review without a provider crawl. Incomplete,
foreign, reconciliation and invalid-ID receipts remain excluded; processed remote
metadata wins the same video ID. Explicit duplicate approval remains authoritative.

Source cleanup reads one exact provider video, validates owner, records fresh
processing, and finalizes only the approved source through the existing digest
and filesystem guards. Its initial normal-flow test caught an intermediate
processing_verified transition that skipped deletion; the targeted finalizer fixes
that case. The legacy cleanup test now creates an explicitly scoped connection.

17 initial integrated request-count tests passed. Final integrated results and
release receipt follow below. No personal uploads or real Google requests were
used. No lockfile, dependency, release configuration or startup worker budget was
changed; fixture workers are explicitly inside a cfg(test) module.

Final integrated native suite: **512 passed, zero failed, nine opt-in ignored**.
All 23 coordinator/title/cleanup request-count tests pass, including the formerly
failing targeted deletion and the immediate warm-cache uploaded-title gate.
Frontend: **576 passed**, production build/TypeScript and Tailwind pass. The
unchanged initial bundle budget passes at 223.12 KiB raw / 69.96 KiB gzip; native
configuration remains four uploads/volume, one startup worker and one production
thread spawn. Five worker-budget operation tests pass; formatting/whitespace pass.
TASK166 rendered queue fixture and TASK168 quota fixture separately pass 12 and
22 checks. Locked/offline production native compilation passes. The complete
reviewed change is published on main through the existing GitHub workflow;
hosted packaging and real-provider behavior remain separate evidence.
