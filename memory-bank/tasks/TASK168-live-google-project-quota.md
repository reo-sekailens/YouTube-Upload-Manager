# TASK168 — Read quota usage and limits from Google Cloud

**Status:** completed
**Owner:** Codex
**Date:** 2026-10-19, Asia/Singapore
**Dependencies:** TASK147, TASK167

## User objective

Quota checks must show actual project usage and remaining allowance from Google
servers, rather than presenting this device's observed requests as a live total.

## Acceptance

- [x] Explicit, stepped read-only Cloud authorization uses installed-app PKCE,
  persists credentials only in OS secure storage, and fences the selected
  channel, OAuth client/bundle and project. Upload/management grants are retained.
- [x] Native requests read Service Usage effective daily limits and Cloud
  Monitoring quota consumption for the configured YouTube project. Preserve
  metric/bucket identity and Pacific quota-day boundaries.
- [x] Show Google-reported used, limit and remaining only when supported by
  matching valid provider evidence. Missing/unsupported/delayed data is labelled
  honestly; expose fetched/sample times and sampling lag. Local estimates remain
  separate. A Google read never clears queue holds or authorizes mutations.
- [x] Manual refresh has bounded requests/pagination and no background provider
  polling. Cloud permissions/API-disabled errors are actionable and redacted.
- [x] Tests cover valid remaining values, empty data, malformed/foreign evidence,
  expired/cancelled or changed bindings, partial failures and UI transitions.
  Record exact local verification and screenshots separately from live proof.

## Research

- Service Usage `services.consumerQuotaMetrics.list` exposes effective quota
  limits, not consumption: https://docs.cloud.google.com/service-usage/docs/reference/rest/v1beta1/services.consumerQuotaMetrics/list
- Monitoring `projects.timeSeries.list` reads quota consumption:
  https://docs.cloud.google.com/monitoring/api/ref_v3/rest/v3/projects.timeSeries/list
- `serviceruntime.googleapis.com/quota/rate/net_usage` measures consumed rate
  quota, sampled every 60 seconds and potentially visible up to 240 seconds later.
  Preserve `quota_metric`; API request counts are not weighted quota units:
  https://docs.cloud.google.com/monitoring/api/metrics_gcp_p_z#serviceruntime
- The two least-privilege API scopes are `monitoring.read` and
  `cloud-platform.read-only`. OAuth consent alone does not grant project IAM.
- Resource Manager v1 `projects.get` resolves the configured project ID to its
  verified project number for Service Usage, cached inside the protected grant:
  https://docs.cloud.google.com/resource-manager/reference/rest/v1/projects/get
  The Google account needs `resourcemanager.projects.get`,
  `serviceusage.quotas.get` and `monitoring.timeSeries.list`. Service Usage Viewer
  plus Monitoring Viewer provides these reads without project administration.

## Evidence

The default quota dialog now opens Google report; local operation estimates are
a separate view. Two-step authorization/reauthorization retains a dedicated
protected grant and a verified project-number mapping. Opening the panel and
polling pending authorization make no Google request; metrics refresh is manual.
Normal reports use Service Usage plus Monitoring, with one initial Resource
Manager lookup and bounded additional pages. No YouTube Data API call is needed.

Fourteen native tests cover quota arithmetic, actual-zero versus missing data,
project/bucket/type validation, partial errors/unreachable resources, disjoint
pagination, duplicates/overlap, Pacific DST windows, midnight ambiguity,
protected-grant compatibility, project mapping and callback/binding fences.
Eighteen focused frontend tests pass; complete suite passes 576 tests. Complete
native suite passes 512 tests with nine existing opt-in cases ignored.

Actual-component Playwright fixture passes 22 checks: explicit refresh,
used/limit/remaining, unknown handling, timestamps/lag, retained report after
failure, approval gating, authorization/cancellation, native-only polling,
separate estimates and desktop/mobile overflow. Console contains React development
info only; no app errors/warnings. Browser plugin unavailable; cached Playwright
CLI used. Inspected screenshots under `task168-live-google-quota-20261019` in the
thread artifact folder: `google-quota-desktop.png`, `google-quota-mobile.png`,
`google-quota-authorization.png`. Shown counts are synthetic, not this account's
live remaining quota.

Strict TypeScript/production frontend build, Tailwind guard, native formatting,
five native-budget operation tests and existing worker configuration budgets pass.
Initial JS is 223.12 KiB raw / 69.96 KiB gzip against unchanged 230/70 KiB limits.
Locked/offline production native compilation passes. The reviewed change is
published on main through the existing GitHub workflow; hosted packaging is
separate from the local verification recorded here.

No personal credentials, live project API requests, sign-in consent, production
IAM acceptance or installed WebView provider behavior were used to validate this
change. Actual metric availability depends on Google exposing it for the project;
missing telemetry remains unknown. Google-reported remaining is delayed and does
not alter saved queue holds or establish a channel upload allowance.
