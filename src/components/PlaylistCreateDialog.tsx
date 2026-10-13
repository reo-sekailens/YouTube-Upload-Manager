import { ArrowLeft, ArrowRight, Globe, Link, LoaderCircle, LockKeyhole, Plus, RefreshCw, ShieldCheck, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState, type FormEvent } from "react";
import { beginDeletionAuthorization, createYouTubePlaylist, enableDeletionSudoMode, loadConnectionSettings } from "../lib/local";
import { openAndCopyGoogleAuthorization } from "../lib/google-authorization";
import type { ConnectionSettings, PlaylistPrivacy, YouTubePlaylist } from "../lib/types";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import "./PlaylistCreateDialog.lazy.css";

type PlaylistCreateDialogProps = {
  expectedChannelId?: string;
  onCreated: (playlist: YouTubePlaylist) => void;
  onClose: () => void;
};

const buttonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primaryButtonClass = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";

const privacyOptions = [
  { value: "private", title: "Private", description: "Only you can view this playlist.", Icon: LockKeyhole },
  { value: "unlisted", title: "Unlisted", description: "Anyone with the playlist link can view it.", Icon: Link },
  { value: "public", title: "Public", description: "Anyone can find and view this playlist.", Icon: Globe },
] as const;

type CreationStep = "name" | "visibility" | "review";
const creationSteps = [
  { key: "name", label: "Name", heading: "Name your playlist", description: "Choose a name for your new YouTube playlist." },
  { key: "visibility", label: "Visibility", heading: "Choose playlist visibility", description: "Choose who can find and view your playlist." },
  { key: "review", label: "Review", heading: "Review your playlist", description: "Check these details before creating the playlist on YouTube. It will be selected when you return." },
] as const;

export function PlaylistVisibilityChoices({ id, privacy, onChange }: { id: string; privacy: PlaylistPrivacy; onChange: (privacy: PlaylistPrivacy) => void }) {
  return (
    <fieldset className="m-0 grid min-w-0 gap-2 border-0 p-0">
      <legend className="mb-2 text-sm font-semibold">Playlist visibility</legend>
      {privacyOptions.map(({ value, title: optionTitle, description, Icon }) => (
        <label className={`flex cursor-pointer items-start gap-3 rounded-lg border p-3 ${privacy === value ? "border-brand bg-active-surface" : "border-line bg-subtle-surface hover:bg-control-hover"}`} key={value}>
          <input checked={privacy === value} className="mt-1 shrink-0 accent-brand focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring" name={`${id}-privacy`} onChange={() => onChange(value)} type="radio" value={value} />
          <Icon aria-hidden="true" className="mt-0.5 shrink-0 text-muted" size={18} />
          <span className="min-w-0"><span className="block text-sm font-semibold">{optionTitle}</span><span className="mt-1 block text-sm leading-snug text-muted">{description}</span></span>
        </label>
      ))}
    </fieldset>
  );
}

export function PlaylistCreationReview({ title, privacy, destinationName }: { title: string; privacy: PlaylistPrivacy; destinationName?: string }) {
  const option = privacyOptions.find((item) => item.value === privacy)!;
  return (
    <dl aria-label="Playlist details to create" className="m-0 grid min-w-0 gap-4 rounded-lg border border-line bg-subtle-surface p-4">
      <div><dt className="text-xs font-semibold text-muted">Playlist name</dt><dd className="mt-1 ml-0 break-words text-base font-semibold text-ink">{title.trim()}</dd></div>
      <div><dt className="text-xs font-semibold text-muted">Playlist visibility</dt><dd className="mt-1 ml-0 text-sm font-semibold text-ink">{option.title}<span className="mt-1 block font-normal leading-snug text-muted">{option.description}</span></dd></div>
      <div><dt className="text-xs font-semibold text-muted">Destination</dt><dd className="mt-1 ml-0 break-words text-sm font-semibold text-ink">{destinationName?.trim() || "Connected channel"}</dd></div>
    </dl>
  );
}

const unconfirmedCreationMessage = "YouTube may have created the playlist, but the app could not confirm the result. Refresh playlists and check for it before creating again.";
const connectionRecheckMessage = "YouTube returned a playlist, but the connected channel could not be rechecked. Refresh playlists and check it before creating again.";
const managementPermissionMessage = "Grant playlist-management permission and enter its temporary 15-minute mode before creating a playlist.";

export function hasPlaylistCreationPermission(settings?: ConnectionSettings): boolean {
  return settings?.deletionAuthorized === true && settings.deletionSudoActive === true;
}

