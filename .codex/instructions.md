# AI implementation checklist

1. Read `AGENTS.md` and the memory-bank files before non-trivial changes.
2. Work from a task file; make its acceptance criteria observable.
3. Keep OAuth tokens, video assets, and personal data out of commits and logs; keep runtime state on the device in OS-protected storage.
4. Keep app work local to the device. Do not add a backend, remote queue, cloud database, object storage, or analytics service.
5. Model upload work as an account-scoped, resumable state machine with auditable local transitions.
6. Use provider sandbox/test paths where available; label live YouTube verification separately.
7. Update the memory bank with decisions and completed evidence at handoff.
