import { comparisonPositionMaximum, youtubeComparisonOrigin } from "./comparison-controls";

export interface ComparisonPlayerSnapshot {
  ready: boolean;
  buffering: boolean;
  paused: boolean;
  ended: boolean;
  time: number;
  duration: number;
  failed?: boolean;
}

export interface ComparisonPlayer {
  snapshot(): ComparisonPlayerSnapshot;
  play(): void;
  pause(): void;
  seek(seconds: number): void;
  mute(muted: boolean): void;
  dispose(): void;
}

export type ComparisonAudio = "left" | "right" | "none";
export interface ComparisonSyncState {
  time: number;
  /** Absolute end of the overlap on the left player's timeline. */
  duration: number;
  start: number;
  /** Operator playback intent, including a temporary buffering hold. */
  playing: boolean;
  waiting: boolean;
  ready: boolean;
  /** Right timestamp minus left timestamp. */
  offset: number;
  linked: boolean;
  audio: ComparisonAudio;
}

export interface ComparisonSync {
  play(): void;
  pause(): void;
  seek(seconds: number): void;
  setOffset(seconds: number): void;
  setLinked(linked: boolean): void;
  setAudio(audio: ComparisonAudio): void;
  dispose(): void;
}

const finiteTime = (value: number) => Number.isFinite(value) ? Math.max(0, value) : 0;
const clamp = (value: number, start: number, end: number) => Math.min(end, Math.max(start, value));
const emptySnapshot = (): ComparisonPlayerSnapshot => ({ ready: false, buffering: false, paused: true, ended: false, time: 0, duration: 0 });

/** Works with both original-file video and the bounded MediaSource player. */
export function createHtmlComparisonPlayer(video: HTMLVideoElement): ComparisonPlayer {
  let disposed = false;
  let waiting = false;
  let failed = false;
  let pendingSeek: number | null = null;
  let playGeneration = 0;
  let wantsPlay = false;
  const applySeek = () => {
    if (disposed || pendingSeek === null || video.readyState < 1) return;
    try {
      video.currentTime = clamp(pendingSeek, 0, finiteTime(video.duration) || comparisonPositionMaximum);
      pendingSeek = null;
    } catch { /* A later metadata event can make the same seek possible. */ }
  };
  const onWaiting = () => { waiting = true; };
  const onPlayable = () => { waiting = false; failed = false; applySeek(); };
  const onError = () => { failed = true; waiting = false; wantsPlay = false; playGeneration += 1; };
  const events: Array<[string, EventListener]> = [
    ["waiting", onWaiting], ["stalled", onWaiting], ["canplay", onPlayable],
    ["playing", onPlayable], ["loadedmetadata", onPlayable], ["durationchange", applySeek],
    ["seeked", onPlayable], ["error", onError],
  ];
  for (const [name, listener] of events) video.addEventListener(name, listener);
  const pause = () => {
    wantsPlay = false;
    playGeneration += 1;
    video.pause();
  };
  return {
    snapshot: () => ({
      ready: !disposed && !failed && video.readyState >= 1,
      // Metadata-only preload does not fetch every frame until play. Treating
      // an idle readyState=1 as buffering would prevent the first play call.
      buffering: !disposed && !failed && !video.ended && (waiting || video.seeking || (!video.paused && video.readyState >= 1 && video.readyState < 3)),
      paused: video.paused,
      ended: video.ended,
      time: pendingSeek ?? finiteTime(video.currentTime),
      duration: finiteTime(video.duration),
      failed,
    }),
    play() {
      if (disposed || failed) return;
      wantsPlay = true;
      const generation = ++playGeneration;
      try {
        void Promise.resolve(video.play()).then(() => {
          // A slow play promise must never undo a later pause or modal close.
          if (disposed || !wantsPlay) video.pause();
        }).catch(() => { if (generation === playGeneration) wantsPlay = false; });
      } catch { wantsPlay = false; }
    },
    pause,
    seek(seconds) { if (!disposed && Number.isFinite(seconds)) { pendingSeek = Math.max(0, seconds); applySeek(); } },
    mute(muted) { if (!disposed) video.muted = muted; },
    dispose() {
      if (disposed) return;
      disposed = true;
      pendingSeek = null;
      pause();
      for (const [name, listener] of events) video.removeEventListener(name, listener);
    },
  };
}

