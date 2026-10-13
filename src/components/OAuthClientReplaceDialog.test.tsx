import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionSettings, OAuthClientReplacementReview, OAuthClientReplacementStatus } from "../lib/types";
import { OAuthClientReplaceDialog, OAuthReplacementLimitSummary, OAuthReplacementReview, prepareReplacementForChannel, replacementCompletedSettings, startReplacementForChannel } from "./OAuthClientReplaceDialog";

const mocks = vi.hoisted(() => ({ prepare: vi.fn(), begin: vi.fn(), status: vi.fn(), cancel: vi.fn(), connection: vi.fn(), open: vi.fn() }));
vi.mock("../lib/local", () => ({ prepareOAuthClientReplacement: mocks.prepare, beginOAuthClientReplacement: mocks.begin, loadOAuthClientReplacementStatus: mocks.status, cancelOAuthClientReplacement: mocks.cancel, loadConnectionSettings: mocks.connection }));
vi.mock("../lib/google-authorization", () => ({ openAndCopyGoogleAuthorization: mocks.open }));

const settings: ConnectionSettings = { connected: true, activeChannelId: "channel-a", activeChannel: "Event channel" };
const review: OAuthClientReplacementReview = { reviewId: "opaque-review-a", channelId: "channel-a", channelName: "Event channel", cloudQuotaPaused: true, channelUploadPaused: true };
const started = { authorizationUrl: "https://accounts.google.com/o/oauth2/v2/auth?state=opaque", attemptId: "attempt-a" };

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("stepped account OAuth JSON replacement", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.connection.mockResolvedValue(settings);
    mocks.prepare.mockResolvedValue(review);
    mocks.begin.mockResolvedValue(started);
    mocks.cancel.mockResolvedValue(settings);
  });

  it("starts at file selection without preparing credentials or opening Google", () => {
    const markup = renderToStaticMarkup(<OAuthClientReplaceDialog channelName="Event channel" expectedChannelId="channel-a" onClose={() => undefined} onReplaced={() => undefined} />);
    expect(markup).toContain("<dialog");
    expect(markup).toContain('aria-modal="true"');
    expect(markup).toContain("aria-labelledby=");
    expect(markup).toContain("aria-describedby=");
    expect(markup).toContain('aria-label="OAuth JSON replacement steps"');
    expect(markup).toContain('aria-current="step"');
    expect(markup).toContain("Choose JSON file");
    expect(markup).toContain("Review");
    expect(markup).toContain("Reconnect");
    expect(markup).toContain("Event channel");
    expect(markup).not.toContain("Continue to Google");
    expect(mocks.prepare).not.toHaveBeenCalled();
    expect(mocks.begin).not.toHaveBeenCalled();
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("prioritizes Cloud quota and keeps the channel pause visible without promising a reset", () => {
    const markup = renderToStaticMarkup(<OAuthReplacementLimitSummary limit={{ kind: "api_quota", channelResumesAt: "2026-10-14T08:00:00Z" }} />);
    expect(markup.indexOf("Google Cloud API quota")).toBeLessThan(markup.indexOf("YouTube channel upload limit"));
    expect(markup).toContain("does not guarantee more API quota");
    expect(markup).toContain("Changing the JSON does not reset this limit");
    expect(markup).toContain("saved 24-hour upload pause stays in place");
  });

  it("uses native review limit flags instead of stale account projections", () => {
    const markup = renderToStaticMarkup(<OAuthReplacementLimitSummary limit={{ kind: "api_quota" }} review={{ ...review, cloudQuotaPaused: false, channelUploadPaused: true }} />);
    expect(markup).not.toContain("Google Cloud API quota");
    expect(markup).toContain("YouTube channel upload limit");
    expect(renderToStaticMarkup(<OAuthReplacementLimitSummary limit={{ kind: "none" }} />)).toBe("");
  });

  it("reviews only safe configuration metadata and the same channel", () => {
    const markup = renderToStaticMarkup(<OAuthReplacementReview review={{ ...review, channelName: "<Event channel>" }} />);
    expect(markup).toContain("&lt;Event channel&gt;");
    expect(markup).not.toContain("<Event channel>");
    expect(markup).toContain("Validated Google Desktop OAuth JSON");
    expect(markup).toContain("Google confirms this same channel");
    expect(markup).toContain("Queued videos and saved upload progress");
    expect(markup).not.toContain("opaque-review-a");
    expect(markup).not.toContain("channel-a");
  });

  it("stages the operator-selected path only for the reviewed connected channel", async () => {
    expect(await prepareReplacementForChannel("selected-client.json", "channel-a", () => true)).toEqual(review);
    expect(mocks.prepare).toHaveBeenCalledExactlyOnceWith("selected-client.json", "channel-a");
    expect(mocks.begin).not.toHaveBeenCalled();
    expect(mocks.cancel).not.toHaveBeenCalled();
  });

  it("rejects a changed or disconnected account before native preparation", async () => {
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(prepareReplacementForChannel("client.json", "channel-a", () => true)).rejects.toThrow("connected channel changed");
    mocks.connection.mockResolvedValueOnce({ connected: false, activeChannelId: "channel-a" });
    await expect(prepareReplacementForChannel("client.json", "channel-a", () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.prepare).not.toHaveBeenCalled();
  });

  it("does not stage an obsolete selection and cancels a late native review once", async () => {
    expect(await prepareReplacementForChannel("client.json", "channel-a", () => false)).toBeUndefined();
    expect(mocks.prepare).not.toHaveBeenCalled();
    const preparing = deferred<OAuthClientReplacementReview>();
    mocks.prepare.mockReturnValueOnce(preparing.promise);
    let active = true;
    const request = prepareReplacementForChannel("client.json", "channel-a", () => active);
    await vi.waitFor(() => expect(mocks.prepare).toHaveBeenCalledOnce());
    active = false;
    preparing.resolve(review);
    expect(await request).toBeUndefined();
    expect(mocks.cancel).toHaveBeenCalledExactlyOnceWith("opaque-review-a");
  });

  it("cancels a review returned for another channel rather than displaying it", async () => {
    mocks.prepare.mockResolvedValueOnce({ ...review, channelId: "channel-b" });
    await expect(prepareReplacementForChannel("client.json", "channel-a", () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.cancel).toHaveBeenCalledExactlyOnceWith("opaque-review-a");
    expect(mocks.begin).not.toHaveBeenCalled();
  });

  it("starts only the reviewed replacement and allows an explicit retry of it", async () => {
    expect(await startReplacementForChannel(review, "channel-a", () => true)).toEqual(started);
    expect(mocks.begin).toHaveBeenCalledExactlyOnceWith("opaque-review-a", "channel-a");
    mocks.begin.mockRejectedValueOnce("Google authorization expired. Reconnect again.");
    await expect(startReplacementForChannel(review, "channel-a", () => true)).rejects.toBe("Google authorization expired. Reconnect again.");
    expect(await startReplacementForChannel(review, "channel-a", () => true)).toEqual(started);
    expect(mocks.begin).toHaveBeenCalledTimes(3);
  });

  it("rejects an unreviewed or changed channel before starting Google consent", async () => {
    await expect(startReplacementForChannel({ ...review, channelId: "channel-b" }, "channel-a", () => true)).rejects.toThrow("connected channel changed");
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(startReplacementForChannel(review, "channel-a", () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.begin).not.toHaveBeenCalled();
  });

  it("cancels a late authorization start after the dialog loses ownership", async () => {
    const starting = deferred<typeof started>();
    mocks.begin.mockReturnValueOnce(starting.promise);
    let active = true;
    const request = startReplacementForChannel(review, "channel-a", () => active);
    await vi.waitFor(() => expect(mocks.begin).toHaveBeenCalledOnce());
    active = false;
    starting.resolve(started);
    expect(await request).toBeUndefined();
    expect(mocks.cancel).toHaveBeenCalledExactlyOnceWith("opaque-review-a");
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("accepts a successful receipt only with a connected matching channel", () => {
    expect(replacementCompletedSettings({ state: "completed", settings }, "channel-a")).toEqual(settings);
    for (const state of ["ready", "authorizing", "failed", "cancelled"] as const) expect(replacementCompletedSettings({ state }, "channel-a")).toBeUndefined();
    expect(() => replacementCompletedSettings({ state: "completed" }, "channel-a")).toThrow("replacement could not be confirmed");
    const wrong: OAuthClientReplacementStatus = { state: "completed", settings: { ...settings, activeChannelId: "channel-b" } };
    expect(() => replacementCompletedSettings(wrong, "channel-a")).toThrow("connected channel changed");
    expect(() => replacementCompletedSettings({ state: "completed", settings: { ...settings, connected: false } }, "channel-a")).toThrow("connected channel changed");
  });
});
