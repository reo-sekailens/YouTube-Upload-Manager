import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { Pause, Play, RotateCcw, RotateCw, X } from "lucide-react";
import "./DuplicateReview.lazy.css";
import {
  clampComparisonPosition,
  moveComparisonPosition,
  sharedComparisonDuration,
  shouldLoadComparisonPlayers,
  youtubeComparisonEmbedUrl,
  youtubeComparisonOrigin,
} from "../lib/comparison-controls";
import {
  beginDeletionAuthorization,
  enableDeletionSudoMode,
  executeDeletionRequest,
  isTauri,
  loadConnectionSettings,
  openYouTubeAccountBrowser,
  requestVideoDeletion,
} from "../lib/local";
import { windowItems } from "../lib/list-windowing";
import { useRetainedWorkspaceState } from "../lib/retained-workspace-state";
import { subscribeLocalStateChanges } from "../lib/state-events";
import type { DuplicateCandidate } from "../lib/types";
import { PaginationControls } from "./PaginationControls";

function playerCommand(
  frame: HTMLIFrameElement | null,
  func: string,
  args: unknown[] = [],
) {
  frame?.contentWindow?.postMessage(
    JSON.stringify({ event: "command", func, args }),
    youtubeComparisonOrigin,
  );
}
function formatDuration(seconds: number) {
  const wholeSeconds = Math.max(0, Math.floor(seconds));
  return `${Math.floor(wholeSeconds / 60)}:${String(wholeSeconds % 60).padStart(2, "0")}`;
}
type DuplicateReviewProps = {
  activeChannelId?: string;
  candidates: DuplicateCandidate[];
  onIgnore: (candidateId: string) => void | Promise<void>;
  onDeletionComplete?: (videoId: string, title: string) => Promise<void> | void;
  onBulkDeletionComplete?: (count: number) => Promise<void> | void;
};
type PendingDeletion = {
  videoId: string;
  title: string;
  label: "Video A" | "Video B";
};
type SelectedDuplicateVideo = PendingDeletion;
type BulkDeletionLogEntry = SelectedDuplicateVideo & {
  status: "queued" | "deleting" | "deleted" | "failed";
};

type PlayerInfoSubscriber = (event: MessageEvent) => void;
const secondaryButtonClass =
  "cursor-pointer rounded-md border border-[#cdd4df] bg-white px-2.5 py-1.5 text-[0.72rem] font-[680] text-[#344a67] transition-colors hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-55";
const dangerButtonClass =
  "cursor-pointer rounded-md border border-[#e5c2c0] bg-white px-2.5 py-1.5 text-[0.72rem] font-[680] text-[#a4413b] transition-colors hover:border-[#d89d98] hover:bg-[#fff5f4] focus-visible:outline-3 focus-visible:outline-[#c44f463d] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-55";
const deletionPanelClass =
  "mt-3.5 grid gap-2.5 rounded-lg border border-[#edcbc8] bg-[#fff8f7] p-3.5";
const deletionLabelClass =
  "grid gap-1.5 text-[0.74rem] font-bold text-[#5e4848] [&_input]:rounded-md [&_input]:border [&_input]:border-[#d8bebb] [&_input]:bg-white [&_input]:px-2.5 [&_input]:py-2 [&_input]:text-[#303040] [&_input]:focus:border-[#b85048] [&_input]:focus:outline-3 [&_input]:focus:outline-[#c44f4629]";
const deletionActionClass =
  "flex flex-wrap items-center justify-end gap-2.5 max-sm:flex-col max-sm:items-stretch max-sm:[&_button]:w-full";
const deletionAuthorizationClass =
  "flex flex-wrap items-center justify-between gap-2.5 max-sm:flex-col max-sm:items-stretch max-sm:[&_button]:w-full";
const logStatusClasses: Record<BulkDeletionLogEntry["status"], string> = {
  queued: "text-[#8d6b2d]",
  deleting: "text-[#a4413b]",
  deleted: "text-[#28714e]",
  failed: "text-[#a4413b]",
};
const matchBadgeClasses = {
  exact: "bg-[#e9f7ef] text-[#28714e]",
  possible: "bg-[#fff5df] text-[#8d6b2d]",
} as const;
const playerInfoSubscribers = new Set<PlayerInfoSubscriber>();
const dispatchPlayerInfo = (event: MessageEvent) => {
  if (event.origin !== youtubeComparisonOrigin) return;
  for (const subscriber of playerInfoSubscribers) subscriber(event);
};
function subscribePlayerInfo(subscriber: PlayerInfoSubscriber) {
  if (playerInfoSubscribers.size === 0)
    window.addEventListener("message", dispatchPlayerInfo);
  playerInfoSubscribers.add(subscriber);
  return () => {
    playerInfoSubscribers.delete(subscriber);
    if (playerInfoSubscribers.size === 0)
      window.removeEventListener("message", dispatchPlayerInfo);
  };
}