let iframeSequence = 0;
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);

/** Only the expected YouTube origin and this exact frame can deliver player state. */
export function createYouTubeComparisonPlayer(iframe: HTMLIFrameElement): ComparisonPlayer {
  const owner = iframe.ownerDocument.defaultView ?? window;
  const target = iframe.contentWindow;
  const id = `duplicate-comparison-${++iframeSequence}`;
  let state = emptySnapshot();
  let disposed = false;
  let failed = false;
  let pendingSeek: number | null = null;
  let muted = true;
  let wantsPlay = false;
  let pauseFenceUntil = 0;
  let timeReportedAt = Date.now();
  const validFrame = () => {
    try { const url = new URL(iframe.src); return !!target && url.origin === youtubeComparisonOrigin && url.pathname.startsWith("/embed/") && iframe.contentWindow === target; }
    catch { return false; }
  };
  const hasCommittedFrame = () => {
    if (!target || !validFrame()) return false;
    // Effects can attach before navigation leaves the parent's about:blank
    // document, or after the frame has already loaded. Cross-origin access
    // being denied is the latter case for this validated embed destination.
    try { return new URL(target.location.href).origin === youtubeComparisonOrigin; }
    catch (error) { return error instanceof DOMException && error.name === "SecurityError"; }
  };
  let committed = hasCommittedFrame();
  let awaitingFirstLoad = !committed;
  const send = (payload: Record<string, unknown>) => {
    if (!disposed && committed && validFrame()) target?.postMessage(JSON.stringify(payload), youtubeComparisonOrigin);
  };
  const command = (func: string, args: unknown[] = []) => send({ event: "command", func, args });
  const currentTime = () => state.time + (!state.paused && !state.buffering ? Math.min(1, Math.max(0, (Date.now() - timeReportedAt) / 1000)) : 0);
  const updatePlayerState = (value: unknown) => {
    if (typeof value !== "number" || ![-1, 0, 1, 2, 3, 5].includes(value)) return;
    state.time = currentTime();
    timeReportedAt = Date.now();
    state.buffering = value === 3;
    state.ended = value === 0;
    state.paused = value !== 1 && value !== 3;
    if (value === 2 && !wantsPlay) pauseFenceUntil = 0;
    if ((value === 1 || value === 3) && !wantsPlay && Date.now() < pauseFenceUntil) {
      state.paused = true;
      command("pauseVideo");
    }
  };
  const applyPending = () => {
    if (!state.ready || failed) return;
    command(muted ? "mute" : "unMute");
    if (pendingSeek !== null) {
      const position = clamp(pendingSeek, 0, state.duration || comparisonPositionMaximum);
      command("seekTo", [position, true]);
      state.time = position;
      timeReportedAt = Date.now();
      pendingSeek = null;
      // YouTube can start a cued video on seek. A restored paused timestamp
      // therefore always includes an explicit pause after the seek command.
      if (!wantsPlay) { pauseFenceUntil = Date.now() + 2_500; command("pauseVideo"); }
    }
  };
  const listen = () => {
    if (state.ready || failed || disposed) return;
    send({ event: "listening", id, channel: "widget" });
    for (const event of ["onReady", "onStateChange", "onError"]) command("addEventListener", [event]);
  };
  const onMessage = (event: MessageEvent) => {
    if (disposed || event.origin !== youtubeComparisonOrigin || event.source !== target || !validFrame()) return;
    if (!committed) { committed = true; listen(); }
    let message: unknown = event.data;
    if (typeof message === "string") {
      if (message.length > 65_536) return;
      try { message = JSON.parse(message); } catch { return; }
    }
    if (!record(message)) return;
    if (message.event === "onReady") {
      if (failed) return;
      state.ready = true;
      applyPending();
    } else if (message.event === "onError") {
      failed = true;
      wantsPlay = false;
      state = { ...state, ready: false, buffering: false, paused: true };
    } else if (message.event === "onStateChange") updatePlayerState(message.info);
    else if (message.event === "infoDelivery" && record(message.info)) {
      const info = message.info;
      if (typeof info.currentTime === "number" && Number.isFinite(info.currentTime) && info.currentTime >= 0) {
        state.time = info.currentTime;
        timeReportedAt = Date.now();
      }
      if (typeof info.duration === "number" && Number.isFinite(info.duration) && info.duration > 0) state.duration = info.duration;
      updatePlayerState(info.playerState);
      // Timing data can arrive during initialization; it is not readiness.
    }
  };
  const onLoad = () => {
    committed = hasCommittedFrame();
    if (!committed) return;
    if (awaitingFirstLoad && state.ready) { awaitingFirstLoad = false; return; }
    awaitingFirstLoad = false;
    state = emptySnapshot(); failed = false; wantsPlay = false; timeReportedAt = Date.now(); listen();
  };
  owner.addEventListener("message", onMessage);
  iframe.addEventListener("load", onLoad);
  const timer = setInterval(listen, 500);
  listen();
  return {
    snapshot: () => ({ ...state, failed, ready: state.ready && !disposed && !failed && validFrame(), time: pendingSeek ?? Math.min(state.duration || comparisonPositionMaximum, currentTime()) }),
    play() { if (!disposed && state.ready && !failed) { wantsPlay = true; pauseFenceUntil = 0; command("playVideo"); } },
    pause() { if (!disposed) { wantsPlay = false; pauseFenceUntil = Date.now() + 2_500; command("pauseVideo"); } },
    seek(seconds) { if (!disposed && Number.isFinite(seconds)) { pendingSeek = Math.max(0, seconds); applyPending(); } },
    mute(value) { if (!disposed) { muted = value; if (state.ready) command(muted ? "mute" : "unMute"); } },
    dispose() {
      if (disposed) return;
      wantsPlay = false;
      command("pauseVideo");
      disposed = true;
      state.ready = false;
      pendingSeek = null;
      clearInterval(timer);
      owner.removeEventListener("message", onMessage);
      iframe.removeEventListener("load", onLoad);
    },
  };
}

