import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./VideoTitleRename.lazy.css";
import {
  beginDeletionAuthorization,
  enableDeletionSudoMode,
  isTauri,
  listRemoteVideos,
  loadConnectionSettings,
} from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import {
  compileTitleRename,
  normalizationTemplates,
  previewTitleRenames,
  selectedTitleRenamePreviews,
  type TitleNormalizationTemplate,
} from "../lib/title-rename";
import {
  getTitleRenameJobSnapshot,
  startTitleRenameJob,
  subscribeTitleRenameJob,
} from "../lib/title-rename-job";
import type { RemoteVideo, VideoTitleRename as VideoTitleRenameRequest } from "../lib/types";

type VideoTitleRenameProps = {
  activeChannel?: string;
  onNotice: (notice: string) => void;
  refreshVersion?: number;
};

const readableTemplate: Record<TitleNormalizationTemplate, string> = {
  "underscores-to-spaces": "Replace underscores with spaces",
  "collapse-whitespace": "Collapse repeated whitespace",
  trim: "Trim leading and trailing whitespace",
};

const secondaryButtonClass =
  "cursor-pointer rounded-md border border-[#cdd4df] bg-white px-2.5 py-1.5 text-[0.72rem] font-[680] text-[#344a67] transition-colors hover:border-[#aeb9c8] hover:bg-[#f3f5f8] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-55";
const primaryButtonClass =
  "cursor-pointer rounded-md border border-[#2463df] bg-[#2463df] px-2.5 py-1.5 text-[0.72rem] font-[680] text-white transition-colors hover:border-[#1b54c6] hover:bg-[#1b54c6] focus-visible:outline-3 focus-visible:outline-[#2d68e847] focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-55";
const activityBorderClass = (status: string) =>
  status === "running"
    ? "border-l-[#2463df]"
    : status === "completed"
      ? "border-l-[#2d8960]"
      : status === "failed"
        ? "border-l-[#c95146]"
        : "border-l-[#9aabc1]";

async function renameRemoteVideos(
  changes: VideoTitleRenameRequest[],
): Promise<RemoteVideo[]> {
  if (!isTauri)
    throw new Error("Video renaming is available only in the signed desktop app.");
  return invoke<RemoteVideo[]>("rename_remote_videos", { changes });
}

