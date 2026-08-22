# YouTube Upload Manager

![YouTube Upload Manager — Upload with confidence](assets/github-repo-header.png)

[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app/)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=111827)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5-3178C6?logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License: AGPL v3+](https://img.shields.io/badge/License-AGPL%20v3%2B-2f7d32.svg)](LICENSE)

A straightforward desktop app for uploading videos to your YouTube channel,
checking for duplicates, and keeping control of your library — all from your
own device.

No technical setup is needed to get familiar with the app: choose your videos,
review their upload settings, and approve the actions you want it to take.
When you are ready to connect YouTube, the app walks you through the one-time
account setup using a Google Cloud project you control.

The application has no service-side queue, media store, or analytics pipeline.
Its only network activity is the Google and YouTube operation you explicitly
authorize and start.

## Capabilities

- Guided first-open Google Cloud setup: create your own project, enable the
  YouTube Data API, create a Desktop OAuth client, and import its JSON.
- Native, multi-file intake with a required Made for Kids declaration,
  visibility, and optional owner-authorized playlist selection.
- Device-local managed media, resumable YouTube uploads, progress/ETA, daily
  quota pause recovery, and safe startup reconciliation.
- Optional watched-folder intake for private or unlisted videos only, with
  stable-file checks, channel binding, and duplicate withholding.
- Duplicate review with local SHA-256 evidence, synchronized uploaded-title
  evidence, lazy YouTube comparison players, search, and audited decisions.
- Pre-ingest checks for arbitrary files including `.insv` and `.lrv`: desktop
  drops default to fast filename matching, while deep SHA-256 matching is
  opt-in and checkpointed per file.
- Desktop builds bundle FFprobe for local video-container metadata, including
  duration, tags, and stream details. No separate FFmpeg install is needed;
  Android and iOS do not execute this desktop sidecar.
- Compact gzip export/import for duplicate hashes and YouTube inventory; media,
  source paths, refresh tokens, client secrets, and upload sessions are never
  exported.
- Typed-ID YouTube deletion review with separate authorization, ownership
  verification, a temporary deletion mode, and durable recovery receipts.
- Explicit cancellation and queue clearing for upload work, pre-ingest dedupe,
  and pending/recoverable deletion requests. Clearing never deletes managed
  media or changes YouTube videos.

## Responsive native architecture

The interface is presentation-only. Hashing, managed-file copying, folder
scans, queue recovery, archive compression, and Google/YouTube API work run in
native background workers, so a large import, deep match, inventory sync, or
remote operation does not freeze the UI.

All multi-step work is durable and crash-safe. Imports, uploads, watched-folder
observations, pre-ingest jobs, and inventory refreshes resume from their
persisted checkpoints. Remote ambiguity is retained for reconciliation rather
than blindly repeating an operation.

## Quick start

1. Install a current [Node.js](https://nodejs.org/) release and
   [Rust](https://www.rust-lang.org/tools/install).
2. Install the desktop prerequisites for your platform from the
   [Tauri setup guide](https://v2.tauri.app/start/prerequisites/).
3. Install dependencies and run the native app:

   ```sh
   npm install
   npm run tauri -- dev
   ```

4. Create a Google Cloud project you control, enable the YouTube Data API,
   configure its OAuth consent screen, and create a **Desktop app** OAuth
   client. Download that client's JSON, then import it from the app's
   connection panel before connecting a YouTube channel you are authorized to
   manage.

Useful local checks:

```sh
npm run check
npm run test
npm run build
```

Build a Windows x64 NSIS installer:

```sh
npm run tauri build -- --bundles nsis
```

The installer is emitted under `src-tauri/target/release/bundle/nsis/`. Build
artifacts are unsigned unless a separate signing process is configured.

## Portable builds

Each GitHub nightly pre-release also includes platform-appropriate portable
artifacts:

- Windows x64: `YouTube-Upload-Manager-windows-x64-portable.zip`; extract it
  and run `YouTube-Upload-Manager.exe`. Windows WebView2 is still required by
  Tauri, but no installer or elevated permission is needed.
- Linux x64: `YouTube-Upload-Manager-linux-x64-portable.AppImage`; mark it
  executable and run it. It does not require a package-manager install.
- macOS Apple Silicon and Intel: portable ZIP archives containing the `.app`
  bundle. Extract the archive and move/open the app as needed.

Android is distributed as an installable debug APK rather than a portable
desktop executable. All nightly artifacts are unsigned; macOS and Windows may
show their normal first-run security warnings.

Desktop FFprobe provenance and licensing are recorded in [NOTICE](NOTICE).
The canonical project [AGPL license](LICENSE) remains unmodified so GitHub can
recognize it correctly.

## Documentation

- [Project home](docs/index.md)
- [Privacy policy](docs/privacy.md)
- [Terms of service](docs/terms.md)
- [GitHub Wiki](../../wiki) (setup, operations, recovery, and security guides)
- [Google OAuth for installed apps](https://developers.google.com/identity/protocols/oauth2/native-app)
- [YouTube Data API](https://developers.google.com/youtube/v3)

## Status

The local app, persistent queue, native OAuth path, background-worker
isolation, resumable-upload workflow, inventory sync, duplicate review,
export/import, and deletion-request review are implemented and locally tested.
Live YouTube verification is intentionally separate: it requires an operator's
authorized test channel.

## Safety and security

OAuth credentials and upload checkpoints are handled by the native layer and
kept out of the webview. The queue, managed media, local inventory, and audit
records stay on the device. Deletion is never automatic: an operator must
select the video, type its exact ID, and pass a fresh authorization and
ownership check before a YouTube delete operation can occur. Cancellation is
also local and auditable; it stops future work without deleting operator media.

This project uses official Google OAuth and YouTube APIs only. It does not
bypass account restrictions, quotas, copyright rules, or YouTube policies.

## Independent project and trademarks

YouTube Upload Manager is an independent project. It is not affiliated with,
endorsed by, sponsored by, or provided by Google or YouTube. Google and YouTube
are trademarks of Google LLC. All other product names, logos, and trademarks
are the property of their respective owners.

## Maintainers

Repository-specific contributor guidance is in [AGENTS.md](AGENTS.md). Internal
project context is maintained in [memory-bank/](memory-bank/README.md).

## License

Licensed under the [GNU Affero General Public License v3.0 or later](LICENSE).
Copyright (C) 2026 Satoshi Katade. See [NOTICE](NOTICE) for the project
copyright, trademark, and attribution notice.
