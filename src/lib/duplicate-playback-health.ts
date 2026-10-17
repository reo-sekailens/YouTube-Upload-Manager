/** Metadata can succeed for HEVC audio while its video track remains undecodable. */
export function watchLocalVideoPicture(video: HTMLVideoElement, onUnsupported: () => void): () => void {
  let active = true;
  let pictureSeen = false;
  let metadataTimer: ReturnType<typeof setTimeout> | undefined;
  let playbackTimer: ReturnType<typeof setTimeout> | undefined;
  let callback: number | undefined;
  const clearPlayback = () => {
    clearTimeout(playbackTimer);
    if (callback !== undefined) video.cancelVideoFrameCallback?.(callback);
    callback = undefined;
  };
  const fail = () => {
    if (!active) return;
    active = false;
    clearTimeout(metadataTimer);
    clearPlayback();
    onUnsupported();
  };
  const frame = () => {
    if (!active) return;
    pictureSeen = true;
    clearTimeout(metadataTimer);
    clearPlayback();
  };
  const metadata = () => {
    clearTimeout(metadataTimer);
    if (pictureSeen || video.videoWidth > 0) return;
    metadataTimer = setTimeout(() => {
      if (active && !pictureSeen && video.readyState >= 1 && video.videoWidth === 0) fail();
    }, 2000);
  };
  const playing = () => {
    clearPlayback();
    if (pictureSeen) return;
    if (typeof video.requestVideoFrameCallback === "function") callback = video.requestVideoFrameCallback(frame);
    playbackTimer = setTimeout(() => {
      const decodedFrames = video.getVideoPlaybackQuality?.().totalVideoFrames ?? 0;
      if (active && !video.paused && !pictureSeen && decodedFrames === 0) fail();
    }, 4000);
  };
  const data = () => {
    // Older webviews without frame callbacks still expose decoded frame counts.
    if ((video.getVideoPlaybackQuality?.().totalVideoFrames ?? 0) > 0) frame();
    metadata();
  };
  video.addEventListener("loadedmetadata", metadata);
  video.addEventListener("loadeddata", data);
  video.addEventListener("play", playing);
  video.addEventListener("pause", clearPlayback);
  video.addEventListener("error", fail);
  if (video.readyState >= 1) metadata();
  return () => {
    active = false;
    clearTimeout(metadataTimer);
    clearPlayback();
    video.removeEventListener("loadedmetadata", metadata);
    video.removeEventListener("loadeddata", data);
    video.removeEventListener("play", playing);
    video.removeEventListener("pause", clearPlayback);
    video.removeEventListener("error", fail);
  };
}
