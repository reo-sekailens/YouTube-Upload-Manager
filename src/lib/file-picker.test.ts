import { describe, expect, it } from "vitest";
import { selectedFilePaths } from "./file-picker";

describe("selectedFilePaths", () => {
  it("preserves every desktop or mobile picker selection", () => {
    expect(selectedFilePaths(["C:\\Camera\\one.insv", "content://media/video/2", "file:///private/two.lrv"])).toEqual([
      "C:\\Camera\\one.insv",
      "content://media/video/2",
      "file:///private/two.lrv",
    ]);
  });

  it("normalizes single-file and cancelled picker results", () => {
    expect(selectedFilePaths("C:\\Camera\\one.mp4")).toEqual(["C:\\Camera\\one.mp4"]);
    expect(selectedFilePaths(null)).toEqual([]);
  });
});
