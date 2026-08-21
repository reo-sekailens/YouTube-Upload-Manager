# Project Brief

## Purpose

YouTube Mass Uploader is intended to provide a cross-platform, local-first native application for repeatable, controlled uploads and safe channel-library maintenance.

## Initial product outcome

An operator can use one locally installed desktop or mobile application to connect the correct YouTube channel, prepare and execute a reviewed upload batch, inspect explainable duplicate candidates, and explicitly remove selected videos while retaining local per-item receipts.

## Principles

- The operator stays in control: uploads require deliberate review and an explicit submission action.
- Each source video has one canonical upload record; retries and status updates attach to it rather than creating duplicate uploads.
- All app state, source-media references, audit records, and OAuth tokens stay on the device. The native command layer, not the webview, calls YouTube APIs; tokens use OS-protected secure storage.
- Batch work is resumable, idempotent where the provider allows it, rate-limit aware, and auditable.
- Duplicate detection is evidence-based and human-reviewed; permanent deletion is never automatic.

## Out of scope until specified

- Circumventing YouTube policies, quotas, copyright checks, or account restrictions.
- Automatically publishing without an operator-approved workflow.
- Treating local fixture success as evidence that production OAuth or uploads work.

## Success signals

- A batch has explicit ownership, account scope, item metadata, lifecycle status, timestamps, and provider result/error references.
- A failed item can be safely retried without silently duplicating a public video.
- Operators can see what completed, what needs attention, and why.
- Core workflows run locally through one signed native app across supported desktop and mobile systems; no application-controlled cloud service is required.
