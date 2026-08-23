import { describe, expect, it } from "vitest";
import {
  normalizeVideoTitle,
  compileTitleRename,
  parseTitleRenameRegex,
  previewTitleRename,
  previewTitleRenames,
  selectedTitleRenamePreviews,
} from "./title-rename";

describe("title rename previews", () => {
  it("reports invalid or bulk-dangerous expressions without throwing", () => {
    expect(parseTitleRenameRegex("")).toEqual({
      ok: false,
      error: "Enter a regular expression to preview renames.",
    });
    expect(parseTitleRenameRegex("[", "g")).toEqual({
      ok: false,
      error: "The regular expression or its flags are invalid.",
    });
    expect(parseTitleRenameRegex("episode", "gg").ok).toBe(false);
  });

  it("uses JavaScript capture replacements and leaves unmatched title values intact", () => {
    const parsed = parseTitleRenameRegex("Episode_(\\d+)", "i");
    if (!parsed.ok) throw new Error("test expression must compile");

    expect(previewTitleRename("Show - Episode_07 - Final", parsed, "Part $1")).toMatchObject({
      renamedTitle: "Show - Part 07 - Final",
      matched: true,
      changed: true,
      valid: true,
    });
    expect(previewTitleRename("Show - Finale", parsed, "Part $1")).toMatchObject({
      renamedTitle: "Show - Finale",
      matched: false,
      changed: false,
    });
  });

  it("returns a preview error rather than throwing while regex input is invalid", () => {
    const compiled = compileTitleRename("[");
    expect(previewTitleRename("Episode 1", compiled, "Part 1")).toEqual({
      title: "Episode 1",
      error: "The regular expression or its flags are invalid.",
    });
  });

  it("normalizes readable titles with composable templates", () => {
    expect(normalizeVideoTitle("  my__video\t title  ", [
      "underscores-to-spaces",
      "collapse-whitespace",
      "trim",
    ])).toBe("my video title");
  });

  it("makes global regex previews independent across titles", () => {
    const result = previewTitleRenames([
      { videoId: "one", title: "clip_01" },
      { videoId: "two", title: "clip_02" },
    ], {
      pattern: "clip_(\\d+)",
      flags: "g",
      replacement: "Episode $1",
    });

    expect(result.regex.ok).toBe(true);
    expect(result.items.map((item) => item.renamedTitle)).toEqual(["Episode 01", "Episode 02"]);
  });

  it("restricts selected and all-match actions to changed, valid matching previews", () => {
    const { items } = previewTitleRenames([
      { videoId: "one", title: "Episode_01" },
      { videoId: "two", title: "Episode_02" },
      { videoId: "three", title: "Trailer" },
    ], {
      pattern: "Episode_(\\d+)",
      replacement: "Episode $1",
      normalization: ["underscores-to-spaces"],
    });

    expect(selectedTitleRenamePreviews(items, "selected", new Set(["two", "three"])).map((item) => item.videoId)).toEqual(["two"]);
    expect(selectedTitleRenamePreviews(items, "all-matches").map((item) => item.videoId)).toEqual(["one", "two"]);
  });

  it("does not make an empty resulting title eligible for rename", () => {
    const { items } = previewTitleRenames([{ videoId: "one", title: "Remove me" }], {
      pattern: ".+",
      replacement: "",
      normalization: ["trim"],
    });

    expect(items[0]).toMatchObject({ matched: true, changed: true, valid: false, renamedTitle: "" });
    expect(selectedTitleRenamePreviews(items, "all-matches")).toEqual([]);
  });
});
