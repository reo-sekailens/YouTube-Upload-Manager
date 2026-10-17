import { afterEach, describe, expect, it, vi } from "vitest";
import {
  compatibilitySegmentStart,
  COMPATIBILITY_BUFFER_AHEAD_SECONDS,
  COMPATIBILITY_BUFFER_BEHIND_SECONDS,
  createCompatibilityPlayback,
} from "./compatibility-playback";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}
const fragment = () => new Uint8Array([1, 2, 3]).buffer;
async function flush() { for (let index = 0; index < 30; index += 1) await Promise.resolve(); }

class FakeBuffer extends EventTarget {
  updating = false;
  mode = "segments";
  timestampOffset = 0;
  appendWindowEnd = Infinity;
  values: Array<[number, number]> = [];
  offsets: number[] = [];
  get buffered() { return { length: this.values.length, start: (index: number) => this.values[index][0], end: (index: number) => this.values[index][1] }; }
  constructor(private source: FakeSource) { super(); }
  update(action: () => void) {
    if (this.updating) throw new Error("Concurrent update");
    this.updating = true;
    this.source.readyState = "open";
    action();
    queueMicrotask(() => { this.updating = false; this.dispatchEvent(new Event("updateend")); });
  }
  appendBuffer(_bytes: ArrayBuffer) {
    this.update(() => {
      this.offsets.push(this.timestampOffset);
      const end = Math.min(this.timestampOffset + 6 + this.source.packetPadding, this.appendWindowEnd);
      this.values.push([this.timestampOffset, end]);
      this.source.extendCodedDuration(end);
      this.values.sort((left, right) => left[0] - right[0]);
    });
  }
  remove(start: number, end: number) {
    this.update(() => {
      this.values = this.values.flatMap(([left, right]) => {
        if (right <= start || left >= end) return [[left, right]] as Array<[number, number]>;
        const kept: Array<[number, number]> = [];
        if (left < start) kept.push([left, start]);
        if (right > end) kept.push([end, right]);
        return kept;
      });
    });
  }
  abort() { this.updating = false; this.dispatchEvent(new Event("abort")); }
}
class FakeSource extends EventTarget {
  readyState = "closed";
  private durationValue = 0;
  packetPadding = 0;
  get duration() { return this.durationValue; }
  set duration(value: number) {
    if (this.buffer.updating) throw new Error("Duration changed during SourceBuffer update");
    const highestEnd = Math.max(0, ...this.buffer.values.map(([, end]) => end));
    if (value < this.durationValue && highestEnd > value) {
      // Chromium requires an explicit asynchronous range removal before a
      // duration can be shortened below the highest coded-frame timestamp.
      throw new Error("Setting duration below buffered coded frames is disallowed");
    }
    this.durationValue = value;
  }
  extendCodedDuration(value: number) { this.durationValue = Math.max(this.durationValue, value); }
  buffer = new FakeBuffer(this);
  addSourceBuffer = vi.fn((_mime: string) => this.buffer as unknown as SourceBuffer);
  endOfStream() {
    if (this.buffer.updating) throw new Error("Cannot end stream during SourceBuffer update");
    this.readyState = "ended";
  }
}
class FakeVideo extends EventTarget {
  currentTime = 0;
  readyState = 1;
  paused = true;
  src = "";
  play = vi.fn(async () => { this.paused = false; this.dispatchEvent(new Event("play")); });
  pause = vi.fn(() => { this.paused = true; this.dispatchEvent(new Event("pause")); });
  constructor(private source: FakeSource) { super(); }
  load() { if (this.src) queueMicrotask(() => { this.source.readyState = "open"; this.source.dispatchEvent(new Event("sourceopen")); }); }
  removeAttribute(name: string) { if (name === "src") this.src = ""; }
  seek(time: number) { this.currentTime = time; this.dispatchEvent(new Event("seeking")); }
  tick(time: number) { this.currentTime = time; this.dispatchEvent(new Event("timeupdate")); }
}
function setup(options: { initialTime?: number; resume?: boolean; duration?: number } = {}) {
  const source = new FakeSource();
  const video = new FakeVideo(source);
  const inspect = vi.fn(async () => ({ durationSeconds: options.duration ?? 120, mimeType: 'video/mp4; codecs="avc1.42E01E, mp4a.40.2"' }));
  const readSegment = vi.fn(async (_time: number) => fragment());
  const onStatus = vi.fn();
  const createObjectURL = vi.fn(() => "blob:compatible-source");
  const revokeObjectURL = vi.fn();
  const dependencies = { inspect, readSegment, onStatus, createMediaSource: () => source as unknown as MediaSource, isTypeSupported: () => true, createObjectURL, revokeObjectURL };
  const start = () => createCompatibilityPlayback(video as unknown as HTMLVideoElement, dependencies, options);
  return { source, video, inspect, readSegment, onStatus, createObjectURL, revokeObjectURL, dependencies, start };
}
afterEach(() => vi.useRealTimers());

