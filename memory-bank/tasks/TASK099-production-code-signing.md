# TASK099 — Production code signing

## Status

blocked

## Objective

Produce a signed Windows release package and verify its Authenticode chain
before production certification.

## Acceptance criteria

- Sign the final NSIS installer and installed executable with the approved
  production certificate.
- Verify Authenticode status, signer identity, timestamp, and artifact hash on
  the exact release candidate.
- Run TASK098 against that exact signed artifact.

## Blocker

- No approved production signing certificate or signing authority is available
  in this workspace.
- Current-user certificate-store inspection found no code-signing certificate;
  the latest installer verifies as `NotSigned`.
