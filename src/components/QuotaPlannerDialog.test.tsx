import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { QuotaEstimateReview, QuotaHoldSummary, QuotaPlannerDialog, estimateQuotaForChannel, loadQuotaForChannel, saveReviewedQuotaLimits } from "./QuotaPlannerDialog";
import { defaultQuotaChoices, type QuotaEstimate, type QuotaSnapshot } from "../lib/quota-planner";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), load: vi.fn(), estimate: vi.fn(), save: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection }));
vi.mock("../lib/quota-planner", async (original) => ({ ...await original<object>(), loadQuotaPlanner: mocks.load, estimateQuotaOperations: mocks.estimate, saveQuotaProjectLimits: mocks.save }));

const snapshot: QuotaSnapshot = { channelId: "channel-a", channelName: "Channel A", scopeId: "scope-a", configuredProjectId: "configured-project", limits: {}, usage: [{ bucket: "video_uploads", requests: 2, estimatedUnits: 2 }, { bucket: "search", requests: 0, estimatedUnits: 0 }, { bucket: "other", requests: 3, estimatedUnits: 401 }], unknownRequests: 1, pacificDay: "2026-10-13", pacificResetAt: "2026-10-14T07:00:00Z", observedAt: "2026-10-13T09:00:00Z", cloudHold: { deadline: "2026-10-14T09:00:00Z", active: true }, channelHold: { deadline: "2026-10-15T09:00:00Z", active: true }, costVersion: "youtube-data-api-2026-10-13-v1", costAccessedAt: "2026-10-13", costSourceUrl: "https://developers.google.com/youtube/v3/determine_quota_cost", costs: [], observationLabel: "This app/device observed · all local channels in this Cloud project", warnings: ["Unrecorded requests and usage from other devices or apps are invisible."] };
const estimate: QuotaEstimate = { snapshot, operations: [{ label: "New video upload sessions", method: "videos.insert", bucket: "video_uploads", requests: 3, retryRequests: 3, costPerRequest: 1, estimatedUnits: 6 }], buckets: [{ bucket: "video_uploads", estimatedRequests: 6, estimatedUnits: 6, observedUnits: 2 }, { bucket: "search", estimatedRequests: 0, estimatedUnits: 0, observedUnits: 0 }, { bucket: "other", estimatedRequests: 0, estimatedUnits: 0, observedUnits: 401 }], warnings: ["Retry reserve does not authorize uncertain writes."] };

describe("stepped quota operation planner", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.load.mockResolvedValue(snapshot); mocks.estimate.mockResolvedValue(estimate); mocks.save.mockResolvedValue(snapshot); });
  it("opens the Google report separately from local estimates without provider calls", () => {
    const markup = renderToStaticMarkup(<QuotaPlannerDialog expectedChannelId="channel-a" activeChannel="Channel A" initialChoices={{ uploads: 3 }} onClose={() => undefined} />);
    expect(markup).toContain('aria-modal="true"'); expect(markup).toContain('aria-label="Quota report source"'); expect(markup).toContain("Check quota from Google"); expect(markup).toContain("Google report"); expect(markup).toContain("Local estimate"); expect(markup).not.toContain('aria-label="Quota planning steps"');
    expect(markup).toContain("Usage samples can be delayed; channel upload allowance remains unknown."); expect(markup).toContain("Loading local quota records");
    expect(mocks.load).not.toHaveBeenCalled(); expect(mocks.estimate).not.toHaveBeenCalled(); expect(mocks.save).not.toHaveBeenCalled();
  });
  it("shows bucket costs as estimates and never presents local counts as remaining quota", () => {
    const markup = renderToStaticMarkup(<QuotaEstimateReview estimate={estimate} proposedLimits={{ uploadCalls: 5 }} timeZone="Asia/Singapore" />);
    for (const text of ["Video upload calls", "Search calls", "Other API units", "Actual project remaining quota", "Unknown", "This app/device observed · all local channels in this Cloud project", "including retry reserve", "exceeds your entered limit", "unclassified requests", "youtube-data-api-2026-10-13-v1", "Retry reserve does not authorize uncertain writes."]) expect(markup).toContain(text);
    expect(markup).not.toContain("remaining quota: 3"); expect(markup).not.toContain("Guaranteed");
  });
  it("shows Cloud and channel pauses independently and preserves the channel deadline beyond reset", () => {
    const markup = renderToStaticMarkup(<QuotaHoldSummary snapshot={snapshot} timeZone="Asia/Singapore" />);
    for (const text of ["Google Cloud API quota", "YouTube channel upload limit", "Saved API pause is active", "Saved channel pause is active", "Asia/Singapore", "Pacific", "does not clear it", "does not guarantee acceptance"]) expect(markup).toContain(text);
  });
  it("loads only the reviewed channel and suppresses a cancelled result", async () => {
    expect(await loadQuotaForChannel("channel-a", () => true)).toEqual(snapshot); expect(mocks.connection).toHaveBeenCalledTimes(2);
    expect(await loadQuotaForChannel("channel-a", () => false)).toBeUndefined(); expect(mocks.load).toHaveBeenCalledOnce();
  });
  it("rejects a different connection before native estimation and a different project afterward", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(estimateQuotaForChannel("channel-a", "scope-a", defaultQuotaChoices(), () => true)).rejects.toThrow("connected channel changed"); expect(mocks.estimate).not.toHaveBeenCalled();
    mocks.estimate.mockResolvedValueOnce({ ...estimate, snapshot: { ...snapshot, scopeId: "scope-b" } });
    await expect(estimateQuotaForChannel("channel-a", "scope-a", defaultQuotaChoices(), () => true)).rejects.toThrow("Cloud project changed");
  });
  it("saves explicit limits only after reviewed approval, preserving zeros and unknown fields", async () => {
    await expect(saveReviewedQuotaLimits("channel-a", "scope-a", { uploadCalls: 0 }, false, () => true)).rejects.toThrow("approve"); expect(mocks.connection).not.toHaveBeenCalled(); expect(mocks.save).not.toHaveBeenCalled();
    expect(await saveReviewedQuotaLimits("channel-a", "scope-a", { uploadCalls: 0 }, true, () => true)).toEqual(snapshot);
    expect(mocks.save).toHaveBeenCalledExactlyOnceWith("channel-a", "scope-a", { uploadCalls: 0 });
  });
  it("rejects a foreign save receipt and suppresses a late save after close", async () => {
    mocks.save.mockResolvedValueOnce({ ...snapshot, channelId: "channel-b" });
    await expect(saveReviewedQuotaLimits("channel-a", "scope-a", {}, true, () => true)).rejects.toThrow("receipt did not match");
    let resolve!: (value: QuotaSnapshot) => void; mocks.save.mockReturnValueOnce(new Promise<QuotaSnapshot>((done) => { resolve = done; }));
    let current = true; const request = saveReviewedQuotaLimits("channel-a", "scope-a", {}, true, () => current);
    await vi.waitFor(() => expect(mocks.save).toHaveBeenCalledTimes(2)); current = false; resolve(snapshot);
    expect(await request).toBeUndefined();
  });
});
