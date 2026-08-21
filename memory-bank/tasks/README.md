# Task Files

Task files are small, reviewable scopes of work. The task index is authoritative; add every task there when it is created, completed, superseded, or blocked.

## File format

Use `TASK###-short-slug.md`, for example `TASK001-foundation.md`.

Each task should contain:

- Objective and non-goals.
- Owner and affected paths/services.
- Acceptance criteria and safety requirements.
- Dependencies and rollout/rollback notes where relevant.
- Validation commands plus the result when completed.

Do not place secrets, raw OAuth responses, local media paths, or personal channel data in task files.
