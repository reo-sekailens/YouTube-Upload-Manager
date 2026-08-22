# TASK023 — GitHub repository header

## Status

`completed`

## Outcome

A 1280 × 640 GitHub social-preview header derives its visual identity from the
canonical upload-arrow SVG. It represents the product as a controlled desktop
tool without using Google or YouTube brand artwork.

## Acceptance criteria

- Reuse the blue rounded-square upload-arrow motif from `src-tauri/icons/icon.svg`.
- Deliver a GitHub-compatible 1280 × 640 PNG and an editable SVG source.
- Use only the product name and accurate upload-management messaging.
- Present the PNG at the top of the GitHub README so non-technical visitors
  immediately see what the application is for.

## Evidence

- `assets/github-repo-header.svg` renders successfully to
  `assets/github-repo-header.png` at 1280 × 640 using ImageMagick.
- `README.md` uses the local PNG as its opening repository image and introduces
  the product in plain, non-technical language before the technical badges.
