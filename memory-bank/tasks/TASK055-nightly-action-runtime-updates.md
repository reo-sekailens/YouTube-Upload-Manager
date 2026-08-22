# TASK055: Nightly action runtime updates

## Status

in-progress

## Scope

Move repository-controlled nightly workflow actions to releases using the Node 24 runtime.

## Evidence

- The prior nightly run warned that repository-controlled action versions targeted deprecated Node 20 runtimes.
- Checkout, setup-node, cache, setup-java, Android SDK setup, artifact upload, and artifact download are upgraded to Node 24-capable releases.
- The GitHub-managed Pages deployment workflow is outside the repository and may still show its own action-runtime annotations.
- A follow-up nightly run is pending GitHub Actions verification.
