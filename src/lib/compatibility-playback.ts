import type { DuplicatePreviewCompatibility } from "./duplicate-media-preview";

export const COMPATIBILITY_SEGMENT_SECONDS = 6;
export const COMPATIBILITY_BUFFER_BEHIND_SECONDS = 6;
export const COMPATIBILITY_BUFFER_AHEAD_SECONDS = 18;
const MAX_FRAGMENT_BYTES = 4 * 1024 * 1024;

export type CompatibilityPlaybackStatus = "loading" | "ready" | "error";
type Dependencies = {
  inspect: () => Promise<DuplicatePreviewCompatibility>;
  readSegment: (startSeconds: number) => Promise<ArrayBuffer>;
  onStatus: (status: CompatibilityPlaybackStatus) => void;
  createMediaSource?: () => MediaSource;
  isTypeSupported?: (mimeType: string) => boolean;
  createObjectURL?: (source: MediaSource) => string;
  revokeObjectURL?: (url: string) => void;
};

export function compatibilitySegmentStart(time: number, duration: number): number {
  const clamped = Math.max(0, Math.min(Number.isFinite(time) ? time : 0, Math.max(0, duration - 0.001)));
  return Math.floor(clamped / COMPATIBILITY_SEGMENT_SECONDS) * COMPATIBILITY_SEGMENT_SECONDS;
}

function ranges(buffer: TimeRanges): Array<[number, number]> {
  return Array.from({ length: buffer.length }, (_, index) => [buffer.start(index), buffer.end(index)]);
}

/**
 * Keeps only a small window of independently encoded fragments in memory. All
 * SourceBuffer operations and native reads are serialized. A seek invalidates
 * an in-flight read before it can append, and closing invalidates every callback.
 */