export default function VideoTitleRename({
  activeChannel,
  onNotice,
  refreshVersion = 0,
}: VideoTitleRenameProps) {
  const [videos, setVideos] = useState<RemoteVideo[]>([]);
  const [pattern, setPattern] = useState("_");
  const [flags, setFlags] = useState("g");
  const [replacement, setReplacement] = useState(" ");
  const [normalization, setNormalization] = useState<TitleNormalizationTemplate[]>([
    "collapse-whitespace",
    "trim",
  ]);
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [managementActive, setManagementActive] = useState(false);
  const [managementAuthorized, setManagementAuthorized] = useState(false);
  const [loading, setLoading] = useState(false);
  const { activity, applying } = useSyncExternalStore(
    subscribeTitleRenameJob,
    getTitleRenameJobSnapshot,
    getTitleRenameJobSnapshot,
  );

  const refresh = useCallback(async () => {
    if (!isTauri || !activeChannel) {
      setVideos([]);
      setSelected(new Set());
      setManagementActive(false);
      setManagementAuthorized(false);
      return;
    }
    setLoading(true);
    try {
      const [nextVideos, settings] = await Promise.all([
        listRemoteVideos(),
        loadConnectionSettings(),
      ]);
      setVideos(nextVideos);
      setManagementAuthorized(settings.deletionAuthorized === true);
      setManagementActive(settings.deletionSudoActive === true);
    } catch (error) {
      onNotice(
        error instanceof Error
          ? error.message
          : "Saved YouTube library titles could not be loaded.",
      );
    } finally {
      setLoading(false);
    }
  }, [activeChannel, onNotice]);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshVersion]);

  const compiled = useMemo(() => compileTitleRename(pattern, flags), [pattern, flags]);
  const preview = useMemo(
    () => previewTitleRenames(videos, { pattern, flags, replacement, normalization }),
    [videos, pattern, flags, replacement, normalization],
  );
  const eligible = useMemo(
    () => selectedTitleRenamePreviews(preview.items, "all-matches"),
    [preview.items],
  );
  const selectedChanges = useMemo(
    () => selectedTitleRenamePreviews(preview.items, "selected", selected),
    [preview.items, selected],
  );
  const allSelected = eligible.length > 0 && eligible.every((item) => selected.has(item.videoId));
  const completedChanges = activity.filter((item) => item.status === "completed").length;
  const activeChange = activity.find((item) => item.status === "running");
  const waitingChanges = activity.filter((item) => item.status === "pending").length;
  const progressPercent = activity.length === 0 ? 0 : Math.round((completedChanges / activity.length) * 100);

  useEffect(() => {
    const eligibleIds = new Set(eligible.map((item) => item.videoId));
    setSelected((current) => new Set([...current].filter((id) => eligibleIds.has(id))));
  }, [eligible]);

  const toggleTemplate = (template: TitleNormalizationTemplate) => {
    setNormalization((current) =>
      current.includes(template)
        ? current.filter((item) => item !== template)
        : [...current, template],
    );
  };
  const toggleSelection = (videoId: string) => setSelected((current) => {
    const next = new Set(current);
    if (next.has(videoId)) next.delete(videoId);
    else next.add(videoId);
    return next;
  });
  const selectAll = () => setSelected(allSelected ? new Set() : new Set(eligible.map((item) => item.videoId)));

  const authorizeManagement = async () => {
    if (!isTauri) return;
    try {
      const { authorizationUrl } = await beginDeletionAuthorization();
      const copied = await openAndCopyGoogleAuthorization(authorizationUrl);
      onNotice(copied
        ? "Google opened to grant video-management permission and the link was copied. Return here after consent; it is active for 15 minutes."
        : "Google opened to grant video-management permission, but clipboard access was unavailable.");
      window.setTimeout(() => void refresh(), 1_000);
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "Video-management authorization could not be started.");
    }
  };
  const activateManagement = async () => {
    try {
      const settings = await enableDeletionSudoMode();
      setManagementActive(settings.deletionSudoActive === true);
      setManagementAuthorized(settings.deletionAuthorized === true);
      onNotice("Temporary video-management mode is active for 15 minutes. No video will change until you apply this reviewed selection.");
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "Video-management mode could not be enabled.");
    }
  };
  const apply = async () => {
    if (selectedChanges.length === 0) return;
    const changes: VideoTitleRenameRequest[] = selectedChanges.map((item) => ({
      videoId: item.videoId,
      previousTitle: item.title,
      title: item.renamedTitle,
    }));
    try {
      if (await startTitleRenameJob(changes, renameRemoteVideos, onNotice)) setSelected(new Set());
    } finally {
      await refresh();
    }
  };

  if (!activeChannel) {
    return (
      <section className="mt-4 rounded-xl border border-line bg-white p-5" aria-labelledby="rename-heading">
        <div className="flex items-start justify-between gap-4 max-sm:flex-col">
          <div>
            <p className="m-0 mb-2 text-[0.67rem] font-bold tracking-[0.1em] text-muted uppercase">EXPLICIT TITLE REVIEW</p>
            <h2 id="rename-heading">Rename YouTube videos</h2>
          </div>
        </div>
        <p className="m-0 mt-1.5 text-[0.78rem] leading-relaxed text-[#617086]">
          Connect and sync a channel before reviewing video titles.
        </p>
      </section>
    );
  }

  return (
    <section className="mt-4 rounded-xl border border-line bg-white p-5" aria-labelledby="rename-heading">
      <div className="flex items-start justify-between gap-4 max-sm:flex-col">
        <div>
          <p className="m-0 mb-2 text-[0.67rem] font-bold tracking-[0.1em] text-muted uppercase">EXPLICIT TITLE REVIEW</p>
          <h2 id="rename-heading">Rename YouTube videos</h2>
        </div>
      </div>
      <div className="grid gap-4">
        <section
          className="rounded-xl border border-[#dce5f1] p-4 [&_h3]:my-1 [&_h3]:text-base [&_h3]:text-[#263b59] [&_p]:mt-1.5 [&_p]:text-[0.78rem] [&_p]:leading-relaxed [&_p]:text-[#617086]"
          aria-labelledby="title-rename-rule-heading"
        >
          <div>
            <p className="m-0 mb-2 text-[0.67rem] font-bold tracking-[0.1em] text-muted uppercase">PREVIEW BEFORE APPLYING</p>
            <h3 id="title-rename-rule-heading">Find and replace title text</h3>
            <p>
              Use capture groups such as <code>$1</code>. Only the text matched by your expression is replaced; every unmatched part of a title stays intact.
            </p>
          </div>
          <div className="mt-3.5 grid gap-2.5 min-[640px]:grid-cols-[minmax(0,2fr)_minmax(5rem,.55fr)_minmax(0,2fr)] [&_label]:grid [&_label]:gap-1 [&_label]:text-[0.73rem] [&_label]:font-bold [&_label]:text-[#465775] [&_input]:w-full [&_input]:rounded-md [&_input]:border [&_input]:border-[#cbd5e3] [&_input]:bg-white [&_input]:px-2.5 [&_input]:py-2 [&_input]:font-medium [&_input]:text-[#27344a] [&_input]:focus:border-[#2463df] [&_input]:focus:outline-3 [&_input]:focus:outline-[#2d68e824]">
            <label>Regular expression<input aria-label="Regular expression" value={pattern} onChange={(event) => setPattern(event.target.value)} placeholder="Season_(\\d+)" spellCheck={false} /></label>
            <label>Flags<input aria-label="Regular expression flags" value={flags} onChange={(event) => setFlags(event.target.value)} placeholder="g" spellCheck={false} /></label>
            <label>Replace matched text with<input aria-label="Replacement" value={replacement} onChange={(event) => setReplacement(event.target.value)} placeholder="Season $1" spellCheck={false} /></label>
          </div>
          {!compiled.ok && <p className="font-bold !text-[#a4413b]" role="alert">{compiled.error}</p>}
          <fieldset className="mt-3.5 flex flex-wrap gap-x-3.5 gap-y-2 border-0 p-0 text-[0.73rem] font-bold text-[#465775] [&_label]:flex [&_label]:items-center [&_label]:gap-1.5 [&_label]:font-semibold [&_input]:accent-[#2463df]">
            <legend>Normalization templates</legend>
            {normalizationTemplates.map((template) => (
              <label key={template}>
                <input checked={normalization.includes(template)} onChange={() => toggleTemplate(template)} type="checkbox" />
                {readableTemplate[template]}
              </label>
            ))}
          </fieldset>
        </section>

        <section
          className="rounded-xl border border-[#dce5f1] p-4"
          aria-busy={loading || applying}
          aria-labelledby="title-rename-review-heading"
        >
          <header className="flex flex-wrap items-center justify-between gap-3.5 [&_h3]:my-1 [&_h3]:text-base [&_h3]:text-[#263b59] [&_p]:mt-1.5 [&_p]:text-[0.78rem] [&_p]:leading-relaxed [&_p]:text-[#617086]">
            <div>
              <p className="m-0 mb-2 text-[0.67rem] font-bold tracking-[0.1em] text-muted uppercase">MATCHED TITLES</p>
              <h3 id="title-rename-review-heading">Review title changes</h3>
              <p>{eligible.length} changed matching title{eligible.length === 1 ? "" : "s"}; {selectedChanges.length} selected.</p>
            </div>
            <button className={secondaryButtonClass} disabled={eligible.length === 0 || applying} onClick={selectAll} type="button">
              {allSelected ? "Clear all" : `Select all matches (${eligible.length})`}
            </button>
          </header>
          {videos.length === 0 && <p className="mt-1.5 text-[0.78rem] leading-relaxed text-[#617086]">{loading ? "Loading saved YouTube videos…" : "Sync the YouTube library to preview title edits."}</p>}
          {videos.length > 0 && eligible.length === 0 && compiled.ok && <p className="mt-1.5 text-[0.78rem] leading-relaxed text-[#617086]">No saved titles have a changed match for this expression and normalization.</p>}
          <div className="mt-3 grid gap-2.5">
            {eligible.map((item) => (
              <article className="grid grid-cols-[auto_minmax(0,1fr)_minmax(0,1fr)] items-center gap-3.5 rounded-lg border border-[#dbe5f2] bg-[#fafcff] px-3 py-2.5 max-sm:grid-cols-[auto_minmax(0,1fr)]" key={item.videoId}>
                <label aria-label={`Select ${item.title} for title rename`}>
                  <input className="accent-[#2463df]" checked={selected.has(item.videoId)} disabled={applying} onChange={() => toggleSelection(item.videoId)} type="checkbox" />
                </label>
                <div>
                  <span className="block text-[0.62rem] font-[750] tracking-[0.07em] text-[#718095]">BEFORE</span>
                  <strong className="mt-0.5 block overflow-wrap-anywhere text-[0.78rem] text-[#2c405d]">{item.title}</strong>
                </div>
                <div className="max-sm:col-start-2">
                  <span className="block text-[0.62rem] font-[750] tracking-[0.07em] text-[#718095]">AFTER</span>
                  <strong className="mt-0.5 block overflow-wrap-anywhere text-[0.78rem] text-[#1c6b49]">{item.renamedTitle}</strong>
                </div>
              </article>
            ))}
          </div>
          {activity.length > 0 && (
            <section className="mt-3.5 grid gap-2.5 rounded-lg border border-[#d9e4f5] bg-[#f5f8fe] p-3" aria-label="Title rename progress" aria-live="polite">
              <div className="flex flex-wrap items-baseline justify-between gap-x-2.5 gap-y-1.5">
                <strong className="text-[0.76rem] text-[#30496c]">{completedChanges} of {activity.length} title changes confirmed</strong>
                <span className="text-[0.72rem] text-[#60728c]">{activeChange ? `Renaming “${activeChange.previousTitle}”` : waitingChanges > 0 ? `${waitingChanges} title change${waitingChanges === 1 ? "" : "s"} waiting` : "Rename review complete"}</span>
              </div>
              <div aria-valuemax={activity.length} aria-valuemin={0} aria-valuenow={completedChanges} className="h-2 overflow-hidden rounded-full bg-[#d7e2f2]" role="progressbar">
                <span className="block h-full rounded-[inherit] bg-[#2463df] transition-[width] duration-200" style={{ width: `${progressPercent}%` }} />
              </div>
              <details className="text-[0.74rem] text-[#445d80] [&_summary]:cursor-pointer [&_summary]:font-[720] [&_ol]:mt-3 [&_ol]:grid [&_ol]:list-none [&_ol]:gap-1.5 [&_ol]:p-0">
                <summary>Rename activity log ({activity.length})</summary>
                <ol>
                  {activity.map((item) => (
                    <li className={`grid gap-0.5 rounded-r border-l-[3px] bg-white px-2 py-2 ${activityBorderClass(item.status)}`} key={item.videoId}>
                      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                        <strong className="text-[0.7rem]">{item.status === "pending" ? "Waiting" : item.status === "running" ? "Renaming" : item.status === "completed" ? "Renamed" : "Needs attention"}</strong>
                        <span className="overflow-wrap-anywhere text-[0.7rem] text-[#66778e]">{item.previousTitle} → {item.title}</span>
                      </div>
                      {item.detail && <p className="m-0 overflow-wrap-anywhere text-[0.7rem] text-[#66778e]">{item.detail}</p>}
                    </li>
                  ))}
                </ol>
              </details>
            </section>
          )}
          <footer className="mt-3.5 flex flex-wrap items-center justify-between gap-3 border-t border-[#e1e8f2] pt-3.5 max-sm:flex-col max-sm:items-stretch max-sm:[&>_*]:w-full max-sm:[&_button]:w-full">
            <div>
              {!managementAuthorized ? (
                <button className={secondaryButtonClass} disabled={applying} onClick={() => void authorizeManagement()} type="button">Grant video-management permission</button>
              ) : !managementActive ? (
                <button className={secondaryButtonClass} disabled={applying} onClick={() => void activateManagement()} type="button">Enter video-management mode (15 min)</button>
              ) : (
                <span className="text-[0.74rem] font-bold text-[#24704d]">Video-management mode is active.</span>
              )}
            </div>
            <button className={primaryButtonClass} disabled={!managementActive || applying || selectedChanges.length === 0} onClick={() => void apply()} type="button">
              {applying ? "Applying reviewed titles…" : `Apply ${selectedChanges.length} title change${selectedChanges.length === 1 ? "" : "s"}`}
            </button>
          </footer>
        </section>
      </div>
    </section>
  );
}

export { VideoTitleRename };
