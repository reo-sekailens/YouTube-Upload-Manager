import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke, isTauri } = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, isTauri, convertFileSrc: vi.fn() }));
import { inspectDuplicatePreviewCompatibility, readDuplicatePreviewSegment } from "./duplicate-media-preview";

beforeEach(() => { invoke.mockReset(); isTauri.mockReturnValue(true); });
describe("compatible playback native boundary", () => {
  it("passes only the existing token to native inspection", async () => {
    invoke.mockResolvedValue({ durationSeconds: 100, mimeType: "video/mp4" });
    await expect(inspectDuplicatePreviewCompatibility("opaque-preview")).resolves.toEqual({ durationSeconds: 100, mimeType: "video/mp4" });
    expect(invoke).toHaveBeenCalledExactlyOnceWith("inspect_duplicate_preview_compatibility", { token: "opaque-preview" });
  });
  it("requests one fragment with its opaque token and original timeline position", async () => {
    const bytes = new Uint8Array([1, 2, 3]).buffer;
    invoke.mockResolvedValue(bytes);
    await expect(readDuplicatePreviewSegment("opaque-preview", 48)).resolves.toBe(bytes);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("read_duplicate_preview_segment", { token: "opaque-preview", startSeconds: 48 });
  });
  it("normalizes native platform byte arrays without copying a full original file", async () => {
    invoke.mockResolvedValue([1, 2, 3]);
    expect(new Uint8Array(await readDuplicatePreviewSegment("opaque-preview", 0))).toEqual(new Uint8Array([1, 2, 3]));
  });
  it("does not issue native requests in a plain browser", async () => {
    isTauri.mockReturnValue(false);
    await expect(inspectDuplicatePreviewCompatibility("opaque-preview")).rejects.toThrow("desktop app");
    await expect(readDuplicatePreviewSegment("opaque-preview", 0)).rejects.toThrow("desktop app");
    expect(invoke).not.toHaveBeenCalled();
  });
});
