import { beforeEach, describe, expect, it, vi } from "vitest";
import { configureRemotePreviewTool, getRemotePreviewConfiguration, prepareRemotePreview, releaseRemotePreview, setRemotePreviewBrowserSession } from "./remote-preview";

const mocks = vi.hoisted(() => ({ native: vi.fn(() => true), invoke: vi.fn(), convert: vi.fn((token: string, scheme: string) => `${scheme}://localhost/${token}`) }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: mocks.native, invoke: mocks.invoke, convertFileSrc: mocks.convert }));

describe("native remote comparison bridge", () => {
  beforeEach(() => { vi.clearAllMocks(); mocks.native.mockReturnValue(true); });

  it("resolves only scoped evidence and exposes an opaque local URL", async () => {
    mocks.invoke.mockResolvedValue({ token: "request-identity" });
    const target = { kind: "candidate", candidateId: "remote:left:right" } as const;
    await expect(prepareRemotePreview(target, "channel-a", "video-a", "request-identity"))
      .resolves.toEqual({ token: "request-identity", url: "media-preview://localhost/request-identity" });
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("prepare_remote_preview", { target, expectedChannelId: "channel-a", videoId: "video-a", requestId: "request-identity" });
    expect(mocks.convert).toHaveBeenCalledExactlyOnceWith("request-identity", "media-preview");
  });

  it("configures explicitly chosen executables and keeps revocation independent of extraction", async () => {
    await configureRemotePreviewTool("C:/tools/yt-dlp.exe", "C:/tools/deno.exe");
    await releaseRemotePreview(["pending-request"]);
    expect(mocks.invoke.mock.calls).toEqual([
      ["configure_remote_preview_tool", { toolPath: "C:/tools/yt-dlp.exe", runtimePath: "C:/tools/deno.exe" }],
      ["release_remote_preview", { tokens: ["pending-request"] }],
    ]);
    await configureRemotePreviewTool("/usr/bin/yt-dlp");
    expect(mocks.invoke).toHaveBeenLastCalledWith("configure_remote_preview_tool", { toolPath: "/usr/bin/yt-dlp", runtimePath: null });
  });

  it("does not run privileged operations in browser mode", async () => {
    mocks.native.mockReturnValue(false);
    await expect(getRemotePreviewConfiguration()).rejects.toThrow("desktop app");
    await expect(configureRemotePreviewTool("tool")).rejects.toThrow("desktop app");
    await expect(setRemotePreviewBrowserSession({ browser: "firefox", profile: "profile-a" }, "channel-a", true)).rejects.toThrow("desktop app");
    await expect(prepareRemotePreview({ kind: "upload", itemId: "item" }, undefined, "video", "request")).rejects.toThrow("desktop app");
    await releaseRemotePreview(["request"]);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("fences browser-session preferences to an explicit channel and final approval", async () => {
    const session = { browser: "firefox", profile: "profile-a" } as const;
    mocks.invoke.mockResolvedValue({ configured: true, supported: true, runtimeConfigured: false, browserSession: session });
    await expect(getRemotePreviewConfiguration("channel-a")).resolves.toMatchObject({ browserSession: session });
    await setRemotePreviewBrowserSession(session, "channel-a", true);
    await setRemotePreviewBrowserSession(null, "channel-a", false);
    expect(mocks.invoke.mock.calls).toEqual([
      ["get_remote_preview_configuration", { expectedChannelId: "channel-a" }],
      ["set_remote_preview_browser_session", { session, expectedChannelId: "channel-a", approved: true }],
      ["set_remote_preview_browser_session", { session: null, expectedChannelId: "channel-a", approved: false }],
    ]);
  });
});
