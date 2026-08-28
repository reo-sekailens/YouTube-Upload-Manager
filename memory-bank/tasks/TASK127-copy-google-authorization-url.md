# TASK127: Copy Google authorization URL

## Status

completed

## Scope

When an operator starts any app-owned Google authorization flow, copy the
native-generated HTTPS Google consent URL before opening it in the browser.

## Evidence

- Ordinary connection plus all three video-management entry points share one
  URL validator, clipboard action, and browser opener.
- Only `https://accounts.google.com` consent URLs are eligible; clipboard
  failure does not prevent authorization from opening.
- Built unsigned installer `YouTube Upload Manager_1.0.2-nightly.5_x64-setup.exe`
  (SHA-256 `509384F6BF2F821D3946C20E9C4D2E7B377FDC5DD438EEB5CD8CE053985CA8AC`).
