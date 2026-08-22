export type GoogleSetupStep = {
  title: string;
  detail: string;
};

/** The ordered operator-owned setup needed before the native OAuth flow can begin. */
export const googleSetupSteps: GoogleSetupStep[] = [
  {
    title: "Use your Google account",
    detail: "Sign in or create the Google account that will own this setup. The app cannot create, view, or store that account.",
  },
  {
    title: "Create a Cloud project",
    detail: "Create a new project in Google Cloud Console. It keeps this app's API settings and OAuth client under your control.",
  },
  {
    title: "Enable YouTube Data API v3",
    detail: "In APIs & Services → Library, find YouTube Data API v3 and select Enable for the new project.",
  },
  {
    title: "Configure Google Auth Platform",
    detail: "Set up branding, audience, and the YouTube Data API scopes. If the audience is External and the app is in Testing, add the Google account you will authorize as a test user.",
  },
  {
    title: "Create a Desktop OAuth client",
    detail: "In Google Auth Platform → Clients, create an OAuth client with application type Desktop app, then download its JSON file.",
  },
  {
    title: "Import the downloaded JSON",
    detail: "Open Connected account to select the downloaded JSON. It is parsed by the native app; its optional client secret is stored only in OS-protected storage.",
  },
];

export function setupStepProgress(step: number): number {
  return Math.min(Math.max(step + 1, 1), googleSetupSteps.length);
}
