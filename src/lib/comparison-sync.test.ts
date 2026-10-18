import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createComparisonSync, createHtmlComparisonPlayer, createYouTubeComparisonPlayer,
  type ComparisonPlayer, type ComparisonPlayerSnapshot, type ComparisonSyncState,
} from "./comparison-sync";

class FakePlayer implements ComparisonPlayer {
  value: ComparisonPlayerSnapshot = { ready: true, buffering: false, paused: true, ended: false, time: 0, duration: 120 };
  applyPlay = true;
  play = vi.fn(() => { if (this.applyPlay) { this.value.paused = false; this.value.ended = false; } });
  pause = vi.fn(() => { this.value.paused = true; });
  seek = vi.fn((time: number) => { this.value.time = Math.min(this.value.duration || Infinity, time); this.value.ended = false; });
  mute = vi.fn();
  dispose = vi.fn();
  snapshot = () => ({ ...this.value });
}
function pair() {
  const left = new FakePlayer();
  const right = new FakePlayer();
  const changes: ComparisonSyncState[] = [];
  const sync = createComparisonSync(left, right, (state) => changes.push(state));
  return { left, right, sync, changes, state: () => changes.at(-1)! };
}
function poll(milliseconds = 100) { vi.advanceTimersByTime(milliseconds); }

beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(10_000); });
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe("comparison synchronization", () => {
  it("starts paused and chooses only one audible side", () => {
    const { left, right, sync, state } = pair();
    poll(500);
    expect(left.play).not.toHaveBeenCalled();
    expect(right.play).not.toHaveBeenCalled();
    expect(state()).toMatchObject({ playing: false, waiting: false, ready: true, time: 0, start: 0, duration: 120, linked: true, audio: "left" });
    expect(left.mute).toHaveBeenLastCalledWith(false);
    expect(right.mute).toHaveBeenLastCalledWith(true);
    sync.setAudio("right");
    expect(left.mute).toHaveBeenLastCalledWith(true);
    expect(right.mute).toHaveBeenLastCalledWith(false);
    sync.setAudio("none");
    expect(right.mute).toHaveBeenLastCalledWith(true);
    sync.dispose();
  });

  it("links play, pause and seeks on an offset-adjusted overlap", () => {
    const { left, right, sync, state } = pair();
    right.value.duration = 100;
    sync.setOffset(15);
    expect(state()).toMatchObject({ start: 0, duration: 85, offset: 15 });
    sync.seek(40);
    expect(left.seek).toHaveBeenLastCalledWith(40);
    expect(right.seek).toHaveBeenLastCalledWith(55);
    sync.play(); poll();
    expect(left.play).toHaveBeenCalledOnce();
    expect(right.play).toHaveBeenCalledOnce();
    sync.pause(); poll();
    expect(state().playing).toBe(false);
    expect(left.value.paused && right.value.paused).toBe(true);
    sync.seek(999);
    expect(left.seek).toHaveBeenLastCalledWith(85);
    expect(right.seek).toHaveBeenLastCalledWith(100);
    sync.setOffset(-20);
    sync.seek(0);
    expect(state()).toMatchObject({ start: 20, duration: 120, time: 20 });
    expect(right.seek).toHaveBeenLastCalledWith(0);
    sync.dispose();
  });

  it("retains a restored timestamp while a fallback player is not ready", () => {
    const left = new FakePlayer();
    const right = new FakePlayer();
    right.value.ready = false;
    right.value.duration = 0;
    let pending = 0;
    right.seek.mockImplementation((time) => { pending = time; });
    const changes: ComparisonSyncState[] = [];
    const sync = createComparisonSync(left, right, (value) => changes.push(value));
    sync.setOffset(5);
    sync.seek(64);
    poll(5_000);
    expect(changes.at(-1)).toMatchObject({ time: 64, playing: false, ready: false });
    expect(right.play).not.toHaveBeenCalled();
    right.value = { ...right.value, ready: true, duration: 100, time: pending };
    poll();
    expect(changes.at(-1)).toMatchObject({ time: 64, ready: true });
    expect(right.value.time).toBe(69);
    expect(left.play).not.toHaveBeenCalled();
    sync.dispose();
  });

  it("holds both players during buffering and resumes only an active playback request", () => {
    const { left, right, sync, state } = pair();
    sync.play(); poll();
    right.value.buffering = true;
    poll();
    expect(state()).toMatchObject({ playing: true, waiting: true });
    expect(left.value.paused && right.value.paused).toBe(true);
    right.value.buffering = false;
    poll();
    expect(state()).toMatchObject({ playing: true, waiting: false });
    expect(left.play).toHaveBeenCalledTimes(2);
    poll();
    right.value.buffering = true;
    poll();
    sync.pause();
    right.value.buffering = false;
    poll(1_000);
    expect(left.play).toHaveBeenCalledTimes(2);
    expect(state()).toMatchObject({ playing: false, waiting: false });
    sync.dispose();
  });

  it("propagates native play/pause and scrubbing from either side", () => {
    const { left, right, sync, state } = pair();
    left.value.paused = false;
    poll(); poll();
    expect(state().playing).toBe(true);
    expect(right.play).toHaveBeenCalledOnce();
    right.value.paused = true;
    poll(); poll();
    expect(state().playing).toBe(false);
    expect(left.value.paused).toBe(true);
    right.value.time = 35;
    poll();
    expect(left.seek).toHaveBeenLastCalledWith(35);
    expect(state().time).toBe(35);
    poll();
    left.value.time = 48;
    poll();
    expect(right.seek).toHaveBeenLastCalledWith(48);
    sync.dispose();
  });

  it("retains native play intent through an initial buffering transition", () => {
    for (const side of ["left", "right"] as const) {
      const { left, right, sync, state } = pair();
      const source = side === "left" ? left : right;
      source.value.paused = false;
      source.value.buffering = true;
      poll();
      expect(state()).toMatchObject({ playing: true, waiting: true });
      expect(left.value.paused && right.value.paused).toBe(true);
      source.value.buffering = false;
      poll();
      expect(state()).toMatchObject({ playing: true, waiting: false });
      expect(left.play).toHaveBeenCalledOnce();
      expect(right.play).toHaveBeenCalledOnce();
      poll();
      sync.pause(); poll();
      source.value.buffering = true;
      poll();
      source.value.buffering = false;
      poll();
      expect(state().playing).toBe(false);
      expect(left.play).toHaveBeenCalledOnce();
      sync.dispose();
    }
  });

  it("accepts native Play immediately after a synchronously acknowledged shared Pause", () => {
    const { left, right, sync, state } = pair();
    sync.play(); poll();
    sync.pause();
    // No controller poll occurs between the shared Pause and native Play.
    right.value.paused = false;
    right.value.buffering = true;
    poll();
    expect(state()).toMatchObject({ playing: true, waiting: true });
    expect(left.value.paused && right.value.paused).toBe(true);
    right.value.buffering = false;
    poll();
    expect(state()).toMatchObject({ playing: true, waiting: false });
    expect(left.play).toHaveBeenCalledTimes(2);
    expect(right.play).toHaveBeenCalledTimes(2);
    sync.dispose();
  });

  it("keeps unacknowledged asynchronous pauses fenced from late playing/buffering states", () => {
    const { left, right, sync, state } = pair();
    sync.play(); poll();
    left.pause.mockImplementation(() => {});
    right.pause.mockImplementation(() => {});
    sync.pause();
    right.value.buffering = true;
    poll();
    expect(state()).toMatchObject({ playing: false, waiting: false });
    left.value.paused = true; right.value.paused = true;
    right.value.buffering = false;
    poll(); poll();
    expect(state().playing).toBe(false);
    expect(left.play).toHaveBeenCalledOnce();
    expect(right.play).toHaveBeenCalledOnce();
    sync.dispose();
  });

  it("bounds drift corrections and never seeks the master for normal follower drift", () => {
    const { left, right, sync } = pair();
    sync.play(); poll();
    left.seek.mockClear(); right.seek.mockClear();
    // Smoothly accumulate follower drift so it cannot be confused with a native scrub.
    for (let index = 0; index < 7; index += 1) {
      left.value.time += 0.3;
      right.value.time += 0.1;
      poll();
    }
    expect(right.seek).toHaveBeenCalledOnce();
    expect(left.seek).not.toHaveBeenCalled();
    for (let index = 0; index < 8; index += 1) { left.value.time += 0.3; right.value.time += 0.1; poll(); }
    expect(right.seek).toHaveBeenCalledOnce();
    sync.dispose();
  });

  it("supports independent playback and stops at the linked overlap end", () => {
    const { left, right, sync, state } = pair();
    sync.setLinked(false);
    sync.play(); poll();
    expect(left.play).toHaveBeenCalledOnce();
    expect(right.play).not.toHaveBeenCalled();
    sync.seek(110);
    expect(right.seek).not.toHaveBeenCalled();
    poll();
    sync.pause(); poll();
    sync.setLinked(true);
    right.value.duration = 115;
    sync.setOffset(5);
    sync.play(); poll(); poll();
    left.value.time = 109.95; right.value.time = 114.95;
    poll();
    expect(state().playing).toBe(false);
    expect(left.value.paused && right.value.paused).toBe(true);
    sync.dispose();
  });

  it("does not retry a rejected play forever or restart after disposal", () => {
    const { left, right, sync, state } = pair();
    right.applyPlay = false;
    sync.play();
    poll(3_000);
    expect(state().playing).toBe(false);
    expect(right.play).toHaveBeenCalledOnce();
    expect(left.value.paused).toBe(true);
    sync.dispose();
    left.value.ready = false; right.value.ready = true;
    poll(5_000);
    expect(right.play).toHaveBeenCalledOnce();
    expect(left.dispose).not.toHaveBeenCalled();
    expect(right.dispose).not.toHaveBeenCalled();
  });

  it("does not start when the two timelines have no overlap", () => {
    const { left, right, sync, state } = pair();
    sync.setOffset(200);
    sync.play(); poll(1_000);
    expect(state()).toMatchObject({ ready: false, waiting: true });
    expect(left.play).not.toHaveBeenCalled();
    expect(right.play).not.toHaveBeenCalled();
    sync.dispose();
  });
});