/** These outcomes have no safe immediate create-again action. */
export function playlistCreationNeedsRefresh(message: string): boolean {
  return [
    unconfirmedCreationMessage,
    "YouTube returned a playlist, but its channel or visibility did not match the review. Refresh playlists and check it before creating again.",
    "Playlist creation stopped unexpectedly. Refresh playlists and check for the reviewed name before creating again.",
    connectionRecheckMessage,
  ].includes(message);
}

export function playlistTitleError(title: string): string | undefined {
  const trimmed = title.trim();
  if (!trimmed) return "Enter a playlist name.";
  if (Array.from(trimmed).length > 150) return "Use 150 characters or fewer for the playlist name.";
  return undefined;
}

/** Keep every request and its eventual result bound to the channel at review. */
export async function createPlaylistForChannel(
  title: string,
  privacy: PlaylistPrivacy,
  expectedChannelId: string | undefined,
  current: () => boolean,
): Promise<YouTubePlaylist | undefined> {
  const validation = playlistTitleError(title);
  if (validation) throw new Error(validation);
  const settings = await loadConnectionSettings();
  assertWorkspaceChannel(settings, expectedChannelId);
  if (!current()) return undefined;
  if (!hasPlaylistCreationPermission(settings)) throw new Error(managementPermissionMessage);
  const playlist = await createYouTubePlaylist(title.trim(), privacy, expectedChannelId);
  if (!current()) return undefined;
  try {
    assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
    return current() ? playlist : undefined;
  } catch {
    throw new Error(connectionRecheckMessage);
  }
}

