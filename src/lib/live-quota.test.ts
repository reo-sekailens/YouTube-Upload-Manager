import { beforeEach, describe, expect, it, vi } from "vitest";
import { beginLiveQuotaAuthorization, cancelLiveQuotaAuthorization, liveQuotaStatusMatches, loadLiveQuotaStatus, openLiveQuotaAuthorization, refreshLiveQuota, reportedQuotaNumber } from "./live-quota";
const mocks = vi.hoisted(() => ({ invoke: vi.fn(), desktop: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: mocks.desktop }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mocks.open }));

describe("native Google quota boundary", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.desktop.mockReturnValue(true); });
  it("routes scoped reads, refresh and authorization to dedicated native commands", async () => {
    await loadLiveQuotaStatus("channel-a"); await refreshLiveQuota("channel-a"); await beginLiveQuotaAuthorization("channel-a"); await cancelLiveQuotaAuthorization("channel-a", "attempt-a");
    expect(mocks.invoke.mock.calls).toEqual([["load_live_quota_status", { expectedChannelId: "channel-a" }], ["refresh_live_quota", { expectedChannelId: "channel-a" }], ["begin_live_quota_authorization", { expectedChannelId: "channel-a" }], ["cancel_live_quota_authorization", { expectedChannelId: "channel-a", attemptId: "attempt-a" }]]);
  });
  it("opens only Google's HTTPS authorization endpoint", async () => {
    await openLiveQuotaAuthorization({ authorizationUrl: "https://accounts.google.com/o/oauth2/v2/auth?state=safe", attemptId: "attempt-a" });
    expect(mocks.open).toHaveBeenCalledExactlyOnceWith("https://accounts.google.com/o/oauth2/v2/auth?state=safe");
    for (const url of ["https://example.com/o/oauth2/v2/auth", "http://accounts.google.com/o/oauth2/v2/auth", "https://accounts.google.com/other", "https://person@accounts.google.com/o/oauth2/v2/auth"]) await expect(openLiveQuotaAuthorization({ authorizationUrl: url, attemptId: "attempt-a" })).rejects.toThrow("unexpected address");
    expect(mocks.open).toHaveBeenCalledOnce();
  });
  it("requires both channel and project receipt identity and preserves unknown numbers", () => {
    const status = { channelId: "a", projectId: "p" } as Parameters<typeof liveQuotaStatusMatches>[0];
    expect(liveQuotaStatusMatches(status, "a", "p")).toBe(true); expect(liveQuotaStatusMatches(status, "b", "p")).toBe(false); expect(liveQuotaStatusMatches(status, "a", "q")).toBe(false); expect(liveQuotaStatusMatches(status, "", "")).toBe(false);
    expect(reportedQuotaNumber(0)).toBe("0"); expect(reportedQuotaNumber(null)).toBe("Unknown"); expect(reportedQuotaNumber(Number.NaN)).toBe("Unknown"); expect(reportedQuotaNumber(-1)).toBe("Unknown");
  });
  it("does not invoke native commands from an ordinary browser", () => {
    mocks.desktop.mockReturnValue(false); expect(() => loadLiveQuotaStatus("a")).toThrow("desktop app"); expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