class FakeVideo extends EventTarget {
  readyState = 4;
  paused = true;
  ended = false;
  seeking = false;
  currentTime = 0;
  duration = 120;
  muted = false;
  play = vi.fn(() => { this.paused = false; return Promise.resolve(); });
  pause = vi.fn(() => { this.paused = true; });
}

describe("HTML video comparison adapter", () => {
  it("defers metadata seeks and exposes real buffering", () => {
    const video = new FakeVideo();
    video.readyState = 0;
    const player = createHtmlComparisonPlayer(video as unknown as HTMLVideoElement);
    player.seek(42);
    expect(player.snapshot()).toMatchObject({ ready: false, time: 42 });
    video.readyState = 1;
    video.dispatchEvent(new Event("loadedmetadata"));
    expect(video.currentTime).toBe(42);
    expect(player.snapshot().buffering).toBe(false);
    video.readyState = 4;
    video.dispatchEvent(new Event("canplay"));
    expect(player.snapshot().buffering).toBe(false);
    video.dispatchEvent(new Event("waiting"));
    expect(player.snapshot().buffering).toBe(true);
    video.dispatchEvent(new Event("playing"));
    expect(player.snapshot().buffering).toBe(false);
    player.dispose();
    expect(player.snapshot().ready).toBe(false);
  });

  it("fences late play promises after pause and disposal", async () => {
    const video = new FakeVideo();
    let resolve!: () => void;
    video.play.mockImplementation(() => new Promise<void>((done) => { resolve = () => { video.paused = false; done(); }; }));
    const player = createHtmlComparisonPlayer(video as unknown as HTMLVideoElement);
    player.play();
    player.pause();
    resolve();
    await Promise.resolve(); await Promise.resolve();
    expect(video.paused).toBe(true);
    player.play();
    player.dispose();
    resolve();
    await Promise.resolve(); await Promise.resolve();
    expect(video.paused).toBe(true);
  });

  it("does not let an older play completion cancel a newer deliberate play", async () => {
    const video = new FakeVideo();
    const resolutions: Array<() => void> = [];
    video.play.mockImplementation(() => new Promise<void>((done) => { resolutions.push(() => { video.paused = false; done(); }); }));
    const player = createHtmlComparisonPlayer(video as unknown as HTMLVideoElement);
    player.play(); player.pause(); player.play();
    resolutions[0]();
    await Promise.resolve(); await Promise.resolve();
    expect(video.paused).toBe(false);
    resolutions[1]();
    await Promise.resolve();
    player.dispose();
  });

  it("allows a first play from metadata-only preload", () => {
    const video = new FakeVideo();
    video.readyState = 1;
    const player = createHtmlComparisonPlayer(video as unknown as HTMLVideoElement);
    const right = new FakePlayer();
    const sync = createComparisonSync(player, right, () => {});
    sync.play();
    expect(video.play).toHaveBeenCalledOnce();
    video.dispatchEvent(new Event("waiting"));
    poll();
    expect(video.paused).toBe(true);
    video.readyState = 4;
    video.dispatchEvent(new Event("canplay"));
    poll();
    expect(video.play).toHaveBeenCalledTimes(2);
    sync.dispose(); player.dispose();
  });
});

