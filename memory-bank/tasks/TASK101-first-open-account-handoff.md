# TASK101: First-open Connected account handoff

## Status

completed

## Scope

Route the final first-open setup action to the Connected account workspace,
where Desktop OAuth JSON import and YouTube connection are managed together.

## Acceptance criteria

- The setup modal no longer opens a JSON picker itself.
- Its final action is named **Open Connected account**.
- The action dismisses setup, selects the Connected account tab, and explains
  the next import step without exposing credentials.

## Evidence

- `GoogleSetupWizard` delegates its final action to the app shell rather than
  importing OAuth JSON directly.
- The app shell selects the account tab and retains the existing safe JSON
  import flow in `ConnectionPanel`.
