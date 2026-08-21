# Privacy Policy

**Effective date:** 22 August 2026

YouTube Mass Uploader is a local-first application for preparing, uploading,
reviewing, and managing videos in a YouTube channel you authorize. This policy
explains what the app handles and where that information goes.

## Information stored on your device

The app keeps its operational data on the device where it is installed. This
includes:

- the saved upload queue, batch and item status, resumable-upload checkpoints,
  and audit receipts;
- imported video copies in the app-managed local media workspace, including
  their names, sizes, and SHA-256 digests;
- locally synchronized YouTube inventory metadata, duplicate-review records,
  and deletion-review requests; and
- locally configured public OAuth client ID and connection metadata.

The app uses this local data to keep queued videos available after a restart or
unexpected close, resume an interrupted upload safely, show prior actions, and
let you review proposed removals. It does not send this local operational
database or media workspace to an application-operated server.

## Google and YouTube data

When you choose to connect a YouTube account, the app uses Google OAuth with
your consent. OAuth access and refresh tokens are stored through the operating
system's protected credential store; they are not exposed to the app's web
interface, written to ordinary app files, included in logs, or placed in source
control.

After you authorize a connection, the native app communicates directly with
Google and YouTube to perform only the actions you select, such as:

- obtaining the identity of the connected channel;
- uploading a video you have queued, including resumable-upload status checks;
- synchronizing metadata for videos available to the authorized channel; and
- deleting a video only after a separately granted deletion permission and the
  app's final typed-ID confirmation.

Video files and related metadata are transferred directly between your device
and Google/YouTube only when you start an upload or another selected YouTube
operation. Google’s handling of data is governed by the
[Google Privacy Policy](https://policies.google.com/privacy) and applicable
YouTube terms and policies.

## What we do not collect or sell

The app does not operate an application backend, cloud queue, remote database,
object store, or telemetry pipeline. It does not use advertising trackers or
analytics SDKs, sell personal information, or share your information with
advertising networks or data brokers.

## Retention and deletion on your device

You control locally stored app data through the application and by uninstalling
it or removing its app data using your operating system. Removing app data may
remove the local queue, managed media copies, saved inventory, audit records,
and local credentials, and can prevent an interrupted upload from being
resumed.

Disconnecting YouTube removes this device's locally protected authorization
credential and local connection state. It does not delete videos from YouTube.
A YouTube video is removed only when you explicitly execute a reviewed deletion
request with the required separate authorization and confirmation. YouTube may
retain data according to its own policies after a deletion request is
processed.

## Security

The app is designed to limit token access to native code and the operating
system credential store. No software can guarantee absolute security, so keep
your device protected and revoke Google account access if you believe it has
been compromised.

## Changes to this policy

If this policy changes materially, the updated version will be published with a
revised effective date.

## Contact

**Contact placeholder — replace before public release:**
`privacy-contact@example.com`

Use this contact for privacy questions, requests, or concerns. The placeholder
is not an active support address.
