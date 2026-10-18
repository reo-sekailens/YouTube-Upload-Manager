import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { ComparisonSyncState } from "../lib/comparison-sync";
import type { DuplicateMediaPreviewTarget } from "../lib/types";
import { createRemoteComparisonSession } from "./DuplicateComparisonPlayers";
import { comparisonTimeLabel, DuplicateComparisonControls, RemoteComparisonSetup } from "./DuplicateComparisonTools";

const state: ComparisonSyncState = { time: 12.5, start: 0, duration: 60, playing: false, waiting: false, ready: true, offset: 5, linked: true, audio: "left" };
const controls = { play: vi.fn(), pause: vi.fn(), seek: vi.fn(), setOffset: vi.fn(), setLinked: vi.fn(), setAudio: vi.fn() };

describe("comparison controls and setup", () => {
  it("exposes shared playback, seek, alignment, link and single-side audio controls", () => {
    const markup = renderToStaticMarkup(<DuplicateComparisonControls state={state} controls={controls} hasPair />);
    expect(markup).toContain('aria-label="Synchronized comparison controls"');
    expect(markup).toContain("Play both");
    expect(markup).toContain('aria-label="Comparison timeline"');
    expect(markup).toContain("Right video offset (seconds)");
    expect(markup).toContain('value="5"');
    expect(markup).toContain("0:10 on the left with 0:15 on the right");
    expect(markup).toContain("Link players");
    expect(markup).toContain("Mute both");
    expect(markup).toContain("synchronization is approximate");
    expect(markup).toContain("0:12");
    expect(markup).toContain("Alignment and audio · +5s offset");
    expect(markup).toContain("<details");
    expect(markup).not.toMatch(/<details[^>]*\sopen(?:[=>\s])/);
  });
  it("explains unavailable controls while preserving individual player access", () => {
    const markup = renderToStaticMarkup(<DuplicateComparisonControls state={{ ...state, ready: false }} hasPair />);
    expect(markup).toContain("Waiting for both players");
    expect(markup).toContain("Individual player controls remain available");
    expect(markup).toContain('disabled=""');
    const single = renderToStaticMarkup(<DuplicateComparisonControls state={state} hasPair={false} />);
    expect(single).toContain("Two available videos are needed");
  });
  it("keeps alignment controls collapsed by default and identifies a negative offset in the summary", () => {
    const markup = renderToStaticMarkup(<DuplicateComparisonControls state={{ ...state, offset: -3 }} controls={controls} hasPair />);
    expect(markup).toContain("Alignment and audio · -3s offset");
    expect(markup.indexOf("Play both")).toBeLessThan(markup.indexOf("<details"));
    expect(markup.indexOf('aria-label="Comparison timeline"')).toBeLessThan(markup.indexOf("<details"));
    const zero = renderToStaticMarkup(<DuplicateComparisonControls state={{ ...state, offset: 0 }} controls={controls} hasPair />);
    expect(zero).toMatch(/<summary[^>]*>Alignment and audio<\/summary>/);
  });
  it("opens setup as a labelled step within the existing modal and requires a selected tool", () => {
    const markup = renderToStaticMarkup(<RemoteComparisonSetup runtimeConfigured={false} onSaved={vi.fn()} onClose={vi.fn()} />);
    expect(markup).toContain("Step 1 of 2");
    expect(markup).toContain("Choose yt-dlp");
    expect(markup).toContain("Choose runtime");
    expect(markup).toContain("Deno (recommended) or Node.js 22 or later");
    expect(markup).toContain("does not guarantee access to every video");
    expect(markup).toContain("does not download tools");
    expect(markup).toContain("Browser session access is a separate, optional setup");
    expect(markup).toMatch(/disabled=""[^>]*>Review setup/);
    expect(markup).not.toContain("<dialog");
  });
  it("formats long and invalid timestamps consistently", () => {
    expect(comparisonTimeLabel(0)).toBe("0:00");
    expect(comparisonTimeLabel(3_661.75)).toBe("1:01:01");
    expect(comparisonTimeLabel(-1)).toBe("0:00");
    expect(comparisonTimeLabel(Number.NaN)).toBe("0:00");
  });
});

describe("remote comparison grant ownership", () => {
  it("cancels pending extraction at close and does not display its late result", async () => {
    let complete!: (grant: { token: string; url: string }) => void;
    const ready = vi.fn();
    const failed = vi.fn();
    const prepare = vi.fn(() => new Promise<{ token: string; url: string }>((resolve) => { complete = resolve; }));
    const release = vi.fn(async (_tokens: string[]) => undefined);
    const dispose = createRemoteComparisonSession({ kind: "upload", itemId: "item-a" }, "channel-a", "video-a", ready, failed, prepare, release);
    const requestId = (prepare.mock.calls[0] as unknown[])[3] as string;
    expect(requestId).toMatch(/^[0-9a-f-]{36}$/);
    dispose(); dispose();
    expect(release).toHaveBeenCalledExactlyOnceWith([requestId]);
    complete({ token: requestId, url: "https://media-preview.localhost/opaque" });
    await Promise.resolve();
    expect(release).toHaveBeenCalledTimes(1);
    expect(ready).not.toHaveBeenCalled();
    expect(failed).not.toHaveBeenCalled();
  });
  it("keeps preparation scoped and releases its opaque grant exactly once", async () => {
    const ready = vi.fn();
    const release = vi.fn(async (_tokens: string[]) => undefined);
    const prepare = vi.fn(async (_target: DuplicateMediaPreviewTarget, _channel: string | undefined, _video: string, requestId = crypto.randomUUID()) => ({ token: requestId, url: "https://media-preview.localhost/opaque" }));
    const target = { kind: "candidate", candidateId: "candidate-a" } as const;
    const dispose = createRemoteComparisonSession(target, "channel-a", "video-a", ready, vi.fn(), prepare, release);
    await Promise.resolve();
    expect(prepare).toHaveBeenCalledWith(target, "channel-a", "video-a", expect.any(String));
    expect(ready).toHaveBeenCalledOnce();
    expect(release).not.toHaveBeenCalled();
    dispose(); dispose();
    expect(release).toHaveBeenCalledExactlyOnceWith([prepare.mock.calls[0][3]]);
  });
  it("reports active failures without exposing native error details or errors after close", async () => {
    const failed = vi.fn();
    const release = vi.fn(async (_tokens: string[]) => undefined);
    createRemoteComparisonSession({ kind: "upload", itemId: "item-a" }, "channel-a", "video-a", vi.fn(), failed, async () => { throw new Error("sensitive native path"); }, release);
    await Promise.resolve();
    expect(failed).toHaveBeenCalledExactlyOnceWith();
    const dispose = createRemoteComparisonSession({ kind: "upload", itemId: "item-a" }, "channel-a", "video-a", vi.fn(), failed, async () => { throw new Error("sensitive native path"); }, release);
    dispose();
    await Promise.resolve();
    expect(failed).toHaveBeenCalledOnce();
  });
});
