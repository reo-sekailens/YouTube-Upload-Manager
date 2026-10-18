import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LiveQuotaPanel, LiveQuotaReport, readLiveQuotaForChannel } from "./LiveQuotaPanel";
import type { LiveQuotaSnapshot, LiveQuotaStatus } from "../lib/live-quota";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), status: vi.fn(), refresh: vi.fn(), begin: vi.fn(), cancel: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection }));
vi.mock("../lib/live-quota", async (original) => ({ ...await original<object>(), loadLiveQuotaStatus: mocks.status, refreshLiveQuota: mocks.refresh, beginLiveQuotaAuthorization: mocks.begin, cancelLiveQuotaAuthorization: mocks.cancel }));

const snapshot: LiveQuotaSnapshot = { fetchedAt: "2026-10-19T08:10:00Z", periodStart: "2026-10-19T07:00:00Z", periodEnd: "2026-10-20T07:00:00Z", metricLagSeconds: 240, limits: [{ metric: "youtube.googleapis.com/default", displayName: "Queries per day", unit: "1/d/{project}", dimensions: { service: "youtube.googleapis.com" }, effectiveLimit: 10000, used: 9958, remaining: 42, sampleThrough: "2026-10-19T08:05:00Z", state: "reported" }] };
const connected: LiveQuotaStatus = { phase: "connected", channelId: "channel-a", projectId: "project-a", detail: "Read-only Cloud access connected.", attemptId: null, snapshot };

describe("Google quota report", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.status.mockResolvedValue(connected); mocks.refresh.mockResolvedValue(connected); });
  it("renders Google's numbers with project identity, fetch and sample timestamps, and sampling lag", () => {
    const markup = renderToStaticMarkup(<LiveQuotaReport snapshot={snapshot} projectId="project-a" timeZone="UTC" />);
    for (const text of ["Google-reported quota", "project-a", "all apps and devices", "Fetched from Google", "Used", "Daily limit", "Remaining", "9,958", "10,000", ">42<", "Usage sampled through", "4 minutes after sampling", "not an instantaneous allowance", "separate channel upload allowance", "youtube.googleapis.com/default"]) expect(markup).toContain(text);
    expect(markup).not.toContain("This app/device observed");
  });
  it("never infers zero usage or a full balance from a missing metric", () => {
    const markup = renderToStaticMarkup(<LiveQuotaReport snapshot={{ ...snapshot, limits: [{ ...snapshot.limits[0], used: null, remaining: null, sampleThrough: null, state: "no_usage_data" }] }} projectId="project-a" timeZone="UTC" />);
    expect(markup.match(/Unknown/g)).toHaveLength(2);
    expect(markup).toContain("Missing data does not mean zero usage"); expect(markup).toContain("No usage sample returned");
    expect(markup).not.toContain(">0<"); expect(markup).not.toContain(">42<");
  });
  it("keeps effective limits and unsupported/empty usage counters explicit", () => {
    const markup = renderToStaticMarkup(<LiveQuotaReport snapshot={{ ...snapshot, limits: [{ ...snapshot.limits[0], used: null, remaining: null, state: "unsupported_bucket" }] }} projectId="project-a" timeZone="UTC" />);
    expect(markup).toContain("not exposed a supported usage counter"); expect(markup).toContain("10,000");
    const empty = renderToStaticMarkup(<LiveQuotaReport snapshot={{ ...snapshot, limits: [] }} projectId="project-a" timeZone="UTC" />);
    expect(empty).toContain("Google returned no supported daily quota buckets. Remaining quota is unknown.");
  });
  it("performs no Google operation while rendering or when project identity is absent", () => {
    const markup = renderToStaticMarkup(<LiveQuotaPanel expectedChannelId="channel-a" expectedProjectId="project-a" timeZone="UTC" />);
    expect(markup).toContain("Loading saved Google Cloud connection");
    const missing = renderToStaticMarkup(<LiveQuotaPanel expectedChannelId="channel-a" timeZone="UTC" />);
    expect(missing).toContain("OAuth JSON does not identify a Cloud project");
    for (const method of [mocks.status, mocks.refresh, mocks.begin, mocks.cancel]) expect(method).not.toHaveBeenCalled();
  });
  it("loads saved native status without refreshing Google and makes metrics refresh explicit", async () => {
    expect(await readLiveQuotaForChannel("channel-a", "project-a", () => true)).toEqual(connected);
    expect(mocks.status).toHaveBeenCalledExactlyOnceWith("channel-a"); expect(mocks.refresh).not.toHaveBeenCalled();
    expect(await readLiveQuotaForChannel("channel-a", "project-a", () => true, true)).toEqual(connected);
    expect(mocks.refresh).toHaveBeenCalledExactlyOnceWith("channel-a");
  });
  it("rejects changed channels before the native call and foreign project receipts afterward", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(readLiveQuotaForChannel("channel-a", "project-a", () => true, true)).rejects.toThrow("connected channel changed"); expect(mocks.refresh).not.toHaveBeenCalled();
    mocks.refresh.mockResolvedValueOnce({ ...connected, projectId: "project-b" });
    await expect(readLiveQuotaForChannel("channel-a", "project-a", () => true, true)).rejects.toThrow("Cloud project changed");
  });
  it("suppresses a late native result after close and propagates provider errors without invented numbers", async () => {
    let resolve!: (status: LiveQuotaStatus) => void;
    mocks.refresh.mockReturnValueOnce(new Promise<LiveQuotaStatus>((done) => { resolve = done; }));
    let current = true; const request = readLiveQuotaForChannel("channel-a", "project-a", () => current, true);
    await vi.waitFor(() => expect(mocks.refresh).toHaveBeenCalledOnce()); current = false; resolve(connected);
    expect(await request).toBeUndefined();
    mocks.refresh.mockRejectedValueOnce(new Error("Monitoring permission denied."));
    await expect(readLiveQuotaForChannel("channel-a", "project-a", () => true, true)).rejects.toThrow("Monitoring permission denied");
  });
});
