import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { createCompatibilityPlayback } from "../lib/compatibility-playback";
import type { CompatibilityPlaybackStatus } from "../lib/compatibility-playback";
import { inspectDuplicatePreviewCompatibility, readDuplicatePreviewSegment } from "../lib/duplicate-media-preview";

export default function CompatibilityVideoPlayer({ token, fileName, initialTime, resume, onPlayer }: {
  token: string;
  fileName: string;
  initialTime: number;
  resume: boolean;
  onPlayer?: (video: HTMLVideoElement | null) => void;
}) {
  const player = useRef<HTMLVideoElement>(null);
  const [status, setStatus] = useState<CompatibilityPlaybackStatus>("loading");
  const [attempt, setAttempt] = useState(0);
  const position = useRef(initialTime);
  const resumeInitially = useRef(resume);
  useEffect(() => {
    const video = player.current;
    if (!video) return;
    onPlayer?.(video);
    const dispose = createCompatibilityPlayback(video, {
      inspect: () => inspectDuplicatePreviewCompatibility(token),
      readSegment: (startSeconds) => readDuplicatePreviewSegment(token, startSeconds),
      onStatus: setStatus,
    }, { initialTime: position.current, resume: resumeInitially.current });
    return () => {
      position.current = video.currentTime || position.current;
      // A retry and StrictMode teardown never acquire permission to start playing.
      resumeInitially.current = false;
      onPlayer?.(null);
      dispose();
    };
  }, [token, attempt, onPlayer]);
  return <>
    <div className="relative aspect-video w-full overflow-hidden rounded-lg bg-black">
      <video aria-label={`Local video: ${fileName} (compatibility player)`} className="absolute inset-0 h-full w-full object-contain" controls playsInline preload="metadata" ref={player} />
      {status === "loading" && <p className="pointer-events-none absolute inset-x-3 top-3 m-0 rounded-md bg-black/75 px-3 py-2 text-sm text-white" role="status">Preparing compatible video…</p>}
    </div>
    {status === "error" ? <div className="mt-3 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">
      <p className="m-0 font-semibold">Compatible playback could not be prepared.</p>
      <p className="mt-1 mb-3">Keep the original file available and unchanged, then retry. Your duplicate decision is unchanged.</p>
      <button className="inline-flex cursor-pointer items-center justify-center gap-2 rounded-md border border-line bg-control px-3 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-3 focus-visible:outline-focus-ring focus-visible:outline-offset-2" onClick={() => { setStatus("loading"); setAttempt((value) => value + 1); }} type="button"><RefreshCw aria-hidden="true" size={16} /> Retry compatibility player</button>
    </div> : <p className="mt-3 mb-0 text-sm leading-relaxed text-muted">Compatibility playback uses small video segments from the original file. Seek with the player controls; the original is unchanged.</p>}
  </>;
}