export function createCompatibilityPlayback(
  video: HTMLVideoElement,
  dependencies: Dependencies,
  { initialTime = 0, resume = false }: { initialTime?: number; resume?: boolean } = {},
): () => void {
  let active = true;
  let failed = false;
  let source: MediaSource | undefined;
  let buffer: SourceBuffer | undefined;
  let objectURL: string | undefined;
  let duration = 0;
  let desiredTime = initialTime;
  let generation = 0;
  let running = false;
  let requested = false;
  let initialized = false;
  let initialPositionPending = true;
  let shouldResume = resume;
  const appended = new Set<number>();
  const pendingCancels = new Set<() => void>();

  const status = (value: CompatibilityPlaybackStatus) => {
    if (active && (!failed || value === "error")) dependencies.onStatus(value);
  };
  const fail = () => {
    if (!active || failed) return;
    failed = true;
    status("error");
  };
  const waitFor = (target: EventTarget, success: string, action?: () => void): Promise<void> => new Promise((resolve, reject) => {
    let settled = false;
    const finish = (failure?: Error) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      target.removeEventListener(success, done);
      target.removeEventListener("error", error);
      target.removeEventListener("abort", aborted);
      pendingCancels.delete(cancel);
      if (failure) reject(failure); else resolve();
    };
    const done = () => finish();
    const error = () => finish(new Error("Playback update failed."));
    const aborted = () => finish(new Error("Playback update stopped."));
    const cancel = () => finish(new Error("Playback closed."));
    const timeout = setTimeout(() => finish(new Error("Playback update timed out.")), 15_000);
    pendingCancels.add(cancel);
    target.addEventListener(success, done, { once: true });
    target.addEventListener("error", error, { once: true });
    target.addEventListener("abort", aborted, { once: true });
    try { action?.(); } catch { error(); }
  });

  const setInitialPosition = () => {
    if (!active || !initialized || !initialPositionPending || video.readyState < 1) return;
    initialPositionPending = false;
    video.currentTime = Math.max(0, Math.min(desiredTime, Math.max(0, duration - 0.05)));
    if (shouldResume) {
      shouldResume = false;
      // A policy rejection leaves the familiar native Play control available.
      void video.play().catch(() => undefined);
    }
  };
  const evict = async (center: number) => {
    if (!buffer) return;
    const keepStart = Math.max(0, center - COMPATIBILITY_BUFFER_BEHIND_SECONDS);
    const keepEnd = Math.min(duration, center + COMPATIBILITY_BUFFER_AHEAD_SECONDS);
    for (const [start, end] of ranges(buffer.buffered)) {
      if (!active) return;
      if (start < keepStart) await waitFor(buffer, "updateend", () => buffer!.remove(start, Math.min(end, keepStart)));
      if (!active) return;
      if (end > keepEnd) await waitFor(buffer, "updateend", () => buffer!.remove(Math.max(start, keepEnd), end));
    }
    for (const start of appended) {
      if (start + COMPATIBILITY_SEGMENT_SECONDS <= keepStart || start >= keepEnd) appended.delete(start);
    }
  };

  const pump = async () => {
    if (!active || failed || !buffer || running) return;
    running = true;
    try {
      while (active && !failed && requested) {
        requested = false;
        const requestGeneration = generation;
        const center = compatibilitySegmentStart(desiredTime, duration);
        await evict(center);
        if (!active || requestGeneration !== generation) continue;
        const segmentStarts = [center];
        // A paused comparison loads only its current fragment. During playback,
        // one next fragment avoids a boundary stall without decoding the file.
        if (!video.paused && center + COMPATIBILITY_SEGMENT_SECONDS < duration) {
          segmentStarts.push(center + COMPATIBILITY_SEGMENT_SECONDS);
        }
        for (const start of segmentStarts) {
          if (!active || failed || requestGeneration !== generation) break;
          if (appended.has(start) && ranges(buffer.buffered).some(([left, right]) => left <= start + 0.2 && right >= Math.min(start + COMPATIBILITY_SEGMENT_SECONDS, duration) - 0.3)) continue;
          status("loading");
          const bytes = await dependencies.readSegment(start);
          if (!active || failed || requestGeneration !== generation) break;
          if (!(bytes instanceof ArrayBuffer) || bytes.byteLength === 0 || bytes.byteLength > MAX_FRAGMENT_BYTES) throw new Error("Invalid playback fragment.");
          buffer.timestampOffset = start;
          await waitFor(buffer, "updateend", () => buffer!.appendBuffer(bytes));
          if (!active) break;
          appended.add(start);
          initialized = true;
          setInitialPosition();
          status("ready");
          if (start + COMPATIBILITY_SEGMENT_SECONDS >= duration && source?.readyState === "open") source.endOfStream();
        }
      }
    } catch {
      fail();
    } finally {
      running = false;
      if (active && !failed && requested) void pump();
    }
  };
  const request = () => {
    if (!active || failed) return;
    requested = true;
    void pump();
  };
  const seek = () => {
    desiredTime = video.currentTime;
    generation += 1;
    request();
  };
  const advance = () => {
    if (initialPositionPending && !initialized) return;
    desiredTime = video.currentTime;
    request();
  };
  const pause = () => { shouldResume = false; };
  video.addEventListener("seeking", seek);
  video.addEventListener("timeupdate", advance);
  video.addEventListener("play", advance);
  video.addEventListener("pause", pause);
  video.addEventListener("loadedmetadata", setInitialPosition);
  video.addEventListener("error", fail);

  status("loading");
  void (async () => {
    const information = await dependencies.inspect();
    if (!active) return;
    duration = information.durationSeconds;
    if (!Number.isFinite(duration) || duration <= 0 || !information.mimeType.startsWith("video/mp4")) throw new Error("Invalid playback information.");
    const supports = dependencies.isTypeSupported ?? ((mimeType: string) => typeof MediaSource !== "undefined" && MediaSource.isTypeSupported(mimeType));
    if (!supports(information.mimeType)) throw new Error("Compatible playback is unavailable.");
    source = dependencies.createMediaSource?.() ?? new MediaSource();
    objectURL = (dependencies.createObjectURL ?? URL.createObjectURL)(source);
    const opened = waitFor(source, "sourceopen");
    video.src = objectURL;
    video.load();
    await opened;
    if (!active) return;
    buffer = source.addSourceBuffer(information.mimeType);
    buffer.mode = "segments";
    source.duration = duration;
    // AAC packet padding can extend a final fragment a few milliseconds past
    // the original. Clip at the known end instead of shrinking duration after
    // append: shrinking can itself start an asynchronous SourceBuffer removal.
    buffer.appendWindowEnd = duration;
    request();
  })().catch(fail);

  return () => {
    if (!active) return;
    active = false;
    generation += 1;
    video.removeEventListener("seeking", seek);
    video.removeEventListener("timeupdate", advance);
    video.removeEventListener("play", advance);
    video.removeEventListener("pause", pause);
    video.removeEventListener("loadedmetadata", setInitialPosition);
    video.removeEventListener("error", fail);
    for (const cancel of [...pendingCancels]) cancel();
    try { if (source?.readyState === "open" && buffer?.updating) buffer.abort(); } catch { /* The webview may already have detached the source. */ }
    video.pause();
    video.removeAttribute("src");
    video.load();
    if (objectURL) (dependencies.revokeObjectURL ?? URL.revokeObjectURL)(objectURL);
    appended.clear();
  };
}