class FakeFrame extends EventTarget {
  src = "https://www.youtube.com/embed/test?enablejsapi=1";
  contentWindow = { postMessage: vi.fn(), location: { href: "https://www.youtube.com/embed/test?enablejsapi=1" } };
  owner = new EventTarget();
  ownerDocument = { defaultView: this.owner };
  send(data: unknown, origin = "https://www.youtube.com", source: unknown = this.contentWindow) {
    const event = new Event("message");
    Object.assign(event, { data, origin, source });
    this.owner.dispatchEvent(event);
  }
  commands() { return this.contentWindow.postMessage.mock.calls.map(([value]) => JSON.parse(value as string) as { event: string; func?: string; args?: unknown[] }); }
}

describe("YouTube comparison adapter", () => {
  it("never posts player commands into an uncommitted about:blank frame", () => {
    const frame = new FakeFrame();
    frame.contentWindow.location.href = "about:blank";
    const player = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    player.mute(false); player.seek(32); player.pause();
    poll(1_000);
    expect(frame.commands()).toHaveLength(0);
    frame.contentWindow.location.href = frame.src;
    frame.dispatchEvent(new Event("load"));
    expect(frame.commands().some((message) => message.event === "listening")).toBe(true);
    frame.send({ event: "onReady" });
    expect(frame.commands()).toEqual(expect.arrayContaining([{ event: "command", func: "seekTo", args: [32, true] }]));
    expect(frame.commands().some((message) => message.func === "playVideo")).toBe(false);
    player.dispose();
  });

  it("disposes a pre-navigation player without sending into the parent origin", () => {
    const frame = new FakeFrame();
    frame.contentWindow.location.href = "about:blank";
    const player = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    player.pause(); player.dispose();
    frame.contentWindow.location.href = frame.src;
    frame.dispatchEvent(new Event("load"));
    poll(1_000);
    expect(frame.commands()).toHaveLength(0);
  });

  it("attaches after a cross-origin embed has already committed", () => {
    const frame = new FakeFrame();
    Object.defineProperty(frame.contentWindow, "location", { get: () => { throw new DOMException("Cross-origin frame", "SecurityError"); } });
    const player = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    expect(frame.commands().some((message) => message.event === "listening")).toBe(true);
    player.dispose();
  });

  it("validates origin, source, payload and actual ready event", () => {
    const frame = new FakeFrame();
    const player = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    const ready = { event: "onReady" };
    frame.send(ready, "https://attacker.example");
    frame.send(ready, "https://www.youtube.com", {});
    frame.send("{broken");
    frame.send({ event: "infoDelivery", info: { currentTime: 15, duration: 120, playerState: 2 } });
    expect(player.snapshot()).toMatchObject({ ready: false, time: 15, duration: 120 });
    frame.send(ready);
    expect(player.snapshot().ready).toBe(true);
    frame.send({ event: "infoDelivery", info: { currentTime: NaN, duration: Infinity } });
    expect(player.snapshot()).toMatchObject({ time: 15, duration: 120 });
    frame.src = "https://attacker.example/embed/test";
    frame.send({ event: "infoDelivery", info: { currentTime: 99 } });
    expect(player.snapshot().time).toBe(15);
    player.dispose();
  });

  it("restores a pending timestamp paused and mutes the chosen channel", () => {
    const frame = new FakeFrame();
    const player = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    player.seek(45);
    player.mute(false);
    expect(player.snapshot()).toMatchObject({ time: 45, ready: false });
    expect(frame.commands().filter((message) => message.func === "playVideo")).toHaveLength(0);
    frame.send(JSON.stringify({ event: "onReady" }));
    expect(frame.commands()).toEqual(expect.arrayContaining([
      { event: "command", func: "seekTo", args: [45, true] },
      { event: "command", func: "pauseVideo", args: [] },
      { event: "command", func: "unMute", args: [] },
    ]));
    player.play();
    expect(frame.commands().at(-1)?.func).toBe("playVideo");
    frame.send({ event: "onStateChange", info: 1 });
    poll(400);
    expect(player.snapshot().time).toBeCloseTo(45.4);
    player.pause();
    frame.send({ event: "onStateChange", info: 1 });
    expect(frame.commands().at(-1)?.func).toBe("pauseVideo");
    player.dispose();
  });

  it("propagates native YouTube Play through state 2 -> 3 -> 1", () => {
    const frame = new FakeFrame();
    const right = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    frame.send({ event: "onReady" });
    frame.send({ event: "infoDelivery", info: { playerState: 2, currentTime: 20, duration: 120 } });
    const left = new FakePlayer();
    let state!: ComparisonSyncState;
    const sync = createComparisonSync(left, right, (value) => { state = value; });
    frame.send({ event: "onStateChange", info: 3 });
    poll();
    expect(state).toMatchObject({ playing: true, waiting: true, time: 20 });
    expect(left.seek).toHaveBeenLastCalledWith(20);
    frame.send({ event: "onStateChange", info: 1 });
    poll();
    expect(left.play).toHaveBeenCalledOnce();
    expect(frame.commands().some((command) => command.func === "playVideo")).toBe(true);
    frame.send({ event: "onStateChange", info: 1 });
    poll();
    expect(state).toMatchObject({ playing: true, waiting: false });
    sync.dispose(); right.dispose();
  });

  it("does not treat a paused iframe seek's buffering as native Play", () => {
    const frame = new FakeFrame();
    const right = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    frame.send({ event: "onReady" });
    frame.send({ event: "infoDelivery", info: { playerState: 2, currentTime: 0, duration: 120 } });
    const left = new FakePlayer();
    let state!: ComparisonSyncState;
    const sync = createComparisonSync(left, right, (value) => { state = value; });
    sync.seek(40);
    poll();
    frame.send({ event: "onStateChange", info: 3 });
    poll();
    frame.send({ event: "onStateChange", info: 1 });
    poll();
    expect(state.playing).toBe(false);
    expect(left.play).not.toHaveBeenCalled();
    expect(frame.commands().some((command) => command.func === "playVideo")).toBe(false);
    sync.dispose(); right.dispose();
  });

  it("stops event delivery and handshake work on disposal", () => {
    const frame = new FakeFrame();
    const player = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    poll(1_000);
    player.dispose();
    const count = frame.commands().length;
    frame.send({ event: "onReady" });
    frame.send({ event: "onStateChange", info: 1 });
    poll(2_000);
    player.play();
    expect(player.snapshot().ready).toBe(false);
    expect(frame.commands()).toHaveLength(count);
  });

  it("reports provider errors and requires a fresh ready event after reload", () => {
    const frame = new FakeFrame();
    const player = createYouTubeComparisonPlayer(frame as unknown as HTMLIFrameElement);
    frame.send({ event: "onReady" });
    frame.send({ event: "onError", info: 153 });
    expect(player.snapshot()).toMatchObject({ ready: false, failed: true, paused: true });
    frame.send({ event: "onReady" });
    expect(player.snapshot().ready).toBe(false);
    frame.dispatchEvent(new Event("load"));
    expect(player.snapshot()).toMatchObject({ ready: false, failed: false });
    frame.send({ event: "infoDelivery", info: { playerState: 1, duration: 100 } });
    expect(player.snapshot().ready).toBe(false);
    frame.send({ event: "onReady" });
    expect(player.snapshot().ready).toBe(true);
    expect(frame.commands().some((command) => command.func === "playVideo")).toBe(false);
    player.dispose();
  });
});
