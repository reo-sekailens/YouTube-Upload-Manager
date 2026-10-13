import { beforeEach, describe, expect, it, vi } from "vitest";
import { deleteUploadPreset, loadUploadPresets, mapUploadManifest, metadataBooleanChoice, metadataFromBooleanChoice, metadataMappingToItems, metadataPreviewReady, resolveUploadMetadata, saveUploadPreset } from "./upload-metadata";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.isTauri }));

describe("channel-scoped upload metadata", () => {
  beforeEach(() => { mocks.invoke.mockReset(); mocks.isTauri.mockReturnValue(true); });

  it("preserves the immutable channel for preset read, save and deletion", async () => {
    await loadUploadPresets("channel-a");
    await saveUploadPreset("channel-a", "Sessions", { title: "{filename}" }, "preset-a");
    await deleteUploadPreset("channel-a", "preset-a");
    expect(mocks.invoke.mock.calls).toEqual([
      ["load_upload_presets", { expectedChannelId: "channel-a" }],
      ["save_upload_preset", { expectedChannelId: "channel-a", id: "preset-a", name: "Sessions", metadata: { title: "{filename}" } }],
      ["delete_upload_preset", { expectedChannelId: "channel-a", id: "preset-a" }],
    ]);
  });

  it("passes only selected source locators to native manifest mapping and resolution", async () => {
    const paths = ["D:/Selected/a.mp4"];
    const itemMetadata = { [paths[0]]: { description: "Deliberate item override" } };
    await mapUploadManifest("channel-a", "D:/Chosen/batch.csv", paths);
    await resolveUploadMetadata("channel-a", paths, { title: "{filename}" }, itemMetadata);
    expect(mocks.invoke.mock.calls).toEqual([
      ["map_upload_manifest", { expectedChannelId: "channel-a", path: "D:/Chosen/batch.csv", selectedPaths: paths }],
      ["resolve_upload_metadata", { expectedChannelId: "channel-a", paths, metadata: { title: "{filename}" }, itemMetadata }],
    ]);
  });

  it("maps results by index while preserving complete overrides with explicitly absent fields", () => {
    expect(metadataMappingToItems({ matches: [{ index: 1, fileName: "same.mp4", metadata: { description: "Second" } }], issues: [] }, ["D:/One/same.mp4", "D:/Two/same.mp4"]))
      .toEqual({ "D:/Two/same.mp4": { description: "Second" } });
  });

  it("rejects duplicate, fractional and unselected mapping indices", () => {
    for (const indices of [[0, 0], [-1], [1], [0.5]]) {
      expect(() => metadataMappingToItems({ matches: indices.map((index) => ({ index, fileName: "a.mp4", metadata: {} })), issues: [] }, ["a.mp4"]))
        .toThrow("did not match the selected files");
    }
  });

  it("does not fabricate native saved presets in a browser", async () => {
    mocks.isTauri.mockReturnValue(false);
    await expect(loadUploadPresets("channel-a")).resolves.toEqual([]);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("includes the explicit matching root and retains raw templates for exactly one native expansion", async () => {
    await mapUploadManifest("channel-a", "D:/Manifest/batch.json", ["D:/Source/a.mp4"], "D:/Source");
    expect(mocks.invoke).toHaveBeenCalledWith("map_upload_manifest", { expectedChannelId: "channel-a", path: "D:/Manifest/batch.json", selectedPaths: ["D:/Source/a.mp4"], rootPath: "D:/Source" });
    expect(metadataMappingToItems({ matches: [{ index: 0, fileName: "{folder}.mp4", metadata: { title: "{folder}" }, templateMetadata: { title: "{filename}" } }], issues: [] }, ["D:/Source/{folder}.mp4"]))
      .toEqual({ "D:/Source/{folder}.mp4": { title: "{filename}" } });
  });

  it("preserves optional booleans and refuses partial or invalid review mappings", () => {
    expect([undefined, false, true].map(metadataBooleanChoice)).toEqual(["", "false", "true"]);
    expect(["", "false", "true"].map(metadataFromBooleanChoice)).toEqual([undefined, false, true]);
    const mapping = { matches: [{ index: 0, fileName: "a.mp4", metadata: {} }], issues: [] };
    expect(metadataPreviewReady(mapping, 1)).toBe(true);
    expect(metadataPreviewReady(mapping, 2)).toBe(false);
    expect(metadataPreviewReady({ ...mapping, issues: ["Invalid title"] }, 1)).toBe(false);
    expect(metadataPreviewReady({ ...mapping, matches: [...mapping.matches, ...mapping.matches] }, 2)).toBe(false);
    expect(metadataPreviewReady({ ...mapping, matches: [{ ...mapping.matches[0], index: -1 }] }, 1)).toBe(false);
    expect(metadataPreviewReady(undefined, 1)).toBe(false);
  });
});
