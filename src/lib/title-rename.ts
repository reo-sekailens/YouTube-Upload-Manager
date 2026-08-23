/**
 * Pure helpers for the title-rename review UI.  They deliberately do not call
 * YouTube: callers must present the resulting preview and obtain confirmation
 * before sending any selected change to the native command layer.
 */

export const normalizationTemplates = [
  "underscores-to-spaces",
  "collapse-whitespace",
  "trim",
] as const;

/** @deprecated Use normalizationTemplates. */
export const titleNormalizationTemplates = normalizationTemplates;

export type TitleNormalizationTemplate = (typeof normalizationTemplates)[number];

export interface TitleRenameRule {
  pattern: string;
  flags?: string;
  replacement: string;
  normalization?: readonly TitleNormalizationTemplate[];
}

export interface TitleRenameRegexSuccess {
  ok: true;
  regex: RegExp;
}

export interface TitleRenameRegexFailure {
  ok: false;
  error: string;
}

export type TitleRenameRegexResult = TitleRenameRegexSuccess | TitleRenameRegexFailure;

export interface TitleRenamePreview {
  title: string;
  renamedTitle: string;
  /** Whether the supplied expression covered any part of this title. */
  matched: boolean;
  /** Whether applying the expression and selected normalizers changes it. */
  changed: boolean;
  /** A non-empty title is required before this preview can be submitted. */
  valid: boolean;
}

export interface TitleRenamePreviewFailure {
  title: string;
  error: string;
}

export type TitleRenamePreviewResult = TitleRenamePreview | TitleRenamePreviewFailure;

export interface TitleRenamePreviewItem extends TitleRenamePreview {
  videoId: string;
}

export type TitleRenameSelection = "selected" | "all-matches";

/**
 * Compiles separate pattern and flags fields without allowing a malformed
 * operator entry to throw while the UI is rendering. Empty expressions are
 * rejected because they match every position and make accidental bulk edits
 * too easy.
 */
export function parseTitleRenameRegex(pattern: string, flags = ""): TitleRenameRegexResult {
  if (pattern.length === 0) {
    return { ok: false, error: "Enter a regular expression to preview renames." };
  }

  try {
    return { ok: true, regex: new RegExp(pattern, flags) };
  } catch {
    return { ok: false, error: "The regular expression or its flags are invalid." };
  }
}

/** User-facing name for compiling the pattern and flags fields. */
export const compileTitleRename = parseTitleRenameRegex;

/** Applies selected readability templates in their declared order. */
export function normalizeVideoTitle(
  title: string,
  templates: readonly TitleNormalizationTemplate[] = [],
): string {
  return templates.reduce((normalized, template) => {
    switch (template) {
      case "underscores-to-spaces":
        return normalized.replace(/_+/g, " ");
      case "collapse-whitespace":
        return normalized.replace(/\s+/g, " ");
      case "trim":
        return normalized.trim();
    }
  }, title);
}

/**
 * Builds one explainable preview. String.replace preserves every unmatched
 * portion of a title, so a pattern can safely rename only its captured part.
 */
function previewTitleRenameWithRegex(
  title: string,
  rule: Pick<TitleRenameRule, "replacement" | "normalization">,
  regex: RegExp,
): TitleRenamePreview {
  // RegExp#test with g/y advances lastIndex. Resetting both before and after
  // makes preview rows independent when a caller supplies a compiled regex.
  regex.lastIndex = 0;
  const matched = regex.test(title);
  regex.lastIndex = 0;
  const replaced = matched ? title.replace(regex, rule.replacement) : title;
  regex.lastIndex = 0;
  const renamedTitle = normalizeVideoTitle(replaced, rule.normalization);

  return {
    title,
    renamedTitle,
    matched,
    changed: matched && renamedTitle !== title,
    valid: renamedTitle.length > 0,
  };
}

/**
 * Preview one rename from a previously compiled rule. Compilation errors are
 * returned as data so an input field can remain usable while being edited.
 */
export function previewTitleRename(
  title: string,
  compiledRegex: TitleRenameRegexResult | RegExp,
  replacement: string,
  normalization: readonly TitleNormalizationTemplate[] = [],
): TitleRenamePreviewResult {
  if (!(compiledRegex instanceof RegExp) && !compiledRegex.ok) {
    return { title, error: compiledRegex.error };
  }

  const regex = compiledRegex instanceof RegExp ? compiledRegex : compiledRegex.regex;
  return previewTitleRenameWithRegex(title, { replacement, normalization }, regex);
}

/** Returns a preview for every supplied account-scoped inventory record. */
export function previewTitleRenames(
  videos: readonly { videoId: string; title: string }[],
  rule: TitleRenameRule,
): { regex: TitleRenameRegexResult; items: TitleRenamePreviewItem[] } {
  const regex = parseTitleRenameRegex(rule.pattern, rule.flags);
  if (!regex.ok) return { regex, items: [] };

  return {
    regex,
    items: videos.map((video) => ({
      videoId: video.videoId,
      ...previewTitleRenameWithRegex(video.title, rule, regex.regex),
    })),
  };
}

/**
 * Produces the only rows eligible for submission. "all-matches" is still
 * restricted to previews with a real, changed title; "selected" additionally
 * requires an explicit video ID selected in the UI.
 */
export function selectedTitleRenamePreviews(
  previews: readonly TitleRenamePreviewItem[],
  selection: TitleRenameSelection,
  selectedVideoIds: ReadonlySet<string> = new Set(),
): TitleRenamePreviewItem[] {
  return previews.filter((preview) =>
    preview.matched &&
    preview.changed &&
    preview.valid &&
    (selection === "all-matches" || selectedVideoIds.has(preview.videoId)),
  );
}
