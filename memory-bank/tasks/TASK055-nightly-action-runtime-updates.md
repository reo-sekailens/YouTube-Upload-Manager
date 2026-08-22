# TASK055: Nightly action runtime updates

## Status

in-progress

## Scope

Move repository-controlled GitHub Actions workflows to releases using the Node 24 runtime.

## Evidence

- The prior nightly run warned that repository-controlled action versions targeted deprecated Node 20 runtimes.
- Checkout and setup-node in the local verification workflow are upgraded alongside the nightly workflow's cache, Java, Android SDK setup, and artifact actions.
- The GitHub-managed Pages deployment workflow is outside the repository and may still show its own action-runtime annotations.
- A follow-up nightly run is pending GitHub Actions verification.
