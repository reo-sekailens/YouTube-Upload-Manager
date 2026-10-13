import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UploadPlanDialog, UploadPlanFile, UploadPlanReview, refreshPlanPlaylistsForChannel, resolvePlanForChannel } from "./UploadPlanDialog";
import { planWithPublication } from "../lib/upload-plan";
import type { ManualUploadSettings } from "../lib/types";
import type { UploadPlanPage } from "../lib/upload-plan";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), playlists: vi.fn(), create: vi.fn(), discard: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection, listYouTubePlaylists: mocks.playlists }));
vi.mock("../lib/manual-upload-defaults", () => ({ loadManualUploadDefaults: vi.fn() }));
vi.mock("../lib/upload-plan", async (original) => ({ ...await original<object>(), createUploadPlan: mocks.create, discardUploadPlan: mocks.discard }));

const settings: ManualUploadSettings = { madeForKids: true, visibility: "unlisted", deleteSourceAfterUpload: true, playlistId: "playlist-a", playlistTitle: "Event archive", metadata: { description: "Template description", notifySubscribers: false }, itemMetadata: { "/chosen/a.mp4": { title: "Deliberate override" } } };
const plan: UploadPlanPage = { id: "preview-a", channelId: "channel-a", channelName: "Channel A", createdAt: new Date().toISOString(), expiresAt: new Date(Date.now() + 900_000).toISOString(), evidenceMode: "cached", evidenceAsOf: "2026-10-12T08:00:00Z", totalFiles: 1, readyFiles: 0, duplicateFiles: 1, blockedFiles: 0, offset: 0, limit: 25, warnings: ["Offline evidence may be stale."], files: [{ ordinal: 0, fileName: "a.mp4", title: "<Reviewed title>", channelId: "channel-a", visibility: "unlisted", madeForKids: true, playlistId: "playlist-a", playlistTitle: "Event archive", playlistEvidence: "unverified_cached", deleteSourceAfterUpload: true, sizeBytes: 4_000_000, sourceStatus: "verified", duplicateDisposition: "review_duplicate", duplicateMatches: ["Exact saved bytes (uploaded): Candidate"], metadata: { title: "<Reviewed title>", description: "Resolved description", tags: ["event", "camera"], publishAt: "2026-11-01T08:00:00Z", notifySubscribers: false }, warnings: ["Ownership is unverified offline."] }] };

