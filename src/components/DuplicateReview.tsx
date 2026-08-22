import { useRef, useState } from "react";
import { clampComparisonPosition, comparisonPositionMaximum, moveComparisonPosition } from "../lib/comparison-controls";
import type { DuplicateCandidate } from "../lib/types";

function playerCommand(frame: HTMLIFrameElement | null, func: string, args: unknown[] = []) {
  frame?.contentWindow?.postMessage(JSON.stringify({ event: "command", func, args }), "https://www.youtube-nocookie.com");
}

function PlayIcon() { return <svg aria-hidden="true" viewBox="0 0 24 24"><path d="m8 5 11 7-11 7V5Z" fill="currentColor" /></svg>; }
function PauseIcon() { return <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M7 5h4v14H7V5Zm6 0h4v14h-4V5Z" fill="currentColor" /></svg>; }
function RewindTenIcon() { return <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M10.8 5.4A8 8 0 1 1 4 13.3h2A6 6 0 1 0 11 7.4V11L4.8 6.2 11 1.5v3.9h-.2Zm1.6 5.1h1.3v5.2h-1.3v-3.8l-1 .8-.7-.9 1.7-1.3Z" fill="currentColor" /></svg>; }
function ForwardTenIcon() { return <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M13.2 5.4A8 8 0 1 0 20 13.3h-2A6 6 0 1 1 13 7.4V11l6.2-4.8L13 1.5v3.9h.2Zm-1.6 5.1h-1.3v5.2h1.3v-3.8l1 .8.7-.9-1.7-1.3Z" fill="currentColor" /></svg>; }

function EmbeddedComparison({ candidate }: { candidate: DuplicateCandidate }) {
  const leftPlayer = useRef<HTMLIFrameElement>(null);
  const rightPlayer = useRef<HTMLIFrameElement>(null);
  const [position, setPosition] = useState(0);
  const [playing, setPlaying] = useState(false);
  const playerSource = (videoId: string) => `https://www.youtube-nocookie.com/embed/${encodeURIComponent(videoId)}?enablejsapi=1&playsinline=1&rel=0`;
  const setBothPosition = (seconds: number) => {
    const next = clampComparisonPosition(seconds);
    setPosition(next);
    playerCommand(leftPlayer.current, "seekTo", [next, true]);
    playerCommand(rightPlayer.current, "seekTo", [next, true]);
  };
  const togglePlayback = () => {
    const command = playing ? "pauseVideo" : "playVideo";
    playerCommand(leftPlayer.current, command);
    playerCommand(rightPlayer.current, command);
    setPlaying((current) => !current);
  };

  return <div className="duplicate-comparison"><div className="duplicate-comparison__players"><section className="duplicate-comparison__player"><header><span>Video A</span><strong>{candidate.leftTitle}</strong><code>{candidate.leftVideoId}</code></header><iframe ref={leftPlayer} src={playerSource(candidate.leftVideoId!)} title={`Video A: ${candidate.leftTitle}`} allow="autoplay; encrypted-media; picture-in-picture" referrerPolicy="strict-origin-when-cross-origin" /></section><section className="duplicate-comparison__player"><header><span>Video B</span><strong>{candidate.rightTitle}</strong><code>{candidate.rightVideoId}</code></header><iframe ref={rightPlayer} src={playerSource(candidate.rightVideoId!)} title={`Video B: ${candidate.rightTitle}`} allow="autoplay; encrypted-media; picture-in-picture" referrerPolicy="strict-origin-when-cross-origin" /></section></div><div className="duplicate-comparison__controls"><div aria-label="Synchronized comparison playback" className="duplicate-comparison__actions" role="group"><button aria-label="Move both videos back 10 seconds" className="comparison-icon-button" onClick={() => setBothPosition(moveComparisonPosition(position, -10))} title="Back 10 seconds" type="button"><RewindTenIcon /></button><button aria-label={playing ? "Pause both videos" : "Play both videos"} className="comparison-icon-button comparison-icon-button--primary" onClick={togglePlayback} title={playing ? "Pause both videos" : "Play both videos"} type="button">{playing ? <PauseIcon /> : <PlayIcon />}</button><button aria-label="Move both videos forward 10 seconds" className="comparison-icon-button" onClick={() => setBothPosition(moveComparisonPosition(position, 10))} title="Forward 10 seconds" type="button"><ForwardTenIcon /></button></div><label htmlFor={`comparison-position-${candidate.id}`}>Synchronized position <output>{position}s</output></label><input id={`comparison-position-${candidate.id}`} aria-label="Synchronized position in seconds" max={comparisonPositionMaximum} min="0" onChange={(event) => setBothPosition(Number(event.target.value))} step="1" type="range" value={position} /><p>Use the icon controls to play, pause, or seek both owner-authorized YouTube embeds together. This comparison never changes either video.</p></div></div>;
}

export function DuplicateReview({ candidates }: { candidates: DuplicateCandidate[] }) {
  if (candidates.length === 0) return <p className="duplicate-review__empty duplicate-review__empty--state">No duplicate candidates found in this account.</p>;
  return <div className="duplicate-review duplicate-review--rail" role="list">{candidates.map((candidate) => {
    const isExact = candidate.confidence === "exact_local";
    const isUploadedTitle = candidate.confidence === "metadata" && Boolean(candidate.leftVideoId && candidate.rightVideoId);
    const decision = candidate.decision?.replaceAll("_", " ") ?? "Unreviewed";
    return <section className="duplicate-group duplicate-group--rail" key={candidate.id} aria-label={`Duplicate candidate: ${candidate.evidence}`} role="listitem"><header className="duplicate-group__header"><div className="duplicate-group__summary"><span className={`match-badge match-badge--${isExact ? "exact" : "possible"}`}>{isExact ? "Exact local match" : isUploadedTitle ? "Uploaded-title candidate" : "Review required"}</span><p>{candidate.evidence}</p></div><span className="duplicate-group__count">{decision}</span></header>{isUploadedTitle ? <EmbeddedComparison candidate={candidate} /> : <div className="duplicate-group__items"><article className="duplicate-card duplicate-card--rail"><div className="duplicate-card__thumbnail" aria-hidden="true"><span>↔</span></div><div className="duplicate-card__body"><span className="duplicate-card__source">Managed local media comparison</span><h3>{candidate.leftTitle}</h3><p>{candidate.rightTitle}</p></div><aside className="duplicate-card__actions"><span className="duplicate-card__review">Deletion requires a separate explicit confirmation.</span></aside></article></div>}</section>;
  })}</div>;
}
