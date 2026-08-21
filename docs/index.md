# YouTube Mass Uploader

YouTube Mass Uploader is a local-first application for preparing, reviewing,
and running controlled YouTube upload batches from your own device.

## Your device stays in control

- The upload queue, managed media copies, inventory cache, audit records, and
  app settings remain on the device.
- OAuth tokens and resumable-upload checkpoints are handled by the native app
  and protected through the operating system's secure storage.
- The app has no application backend, cloud queue, media store, or analytics
  pipeline. Network activity is limited to the Google and YouTube actions you
  explicitly start.

## Key features

- Import videos into a persistent local queue so an app restart or source-file
  move does not require dragging them in again.
- Resume interrupted uploads from provider-confirmed checkpoints instead of
  starting a possible duplicate upload.
- Sync the connected channel's video library locally and review explainable
  duplicate candidates.
- Create explicit, typed-ID deletion requests. A request is never an automatic
  deletion and permanent deletion requires fresh Google re-authorization and
  an ownership check.

## Google and YouTube account requirement

You need a Google OAuth client configured for this application and a YouTube
channel you are authorized to manage. The application uses official Google
OAuth and YouTube Data API operations; it does not bypass YouTube policy,
quota, ownership, copyright, or account restrictions. Start with a
non-production test channel before relying on uploads or deletion workflows.

## Learn more

- [Privacy](privacy.md)
- [Terms of service](terms.md)
- [Project source and setup](../README.md)
- [Google OAuth for installed apps](https://developers.google.com/identity/protocols/oauth2/native-app)
- [YouTube Data API](https://developers.google.com/youtube/v3)

## Open source license

The source code is licensed under the GNU Affero General Public License v3.0
or later. See the [license](../LICENSE).