describe("stepped whole-batch upload review", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.create.mockResolvedValue(plan); mocks.discard.mockResolvedValue(undefined); mocks.playlists.mockResolvedValue([{ id: "playlist-a", title: "Event archive" }]); });

  it("starts at Select and leaves planning, jobs and provider reads untouched", () => {
    const markup = renderToStaticMarkup(<UploadPlanDialog expectedChannelId="channel-a" activeChannel="Channel A" initialPaths={["/private/a.mp4"]} initialSettings={settings} onClose={() => undefined} />);
    expect(markup).toContain("Select videos to preview"); expect(markup).toContain('aria-modal="true"');
    expect(markup).toContain('aria-label="Batch preview steps"'); expect(markup).toContain("Resolve choices"); expect(markup).toContain("Review");
    expect(markup).toContain("originals stay in place"); expect(markup).toContain("a.mp4"); expect(markup).not.toContain("/private/a.mp4");
    expect(mocks.create).not.toHaveBeenCalled(); expect(mocks.connection).not.toHaveBeenCalled(); expect(mocks.playlists).not.toHaveBeenCalled();
  });

  it("reviews each file’s complete resolved choices, duplicate evidence and cached age safely", () => {
    const markup = renderToStaticMarkup(<UploadPlanReview plan={plan} />);
    for (const text of ["channel-a", "unlisted", "Made for kids", "Event archive", "ownership pending", "bytes verified", "Delete only after YouTube processing succeeds", "Resolved description", "event, camera", "2026-11-01T08:00:00Z", "false", "Exact saved bytes", "Duplicate candidate", "Offline · cached YouTube evidence", "evidence from"]) expect(markup).toContain(text);
    expect(markup).toContain("&lt;Reviewed title&gt;"); expect(markup).not.toContain("<Reviewed title>");
  });

  it("resolves all selected choices with an offline default and checks the channel twice", async () => {
    expect(await resolvePlanForChannel(["/chosen/a.mp4"], settings, "channel-a", false, () => true)).toEqual(plan);
    expect(mocks.create).toHaveBeenCalledExactlyOnceWith(["/chosen/a.mp4"], settings, "channel-a", false);
    expect(mocks.connection).toHaveBeenCalledTimes(2); expect(mocks.discard).not.toHaveBeenCalled(); expect(mocks.playlists).not.toHaveBeenCalled();
  });

  it("rejects a changed channel before starting native source reads", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(resolvePlanForChannel(["/chosen/a.mp4"], settings, "channel-a", true, () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.create).not.toHaveBeenCalled();
  });

  it("revokes a late created plan after cancellation without accepting it", async () => {
    let resolve!: (value: UploadPlanPage) => void;
    mocks.create.mockReturnValue(new Promise<UploadPlanPage>((done) => { resolve = done; }));
    let current = true;
    const request = resolvePlanForChannel(["/chosen/a.mp4"], settings, "channel-a", false, () => current);
    await vi.waitFor(() => expect(mocks.create).toHaveBeenCalledOnce()); current = false; resolve(plan);
    expect(await request).toBeUndefined(); expect(mocks.discard).toHaveBeenCalledExactlyOnceWith("preview-a", "channel-a");
  });

  it("revokes a receipt when the connection changes during byte verification", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-a" }).mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(resolvePlanForChannel(["/chosen/a.mp4"], settings, "channel-a", false, () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.discard).toHaveBeenCalledExactlyOnceWith("preview-a", "channel-a");
  });

  it("rejects a foreign or already expired plan rather than approving stale choices", async () => {
    for (const result of [{ ...plan, channelId: "channel-b" }, { ...plan, expiresAt: "2020-01-01T00:00:00Z" }]) {
      mocks.create.mockResolvedValueOnce(result);
      await expect(resolvePlanForChannel(["/chosen/a.mp4"], settings, "channel-a", false, () => true)).rejects.toThrow();
    }
    expect(mocks.discard).toHaveBeenCalledTimes(2);
  });

  it("refreshes playlists explicitly and suppresses stale owner-read results", async () => {
    expect(await refreshPlanPlaylistsForChannel("channel-a", () => true)).toEqual([{ id: "playlist-a", title: "Event archive" }]);
    expect(mocks.connection).toHaveBeenCalledTimes(2); expect(mocks.playlists).toHaveBeenCalledOnce();
    expect(await refreshPlanPlaylistsForChannel("channel-a", () => false)).toBeUndefined(); expect(mocks.playlists).toHaveBeenCalledOnce();
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-a" }).mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(refreshPlanPlaylistsForChannel("channel-a", () => true)).rejects.toThrow("connected channel changed");
  });

  it("shows reviewed UTC and timezone evidence with distinct absent attachment results", () => {
    const markup = renderToStaticMarkup(<UploadPlanFile file={{ ...plan.files[0], visibility: "private", publicationContext: { timeZone: "Asia/Singapore", tzdbVersion: "2025b" } }} />);
    for (const text of ["Publication schedule", "2026-11-01 08:00:00 UTC", "Asia/Singapore", "rules 2025b", "Local time: 2026-11-01 16:00", "separate confirmed publication job", "Thumbnail", "Captions", "None selected", "Notify subscribers", "Resolved metadata"]) expect(markup).toContain(text);
    expect(markup).not.toContain("asset verified");
  });

  it("discloses an unavailable timezone receipt and explicit empty metadata without fabricating verification", () => {
    const markup = renderToStaticMarkup(<UploadPlanFile file={{ ...plan.files[0], metadata: { description: "", tags: [], publishAt: "2026-11-01T08:00:00Z" } }} />);
    expect(markup).toContain("Timezone receipt unavailable"); expect(markup.match(/Empty \(explicit\)/g)).toHaveLength(2); expect(markup).toContain("None selected"); expect(markup).not.toContain("rules 2025b");
  });

  it("submits advanced metadata and timezone context only to read-only plan resolution", async () => {
    const paths = ["/chosen/a.mp4"];
    const advanced = planWithPublication(paths, settings, { publishAtByPath: { [paths[0]]: "2027-01-01T00:00:00Z" }, timeZone: "UTC", tzdbVersion: "2025b" });
    await resolvePlanForChannel(paths, advanced, "channel-a", false, () => true);
    expect(mocks.create).toHaveBeenCalledExactlyOnceWith(paths, advanced, "channel-a", false); expect(mocks.playlists).not.toHaveBeenCalled(); expect(mocks.discard).not.toHaveBeenCalled();
    expect(advanced.itemMetadata?.[paths[0]]).toEqual({ title: "Deliberate override", publishAt: "2027-01-01T00:00:00Z" });
  });

  it("shows native sealed attachment declarations without claiming provider completion", () => {
    const markup = renderToStaticMarkup(<UploadPlanFile file={{ ...plan.files[0], assetJobs: [{ kind: "thumbnail", fileName: "cover <original>.png", dimensions: "1280 × 720" }, { kind: "caption", fileName: "session.srt", cueCount: 3, language: "en", name: "English", draft: false }] }} />);
    for (const text of ["cover &lt;original&gt;.png", "1280 × 720", "selected original", "session.srt", "3 timed cues", "visible caption requested", "inspected for this preview", "checked again before import", "only after the video receipt is confirmed"]) expect(markup).toContain(text);
    expect(markup).not.toContain("None selected"); expect(markup).not.toContain("Attachment confirmed");
  });
});
