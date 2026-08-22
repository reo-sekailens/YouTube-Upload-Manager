# TASK071: Folder-monitor live overview

## Status

completed

## Scope

Expose a bounded, folder-scoped live view of watched-file activity, queued and
uploading items, and safe scan/audit history while keeping automatic polling
native and retaining a manual refresh scan action.

## Acceptance criteria

- The watched folder shows currently uploading/dispatching files and its queue.
- The panel renders a bounded per-folder observation list and collapsed safe
  scan log.
- Monitoring continues automatically while enabled; the manual button is named
  **Refresh scan**.
- No source paths, OAuth data, or provider payloads reach the webview.

## Evidence

- Native `load_folder_monitor_overview` returns at most 200 channel-scoped
  observations and 200 folder-monitor audit entries, with each observation
  reduced to a filename before it crosses into the webview.
- The folder panel polls that native overview every five seconds while running,
  displays active and queued watched-folder files, and keeps scan logs and the
  broader observed-file list collapsed until opened.
- `Refresh scan` still explicitly starts a background scan. The native polling
  loop records a safe error state if an automatic scan fails, then continues
  polling rather than stopping permanently.
- Local validation: focused native overview test, focused frontend command test,
  TypeScript check, and whitespace diff check passed. Visual browser preview
  remains limited to its safe disabled-mode state; a signed desktop session is
  required to exercise populated activity rows.
