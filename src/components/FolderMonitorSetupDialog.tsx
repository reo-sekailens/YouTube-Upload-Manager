import { ArrowLeft, ArrowRight, FolderOpen, LoaderCircle, RefreshCw, ShieldCheck, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState, type FormEvent } from "react";
import { enableFolderMonitor } from "../lib/folder-monitor";
import { listYouTubePlaylists, loadConnectionSettings } from "../lib/local";
import type { FolderMonitorSettings, FolderMonitorVisibility, YouTubePlaylist } from "../lib/types";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import "./FolderMonitorSetupDialog.lazy.css";

type FolderMonitorSetupDialogProps = {
  expectedChannelId: string;
  activeChannel?: string;
  monitor?: FolderMonitorSettings;
  onSaved: (settings: FolderMonitorSettings) => void;
  onClose: () => void;
};

export type FolderMonitorChoices = {
  folderPath: string;
  visibility: FolderMonitorVisibility;
  madeForKids: boolean;
  deleteSourceAfterUpload: boolean;
  playlistId?: string;
  playlistTitle?: string;
};

const buttonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primaryButtonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const controlClass = "min-h-11 w-full min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring";
const steps = [
  { key: "folder", label: "Folder", title: "Choose a folder", description: "Add a folder without interrupting your other monitors." },
  { key: "choices", label: "Upload choices", title: "Choose automatic upload settings", description: "These choices apply to supported videos discovered in this folder." },
  { key: "review", label: "Review", title: "Approve folder monitoring", description: "Review the destination and approve recurring automatic uploads." },
] as const;
type Step = typeof steps[number]["key"];

/** Native admission and the eventual UI receipt stay bound to the reviewed channel. */
export async function saveFolderMonitorForChannel(choices: FolderMonitorChoices, monitorId: string | undefined, expectedChannelId: string, approved: boolean, current: () => boolean): Promise<FolderMonitorSettings | undefined> {
  if (!choices.folderPath.trim()) throw new Error("Choose a local folder.");
  if (!approved) throw new Error("Approve recurring automatic uploads before enabling this folder.");
  assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
  if (!current()) return undefined;
  const saved = await enableFolderMonitor(choices.folderPath, choices.visibility, choices.madeForKids, choices.deleteSourceAfterUpload, choices.playlistId, choices.playlistTitle, monitorId, expectedChannelId);
  if (!current()) return undefined;
  if (saved.channelId !== expectedChannelId || !saved.id) throw new Error("The folder result did not match the reviewed channel. Refresh folders before continuing.");
  return saved;
}

export function FolderMonitorApprovalReview({ choices, channelName }: { choices: FolderMonitorChoices; channelName?: string }) {
  return <dl aria-label="Folder monitoring details" className="m-0 grid gap-4 rounded-lg border border-line bg-subtle-surface p-4 sm:grid-cols-2">
    <div className="min-w-0 sm:col-span-2"><dt className="text-xs font-semibold text-muted">Folder</dt><dd className="mt-1 ml-0 break-all text-sm font-semibold">{choices.folderPath}</dd></div>
    <div className="min-w-0"><dt className="text-xs font-semibold text-muted">YouTube channel</dt><dd className="mt-1 ml-0 break-words text-sm font-semibold">{channelName || "Connected channel"}</dd></div>
    <div><dt className="text-xs font-semibold text-muted">Visibility</dt><dd className="mt-1 ml-0 text-sm font-semibold capitalize">{choices.visibility}</dd></div>
    <div><dt className="text-xs font-semibold text-muted">Audience</dt><dd className="mt-1 ml-0 text-sm">{choices.madeForKids ? "Made for kids" : "Not made for kids"}</dd></div>
    <div className="min-w-0"><dt className="text-xs font-semibold text-muted">Playlist</dt><dd className="mt-1 ml-0 break-words text-sm">{choices.playlistTitle || "No playlist"}</dd></div>
    <div className="sm:col-span-2"><dt className="text-xs font-semibold text-muted">Original source cleanup</dt><dd className="mt-1 ml-0 text-sm">{choices.deleteSourceAfterUpload ? "Delete automatically only after YouTube processing succeeds" : "Keep original files"}</dd></div>
  </dl>;
}