function DeleteProgress({
  completed,
  total,
  stage,
}: {
  completed: number;
  total: number;
  stage: string;
}) {
  const percentage =
    total > 0 ? Math.min(100, Math.round((completed / total) * 100)) : 0;
  return (
    <div
      aria-label="Duplicate deletion progress"
      aria-live="polite"
      aria-valuemax={total}
      aria-valuemin={0}
      aria-valuenow={completed}
      className="grid gap-1.5 rounded-md border border-[#edcbc8] bg-white p-2.5"
      role="progressbar"
    >
      <div className="flex items-baseline justify-between gap-2.5 text-[0.72rem] leading-snug font-[680] text-[#704b49]">
        <span>{stage}</span>
        <strong className="shrink-0 text-[0.7rem] text-[#a4413b]">
          {completed} of {total}
        </strong>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-[#f0d9d6]">
        <span
          className="block h-full min-w-0 rounded-[inherit] bg-[#c95146] transition-[width] duration-250"
          style={{ width: `${percentage}%` }}
        />
      </div>
    </div>
  );
}

function BulkDeletionLog({ entries }: { entries: BulkDeletionLogEntry[] }) {
  const deleted = entries.filter((entry) => entry.status === "deleted").length;
  const deleting = entries.filter(
    (entry) => entry.status === "deleting",
  ).length;
  return (
    <details className="rounded-md border border-[#edcbc8] bg-white text-[0.72rem] text-[#704b49]">
      <summary className="flex cursor-pointer items-center justify-between gap-2.5 p-2.5 font-bold [&_span]:text-[0.68rem] [&_span]:font-[680] [&_span]:text-[#a4413b]">
        Deletion activity log{" "}
        <span>
          {deleted} deleted ·{" "}
          {deleting > 0 ? "1 deleting" : `${entries.length - deleted} queued`}
        </span>
      </summary>
      <ol className="grid list-none gap-1.5 border-t border-[#f0ddda] px-2.5 pt-2 pb-2.5">
        {entries.map((entry) => (
          <li
            className="grid grid-cols-[auto_minmax(0,1fr)] items-baseline gap-x-2 gap-y-0.5"
            key={entry.videoId}
          >
            <span className={`col-start-1 whitespace-nowrap text-[0.67rem] font-bold ${logStatusClasses[entry.status]}`}>
              {entry.status === "queued"
                ? "About to delete"
                : entry.status === "deleting"
                  ? "Deleting now"
                  : entry.status === "deleted"
                    ? "Deleted"
                    : "Could not delete"}
            </span>
            <strong className="col-start-2 overflow-wrap-anywhere text-[0.7rem] text-[#4d3a39]">
              {entry.label}: {entry.title}
            </strong>
            <code className="col-start-2 px-1.5 py-0.5 text-[0.65rem]">
              {entry.videoId}
            </code>
          </li>
        ))}
      </ol>
    </details>
  );
}

function EmbeddedComparison({
  candidate,
  onDeletionComplete,
  selected,
  onSelect,
  deletionAuthorized,
  deletionSudoActive,
  authorizingDeletion,
  deletionAuthorizationError,
  onAuthorizeDeletion,
  onEnableDeletionMode,
}: {
  candidate: DuplicateCandidate;
  onDeletionComplete?: DuplicateReviewProps["onDeletionComplete"];
  selected: Set<string>;
  onSelect: (video: SelectedDuplicateVideo, checked: boolean) => void;
  deletionAuthorized: boolean;
  deletionSudoActive: boolean;
  authorizingDeletion: boolean;
  deletionAuthorizationError: string;
  onAuthorizeDeletion: () => Promise<void>;
  onEnableDeletionMode: () => Promise<void>;
}) {
  const leftPlayer = useRef<HTMLIFrameElement>(null);
  const rightPlayer = useRef<HTMLIFrameElement>(null);
  const [position, setPosition] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [leftDuration, setLeftDuration] = useState(0);
  const [rightDuration, setRightDuration] = useState(0);
  const [previewLoaded, setPreviewLoaded] = useState(false);
  const [readyPlayerCount, setReadyPlayerCount] = useState(0);
  const [accountBrowserError, setAccountBrowserError] = useState("");
  const [pendingDeletion, setPendingDeletion] = useState<PendingDeletion>();
  const [deletionConfirmation, setDeletionConfirmation] = useState("");
  const [deleting, setDeleting] = useState(false);
  const [deleteProgress, setDeleteProgress] = useState(0);
  const [deletionError, setDeletionError] = useState("");
  const maximumPosition = useMemo(
    () => sharedComparisonDuration([leftDuration, rightDuration]),
    [leftDuration, rightDuration],
  );
  const playerSource = youtubeComparisonEmbedUrl;
  const openInYouTube = async (videoId: string) => {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(
      `https://www.youtube.com/watch?v=${encodeURIComponent(videoId)}`,
    );
  };

  useEffect(() => {
    if (!previewLoaded) return;
    const receivePlayerInfo = (event: MessageEvent) => {
      let message: unknown;
      try {
        message =
          typeof event.data === "string" ? JSON.parse(event.data) : event.data;
      } catch {
        return;
      }
      if (
        !message ||
        typeof message !== "object" ||
        !("event" in message) ||
        message.event !== "infoDelivery" ||
        !("info" in message) ||
        !message.info ||
        typeof message.info !== "object"
      )
        return;
      const info = message.info as {
        currentTime?: unknown;
        duration?: unknown;
      };
      const duration =
        typeof info.duration === "number" && Number.isFinite(info.duration)
          ? info.duration
          : 0;
      if (event.source === leftPlayer.current?.contentWindow) {
        if (duration > 0) setLeftDuration(duration);
        if (
          typeof info.currentTime === "number" &&
          Number.isFinite(info.currentTime)
        )
          setPosition(info.currentTime);
      }
      if (event.source === rightPlayer.current?.contentWindow && duration > 0)
        setRightDuration(duration);
    };
    return subscribePlayerInfo(receivePlayerInfo);
  }, [previewLoaded]);
  useEffect(() => {
    setPosition((current) => clampComparisonPosition(current, maximumPosition));
  }, [maximumPosition]);
  useEffect(() => {
    if (!previewLoaded || !playing || readyPlayerCount < 2) return;
    playerCommand(leftPlayer.current, "playVideo");
    playerCommand(rightPlayer.current, "playVideo");
  }, [playing, previewLoaded, readyPlayerCount]);

  const setBothPosition = (seconds: number) => {
    const next = clampComparisonPosition(seconds, maximumPosition);
    setPosition(next);
    playerCommand(leftPlayer.current, "seekTo", [next, true]);
    playerCommand(rightPlayer.current, "seekTo", [next, true]);
  };
  const togglePlayback = () => {
    if (!previewLoaded) {
      setReadyPlayerCount(0);
      setPreviewLoaded(true);
      setPlaying(true);
      return;
    }
    const command = playing ? "pauseVideo" : "playVideo";
    playerCommand(leftPlayer.current, command);
    playerCommand(rightPlayer.current, command);
    setPlaying((current) => !current);
  };
  const unloadPreview = () => {
    playerCommand(leftPlayer.current, "pauseVideo");
    playerCommand(rightPlayer.current, "pauseVideo");
    setPlaying(false);
    setPosition(0);
    setLeftDuration(0);
    setRightDuration(0);
    setReadyPlayerCount(0);
    setPreviewLoaded(false);
  };
  const openAccountBrowser = async () => {
    setAccountBrowserError("");
    try {
      await openYouTubeAccountBrowser();
    } catch (error) {
      setAccountBrowserError(
        error instanceof Error
          ? error.message
          : "The YouTube account browser could not be opened.",
      );
    }
  };
  const startDeletion = (
    videoId: string,
    title: string,
    label: PendingDeletion["label"],
  ) => {
    setPendingDeletion({ videoId, title, label });
    setDeletionConfirmation("");
    setDeletionError("");
  };
  const authorizeDeletion = async () => {
    if (!isTauri) return;
    setDeletionError("");
    try {
      await onAuthorizeDeletion();
    } catch (error) {
      setDeletionError(
        error instanceof Error
          ? error.message
          : "Deletion authorization could not be started.",
      );
    }
  };
  const enableDeletionMode = async () => {
    setDeleting(true);
    setDeletionError("");
    try {
      await onEnableDeletionMode();
    } catch (error) {
      setDeletionError(
        error instanceof Error
          ? error.message
          : "Temporary deletion mode could not be enabled.",
      );
    } finally {
      setDeleting(false);
    }
  };
  const deleteDirectly = async () => {
    if (
      !pendingDeletion ||
      deletionConfirmation !== pendingDeletion.videoId ||
      !deletionSudoActive
    )
      return;
    setDeleting(true);
    setDeletionError("");
    setDeleteProgress(1);
    try {
      const request = await requestVideoDeletion(
        pendingDeletion.videoId,
        deletionConfirmation,
      );
      setDeleteProgress(2);
      const completed = await executeDeletionRequest(
        request.id,
        deletionConfirmation,
      );
      await onDeletionComplete?.(completed.videoId, pendingDeletion.title);
      setPendingDeletion(undefined);
      setDeletionConfirmation("");
    } catch (error) {
      setDeletionError(
        error instanceof Error
          ? error.message
          : "YouTube did not confirm the deletion. The video remains in the local review queue.",
      );
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="grid gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2.5 rounded-md border border-[#dce5f1] bg-[#f4f7fb] px-2.5 py-2 text-[0.73rem] leading-snug text-[#5c6c82] [&>div]:flex [&>div]:flex-wrap [&>div]:gap-2 [&>p]:basis-full [&>p]:m-0 [&>p]:text-[#a4413b]">
        <span>
          {previewLoaded
            ? readyPlayerCount >= 2
              ? "Embedded side-by-side comparison is active."
              : "Loading side-by-side comparison…"
            : "Embedded players stay unloaded until you press Play."}
        </span>
        <div>
          <button
            className={secondaryButtonClass}
            disabled={!isTauri}
            onClick={() => void openAccountBrowser()}
            type="button"
          >
            Open YouTube account browser
          </button>
          {previewLoaded && (
            <button
              aria-label="Close comparison"
              className={secondaryButtonClass}
              onClick={unloadPreview}
              title="Close comparison"
              type="button"
            >
              <X aria-hidden="true" size={16} />
            </button>
          )}
        </div>
        {accountBrowserError && <p role="alert">{accountBrowserError}</p>}
      </div>
      <div className="grid gap-3 min-[820px]:grid-cols-2">
        <section className="grid gap-2.5">
          <header className="grid gap-1">
            <span className="text-[0.72rem] font-bold text-[#52617a]">Video A</span>
            <strong className="overflow-wrap-anywhere text-[0.85rem] text-[#2d3f5d]">{candidate.leftTitle}</strong>
            <code className="overflow-wrap-anywhere text-[0.68rem] text-[#65758b]">{candidate.leftVideoId}</code>
          </header>
          {shouldLoadComparisonPlayers(playing, previewLoaded) ? (
            <iframe
              className="aspect-video w-full rounded-md border-0"
              onLoad={() => setReadyPlayerCount((count) => count + 1)}
              ref={leftPlayer}
              src={playerSource(candidate.leftVideoId!)}
              title={`Video A: ${candidate.leftTitle}`}
              allow="autoplay; encrypted-media; picture-in-picture"
              referrerPolicy="strict-origin-when-cross-origin"
            />
          ) : (
            <div className="flex aspect-video items-center justify-center rounded-md border border-dashed border-[#c8d1dd] bg-[#f1f3f6] px-3 text-center text-[0.74rem] text-[#718096]">
              Video A loads after you press Play.
            </div>
          )}
          <div className="flex flex-wrap gap-2">
            <label className="mt-2 inline-flex cursor-pointer items-center gap-1.5 text-[0.7rem] font-[680] text-[#52617a] has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-55 [&_input]:m-0 [&_input]:accent-[#2463df]">
              <input
                checked={selected.has(candidate.leftVideoId!)}
                disabled={deleting}
                onChange={(event) =>
                  onSelect(
                    {
                      videoId: candidate.leftVideoId!,
                      title: candidate.leftTitle,
                      label: "Video A",
                    },
                    event.target.checked,
                  )
                }
                type="checkbox"
              />{" "}
              Select A
            </label>
            <button
              className={secondaryButtonClass}
              onClick={() => void openInYouTube(candidate.leftVideoId!)}
              type="button"
            >
              Open Video A in YouTube
            </button>
            <button
              className={dangerButtonClass}
              disabled={deleting}
              onClick={() =>
                startDeletion(
                  candidate.leftVideoId!,
                  candidate.leftTitle,
                  "Video A",
                )
              }
              type="button"
            >
              Delete Video A
            </button>
          </div>
        </section>
        <section className="grid gap-2.5">
          <header className="grid gap-1">
            <span className="text-[0.72rem] font-bold text-[#52617a]">Video B</span>
            <strong className="overflow-wrap-anywhere text-[0.85rem] text-[#2d3f5d]">{candidate.rightTitle}</strong>
            <code className="overflow-wrap-anywhere text-[0.68rem] text-[#65758b]">{candidate.rightVideoId}</code>
          </header>
          {shouldLoadComparisonPlayers(playing, previewLoaded) ? (
            <iframe
              className="aspect-video w-full rounded-md border-0"
              onLoad={() => setReadyPlayerCount((count) => count + 1)}
              ref={rightPlayer}
              src={playerSource(candidate.rightVideoId!)}
              title={`Video B: ${candidate.rightTitle}`}
              allow="autoplay; encrypted-media; picture-in-picture"
              referrerPolicy="strict-origin-when-cross-origin"
            />
          ) : (
            <div className="flex aspect-video items-center justify-center rounded-md border border-dashed border-[#c8d1dd] bg-[#f1f3f6] px-3 text-center text-[0.74rem] text-[#718096]">
              Video B loads after you press Play.
            </div>
          )}
          <div className="flex flex-wrap gap-2">
            <label className="mt-2 inline-flex cursor-pointer items-center gap-1.5 text-[0.7rem] font-[680] text-[#52617a] has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-55 [&_input]:m-0 [&_input]:accent-[#2463df]">
              <input
                checked={selected.has(candidate.rightVideoId!)}
                disabled={deleting}
                onChange={(event) =>
                  onSelect(
                    {
                      videoId: candidate.rightVideoId!,
                      title: candidate.rightTitle,
                      label: "Video B",
                    },
                    event.target.checked,
                  )
                }
                type="checkbox"
              />{" "}
              Select B
            </label>
            <button
              className={secondaryButtonClass}
              onClick={() => void openInYouTube(candidate.rightVideoId!)}
              type="button"
            >
              Open Video B in YouTube
            </button>
            <button
              className={dangerButtonClass}
              disabled={deleting}
              onClick={() =>
                startDeletion(
                  candidate.rightVideoId!,
                  candidate.rightTitle,
                  "Video B",
                )
              }
              type="button"
            >
              Delete Video B
            </button>
          </div>
        </section>
      </div>
      {pendingDeletion && (
        <section
          className={deletionPanelClass}
          aria-labelledby={`duplicate-delete-${candidate.id}`}
        >
          <p className="m-0 text-[0.7rem] font-bold tracking-[0.08em] text-[#a4413b]">PERMANENT, IRREVERSIBLE ACTION</p>
          <h3 className="m-0 text-[0.93rem] text-[#3e2a2a]" id={`duplicate-delete-${candidate.id}`}>
            Delete {pendingDeletion.label}: “{pendingDeletion.title}”
          </h3>
          <p className="m-0 text-[0.75rem] leading-relaxed text-[#6e5554]">
            This removes this video from YouTube directly from this duplicate
            card. A temporary deletion mode, an exact typed video ID, and a
            fresh channel-ownership check are required.
          </p>
          <code className="w-fit rounded border border-[#efdad7] bg-white px-2 py-1.5 text-[0.75rem] text-[#8e3833]">{pendingDeletion.videoId}</code>
          {!deletionAuthorized && (
            <div className={deletionAuthorizationClass}>
              <span className="text-[0.73rem] text-[#8a5450]">Deletion permission is not active.</span>
              <button
                className={dangerButtonClass}
                disabled={authorizingDeletion || deleting || !isTauri}
                onClick={() => void authorizeDeletion()}
                type="button"
              >
                {authorizingDeletion
                  ? "Waiting for Google…"
                  : "Grant deletion permission"}
              </button>
            </div>
          )}
          {deletionAuthorized && !deletionSudoActive && (
            <div className={deletionAuthorizationClass}>
              <span className="text-[0.73rem] text-[#8a5450]">Deletion mode is off.</span>
              <button
                className={dangerButtonClass}
                disabled={deleting || !isTauri}
                onClick={() => void enableDeletionMode()}
                type="button"
              >
                Enter deletion mode (15 min)
              </button>
            </div>
          )}
          {deletionSudoActive && (
            <label className={deletionLabelClass} htmlFor={`duplicate-delete-confirmation-${candidate.id}`}>
              Type the exact video ID to permanently delete it
              <input
                id={`duplicate-delete-confirmation-${candidate.id}`}
                autoComplete="off"
                autoFocus
                onChange={(event) =>
                  setDeletionConfirmation(event.target.value)
                }
                placeholder={pendingDeletion.videoId}
                spellCheck={false}
                value={deletionConfirmation}
              />
            </label>
          )}
          {deleting && (
            <DeleteProgress
              completed={deleteProgress}
              stage={
                deleteProgress === 1
                  ? "Confirming the deletion request…"
                  : "Deleting this video from YouTube…"
              }
              total={2}
            />
          )}
          {(deletionError || deletionAuthorizationError) && (
            <p className="font-[650] text-[#a4413b]" role="alert">
              {deletionError || deletionAuthorizationError}
            </p>
          )}
          <div className={deletionActionClass}>
            <button
              className={secondaryButtonClass}
              disabled={deleting}
              onClick={() => {
                setPendingDeletion(undefined);
                setDeletionConfirmation("");
                setDeletionError("");
                setDeleteProgress(0);
              }}
              type="button"
            >
              Keep video
            </button>
            {deletionSudoActive && (
              <button
                className={dangerButtonClass}
                disabled={
                  deleting || deletionConfirmation !== pendingDeletion.videoId
                }
                onClick={() => void deleteDirectly()}
                type="button"
              >
                {deleting ? "Deleting…" : "Delete permanently from YouTube"}
              </button>
            )}
          </div>
        </section>
      )}
      <div className="grid gap-2 rounded-md border border-[#dce5f1] bg-[#f8fafc] p-2.5 [&>label]:grid [&>label]:gap-1 [&>label]:text-[0.72rem] [&>label]:font-bold [&>label]:text-[#52617a] [&_output]:text-[0.7rem] [&_output]:font-medium [&_output]:text-[#65758b] [&>input]:w-full [&>p]:m-0 [&>p]:text-[0.7rem] [&>p]:leading-snug [&>p]:text-[#65758b]">
        <div
          aria-label="Synchronized comparison playback"
          className="inline-flex gap-1.5"
          role="group"
        >
          <button
            aria-label="Move both videos back 10 seconds"
            className="inline-flex size-9 cursor-pointer items-center justify-center rounded-md border border-[#cdd4df] bg-white p-0 text-[#34405a] transition-colors hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50 [&_svg]:size-[1.18rem]"
            disabled={!previewLoaded || readyPlayerCount < 2}
            onClick={() =>
              setBothPosition(
                moveComparisonPosition(position, -10, maximumPosition),
              )
            }
            title="Back 10 seconds"
            type="button"
          >
            <RotateCcw aria-hidden="true" />
          </button>
          <button
            aria-label={playing ? "Pause both videos" : "Play both videos"}
            className="inline-flex size-9 cursor-pointer items-center justify-center rounded-md border border-[#2463df] bg-[#2463df] p-0 text-white transition-colors hover:border-[#1b54c6] hover:bg-[#1b54c6] hover:shadow-[0_3px_8px_rgba(31,78,181,0.16)] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 [&_svg]:size-[1.18rem]"
            onClick={togglePlayback}
            title={playing ? "Pause both videos" : "Play both videos"}
            type="button"
          >
            {playing ? <Pause aria-hidden="true" /> : <Play aria-hidden="true" />}
          </button>
          <button
            aria-label="Move both videos forward 10 seconds"
            className="inline-flex size-9 cursor-pointer items-center justify-center rounded-md border border-[#cdd4df] bg-white p-0 text-[#34405a] transition-colors hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50 [&_svg]:size-[1.18rem]"
            disabled={!previewLoaded || readyPlayerCount < 2}
            onClick={() =>
              setBothPosition(
                moveComparisonPosition(position, 10, maximumPosition),
              )
            }
            title="Forward 10 seconds"
            type="button"
          >
            <RotateCw aria-hidden="true" />
          </button>
        </div>
        <label htmlFor={`comparison-position-${candidate.id}`}>
          Synchronized position{" "}
          <output>
            {previewLoaded
              ? `${formatDuration(position)} / ${leftDuration > 0 && rightDuration > 0 ? formatDuration(maximumPosition) : "Loading length…"}`
              : "Press Play to load"}
          </output>
        </label>
        <input
          disabled={!previewLoaded || readyPlayerCount < 2}
          id={`comparison-position-${candidate.id}`}
          aria-label="Synchronized position in seconds"
          max={maximumPosition}
          min="0"
          onChange={(event) => setBothPosition(Number(event.target.value))}
          step="1"
          type="range"
          value={position}
        />
        <p>
          Sign in through the YouTube account browser before playing. Comparison
          uses YouTube's standard embed origin and may reflect that WebView
          session, but video embedding, account, and third-party-cookie policies
          still apply.
        </p>
      </div>
    </div>
  );
}

export function DuplicateReview({
  activeChannelId,
  candidates,
  onIgnore,
  onDeletionComplete,
  onBulkDeletionComplete,
}: DuplicateReviewProps) {
  const [selectedVideos, setSelectedVideos] =
    useRetainedWorkspaceState<Map<string, SelectedDuplicateVideo>>(
      "dedupe.selected-videos",
      () => new Map(),
    );
  const [searchQuery, setSearchQuery] = useRetainedWorkspaceState(
    "dedupe.candidate-search",
    "",
  );
  const deferredSearchQuery = useDeferredValue(searchQuery);
  const [page, setPage] = useRetainedWorkspaceState(
    "dedupe.candidate-page",
    1,
  );
  const [bulkConfirmation, setBulkConfirmation] = useState("");
  const [bulkOpen, setBulkOpen] = useState(false);
  const [bulkAuthorized, setBulkAuthorized] = useState(false);
  const [bulkSudoActive, setBulkSudoActive] = useState(false);
  const [bulkAuthorizing, setBulkAuthorizing] = useRetainedWorkspaceState(
    "dedupe.deletion-authorizing",
    false,
  );
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [bulkError, setBulkError] = useState("");
  const [bulkProgress, setBulkProgress] = useState({
    completed: 0,
    total: 0,
    stage: "",
  });
  const [bulkDeletionLog, setBulkDeletionLog] = useState<
    BulkDeletionLogEntry[]
  >([]);
  const selectableVideos = useMemo(
    () =>
      candidates.flatMap((candidate) =>
        candidate.confidence === "metadata" &&
        candidate.leftVideoId &&
        candidate.rightVideoId
          ? [
              {
                videoId: candidate.leftVideoId,
                title: candidate.leftTitle,
                label: "Video A" as const,
              },
              {
                videoId: candidate.rightVideoId,
                title: candidate.rightTitle,
                label: "Video B" as const,
              },
            ]
          : [],
      ),
    [candidates],
  );
  const filteredCandidates = useMemo(() => {
    const query = deferredSearchQuery.trim().toLocaleLowerCase();
    if (!query) return candidates;
    return candidates.filter(
      (candidate) =>
        candidate.leftTitle.toLocaleLowerCase().includes(query) ||
        candidate.rightTitle.toLocaleLowerCase().includes(query),
    );
  }, [candidates, deferredSearchQuery]);
  const visibleCandidates = useMemo(
    () => windowItems(filteredCandidates, page),
    [filteredCandidates, page],
  );
  const selectedVideoIds = useMemo(
    () => new Set(selectedVideos.keys()),
    [selectedVideos],
  );
  const bulkPhrase = `DELETE ${selectedVideos.size} VIDEO${selectedVideos.size === 1 ? "" : "S"}`;

  useEffect(() => {
    if (!isTauri) return;
    void loadConnectionSettings()
      .then((settings) => {
        setBulkAuthorized(settings.deletionAuthorized === true);
        setBulkSudoActive(settings.deletionSudoActive === true);
      })
      .catch(() => undefined);
  }, []);
  useEffect(() => {
    if (!isTauri || !activeChannelId) return;
    let active = true;
    let unsubscribe: (() => void) | undefined;
    void subscribeLocalStateChanges((batch) => {
      if (
        !batch.changes.some(
          (change) =>
            change.channelId === activeChannelId &&
            change.surface === "connection",
        )
      )
        return;
      void loadConnectionSettings()
        .then((settings) => {
          if (!active) return;
          setBulkAuthorized(settings.deletionAuthorized === true);
          setBulkSudoActive(settings.deletionSudoActive === true);
          if (settings.deletionAuthorized) setBulkAuthorizing(false);
        })
        .catch(() => undefined);
    })
      .then((stop) => {
        if (active) unsubscribe = stop;
        else stop();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [activeChannelId, setBulkAuthorizing]);

  useEffect(() => {
    if (!bulkAuthorizing || !isTauri) return;
    let active = true;
    let timeout: number | undefined;
    const delays = [250, 500, 1_000, 2_000, 4_000, 8_000, 16_000];
    const check = async (attempt: number) => {
      try {
        const settings = await loadConnectionSettings();
        if (!active) return;
        if (settings.deletionAuthorized) {
          setBulkAuthorized(true);
          setBulkSudoActive(settings.deletionSudoActive === true);
          setBulkAuthorizing(false);
          return;
        }
      } catch {
        /* Event delivery remains primary; this fallback is intentionally bounded. */
      }
      if (active && attempt + 1 < delays.length)
        timeout = window.setTimeout(
          () => void check(attempt + 1),
          delays[attempt + 1],
        );
    };
    timeout = window.setTimeout(() => void check(0), delays[0]);
    return () => {
      active = false;
      if (timeout !== undefined) window.clearTimeout(timeout);
    };
  }, [bulkAuthorizing, setBulkAuthorizing]);

  const toggleSelection = useCallback((video: SelectedDuplicateVideo, checked: boolean) =>
    setSelectedVideos((current) => {
      const next = new Map(current);
      if (checked) next.set(video.videoId, video);
      else next.delete(video.videoId);
      return next;
    }), [setSelectedVideos]);
  const selectSide = (label: SelectedDuplicateVideo["label"]) =>
    setSelectedVideos(
      new Map(
        selectableVideos
          .filter((video) => video.label === label)
          .map((video) => [video.videoId, video]),
      ),
    );
  const authorizeBulk = useCallback(async () => {
    if (!isTauri) return;
    setBulkAuthorizing(true);
    setBulkError("");
    try {
      const { authorizationUrl } = await beginDeletionAuthorization();
      const url = new URL(authorizationUrl);
      if (url.protocol !== "https:")
        throw new Error("The deletion authorization request must use HTTPS.");
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url.toString());
    } catch (error) {
      setBulkAuthorizing(false);
      setBulkError(
        error instanceof Error
          ? error.message
          : "Deletion authorization could not be started.",
      );
    }
  }, [setBulkAuthorizing]);
  const enableBulkMode = useCallback(async () => {
    setBulkDeleting(true);
    setBulkError("");
    try {
      const settings = await enableDeletionSudoMode();
      setBulkSudoActive(settings.deletionSudoActive === true);
    } catch (error) {
      setBulkError(
        error instanceof Error
          ? error.message
          : "Temporary deletion mode could not be enabled.",
      );
    } finally {
      setBulkDeleting(false);
    }
  }, []);
  const deleteSelected = async () => {
    if (
      !bulkSudoActive ||
      bulkConfirmation !== bulkPhrase ||
      selectedVideos.size === 0
    )
      return;
    const videos = [...selectedVideos.values()];
    setBulkDeleting(true);
    setBulkError("");
    setBulkProgress({
      completed: 0,
      total: videos.length,
      stage: "Preparing confirmed deletions…",
    });
    let deleted = 0;
    let activeVideo: SelectedDuplicateVideo | undefined;
    try {
      for (const video of videos) {
        activeVideo = video;
        setBulkProgress({
          completed: deleted,
          total: videos.length,
          stage: `Deleting “${video.title}”…`,
        });
        setBulkDeletionLog((current) =>
          current.map((entry) =>
            entry.videoId === video.videoId
              ? { ...entry, status: "deleting" }
              : entry,
          ),
        );
        const request = await requestVideoDeletion(
          video.videoId,
          video.videoId,
        );
        await executeDeletionRequest(request.id, video.videoId);
        deleted += 1;
        setBulkProgress({
          completed: deleted,
          total: videos.length,
          stage: `Deleted ${deleted} of ${videos.length} video${videos.length === 1 ? "" : "s"}.`,
        });
        setBulkDeletionLog((current) =>
          current.map((entry) =>
            entry.videoId === video.videoId
              ? { ...entry, status: "deleted" }
              : entry,
          ),
        );
        setSelectedVideos((current) => {
          const next = new Map(current);
          next.delete(video.videoId);
          return next;
        });
      }
      await onBulkDeletionComplete?.(deleted);
      setBulkOpen(false);
      setBulkConfirmation("");
    } catch (error) {
      if (activeVideo)
        setBulkDeletionLog((current) =>
          current.map((entry) =>
            entry.videoId === activeVideo?.videoId
              ? { ...entry, status: "failed" }
              : entry,
          ),
        );
      setBulkError(
        `${deleted} video${deleted === 1 ? " was" : "s were"} deleted. ${error instanceof Error ? error.message : "The remaining videos were not deleted."}`,
      );
    } finally {
      setBulkDeleting(false);
    }
  };
  if (candidates.length === 0)
    return (
      <p className="m-0 py-1 text-[0.78rem] text-[#65758b]">
        No duplicate candidates found in this account.
      </p>
    );
  return (
    <div
      aria-busy={searchQuery !== deferredSearchQuery}
      className="grid gap-3"
    >
      <label
        className="grid gap-1.5 text-[0.74rem] font-bold text-[#465775] [&_input]:w-full [&_input]:max-w-[32rem] [&_input]:rounded-md [&_input]:border [&_input]:border-[#cbd5e3] [&_input]:bg-white [&_input]:px-2.5 [&_input]:py-2 [&_input]:font-medium [&_input]:text-[#27344a] [&_input]:focus:border-[#2463df] [&_input]:focus:outline-3 [&_input]:focus:outline-[#2d68e824]"
        htmlFor="duplicate-title-search"
      >
        Search video titles
        <input
          autoComplete="off"
          id="duplicate-title-search"
          onChange={(event) => {
            setSearchQuery(event.target.value);
            setPage(1);
          }}
          placeholder="Search titles"
          type="search"
          value={searchQuery}
        />
      </label>
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-[#d8e4f7] bg-[#f2f6fd] px-3 py-3 max-sm:flex-col max-sm:items-stretch [&>div:first-child]:grid [&>div:first-child]:gap-0.5 [&>div:first-child_strong]:text-[0.8rem] [&>div:first-child_strong]:text-[#314a70] [&>div:first-child_span]:text-[0.71rem] [&>div:first-child_span]:leading-snug [&>div:first-child_span]:text-[#65758b] [&>div:last-child]:flex [&>div:last-child]:flex-wrap [&>div:last-child]:gap-2 max-sm:[&>div:last-child]:flex-col max-sm:[&_button]:w-full">
        <div>
          <strong>{selectedVideos.size} selected for deletion</strong>
          <span>
            Select either video in any duplicate entry. Select-all includes
            every result, including candidates hidden by search or pagination.
          </span>
        </div>
        <div>
          <button
            className={secondaryButtonClass}
            disabled={bulkDeleting || selectableVideos.length === 0}
            onClick={() => selectSide("Video A")}
            type="button"
          >
            Select all Video A results
          </button>
          <button
            className={secondaryButtonClass}
            disabled={bulkDeleting || selectableVideos.length === 0}
            onClick={() => selectSide("Video B")}
            type="button"
          >
            Select all Video B results
          </button>
          <button
            className={dangerButtonClass}
            disabled={bulkDeleting || selectedVideos.size === 0}
            onClick={() => {
              setBulkOpen(true);
              setBulkConfirmation("");
              setBulkError("");
              setBulkProgress({
                completed: 0,
                total: selectedVideos.size,
                stage: "Ready to delete",
              });
              setBulkDeletionLog(
                [...selectedVideos.values()].map((video) => ({
                  ...video,
                  status: "queued",
                })),
              );
            }}
            type="button"
          >
            Delete selected ({selectedVideos.size})
          </button>
        </div>
      </div>
      {bulkOpen && (
        <section
          className={deletionPanelClass}
          aria-labelledby="bulk-delete-heading"
        >
          <p className="m-0 text-[0.7rem] font-bold tracking-[0.08em] text-[#a4413b]">BULK PERMANENT DELETION</p>
          <h3 className="m-0 text-[0.93rem] text-[#3e2a2a]" id="bulk-delete-heading">
            Delete {selectedVideos.size} selected YouTube video
            {selectedVideos.size === 1 ? "" : "s"}
          </h3>
          <p className="m-0 text-[0.75rem] leading-relaxed text-[#6e5554] [&_code]:mx-0.5 [&_code]:w-fit [&_code]:rounded [&_code]:border [&_code]:border-[#efdad7] [&_code]:bg-white [&_code]:px-2 [&_code]:py-1.5 [&_code]:text-[0.75rem] [&_code]:text-[#8e3833]">
            Each selected video is rechecked against the currently authorized
            channel immediately before deletion. Type <code>{bulkPhrase}</code>{" "}
            to confirm this batch.
          </p>
          <ul className="m-0 grid gap-1 pl-4.5 text-[0.73rem] text-[#654f4e] [&_code]:ml-1 [&_code]:text-[0.68rem]">
            {[...selectedVideos.values()].map((video) => (
              <li key={video.videoId}>
                {video.label}: {video.title} <code>{video.videoId}</code>
              </li>
            ))}
          </ul>
          {bulkDeletionLog.length > 0 && (
            <BulkDeletionLog entries={bulkDeletionLog} />
          )}
          {(bulkDeleting || bulkProgress.completed > 0) && (
            <DeleteProgress
              completed={bulkProgress.completed}
              stage={bulkProgress.stage || "Preparing confirmed deletions…"}
              total={bulkProgress.total}
            />
          )}
          {!bulkAuthorized && (
            <div className={deletionAuthorizationClass}>
              <span className="text-[0.73rem] text-[#8a5450]">Deletion permission is not active.</span>
              <button
                className={dangerButtonClass}
                disabled={bulkAuthorizing || bulkDeleting || !isTauri}
                onClick={() => void authorizeBulk()}
                type="button"
              >
                {bulkAuthorizing
                  ? "Waiting for Google…"
                  : "Grant deletion permission"}
              </button>
            </div>
          )}
          {bulkAuthorized && !bulkSudoActive && (
            <div className={deletionAuthorizationClass}>
              <span className="text-[0.73rem] text-[#8a5450]">Deletion mode is off.</span>
              <button
                className={dangerButtonClass}
                disabled={bulkDeleting || !isTauri}
                onClick={() => void enableBulkMode()}
                type="button"
              >
                Enter deletion mode (15 min)
              </button>
            </div>
          )}
          {bulkSudoActive && (
            <label className={deletionLabelClass} htmlFor="bulk-delete-confirmation">
              Type {bulkPhrase} to permanently delete the selected videos
              <input
                autoComplete="off"
                autoFocus
                id="bulk-delete-confirmation"
                onChange={(event) => setBulkConfirmation(event.target.value)}
                placeholder={bulkPhrase}
                spellCheck={false}
                value={bulkConfirmation}
              />
            </label>
          )}
          {bulkError && (
            <p className="font-[650] text-[#a4413b]" role="alert">
              {bulkError}
            </p>
          )}
          <div className={deletionActionClass}>
            <button
              className={secondaryButtonClass}
              disabled={bulkDeleting}
              onClick={() => {
                setBulkOpen(false);
                setBulkConfirmation("");
                setBulkError("");
                setBulkProgress({ completed: 0, total: 0, stage: "" });
                setBulkDeletionLog([]);
              }}
              type="button"
            >
              Keep selected videos
            </button>
            {bulkSudoActive && (
              <button
                className={dangerButtonClass}
                disabled={bulkDeleting || bulkConfirmation !== bulkPhrase}
                onClick={() => void deleteSelected()}
                type="button"
              >
                {bulkDeleting
                  ? "Deleting selected videos…"
                  : `Delete ${selectedVideos.size} videos permanently`}
              </button>
            )}
          </div>
        </section>
      )}
      {filteredCandidates.length === 0 ? (
        <p
          className="m-0 py-1 text-[0.78rem] text-[#65758b]"
          role="status"
        >
          No duplicate video titles match “{searchQuery}”.
        </p>
      ) : (
        <div role="list">
          {visibleCandidates.items.map((candidate) => {
            const isExact = candidate.confidence === "exact_local";
            const isUploadedTitle =
              candidate.confidence === "metadata" &&
              Boolean(candidate.leftVideoId && candidate.rightVideoId);
            const decision =
              candidate.decision?.replaceAll("_", " ") ?? "Unreviewed";
            return (
              <section
                className="grid gap-3 rounded-lg border border-[#e1e6ee] bg-white p-3.5"
                data-duplicate-record
                key={candidate.id}
                aria-label={`Duplicate candidate: ${candidate.evidence}`}
                role="listitem"
              >
                <header className="flex flex-wrap items-start justify-between gap-2.5 max-sm:flex-col">
                  <div className="grid gap-1">
                    <span
                      className={`w-fit rounded-full px-2 py-1 text-[0.68rem] font-bold ${matchBadgeClasses[isExact ? "exact" : "possible"]}`}
                    >
                      {isExact
                        ? "Exact local match"
                        : isUploadedTitle
                          ? "Uploaded-title candidate"
                          : "Review required"}
                    </span>
                    <p className="m-0 text-[0.74rem] leading-snug text-[#65758b]">{candidate.evidence}</p>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-[0.7rem] font-bold text-[#52617a]">{decision}</span>
                    <button
                      className={secondaryButtonClass}
                      onClick={() => void onIgnore(candidate.id)}
                      type="button"
                    >
                      Ignore match
                    </button>
                  </div>
                </header>
                {isUploadedTitle ? (
                  <EmbeddedComparison
                    authorizingDeletion={bulkAuthorizing}
                    candidate={candidate}
                    deletionAuthorizationError={bulkError}
                    deletionAuthorized={bulkAuthorized}
                    deletionSudoActive={bulkSudoActive}
                    onDeletionComplete={onDeletionComplete}
                    onAuthorizeDeletion={authorizeBulk}
                    onEnableDeletionMode={enableBulkMode}
                    onSelect={toggleSelection}
                    selected={selectedVideoIds}
                  />
                ) : (
                  <div>
                    <article className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 rounded-md border border-[#e1e6ee] bg-[#fafbfc] p-3 max-sm:grid-cols-[auto_minmax(0,1fr)]">
                      <div
                        className="flex size-9 items-center justify-center rounded-full bg-[#edf3ff] text-lg text-[#2463df]"
                        aria-hidden="true"
                      >
                        <span>↔</span>
                      </div>
                      <div className="min-w-0">
                        <span className="text-[0.68rem] font-bold uppercase tracking-wide text-[#65758b]">
                          Managed local media comparison
                        </span>
                        <h3 className="mt-1 overflow-wrap-anywhere text-[0.86rem] text-[#2d3f5d]">{candidate.leftTitle}</h3>
                        <p className="m-0 overflow-wrap-anywhere text-[0.74rem] text-[#65758b]">{candidate.rightTitle}</p>
                      </div>
                      <aside className="max-sm:col-span-2">
                        <span className="text-[0.7rem] leading-snug text-[#65758b]">
                          Deletion requires a separate explicit confirmation.
                        </span>
                      </aside>
                    </article>
                  </div>
                )}
              </section>
            );
          })}
        </div>
      )}
      <PaginationControls
        end={visibleCandidates.end}
        label="Duplicate candidates"
        onPageChange={setPage}
        page={visibleCandidates.page}
        pageCount={visibleCandidates.pageCount}
        start={visibleCandidates.start}
        total={visibleCandidates.total}
      />
    </div>
  );
}
