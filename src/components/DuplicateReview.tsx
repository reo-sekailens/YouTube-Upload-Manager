import {
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
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
function PlayIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24">
      <path d="m8 5 11 7-11 7V5Z" fill="currentColor" />
    </svg>
  );
}
function PauseIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24">
      <path d="M7 5h4v14H7V5Zm6 0h4v14h-4V5Z" fill="currentColor" />
    </svg>
  );
}
function RewindTenIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24">
      <path
        d="M10.8 5.4A8 8 0 1 1 4 13.3h2A6 6 0 1 0 11 7.4V11L4.8 6.2 11 1.5v3.9h-.2Zm1.6 5.1h1.3v5.2h-1.3v-3.8l-1 .8-.7-.9 1.7-1.3Z"
        fill="currentColor"
      />
    </svg>
  );
}
function ForwardTenIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24">
      <path
        d="M13.2 5.4A8 8 0 1 0 20 13.3h-2A6 6 0 1 1 13 7.4v3.6l6.2-4.8L13 1.5v3.9h.2Zm-1.6 5.1h-1.3v5.2h1.3v-3.8l1 .8.7-.9-1.7-1.3Z"
        fill="currentColor"
      />
    </svg>
  );
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
      className="duplicate-delete-progress"
      role="progressbar"
    >
      <div>
        <span>{stage}</span>
        <strong>
          {completed} of {total}
        </strong>
      </div>
      <div className="duplicate-delete-progress__track">
        <span style={{ width: `${percentage}%` }} />
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
    <details className="duplicate-deletion-log">
      <summary>
        Deletion activity log{" "}
        <span>
          {deleted} deleted ·{" "}
          {deleting > 0 ? "1 deleting" : `${entries.length - deleted} queued`}
        </span>
      </summary>
      <ol>
        {entries.map((entry) => (
          <li
            className={`duplicate-deletion-log__entry duplicate-deletion-log__entry--${entry.status}`}
            key={entry.videoId}
          >
            <span>
              {entry.status === "queued"
                ? "About to delete"
                : entry.status === "deleting"
                  ? "Deleting now"
                  : entry.status === "deleted"
                    ? "Deleted"
                    : "Could not delete"}
            </span>
            <strong>
              {entry.label}: {entry.title}
            </strong>
            <code>{entry.videoId}</code>
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
    <div className="duplicate-comparison">
      <div className="duplicate-comparison__preview-bar">
        <span>
          {previewLoaded
            ? readyPlayerCount >= 2
              ? "Embedded side-by-side comparison is active."
              : "Loading side-by-side comparison…"
            : "Embedded players stay unloaded until you press Play."}
        </span>
        <div>
          <button
            className="secondary-action"
            disabled={!isTauri}
            onClick={() => void openAccountBrowser()}
            type="button"
          >
            Open YouTube account browser
          </button>
          {previewLoaded && (
            <button
              className="secondary-action"
              onClick={unloadPreview}
              type="button"
            >
              Close comparison
            </button>
          )}
        </div>
        {accountBrowserError && <p role="alert">{accountBrowserError}</p>}
      </div>
      <div className="duplicate-comparison__players">
        <section className="duplicate-comparison__player">
          <header>
            <span>Video A</span>
            <strong>{candidate.leftTitle}</strong>
            <code>{candidate.leftVideoId}</code>
          </header>
          {shouldLoadComparisonPlayers(playing, previewLoaded) ? (
            <iframe
              onLoad={() => setReadyPlayerCount((count) => count + 1)}
              ref={leftPlayer}
              src={playerSource(candidate.leftVideoId!)}
              title={`Video A: ${candidate.leftTitle}`}
              allow="autoplay; encrypted-media; picture-in-picture"
              referrerPolicy="strict-origin-when-cross-origin"
            />
          ) : (
            <div className="duplicate-comparison__placeholder">
              Video A loads after you press Play.
            </div>
          )}
          <div className="duplicate-comparison__video-actions">
            <label className="duplicate-comparison__select">
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
              className="duplicate-comparison__open"
              onClick={() => void openInYouTube(candidate.leftVideoId!)}
              type="button"
            >
              Open Video A in YouTube
            </button>
            <button
              className="duplicate-comparison__delete"
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
        <section className="duplicate-comparison__player">
          <header>
            <span>Video B</span>
            <strong>{candidate.rightTitle}</strong>
            <code>{candidate.rightVideoId}</code>
          </header>
          {shouldLoadComparisonPlayers(playing, previewLoaded) ? (
            <iframe
              onLoad={() => setReadyPlayerCount((count) => count + 1)}
              ref={rightPlayer}
              src={playerSource(candidate.rightVideoId!)}
              title={`Video B: ${candidate.rightTitle}`}
              allow="autoplay; encrypted-media; picture-in-picture"
              referrerPolicy="strict-origin-when-cross-origin"
            />
          ) : (
            <div className="duplicate-comparison__placeholder">
              Video B loads after you press Play.
            </div>
          )}
          <div className="duplicate-comparison__video-actions">
            <label className="duplicate-comparison__select">
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
              className="duplicate-comparison__open"
              onClick={() => void openInYouTube(candidate.rightVideoId!)}
              type="button"
            >
              Open Video B in YouTube
            </button>
            <button
              className="duplicate-comparison__delete"
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
          className="duplicate-comparison__delete-confirmation"
          aria-labelledby={`duplicate-delete-${candidate.id}`}
        >
          <p className="eyebrow">PERMANENT, IRREVERSIBLE ACTION</p>
          <h3 id={`duplicate-delete-${candidate.id}`}>
            Delete {pendingDeletion.label}: “{pendingDeletion.title}”
          </h3>
          <p>
            This removes this video from YouTube directly from this duplicate
            card. A temporary deletion mode, an exact typed video ID, and a
            fresh channel-ownership check are required.
          </p>
          <code>{pendingDeletion.videoId}</code>
          {!deletionAuthorized && (
            <div className="duplicate-comparison__authorization">
              <span>Deletion permission is not active.</span>
              <button
                className="danger-button"
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
            <div className="duplicate-comparison__authorization">
              <span>Deletion mode is off.</span>
              <button
                className="danger-button"
                disabled={deleting || !isTauri}
                onClick={() => void enableDeletionMode()}
                type="button"
              >
                Enter deletion mode (15 min)
              </button>
            </div>
          )}
          {deletionSudoActive && (
            <label htmlFor={`duplicate-delete-confirmation-${candidate.id}`}>
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
            <p className="duplicate-comparison__delete-error" role="alert">
              {deletionError || deletionAuthorizationError}
            </p>
          )}
          <div className="duplicate-comparison__delete-actions">
            <button
              className="secondary-action"
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
                className="danger-button"
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
      <div className="duplicate-comparison__controls">
        <div
          aria-label="Synchronized comparison playback"
          className="duplicate-comparison__actions"
          role="group"
        >
          <button
            aria-label="Move both videos back 10 seconds"
            className="comparison-icon-button"
            disabled={!previewLoaded || readyPlayerCount < 2}
            onClick={() =>
              setBothPosition(
                moveComparisonPosition(position, -10, maximumPosition),
              )
            }
            title="Back 10 seconds"
            type="button"
          >
            <RewindTenIcon />
          </button>
          <button
            aria-label={playing ? "Pause both videos" : "Play both videos"}
            className="comparison-icon-button comparison-icon-button--primary"
            onClick={togglePlayback}
            title={playing ? "Pause both videos" : "Play both videos"}
            type="button"
          >
            {playing ? <PauseIcon /> : <PlayIcon />}
          </button>
          <button
            aria-label="Move both videos forward 10 seconds"
            className="comparison-icon-button"
            disabled={!previewLoaded || readyPlayerCount < 2}
            onClick={() =>
              setBothPosition(
                moveComparisonPosition(position, 10, maximumPosition),
              )
            }
            title="Forward 10 seconds"
            type="button"
          >
            <ForwardTenIcon />
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
      <p className="duplicate-review__empty duplicate-review__empty--state">
        No duplicate candidates found in this account.
      </p>
    );
  return (
    <div
      aria-busy={searchQuery !== deferredSearchQuery}
      className="duplicate-review duplicate-review--rail"
    >
      <label
        className="duplicate-review__search"
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
      <div className="duplicate-bulk-toolbar">
        <div>
          <strong>{selectedVideos.size} selected for deletion</strong>
          <span>
            Select either video in any duplicate entry. Select-all includes
            every result, including candidates hidden by search or pagination.
          </span>
        </div>
        <div>
          <button
            className="secondary-action"
            disabled={bulkDeleting || selectableVideos.length === 0}
            onClick={() => selectSide("Video A")}
            type="button"
          >
            Select all Video A results
          </button>
          <button
            className="secondary-action"
            disabled={bulkDeleting || selectableVideos.length === 0}
            onClick={() => selectSide("Video B")}
            type="button"
          >
            Select all Video B results
          </button>
          <button
            className="danger-button"
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
          className="duplicate-comparison__delete-confirmation"
          aria-labelledby="bulk-delete-heading"
        >
          <p className="eyebrow">BULK PERMANENT DELETION</p>
          <h3 id="bulk-delete-heading">
            Delete {selectedVideos.size} selected YouTube video
            {selectedVideos.size === 1 ? "" : "s"}
          </h3>
          <p>
            Each selected video is rechecked against the currently authorized
            channel immediately before deletion. Type <code>{bulkPhrase}</code>{" "}
            to confirm this batch.
          </p>
          <ul className="duplicate-bulk-selection">
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
            <div className="duplicate-comparison__authorization">
              <span>Deletion permission is not active.</span>
              <button
                className="danger-button"
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
            <div className="duplicate-comparison__authorization">
              <span>Deletion mode is off.</span>
              <button
                className="danger-button"
                disabled={bulkDeleting || !isTauri}
                onClick={() => void enableBulkMode()}
                type="button"
              >
                Enter deletion mode (15 min)
              </button>
            </div>
          )}
          {bulkSudoActive && (
            <label htmlFor="bulk-delete-confirmation">
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
            <p className="duplicate-comparison__delete-error" role="alert">
              {bulkError}
            </p>
          )}
          <div className="duplicate-comparison__delete-actions">
            <button
              className="secondary-action"
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
                className="danger-button"
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
          className="duplicate-review__empty duplicate-review__empty--state"
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
                className="duplicate-group duplicate-group--rail"
                data-duplicate-record
                key={candidate.id}
                aria-label={`Duplicate candidate: ${candidate.evidence}`}
                role="listitem"
              >
                <header className="duplicate-group__header">
                  <div className="duplicate-group__summary">
                    <span
                      className={`match-badge match-badge--${isExact ? "exact" : "possible"}`}
                    >
                      {isExact
                        ? "Exact local match"
                        : isUploadedTitle
                          ? "Uploaded-title candidate"
                          : "Review required"}
                    </span>
                    <p>{candidate.evidence}</p>
                  </div>
                  <div className="duplicate-group__actions">
                    <span className="duplicate-group__count">{decision}</span>
                    <button
                      className="secondary-action"
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
                  <div className="duplicate-group__items">
                    <article className="duplicate-card duplicate-card--rail">
                      <div
                        className="duplicate-card__thumbnail"
                        aria-hidden="true"
                      >
                        <span>↔</span>
                      </div>
                      <div className="duplicate-card__body">
                        <span className="duplicate-card__source">
                          Managed local media comparison
                        </span>
                        <h3>{candidate.leftTitle}</h3>
                        <p>{candidate.rightTitle}</p>
                      </div>
                      <aside className="duplicate-card__actions">
                        <span className="duplicate-card__review">
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
