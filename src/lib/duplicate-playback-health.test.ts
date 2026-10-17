import { afterEach, describe, expect, it, vi } from "vitest";
import { watchLocalVideoPicture } from "./duplicate-playback-health";

class Video extends EventTarget {
  readyState = 1;
  videoWidth = 0;
  paused = true;
  totalVideoFrames = 0;
  callback?: VideoFrameRequestCallback;
  requestVideoFrameCallback = vi.fn((callback: VideoFrameRequestCallback) => { this.callback = callback; return 1; });
  cancelVideoFrameCallback = vi.fn();
  getVideoPlaybackQuality() { return { totalVideoFrames: this.totalVideoFrames }; }
  play() { this.paused = false; this.dispatchEvent(new Event("play")); }
  pause() { this.paused = true; this.dispatchEvent(new Event("pause")); }
}
afterEach(() => vi.useRealTimers());

describe("local picture health", () => {
  it("detects metadata and audio without a visible video track even when paused", async () => {
    vi.useFakeTimers();
    const video = new Video();
    const fallback = vi.fn();
    const dispose = watchLocalVideoPicture(video as unknown as HTMLVideoElement, fallback);
    await vi.advanceTimersByTimeAsync(1999);
    expect(fallback).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(fallback).toHaveBeenCalledExactlyOnceWith();
    video.dispatchEvent(new Event("error"));
    expect(fallback).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("does not mistake ordinary metadata preload with video dimensions for a failure", async () => {
    vi.useFakeTimers();
    const video = new Video();
    video.videoWidth = 1920;
    const fallback = vi.fn();
    const dispose = watchLocalVideoPicture(video as unknown as HTMLVideoElement, fallback);
    await vi.runAllTimersAsync();
    expect(fallback).not.toHaveBeenCalled();
    dispose();
  });

  it("detects playback with dimensions but no presented or decoded frames", async () => {
    vi.useFakeTimers();
    const video = new Video();
    video.videoWidth = 1920;
    const fallback = vi.fn();
    const dispose = watchLocalVideoPicture(video as unknown as HTMLVideoElement, fallback);
    video.play();
    await vi.advanceTimersByTimeAsync(4000);
    expect(fallback).toHaveBeenCalledExactlyOnceWith();
    expect(video.cancelVideoFrameCallback).toHaveBeenCalledWith(1);
    dispose();
  });

  it("keeps healthy decoded video on its original player", async () => {
    vi.useFakeTimers();
    const video = new Video();
    const fallback = vi.fn();
    const dispose = watchLocalVideoPicture(video as unknown as HTMLVideoElement, fallback);
    video.play();
    video.callback?.(0, { presentedFrames: 1 } as VideoFrameCallbackMetadata);
    await vi.runAllTimersAsync();
    expect(fallback).not.toHaveBeenCalled();
    dispose();
  });

  it("clears a pending playback check on pause and starts a fresh check on resume", async () => {
    vi.useFakeTimers();
    const video = new Video();
    video.videoWidth = 1280;
    const fallback = vi.fn();
    const dispose = watchLocalVideoPicture(video as unknown as HTMLVideoElement, fallback);
    video.play();
    await vi.advanceTimersByTimeAsync(3000);
    video.pause();
    await vi.advanceTimersByTimeAsync(6000);
    expect(fallback).not.toHaveBeenCalled();
    video.play();
    await vi.advanceTimersByTimeAsync(4000);
    expect(fallback).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("ignores every pending callback after player teardown or source switch", async () => {
    vi.useFakeTimers();
    const video = new Video();
    const fallback = vi.fn();
    const dispose = watchLocalVideoPicture(video as unknown as HTMLVideoElement, fallback);
    video.play();
    const frame = video.callback;
    dispose();
    frame?.(0, {} as VideoFrameCallbackMetadata);
    video.dispatchEvent(new Event("error"));
    video.dispatchEvent(new Event("loadedmetadata"));
    await vi.runAllTimersAsync();
    expect(fallback).not.toHaveBeenCalled();
  });
});
