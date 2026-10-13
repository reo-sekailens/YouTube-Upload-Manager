import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyFolderRules, folderRulesError, loadFolderRules, parseFolderPatterns, previewFolderRules, saveFolderRules } from "./folder-rules";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), native: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.native }));
describe("folder rule commands", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.native.mockReturnValue(true); });
  it("keeps load, no-write preview and approved save bound to one folder and channel", async () => {
    await loadFolderRules("monitor-a", "channel-a"); await previewFolderRules("monitor-a", "channel-a", emptyFolderRules); await saveFolderRules("monitor-a", "channel-a", emptyFolderRules, "fingerprint", true);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "load_folder_rules", { monitorId: "monitor-a", expectedChannelId: "channel-a" });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "preview_folder_rules", { monitorId: "monitor-a", expectedChannelId: "channel-a", rules: emptyFolderRules });
    expect(mocks.invoke).toHaveBeenNthCalledWith(3, "save_folder_rules", { monitorId: "monitor-a", expectedChannelId: "channel-a", rules: emptyFolderRules, previewFingerprint: "fingerprint", approved: true });
  });
  it("does not emulate native rules outside the app", () => { mocks.native.mockReturnValue(false); expect(() => loadFolderRules("monitor-a", "channel-a")).toThrow("desktop app"); expect(mocks.invoke).not.toHaveBeenCalled(); });
  it("parses one trimmed filename pattern per line and preserves literals", () => { expect(parseFolderPatterns(" Camera A_*.MP4\r\n\nSession_??.mov ")).toEqual(["Camera A_*.MP4", "Session_??.mov"]); expect(folderRulesError({ ...emptyFolderRules, includePatterns: ["Camera A_*.MP4", "Session_??.mov"] })).toBeUndefined(); });
  it("rejects paths, unsupported bracket patterns, duplicate lists and excessive input", () => { for (const pattern of ["../*.mp4", "C:\\*.mp4", "[a-z]*", "{one,two}.*", "clip\0*"]) expect(folderRulesError({ ...emptyFolderRules, includePatterns: [pattern] })).toBeDefined(); expect(folderRulesError({ ...emptyFolderRules, includePatterns: ["*.mp4", "*.MP4"] })).toContain("repeated"); expect(folderRulesError({ ...emptyFolderRules, excludePatterns: Array.from({ length: 33 }, (_, index) => `${index}*`) })).toContain("32"); });
  it("accepts simple marker suffixes while preventing paths or video markers", () => { for (const suffix of [".ready", ".export-done", ".ready_done"]) expect(folderRulesError({ ...emptyFolderRules, readyMarkerSuffix: suffix })).toBeUndefined(); for (const suffix of ["ready", ".", "../ready", ".foo/ready", ".MP4", ".3g2", ".[ready]", "." + "r".repeat(32)]) expect(folderRulesError({ ...emptyFolderRules, readyMarkerSuffix: suffix })).toBeDefined(); });
});