describe("bounded compatible duplicate playback", () => {
  it("loads only the requested paused fragment and restores the selected position without starting playback", async () => {
    const test = setup({ initialTime: 14.5 });
    const dispose = test.start();
    await flush();
    expect(test.readSegment).toHaveBeenCalledExactlyOnceWith(12);
    expect(test.source.buffer.offsets).toEqual([12]);
    expect(test.video.currentTime).toBe(14.5);
    expect(test.video.play).not.toHaveBeenCalled();
    expect(test.source.duration).toBe(120);
    expect(test.onStatus).toHaveBeenLastCalledWith("ready");
    dispose();
    dispose();
    expect(test.revokeObjectURL).toHaveBeenCalledExactlyOnceWith("blob:compatible-source");
    expect(test.video.src).toBe("");
  });

  it("serializes reads and discards the previous seek's late fragment", async () => {
    const test = setup();
    const pending = deferred<ArrayBuffer>();
    test.readSegment.mockImplementationOnce(() => pending.promise);
    const dispose = test.start();
    await flush();
    test.video.seek(43);
    test.video.seek(61);
    await flush();
    expect(test.readSegment).toHaveBeenCalledTimes(1);
    pending.resolve(fragment());
    await flush();
    expect(test.readSegment.mock.calls.map(([time]) => time)).toEqual([0, 60]);
    expect(test.source.buffer.offsets).toEqual([60]);
    expect(test.video.currentTime).toBe(61);
    expect(test.onStatus).toHaveBeenLastCalledWith("ready");
    dispose();
  });

  it("ignores an inspection received after close without creating an object URL", async () => {
    const test = setup();
    const pending = deferred<Awaited<ReturnType<typeof test.inspect>>>();
    test.inspect.mockImplementationOnce(() => pending.promise);
    const dispose = test.start();
    dispose();
    pending.resolve({ durationSeconds: 120, mimeType: "video/mp4" });
    await flush();
    expect(test.createObjectURL).not.toHaveBeenCalled();
    expect(test.readSegment).not.toHaveBeenCalled();
    expect(test.onStatus.mock.calls).toEqual([["loading"]]);
  });

  it("ignores late fragment success or failure after close and removes all playback listeners", async () => {
    for (const reject of [false, true]) {
      const test = setup();
      const pending = deferred<ArrayBuffer>();
      test.readSegment.mockImplementationOnce(() => pending.promise);
      const dispose = test.start();
      await flush();
      dispose();
      const calls = test.onStatus.mock.calls.length;
      if (reject) pending.reject(new Error("private file path")); else pending.resolve(fragment());
      test.video.seek(60);
      test.video.tick(70);
      await flush();
      expect(test.source.buffer.offsets).toEqual([]);
      expect(test.readSegment).toHaveBeenCalledTimes(1);
      expect(test.onStatus).toHaveBeenCalledTimes(calls);
      expect(test.revokeObjectURL).toHaveBeenCalledTimes(1);
    }
  });

  it("evicts old and distant ranges during playback and supports a backward seek", async () => {
    const test = setup({ resume: true });
    const dispose = test.start();
    await flush();
    expect(test.readSegment.mock.calls.map(([time]) => time)).toEqual([0, 6]);
    for (const time of [12, 24, 48, 12]) {
      test.video.seek(time);
      await flush();
      const values = test.source.buffer.values;
      expect(values.every(([start, end]) => start >= Math.max(0, time - COMPATIBILITY_BUFFER_BEHIND_SECONDS) && end <= time + COMPATIBILITY_BUFFER_AHEAD_SECONDS)).toBe(true);
      expect(values.reduce((total, [start, end]) => total + end - start, 0)).toBeLessThanOrEqual(24);
      expect(test.source.buffer.updating).toBe(false);
    }
    expect(test.readSegment.mock.calls.filter(([time]) => time === 12)).toHaveLength(2);
    expect(test.onStatus).not.toHaveBeenCalledWith("error");
    dispose();
  });

  it("stops prefetching when paused and resumes only on the player's Play event", async () => {
    const test = setup();
    const dispose = test.start();
    await flush();
    expect(test.readSegment.mock.calls.map(([time]) => time)).toEqual([0]);
    await test.video.play();
    await flush();
    expect(test.readSegment.mock.calls.map(([time]) => time)).toEqual([0, 6]);
    test.video.pause();
    test.video.seek(30);
    await flush();
    expect(test.readSegment.mock.calls.map(([time]) => time)).toEqual([0, 6, 30]);
    await test.video.play();
    await flush();
    expect(test.readSegment.mock.calls.map(([time]) => time)).toEqual([0, 6, 30, 36]);
    dispose();
  });

  it("refills a fragment evicted by the webview instead of trusting its old receipt", async () => {
    const test = setup();
    const dispose = test.start();
    await flush();
    test.source.buffer.values = [];
    test.video.tick(2);
    await flush();
    expect(test.readSegment.mock.calls.map(([time]) => time)).toEqual([0, 0]);
    dispose();
  });

  it("finishes at the real duration and reopens the stream on a backward seek", async () => {
    const test = setup({ initialTime: 29, duration: 30 });
    const dispose = test.start();
    await flush();
    expect(test.source.readyState).toBe("ended");
    test.video.seek(5);
    await flush();
    expect(test.source.readyState).toBe("open");
    expect(test.source.buffer.offsets).toEqual([24, 0]);
    expect(test.source.duration).toBe(30);
    dispose();
  });

  it("clips AAC packet padding instead of shrinking duration below buffered coded frames", async () => {
    const test = setup({ duration: 18, initialTime: 14 });
    test.source.packetPadding = 0.021;
    const dispose = test.start();
    await flush();
    expect(test.source.buffer.appendWindowEnd).toBe(18);
    expect(test.source.duration).toBe(18);
    expect(test.source.readyState).toBe("ended");
    test.video.seek(2);
    await flush();
    expect(test.source.buffer.offsets).toEqual([12, 0]);
    expect(test.onStatus).not.toHaveBeenCalledWith("error");
    expect(test.onStatus).toHaveBeenLastCalledWith("ready");
    expect(test.source.buffer.updating).toBe(false);
    dispose();
  });

  it("fails safely for unsupported codecs, bad duration, and empty fragments", async () => {
    for (const condition of ["unsupported", "duration", "empty"] as const) {
      const test = setup();
      if (condition === "unsupported") test.dependencies.isTypeSupported = () => false;
      if (condition === "duration") test.inspect.mockResolvedValueOnce({ durationSeconds: NaN, mimeType: "video/mp4" });
      if (condition === "empty") test.readSegment.mockResolvedValueOnce(new ArrayBuffer(0));
      const dispose = test.start();
      await flush();
      expect(test.onStatus).toHaveBeenLastCalledWith("error");
      expect(test.source.buffer.offsets).toEqual([]);
      dispose();
    }
  });

  it("retains an error when a read finishes after a media error", async () => {
    const test = setup();
    const pending = deferred<ArrayBuffer>();
    test.readSegment.mockImplementationOnce(() => pending.promise);
    const dispose = test.start();
    await flush();
    test.video.dispatchEvent(new Event("error"));
    pending.resolve(fragment());
    await flush();
    expect(test.source.buffer.offsets).toEqual([]);
    expect(test.onStatus).toHaveBeenLastCalledWith("error");
    dispose();
  });

  it("rejects a fragment above the native four MiB bound before appending", async () => {
    const test = setup();
    test.readSegment.mockResolvedValueOnce(new ArrayBuffer(4 * 1024 * 1024 + 1));
    const dispose = test.start();
    await flush();
    expect(test.onStatus).toHaveBeenLastCalledWith("error");
    expect(test.source.buffer.offsets).toEqual([]);
    dispose();
  });

  it("cleans up the source-open timeout when closed before sourceopen", async () => {
    vi.useFakeTimers();
    const test = setup();
    test.video.load = () => undefined;
    const dispose = test.start();
    await flush();
    dispose();
    await vi.runAllTimersAsync();
    expect(test.onStatus).not.toHaveBeenCalledWith("error");
    expect(test.readSegment).not.toHaveBeenCalled();
    expect(test.revokeObjectURL).toHaveBeenCalledTimes(1);
  });

  it("clamps seeks to complete, non-negative six-second fragment identities", () => {
    expect(compatibilitySegmentStart(-1, 30)).toBe(0);
    expect(compatibilitySegmentStart(Infinity, 30)).toBe(0);
    expect(compatibilitySegmentStart(30, 30)).toBe(24);
    expect(compatibilitySegmentStart(500, 30)).toBe(24);
    expect(compatibilitySegmentStart(6.1, 30)).toBe(6);
  });
});