export function PlaylistCreateDialog({ expectedChannelId, onCreated, onClose }: PlaylistCreateDialogProps) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const stepHeading = useRef<HTMLHeadingElement>(null);
  const channelAtOpen = useRef(expectedChannelId);
  const callbacks = useRef({ onCreated, onClose });
  callbacks.current = { onCreated, onClose };
  const closed = useRef(false);
  const pending = useRef(false);
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const [title, setTitle] = useState("");
  const [privacy, setPrivacy] = useState<PlaylistPrivacy>("private");
  const [step, setStep] = useState<CreationStep>("name");
  const [busy, setBusy] = useState(false);
  const [validated, setValidated] = useState(false);
  const [creationError, setCreationError] = useState("");
  const [permission, setPermission] = useState<ConnectionSettings>();
  const [permissionBusy, setPermissionBusy] = useState(false);
  const [permissionError, setPermissionError] = useState("");
  const [permissionNotice, setPermissionNotice] = useState("");
  const needsRefresh = playlistCreationNeedsRefresh(creationError);
  const titleError = validated ? playlistTitleError(title) : undefined;
  const channelMatches = channelAtOpen.current === expectedChannelId;
  const titleLength = Array.from(title.trim()).length;
  const stepDetails = creationSteps.find((item) => item.key === step)!;
  const working = busy || permissionBusy;
  const permissionReady = hasPlaylistCreationPermission(permission);

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
    nameInput.current?.focus();
    return () => {
      element?.close();
      if (previousFocus?.isConnected) previousFocus.focus();
    };
  }, []);

  useEffect(() => {
    if (!channelMatches) finish();
  }, [channelMatches, finish]);

  useEffect(() => {
    if (step === "name") nameInput.current?.focus();
    else stepHeading.current?.focus();
    dialog.current?.scrollTo({ top: 0 });
  }, [step]);

  const checkPermission = useCallback(async (action: "refresh" | "grant" | "enable" = "refresh") => {
    if (pending.current || closed.current || !expectedChannelId) return;
    pending.current = true;
    setPermissionBusy(true);
    setPermissionError("");
    const requestCurrent = fence.begin();
    const current = () => !closed.current && requestCurrent();
    try {
      let settings = await loadConnectionSettings();
      assertWorkspaceChannel(settings, expectedChannelId);
      if (!current()) return;
      if (action === "grant") {
        const { authorizationUrl } = await beginDeletionAuthorization();
        assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
        if (!current()) return;
        const copied = await openAndCopyGoogleAuthorization(authorizationUrl);
        if (!current()) return;
        setPermissionNotice(copied ? "Google opened and the authorization link was copied. Complete approval, return here, then refresh permission." : "Google opened for approval. Complete approval, return here, then refresh permission.");
      } else if (action === "enable") {
        settings = await enableDeletionSudoMode();
        assertWorkspaceChannel(settings, expectedChannelId);
        if (!current()) return;
        setPermissionNotice("Playlist-management mode is active for 15 minutes.");
      } else {
        setPermissionNotice(settings.deletionAuthorized ? "Playlist-management permission is granted." : "Grant Google's permission, then refresh permission after approval.");
      }
      if (current()) setPermission(settings);
    } catch (error) {
      if (current()) {
        setPermission(undefined);
        setPermissionError(workspaceErrorMessage(error, "Playlist-management permission could not be checked. Refresh permission to try again."));
      }
    } finally {
      pending.current = false;
      if (current()) setPermissionBusy(false);
    }
  }, [expectedChannelId, fence]);

  useEffect(() => {
    if (step === "review") void checkPermission();
  }, [step, checkPermission]);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (pending.current || closed.current || needsRefresh) return;
    setValidated(true);
    if (playlistTitleError(title)) {
      setStep("name");
      nameInput.current?.focus();
      return;
    }
    if (!expectedChannelId || !channelMatches) return;
    if (step === "name") { setStep("visibility"); return; }
    if (step === "visibility") { setStep("review"); return; }
    if (!permissionReady) {
      setCreationError(managementPermissionMessage);
      return;
    }
    pending.current = true;
    setBusy(true);
    setCreationError("");
    const requestCurrent = fence.begin();
    const current = () => !closed.current && requestCurrent();
    try {
      const playlist = await createPlaylistForChannel(title, privacy, expectedChannelId, current);
      if (playlist && current()) {
        callbacks.current.onCreated(playlist);
        finish();
      }
    } catch (error) {
      if (current()) {
        const message = workspaceErrorMessage(error, unconfirmedCreationMessage);
        setCreationError(message);
        if (message === managementPermissionMessage) setPermission((previous) => previous ? { ...previous, deletionSudoActive: false } : undefined);
      }
    } finally {
      pending.current = false;
      if (current()) setBusy(false);
    }
  };

  return (
    <dialog
      aria-describedby={`${id}-description`}
      aria-labelledby={`${id}-heading`}
      aria-modal="true"
      className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-lg overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55"
      onCancel={(event) => { event.preventDefault(); if (!pending.current) finish(); }}
      onClose={() => { if (!dialog.current?.open) finish(); }}
      ref={dialog}
    >
      <header className="flex items-start justify-between gap-4 border-b border-line p-4 sm:p-6">
        <div>
          <p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted uppercase">Create playlist</p>
          <h2 className="m-0 text-lg font-bold tracking-tight focus:outline-none" id={`${id}-heading`} ref={stepHeading} tabIndex={-1}>{stepDetails.heading}</h2>
          <p className="mt-2 mb-0 text-sm leading-relaxed text-muted" id={`${id}-description`}>{stepDetails.description}</p>
        </div>
        <button aria-label="Close playlist creation" className={`${buttonClass} shrink-0 px-2`} disabled={working} onClick={finish} title="Close (Escape)" type="button"><X aria-hidden="true" size={20} /></button>
      </header>
      <ol aria-label="Playlist creation steps" className="m-0 flex list-none items-center gap-3 border-b border-line bg-subtle-surface px-4 py-3 sm:px-6">
        {creationSteps.map((item, index) => <li aria-current={step === item.key ? "step" : undefined} className={`flex min-w-0 flex-1 items-center gap-2 text-xs ${step === item.key ? "font-semibold text-active-ink" : "text-muted"}`} key={item.key}><span aria-hidden="true" className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-full ${step === item.key ? "bg-active-surface" : "border border-line bg-surface"}`}>{index + 1}</span><span>{item.label}</span></li>)}
      </ol>
      <form aria-busy={working} noValidate onSubmit={(event) => void submit(event)}>
        <fieldset className="m-0 grid min-w-0 gap-5 border-0 p-4 sm:p-6" disabled={working || needsRefresh}>
          {step === "name" ? <div className="grid gap-2">
            <label className="text-sm font-semibold" htmlFor={`${id}-title`}>Playlist name <span className="font-normal text-muted">(required)</span></label>
            <input
              aria-describedby={`${id}-length${titleError ? ` ${id}-name-error` : ""}`}
              aria-invalid={Boolean(titleError)}
              autoComplete="off"
              className="min-h-11 w-full rounded-md border border-line bg-control px-3 py-2 text-base text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring sm:text-sm"
              id={`${id}-title`}
              onChange={(event) => { setTitle(event.target.value); setCreationError(""); }}
              placeholder="e.g. Event highlights"
              ref={nameInput}
              required
              type="text"
              value={title}
            />
            <p className={`m-0 text-xs ${titleLength > 150 ? "text-danger" : "text-muted"}`} id={`${id}-length`}>{titleLength} / 150 characters</p>
            {titleError ? <p className="m-0 text-sm text-danger" id={`${id}-name-error`} role="alert">{titleError}</p> : null}
          </div> : step === "visibility" ? <PlaylistVisibilityChoices id={id} onChange={(value) => { setPrivacy(value); setCreationError(""); }} privacy={privacy} /> : <PlaylistCreationReview destinationName={permission?.activeChannel} privacy={privacy} title={title} />}
          {step !== "name" ? <p className="m-0 text-sm leading-relaxed text-muted">Playlist visibility does not change the visibility of its videos.</p> : null}
          {step === "review" ? <section aria-label="Playlist-management permission" className="grid gap-3 rounded-lg border border-line bg-subtle-surface p-4">
            <h3 className="m-0 flex items-center gap-2 text-sm font-semibold"><ShieldCheck aria-hidden="true" size={18} />Playlist-management permission</h3>
            <p className="m-0 text-sm leading-relaxed text-muted">{permissionReady ? "Permission is ready and temporary playlist-management mode is active." : "Google approval is required to create playlists. Then enter playlist-management mode, which lasts 15 minutes."}</p>
            <div className="flex flex-wrap gap-2">
              {!permission?.deletionAuthorized ? <button className={buttonClass} disabled={working || needsRefresh} onClick={() => void checkPermission("grant")} type="button">Grant playlist-management permission</button> : !permissionReady ? <button className={buttonClass} disabled={working || needsRefresh} onClick={() => void checkPermission("enable")} type="button">Enter playlist-management mode</button> : null}
              <button className={buttonClass} disabled={working || needsRefresh} onClick={() => void checkPermission()} type="button"><RefreshCw aria-hidden="true" size={16} />Refresh permission</button>
            </div>
            {permission?.deletionAuthorized ? <button className="min-h-10 justify-self-start border-0 bg-transparent px-0 text-left text-sm font-semibold text-brand underline decoration-brand/40 underline-offset-4 hover:decoration-brand focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50" disabled={working || needsRefresh} onClick={() => void checkPermission("grant")} type="button">Reauthorize playlist permission</button> : null}
            {permissionBusy ? <p className="m-0 text-sm leading-relaxed text-muted" role="status">Checking playlist-management permission…</p> : permissionNotice ? <p className="m-0 text-sm leading-relaxed text-muted" role="status">{permissionNotice}</p> : null}
            {permissionError ? <p className="m-0 text-sm leading-relaxed text-danger" role="alert">{permissionError}</p> : null}
          </section> : null}
          {!expectedChannelId ? <p className="m-0 rounded-md bg-warning-surface p-3 text-sm text-warning-ink" role="alert">Connect a YouTube channel before creating a playlist.</p> : null}
          {creationError ? <p className="m-0 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">{creationError}</p> : null}
          {busy ? <p className="m-0 text-sm leading-relaxed text-muted" role="status">Creating your playlist on YouTube. Keep this dialog open while the result is confirmed.</p> : null}
        </fieldset>
        <footer className="flex flex-wrap items-center gap-2 border-t border-line bg-subtle-surface p-4 sm:px-6">
          {step !== "name" ? <button className={buttonClass} disabled={working || needsRefresh} onClick={() => { setCreationError(""); setStep(step === "review" ? "visibility" : "name"); }} type="button"><ArrowLeft aria-hidden="true" size={16} /> Back</button> : null}
          <span aria-hidden="true" className="flex-1" />
          <button className={buttonClass} disabled={working} onClick={finish} type="button">{needsRefresh ? "Close and check playlists" : "Cancel"}</button>
          <button className={primaryButtonClass} disabled={working || needsRefresh || !expectedChannelId || !channelMatches || (step === "review" && !permissionReady)} type="submit">
            {busy ? <LoaderCircle aria-hidden="true" className="animate-spin motion-reduce:animate-none" size={16} /> : step === "review" ? <Plus aria-hidden="true" size={16} /> : <ArrowRight aria-hidden="true" size={16} />}
            {busy ? "Creating…" : step === "review" ? "Create playlist" : step === "name" ? "Next: Visibility" : "Next: Review"}
          </button>
        </footer>
      </form>
    </dialog>
  );
}
