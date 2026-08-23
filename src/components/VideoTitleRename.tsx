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

async function renameRemoteVideos(
  changes: VideoTitleRenameRequest[],
): Promise<RemoteVideo[]> {
  if (!isTauri)
    throw new Error("Video renaming is available only in the signed desktop app.");
  return invoke<RemoteVideo[]>("rename_remote_videos", { changes });
}

export default function VideoTitleRename({ activeChannel, onNotice, refreshVersion = 0 }: VideoTitleRenameProps) {
  const [videos, setVideos] = useState<RemoteVideo[]>([]);
  const [pattern, setPattern] = useState("_");
  const [flags, setFlags] = useState("g");
  const [replacement, setReplacement] = useState(" ");
  const [normalization, setNormalization] = useState<TitleNormalizationTemplate[]>(["collapse-whitespace", "trim"]);
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
      setVideos([]); setSelected(new Set()); setManagementActive(false); setManagementAuthorized(false); return;
    }
    setLoading(true);
    try {
      const [nextVideos, settings] = await Promise.all([listRemoteVideos(), loadConnectionSettings()]);
      setVideos(nextVideos); setManagementAuthorized(settings.deletionAuthorized === true); setManagementActive(settings.deletionSudoActive === true);
    } catch (error) {
      onNotice(error instanceof Error ? error.message : "Saved YouTube library titles could not be loaded.");
    } finally { setLoading(false); }
  }, [activeChannel, onNotice]);

  useEffect(() => { void refresh(); }, [refresh, refreshVersion]);

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
    setNormalization((current) => current.includes(template) ? current.filter((item) => item !== template) : [...current, template]);
  };
  const toggleSelection = (videoId: string) => setSelected((current) => {
    const next = new Set(current); if (next.has(videoId)) next.delete(videoId); else next.add(videoId); return next;
  });
  const selectAll = () => setSelected(allSelected ? new Set() : new Set(eligible.map((item) => item.videoId)));

  const authorizeManagement = async () => {
    if (!isTauri) return;
    try {
      const { authorizationUrl } = await beginDeletionAuthorization();
      const url = new URL(authorizationUrl);
      if (url.protocol !== "https:") throw new Error("The video-management authorization request must use HTTPS.");
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url.toString());
      onNotice("Google opened to grant the separate video-management permission. Return here after consent; it is active for 15 minutes.");
      window.setTimeout(() => void refresh(), 1_000);
    } catch (error) { onNotice(error instanceof Error ? error.message : "Video-management authorization could not be started."); }
  };
  const activateManagement = async () => {
    try {
      const settings = await enableDeletionSudoMode();
      setManagementActive(settings.deletionSudoActive === true);
      setManagementAuthorized(settings.deletionAuthorized === true);
      onNotice("Temporary video-management mode is active for 15 minutes. No video will change until you apply this reviewed selection.");
    } catch (error) { onNotice(error instanceof Error ? error.message : "Video-management mode could not be enabled."); }
  };
  const apply = async () => {
    if (selectedChanges.length === 0) return;
    const changes: VideoTitleRenameRequest[] = selectedChanges.map((item) => ({ videoId: item.videoId, previousTitle: item.title, title: item.renamedTitle }));
    try {
      if (await startTitleRenameJob(changes, renameRemoteVideos, onNotice)) setSelected(new Set());
    } finally {
      await refresh();
    }
  };

  if (!activeChannel) return <section className="panel" aria-labelledby="rename-heading"><div className="section-heading"><div><p className="eyebrow">EXPLICIT TITLE REVIEW</p><h2 id="rename-heading">Rename YouTube videos</h2></div></div><p className="video-title-rename__empty">Connect and sync a channel before reviewing video titles.</p></section>;
  return <section className="panel" aria-labelledby="rename-heading"><div className="section-heading"><div><p className="eyebrow">EXPLICIT TITLE REVIEW</p><h2 id="rename-heading">Rename YouTube videos</h2></div></div><div className="video-title-rename">
    <section className="video-title-rename__rule" aria-labelledby="title-rename-rule-heading">
      <div><p className="eyebrow">PREVIEW BEFORE APPLYING</p><h3 id="title-rename-rule-heading">Find and replace title text</h3><p>Use capture groups such as <code>$1</code>. Only the text matched by your expression is replaced; every unmatched part of a title stays intact.</p></div>
      <div className="video-title-rename__fields">
        <label>Regular expression<input aria-label="Regular expression" value={pattern} onChange={(event) => setPattern(event.target.value)} placeholder="Season_(\\d+)" spellCheck={false} /></label>
        <label>Flags<input aria-label="Regular expression flags" value={flags} onChange={(event) => setFlags(event.target.value)} placeholder="g" spellCheck={false} /></label>
        <label>Replace matched text with<input aria-label="Replacement" value={replacement} onChange={(event) => setReplacement(event.target.value)} placeholder="Season $1" spellCheck={false} /></label>
      </div>
      {!compiled.ok && <p className="video-title-rename__error" role="alert">{compiled.error}</p>}
      <fieldset><legend>Normalization templates</legend>{normalizationTemplates.map((template) => <label key={template}><input checked={normalization.includes(template)} onChange={() => toggleTemplate(template)} type="checkbox" /> {readableTemplate[template]}</label>)}</fieldset>
    </section>
    <section className="video-title-rename__review" aria-labelledby="title-rename-review-heading" aria-busy={loading || applying}>
      <header><div><p className="eyebrow">MATCHED TITLES</p><h3 id="title-rename-review-heading">Review title changes</h3><p>{eligible.length} changed matching title{eligible.length === 1 ? "" : "s"}; {selectedChanges.length} selected.</p></div><button className="secondary-action" disabled={eligible.length === 0 || applying} onClick={selectAll} type="button">{allSelected ? "Clear all" : `Select all matches (${eligible.length})`}</button></header>
      {videos.length === 0 && <p className="video-title-rename__empty">{loading ? "Loading saved YouTube videos…" : "Sync the YouTube library to preview title edits."}</p>}
      {videos.length > 0 && eligible.length === 0 && compiled.ok && <p className="video-title-rename__empty">No saved titles have a changed match for this expression and normalization.</p>}
      <div className="video-title-rename__list">{eligible.map((item) => <article key={item.videoId}><label aria-label={`Select ${item.title} for title rename`}><input checked={selected.has(item.videoId)} disabled={applying} onChange={() => toggleSelection(item.videoId)} type="checkbox" /></label><div><span>BEFORE</span><strong>{item.title}</strong></div><div><span>AFTER</span><strong>{item.renamedTitle}</strong></div></article>)}</div>
      {activity.length > 0 && <section className="video-title-rename__progress" aria-live="polite" aria-label="Title rename progress"><div><strong>{completedChanges} of {activity.length} title changes confirmed</strong><span>{activeChange ? `Renaming “${activeChange.previousTitle}”` : waitingChanges > 0 ? `${waitingChanges} title change${waitingChanges === 1 ? "" : "s"} waiting` : "Rename review complete"}</span></div><div aria-valuemax={activity.length} aria-valuemin={0} aria-valuenow={completedChanges} className="video-title-rename__progress-track" role="progressbar"><span style={{ width: `${progressPercent}%` }} /></div><details><summary>Rename activity log ({activity.length})</summary><ol>{activity.map((item) => <li className={`video-title-rename__activity--${item.status}`} key={item.videoId}><div><strong>{item.status === "pending" ? "Waiting" : item.status === "running" ? "Renaming" : item.status === "completed" ? "Renamed" : "Needs attention"}</strong><span>{item.previousTitle} → {item.title}</span></div>{item.detail && <p>{item.detail}</p>}</li>)}</ol></details></section>}
      <footer><div>{!managementAuthorized ? <button className="secondary-action" disabled={applying} onClick={() => void authorizeManagement()} type="button">Grant video-management permission</button> : !managementActive ? <button className="secondary-action" disabled={applying} onClick={() => void activateManagement()} type="button">Enter video-management mode (15 min)</button> : <span className="video-title-rename__ready">Video-management mode is active.</span>}</div><button className="secondary-action video-title-rename__apply" disabled={!managementActive || applying || selectedChanges.length === 0} onClick={() => void apply()} type="button">{applying ? "Applying reviewed titles…" : `Apply ${selectedChanges.length} title change${selectedChanges.length === 1 ? "" : "s"}`}</button></footer>
    </section>
  </div></section>;
}

export { VideoTitleRename };