export function FolderMonitorSetupDialog({ expectedChannelId, activeChannel, monitor, onSaved, onClose }: FolderMonitorSetupDialogProps) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const channelAtOpen = useRef(expectedChannelId);
  const callbacks = useRef({ onSaved, onClose });
  callbacks.current = { onSaved, onClose };
  const closed = useRef(false);
  const pending = useRef(false);
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState<Step>("folder");
  const [folderPath, setFolderPath] = useState(monitor?.folderPath || "");
  const [visibility, setVisibility] = useState<FolderMonitorVisibility>(monitor?.visibility || "private");
  const [madeForKids, setMadeForKids] = useState(monitor?.madeForKids || false);
  const [cleanup, setCleanup] = useState(monitor?.deleteSourceAfterUpload || false);
  const [playlistId, setPlaylistId] = useState(monitor?.playlistId || "");
  const [playlists, setPlaylists] = useState<YouTubePlaylist[]>([]);
  const [playlistLoaded, setPlaylistLoaded] = useState(false);
  const [playlistError, setPlaylistError] = useState("");
  const [approved, setApproved] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const channelMatches = channelAtOpen.current === expectedChannelId;
  const details = steps.find((candidate) => candidate.key === step)!;
  const playlist = playlists.find((candidate) => candidate.id === playlistId);
  const choices: FolderMonitorChoices = { folderPath, visibility, madeForKids, deleteSourceAfterUpload: cleanup, playlistId: playlistId || undefined, playlistTitle: playlist?.title || (playlistId === monitor?.playlistId ? monitor?.playlistTitle : undefined) };
  const actionLabel = monitor ? monitor.enabled ? "Save and monitor" : "Save and resume" : "Enable folder";

  const finish = useCallback(() => {
    if (closed.current) return;
    closed.current = true;
    fence.invalidate();
    dialog.current?.close();
    callbacks.current.onClose();
  }, [fence]);

  useEffect(() => {
    const element = dialog.current;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : undefined;
    element?.showModal();
    heading.current?.focus();
    return () => { element?.close(); if (previousFocus?.isConnected) previousFocus.focus(); };
  }, []);
  useEffect(() => { if (!channelMatches) finish(); }, [channelMatches, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step]);

  const refreshPlaylists = useCallback(async () => {
    if (closed.current || pending.current) return;
    pending.current = true;
    setBusy(true);
    setPlaylistError("");
    const requestCurrent = fence.begin();
    const current = () => !closed.current && requestCurrent();
    try {
      assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
      if (!current()) return;
      const loaded = await listYouTubePlaylists();
      if (!current()) return;
      assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
      if (current()) { setPlaylists(loaded); setPlaylistLoaded(true); }
    } catch (failure) {
      if (current()) setPlaylistError(workspaceErrorMessage(failure, "Playlists could not be loaded. Refresh playlists to try again."));
    } finally { pending.current = false; if (current()) setBusy(false); }
  }, [expectedChannelId, fence]);

  useEffect(() => { if (step === "choices" && !playlistLoaded) void refreshPlaylists(); }, [step, playlistLoaded, refreshPlaylists]);

  const chooseFolder = async () => {
    if (closed.current || pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    const requestCurrent = fence.begin();
    const current = () => !closed.current && requestCurrent();
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, multiple: false });
      if (current() && typeof selected === "string") setFolderPath(selected);
    } catch (failure) { if (current()) setError(workspaceErrorMessage(failure, "A local folder could not be chosen.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  };

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (pending.current || closed.current || !channelMatches) return;
    setError("");
    if (!folderPath.trim()) { setError("Choose a local folder."); setStep("folder"); return; }
    if (step === "folder") { setStep("choices"); return; }
    if (playlistId && (!playlistLoaded || !playlist)) { setError("Refresh playlists and choose a current playlist, or select No playlist."); setStep("choices"); return; }
    if (step === "choices") { setStep("review"); return; }
    if (!approved) { setError("Approve recurring automatic uploads before enabling this folder."); return; }
    pending.current = true;
    setBusy(true);
    const requestCurrent = fence.begin();
    const current = () => !closed.current && requestCurrent();
    try {
      const saved = await saveFolderMonitorForChannel(choices, monitor?.id, expectedChannelId, approved, current);
      if (saved && current()) { callbacks.current.onSaved(saved); finish(); }
    } catch (failure) { if (current()) setError(workspaceErrorMessage(failure, "This folder could not be enabled. Your other folders were not changed.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  };

  return <dialog aria-describedby={`${id}-description`} aria-labelledby={`${id}-heading`} aria-modal="true" className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-xl overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55" onCancel={(event) => { event.preventDefault(); if (!pending.current) finish(); }} onClose={() => { if (!dialog.current?.open) finish(); }} ref={dialog}>
    <header className="flex items-start justify-between gap-4 border-b border-line p-4 sm:p-6"><div><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted uppercase">{monitor ? "Edit watched folder" : "Add watched folder"}</p><h2 className="m-0 text-lg font-bold tracking-tight focus:outline-none" id={`${id}-heading`} ref={heading} tabIndex={-1}>{details.title}</h2><p className="mt-2 mb-0 text-sm leading-relaxed text-muted" id={`${id}-description`}>{step === "folder" && monitor ? "This folder remains bound to the same channel. Review its upload choices before saving." : details.description}</p></div><button aria-label="Close folder setup" className={buttonClass} disabled={busy} onClick={finish} type="button"><X aria-hidden="true" size={18} /></button></header>
    <ol aria-label="Folder monitoring steps" className="m-0 flex list-none gap-3 border-b border-line bg-subtle-surface px-4 py-3 sm:px-6">{steps.map((item, index) => <li aria-current={step === item.key ? "step" : undefined} className={`flex min-w-0 flex-1 items-center gap-2 text-xs ${step === item.key ? "font-semibold text-active-ink" : "text-muted"}`} key={item.key}><span aria-hidden="true" className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-full ${step === item.key ? "bg-active-surface" : "border border-line bg-surface"}`}>{index + 1}</span><span>{item.label}</span></li>)}</ol>
    <form aria-busy={busy} noValidate onSubmit={(event) => void submit(event)}>
      <fieldset className="m-0 grid min-w-0 gap-5 border-0 p-4 sm:p-6" disabled={busy}>
        {step === "folder" ? <div className="grid gap-3"><div className="rounded-lg border border-line bg-subtle-surface p-4"><span className="block text-xs font-semibold text-muted">Local folder</span><p className="mt-2 mb-0 break-all text-sm font-semibold">{folderPath || "No folder selected"}</p></div>{!monitor ? <button className={`${buttonClass} justify-self-start`} onClick={() => void chooseFolder()} type="button"><FolderOpen aria-hidden="true" size={18} />{folderPath ? "Choose another folder" : "Choose folder"}</button> : null}<p className="m-0 text-sm leading-relaxed text-muted">Existing and newly added supported direct-child videos upload after two unchanged scans. Subfolders are not scanned.</p></div> : step === "choices" ? <>
          <div className="grid gap-4 sm:grid-cols-2"><label className="grid gap-2 text-sm font-semibold">Visibility<select className={controlClass} onChange={(event) => setVisibility(event.target.value as FolderMonitorVisibility)} value={visibility}><option value="private">Private</option><option value="unlisted">Unlisted</option></select></label><label className="grid gap-2 text-sm font-semibold">Audience<select className={controlClass} onChange={(event) => setMadeForKids(event.target.value === "yes")} value={madeForKids ? "yes" : "no"}><option value="no">Not made for kids</option><option value="yes">Made for kids</option></select></label></div>
          <div className="grid gap-2"><label className="text-sm font-semibold" htmlFor={`${id}-playlist`}>Add to playlist</label><select className={controlClass} id={`${id}-playlist`} onChange={(event) => setPlaylistId(event.target.value)} value={playlistId}><option value="">No playlist</option>{playlistId && !playlist ? <option value={playlistId}>{monitor?.playlistTitle || "Saved playlist"} (refresh to verify)</option> : null}{playlists.map((item) => <option key={item.id} value={item.id}>{item.title}</option>)}</select><button className={`${buttonClass} justify-self-start`} onClick={() => void refreshPlaylists()} type="button"><RefreshCw aria-hidden="true" size={16} />Refresh playlists</button>{playlistError ? <p className="m-0 text-sm text-danger" role="alert">{playlistError}</p> : null}</div>
          <label className="flex items-start gap-3 rounded-lg border border-line bg-subtle-surface p-4 text-sm leading-relaxed"><input checked={cleanup} className="mt-1 shrink-0 accent-brand" onChange={(event) => setCleanup(event.target.checked)} type="checkbox" /><span><span className="block font-semibold">Automatically delete the original after upload</span><span className="mt-1 block text-muted">Cleanup runs only after YouTube processing succeeds and the original passes integrity checks. Otherwise it is retained.</span></span></label>
        </> : <><FolderMonitorApprovalReview channelName={activeChannel} choices={choices} /><div className="rounded-lg border border-line bg-notice-surface p-4 text-sm leading-relaxed text-notice-ink"><strong className="flex items-center gap-2"><ShieldCheck aria-hidden="true" size={18} />Recurring approval</strong><p className="mt-2 mb-0">Existing and newly added supported direct-child videos upload after two unchanged scans while the app is running. Keep each original available and unchanged until upload or recovery completes.</p></div><label className="flex items-start gap-3 text-sm leading-relaxed"><input checked={approved} className="mt-1 shrink-0 accent-brand" onChange={(event) => setApproved(event.target.checked)} required type="checkbox" /><span>I approve recurring automatic uploads from this folder to <strong>{activeChannel || "the connected channel"}</strong> with these choices.</span></label></>}
        {error ? <p className="m-0 rounded-lg bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">{error}</p> : null}
        {busy ? <p className="m-0 text-sm text-muted" role="status">{step === "review" ? "Saving this folder’s monitoring settings…" : "Loading…"}</p> : null}
      </fieldset>
      <footer className="flex flex-wrap items-center gap-2 border-t border-line bg-subtle-surface p-4 sm:px-6">{step !== "folder" ? <button className={buttonClass} disabled={busy} onClick={() => { setApproved(false); setError(""); setStep(step === "review" ? "choices" : "folder"); }} type="button"><ArrowLeft aria-hidden="true" size={16} />Back</button> : null}<span aria-hidden="true" className="flex-1" /><button className={buttonClass} disabled={busy} onClick={finish} type="button">Cancel</button><button className={primaryButtonClass} disabled={busy || !channelMatches || (step === "folder" && !folderPath) || (step === "review" && !approved)} type="submit">{busy ? <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" size={16} /> : <ArrowRight aria-hidden="true" size={16} />}{busy ? "Working…" : step === "review" ? actionLabel : step === "folder" ? "Next: Upload choices" : "Next: Review"}</button></footer>
    </form>
  </dialog>;
}
