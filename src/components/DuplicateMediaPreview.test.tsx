import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { DuplicateMediaPreviewEvidence } from "../lib/types";
import {
  createDuplicatePreviewSession,
  DuplicateMediaPreview,
  DuplicateMediaPreviewContent,
} from "./DuplicateMediaPreview";

function evidence(): DuplicateMediaPreviewEvidence {
  return {
    localSources: [{ token: "opaque-source-a", url: "https://media-preview.localhost/opaque-source-a", title: "Camera original", fileName: "camera.mp4" }],
    youtubeVideos: [{ videoId: "matched-video", title: "Uploaded camera video" }],
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

describe("duplicate playback comparison", () => {
  it("starts with an accessible modal and no player before native preparation", () => {
    const markup = renderToStaticMarkup(<DuplicateMediaPreview expectedChannelId="channel-a" onClose={() => undefined} target={{ kind: "upload", itemId: "item-a" }} />);
    expect(markup).toContain("<dialog");
    expect(markup).toContain('aria-modal="true"');
    expect(markup).toContain("aria-labelledby=");
    expect(markup).toContain("aria-describedby=");
    expect(markup).toContain('aria-label="Close video comparison"');
    expect(markup).toContain("Preparing video comparison…");
    expect(markup).not.toContain("<video");
    expect(markup).not.toContain("<iframe");
  });

  it("shows native local playback beside the actual matched YouTube embed without autoplay", () => {
    const markup = renderToStaticMarkup(<DuplicateMediaPreviewContent evidence={evidence()} />);
    expect(markup).toContain('<video aria-label="Local video: camera.mp4"');
    expect(markup).toContain('controls=""');
    expect(markup).toContain('preload="metadata"');
    expect(markup).toContain('src="https://media-preview.localhost/opaque-source-a"');
    expect(markup).toContain("https://www.youtube.com/embed/matched-video?");
    expect(markup).toContain('title="Matched YouTube video: Uploaded camera video"');
    expect(markup).toContain('referrerPolicy="strict-origin-when-cross-origin"');
    expect(markup).toContain("min-h-[200px]");
    expect(markup).toContain("md:grid-cols-2");
    expect(markup).toContain("Use compatibility player");
    expect(markup).toContain("If the picture stays black or only audio plays");
    expect(markup).toContain('allow="autoplay;');
    expect(markup).not.toMatch(/[?&]autoplay=/i);
    expect(markup).not.toContain('autoplay=""');
    expect(markup).not.toContain("<a ");
  });

  it("mounts only the selected online match and provides its other matched titles in a selector", () => {
    const value = evidence();
    value.youtubeVideos.push({ videoId: "second-match", title: "Another uploaded match" });
    const markup = renderToStaticMarkup(<DuplicateMediaPreviewContent evidence={value} />);
    expect(markup.match(/<iframe/g)).toHaveLength(1);
    expect(markup).toContain("YouTube match to compare");
    expect(markup).toContain("Another uploaded match");
    expect(markup).not.toContain("/embed/second-match");
  });

  it("compares two local sources simultaneously when no online match exists", () => {
    const value = evidence();
    value.youtubeVideos = [];
    value.localSources.push({ token: "opaque-source-b", url: "https://media-preview.localhost/opaque-source-b", title: "Earlier local upload", fileName: "earlier.mp4" });
    const markup = renderToStaticMarkup(<DuplicateMediaPreviewContent evidence={value} />);
    expect(markup.match(/<video /g)).toHaveLength(2);
    expect(markup).toContain('aria-label="Local video: camera.mp4"');
    expect(markup).toContain('aria-label="Local video: earlier.mp4"');
    expect(markup).toContain("Matching local file to compare");
    expect(markup).not.toContain("<iframe");
  });

  it("compares both online uploads simultaneously when no local source exists", () => {
    const value = evidence();
    value.localSources = [];
    value.youtubeVideos.push({ videoId: "second-match", title: "Another uploaded match" });
    const markup = renderToStaticMarkup(<DuplicateMediaPreviewContent evidence={value} />);
    expect(markup.match(/<iframe /g)).toHaveLength(2);
    expect(markup).toContain("/embed/matched-video");
    expect(markup).toContain("/embed/second-match");
    expect(markup).not.toContain("<video");
  });

  it("keeps partial matches usable and explains an unavailable source", () => {
    const value = evidence();
    value.localSources = [];
    const markup = renderToStaticMarkup(<DuplicateMediaPreviewContent evidence={value} />);
    expect(markup).toContain("/embed/matched-video");
    expect(markup).toContain("No available local file was found");
  });

  it("preserves the unavailable local pane beside online evidence instead of replacing it with a second online player", () => {
    const value = evidence();
    value.localSources = [];
    value.unavailableLocalSources = 1;
    value.youtubeVideos.push({ videoId: "second-match", title: "Another uploaded match" });
    const markup = renderToStaticMarkup(<DuplicateMediaPreviewContent evidence={value} />);
    expect(markup.match(/<iframe /g)).toHaveLength(1);
    expect(markup).toContain("1 local file is unavailable for playback");
    expect(markup).toContain("Suspected local duplicate");
    expect(markup).toContain("The original file could not be opened for playback");
    expect(markup).toContain("/embed/matched-video");
    expect(markup).not.toContain("/embed/second-match");
  });

  it("reports unavailable local files without hiding the playable local and remote matches", () => {
    const value = evidence();
    value.unavailableLocalSources = 2;
    const markup = renderToStaticMarkup(<DuplicateMediaPreviewContent evidence={value} />);
    expect(markup).toContain("2 local files are unavailable for playback");
    expect(markup.match(/<video /g)).toHaveLength(1);
    expect(markup.match(/<iframe /g)).toHaveLength(1);
    expect(markup).toContain("Available matches are shown below");
  });
});

describe("preview grant ownership", () => {
  it("releases each granted token once after a successfully displayed preview", async () => {
    const value = evidence();
    value.localSources.push({ ...value.localSources[0] });
    const ready = vi.fn();
    const fail = vi.fn();
    const release = vi.fn(async (_tokens: string[]) => undefined);
    const dispose = createDuplicatePreviewSession(Promise.resolve(value), ready, fail, release);
    await Promise.resolve();
    expect(ready).toHaveBeenCalledWith(value);
    expect(release).not.toHaveBeenCalled();
    dispose();
    dispose();
    expect(release).toHaveBeenCalledExactlyOnceWith(["opaque-source-a"]);
    expect(fail).not.toHaveBeenCalled();
  });

  it("releases a late acquisition without exposing it after close or channel change", async () => {
    const pending = deferred<DuplicateMediaPreviewEvidence>();
    const ready = vi.fn();
    const fail = vi.fn();
    const release = vi.fn(async (_tokens: string[]) => undefined);
    const dispose = createDuplicatePreviewSession(pending.promise, ready, fail, release);
    dispose();
    pending.resolve(evidence());
    await pending.promise;
    expect(release).toHaveBeenCalledExactlyOnceWith(["opaque-source-a"]);
    expect(ready).not.toHaveBeenCalled();
    expect(fail).not.toHaveBeenCalled();
    dispose();
    expect(release).toHaveBeenCalledTimes(1);
  });

  it("disposes obsolete requests independently of the currently displayed preview", async () => {
    const oldRequest = deferred<DuplicateMediaPreviewEvidence>();
    const oldReady = vi.fn();
    const newReady = vi.fn();
    const release = vi.fn(async (_tokens: string[]) => undefined);
    const disposeOld = createDuplicatePreviewSession(oldRequest.promise, oldReady, vi.fn(), release);
    disposeOld();
    const next = evidence();
    next.localSources[0] = { ...next.localSources[0], token: "new-source" };
    const disposeNew = createDuplicatePreviewSession(Promise.resolve(next), newReady, vi.fn(), release);
    oldRequest.resolve(evidence());
    await Promise.resolve();
    expect(oldReady).not.toHaveBeenCalled();
    expect(newReady).toHaveBeenCalledWith(next);
    expect(release).toHaveBeenCalledExactlyOnceWith(["opaque-source-a"]);
    disposeNew();
    expect(release).toHaveBeenLastCalledWith(["new-source"]);
  });

  it("reports active preparation errors and ignores errors received after closing", async () => {
    const fail = vi.fn();
    const pending = deferred<DuplicateMediaPreviewEvidence>();
    const dispose = createDuplicatePreviewSession(pending.promise, vi.fn(), fail, vi.fn(async () => undefined));
    dispose();
    pending.reject(new Error("private native detail"));
    await Promise.resolve();
    expect(fail).not.toHaveBeenCalled();
    createDuplicatePreviewSession(Promise.reject(new Error("private native detail")), vi.fn(), fail, vi.fn(async () => undefined));
    await Promise.resolve();
    expect(fail).toHaveBeenCalledExactlyOnceWith();
  });
});