type CommandFence = { paused: boolean | null; pauseUntil: number; seek: number | null; seekUntil: number };
const newFence = (): CommandFence => ({ paused: null, pauseUntil: 0, seek: null, seekUntil: 0 });

/** A left-master clock with bounded corrections; it does not own adapter lifetime. */
export function createComparisonSync(left: ComparisonPlayer, right: ComparisonPlayer, onChange: (state: ComparisonSyncState) => void): ComparisonSync {
  const players = [left, right];
  const fences = [newFence(), newFence()];
  let disposed = false;
  let wantsPlay = false;
  let held = false;
  let offset = 0;
  let linked = true;
  let audio: ComparisonAudio = "left";
  let requestedTime: number | null = null;
  let previous: ComparisonPlayerSnapshot[] | null = null;
  let lastPoll = Date.now();
  let lastCorrection = -Infinity;
  let lastState: ComparisonSyncState | null = null;
  const snapshots = () => players.map((player) => { try { return player.snapshot(); } catch { return emptySnapshot(); } });
  const bounds = (values: ComparisonPlayerSnapshot[]) => {
    const start = linked ? Math.max(0, -offset) : 0;
    const end = linked ? Math.min(values[0].duration || comparisonPositionMaximum, (values[1].duration || comparisonPositionMaximum) - offset) : values[0].duration || comparisonPositionMaximum;
    return { start: Math.min(start, values[0].duration || comparisonPositionMaximum), end: Math.max(Math.min(start, values[0].duration || comparisonPositionMaximum), end) };
  };
  const pausePlayer = (index: number, values: ComparisonPlayerSnapshot[]) => {
    const fence = fences[index];
    if ((!values[index].paused || fence.paused === false) && fence.paused !== true) {
      fence.paused = true; fence.pauseUntil = Date.now() + 2_500;
      players[index].pause();
    }
  };
  const pausePair = (values = snapshots()) => { pausePlayer(0, values); pausePlayer(1, values); };
  const seekPlayer = (index: number, position: number, values: ComparisonPlayerSnapshot[]) => {
    fences[index].seek = position;
    fences[index].seekUntil = Date.now() + 2_500;
    players[index].seek(position);
    // A seek into an unready player remains pending at controller level too.
    if (!values[index].ready) fences[index].seekUntil = Infinity;
  };
  const seekPair = (position: number, values = snapshots()) => {
    const range = bounds(values);
    requestedTime = clamp(position, range.start, range.end);
    seekPlayer(0, requestedTime, values);
    if (linked) seekPlayer(1, requestedTime + offset, values);
  };
  const emit = (values = snapshots()) => {
    const range = bounds(values);
    const ready = values[0].ready && (!linked || values[1].ready) && range.end > range.start;
    const state: ComparisonSyncState = {
      time: clamp(requestedTime ?? finiteTime(values[0].time), range.start, range.end),
      duration: range.end, start: range.start, playing: wantsPlay,
      waiting: wantsPlay && (!ready || values[0].buffering || (linked && values[1].buffering) || held),
      ready, offset, linked, audio,
    };
    if (!lastState || Object.keys(state).some((key) => state[key as keyof ComparisonSyncState] !== lastState?.[key as keyof ComparisonSyncState])) {
      lastState = state;
      onChange(state);
    }
  };
  const applyAudio = () => { left.mute(audio !== "left"); right.mute(audio !== "right"); };
  const tick = () => {
    if (disposed) return;
    const values = snapshots();
    const now = Date.now();
    const elapsed = Math.max(0, (now - lastPoll) / 1000);
    const ownedPause: boolean[] = [];
    const ownedSeek: boolean[] = [];
    let rejectedPlay = false;
    for (let index = 0; index < 2; index += 1) {
      const fence = fences[index];
      ownedPause[index] = fence.paused !== null;
      if (fence.paused !== null) {
        if (fence.paused === values[index].paused) fence.paused = null;
        else if (now >= fence.pauseUntil) {
          if (fence.paused === false) rejectedPlay = true;
          fence.paused = null;
        }
      }
      ownedSeek[index] = fence.seek !== null;
      if (fence.seek !== null && values[index].ready) {
        if (fence.seekUntil === Infinity) fence.seekUntil = now + 2_500;
        if (Math.abs(values[index].time - fence.seek) < 0.65 || now >= fence.seekUntil) fence.seek = null;
      }
    }
    if (rejectedPlay) { wantsPlay = false; held = false; pausePair(values); }
    if (previous && !rejectedPlay) {
      // Native controls count as deliberate input; our own asynchronous
      // commands are fenced so they cannot be mistaken for another click.
      const active = linked ? [0, 1] : [0];
      const pausedByUser = active.some((index) => !ownedPause[index] && !previous![index].paused && values[index].paused && !previous![index].buffering && !values[index].buffering && !values[index].ended);
      // A native Play usually becomes unpaused while it is still buffering.
      // Capture that transition before the buffering hold pauses the pair;
      // Command fences distinguish it from our own buffering-hold resume.
      // The iframe adapter also keeps cued seeks paused while their commands
      // settle; HTML currentTime changes never unpause a video themselves.
      const playedByUser = active.find((index) => values[index].ready && previous![index].ready && !ownedPause[index] && previous![index].paused && !values[index].paused);
      if (pausedByUser) { wantsPlay = false; held = false; pausePair(values); }
      else if (playedByUser !== undefined) {
        wantsPlay = true;
        if (linked) seekPair(values[playedByUser].time - (playedByUser === 1 ? offset : 0), values);
      }
      const scrubbed = active.find((index) => !ownedSeek[index] && !ownedPause[index] && Math.abs(values[index].time - previous![index].time) > Math.max(1, elapsed * 2.5));
      if (scrubbed !== undefined) seekPair(values[scrubbed].time - (scrubbed === 1 ? offset : 0), values);
    }
    const range = bounds(values);
    const ready = values[0].ready && (!linked || values[1].ready) && range.end > range.start;
    if (requestedTime !== null && ready) {
      const desired = clamp(requestedTime, range.start, range.end);
      if (Math.abs(values[0].time - desired) < 0.65 && (!linked || Math.abs(values[1].time - desired - offset) < 0.65)) requestedTime = null;
      else if (fences[0].seek === null && (!linked || fences[1].seek === null)) requestedTime = null;
    }
    const buffering = !ready || values[0].buffering || (linked && values[1].buffering);
    if (wantsPlay && requestedTime === null && ready && (values[0].ended || (linked && values[1].ended) || values[0].time >= range.end - 0.08)) {
      wantsPlay = false; held = false; pausePair(values);
    } else if (wantsPlay && buffering) {
      held = true;
      pausePlayer(0, values);
      if (linked) pausePlayer(1, values);
    } else if (wantsPlay) {
      const active = linked ? [0, 1] : [0];
      if (!held || active.every((index) => values[index].paused && fences[index].paused !== true)) {
        held = false;
        for (const index of active) if (values[index].paused && fences[index].paused !== false) {
          fences[index].paused = false; fences[index].pauseUntil = now + 2_500;
          players[index].play();
        }
      }
      if (linked && !held && !values[0].paused && !values[1].paused && fences.every((fence) => fence.seek === null) && now - lastCorrection >= 2_000 && Math.abs(values[1].time - values[0].time - offset) > 0.9) {
        seekPlayer(1, clamp(values[0].time + offset, 0, values[1].duration || comparisonPositionMaximum), values);
        lastCorrection = now;
      }
    }
    previous = values;
    lastPoll = now;
    emit(values);
  };
  applyAudio();
  previous = snapshots();
  emit(previous);
  const timer = setInterval(tick, 100);
  return {
    play() {
      if (disposed) return;
      const values = snapshots();
      const range = bounds(values);
      const position = requestedTime ?? values[0].time;
      seekPair(position >= range.end - 0.08 ? range.start : position, values);
      wantsPlay = true;
      tick();
    },
    pause() {
      if (disposed) return;
      wantsPlay = false;
      held = false;
      pausePair();
      // HTML pause updates synchronously. Observe that acknowledgement now so
      // a native Play before our next poll is a fresh operator action. Iframe
      // pauses that have not arrived retain their asynchronous command fence.
      const values = snapshots();
      for (let index = 0; index < 2; index += 1) if (fences[index].paused === true && values[index].paused) fences[index].paused = null;
      previous = values;
      lastPoll = Date.now();
      emit(values);
    },
    seek(seconds) { if (!disposed && Number.isFinite(seconds)) { seekPair(seconds); emit(); } },
    setOffset(seconds) {
      if (disposed || !Number.isFinite(seconds)) return;
      offset = clamp(seconds, -comparisonPositionMaximum, comparisonPositionMaximum);
      if (linked) seekPair(requestedTime ?? snapshots()[0].time);
      emit();
    },
    setLinked(value) {
      if (disposed || linked === value) return;
      linked = value;
      held = false;
      if (linked) seekPair(requestedTime ?? snapshots()[0].time);
      emit();
    },
    setAudio(value) { if (!disposed && ["left", "right", "none"].includes(value)) { audio = value; applyAudio(); emit(); } },
    dispose() {
      if (disposed) return;
      disposed = true;
      wantsPlay = false;
      clearInterval(timer);
      // Cancel pending play requests even if both snapshots still say paused.
      left.pause(); right.pause();
    },
  };
}
