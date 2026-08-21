# YouTube Mass Uploader

[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white)](https://v2.tauri.app/)
[![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=111827)](https://react.dev/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5-3178C6?logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)](https://www.rust-lang.org/)

A cross-platform, local-first application for preparing, reviewing, and
running controlled YouTube upload batches from your own device.

The application has no service-side queue, media store, or analytics pipeline.
Its only network activity is the Google and YouTube operation you explicitly
authorize and start.

## Capabilities

- Import videos into a persistent, device-local workspace and queue.
- Recover interrupted imports and resumable uploads from saved checkpoints.
- Connect an operator-authorized YouTube channel through installed-app OAuth.
- Upload private videos directly to YouTube from the native application.
- Sync the channel library locally and review explainable duplicate candidates.
- Create typed-ID deletion requests, then cancel or separately authorize them.

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

4. Before connecting, configure your own Google OAuth client ID in the app and
   use a YouTube channel you are authorized to manage. Begin with a test
   channel.

Useful local checks:

```sh
npm run check
npm run test
npm run build
```

## Documentation

- [Project home](docs/index.md)
- [Privacy policy](docs/privacy.md)
- [Terms of service](docs/terms.md)
- [Google OAuth for installed apps](https://developers.google.com/identity/protocols/oauth2/native-app)
- [YouTube Data API](https://developers.google.com/youtube/v3)

## Status

The local app, persistent queue, native OAuth path, resumable-upload workflow,
inventory sync, and deletion-request review are implemented and locally tested.
Live YouTube verification is intentionally separate: it requires an operator's
configured OAuth client and authorized test channel.

## Safety and security

OAuth credentials and upload checkpoints are handled by the native layer and
kept out of the webview. The queue, managed media, local inventory, and audit
records stay on the device. Deletion is never automatic: an operator must
select the video, type its exact ID, and pass a fresh authorization and
ownership check before a YouTube delete operation can occur.

This project uses official Google OAuth and YouTube APIs only. It does not
bypass account restrictions, quotas, copyright rules, or YouTube policies.

## Maintainers

Repository-specific contributor guidance is in [AGENTS.md](AGENTS.md). Internal
project context is maintained in [memory-bank/](memory-bank/README.md).
