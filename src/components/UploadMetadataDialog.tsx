import { ArrowLeft, ArrowRight, Check, FileJson, FolderOpen, LoaderCircle, RefreshCw, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { loadConnectionSettings } from "../lib/local";
import { deleteUploadPreset, loadUploadPresets, mapUploadManifest, metadataBooleanChoice, metadataFileName, metadataFromBooleanChoice, metadataMappingToItems, metadataPreviewReady, resolveUploadMetadata, saveUploadPreset } from "../lib/upload-metadata";
import { assertWorkspaceChannel, useWorkspaceRequestFence, workspaceErrorMessage } from "../lib/workspace-requests";
import type { UploadMetadata, UploadMetadataMapping, UploadMetadataPreset } from "../lib/types";
import "./UploadMetadataDialog.lazy.css";

export type UploadMetadataDialogProps = {
  expectedChannelId: string;
  paths: string[];
  initialMetadata?: UploadMetadata;
  initialItemMetadata?: Record<string, UploadMetadata>;
  onApply: (choices: { metadata: UploadMetadata; itemMetadata: Record<string, UploadMetadata> }) => void;
  onClose: () => void;
};

const button = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-line bg-control px-4 py-2 text-sm font-semibold text-ink hover:bg-control-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const primary = "inline-flex min-h-11 items-center justify-center gap-2 rounded-md border border-brand bg-brand px-4 py-2 text-sm font-semibold text-white hover:border-brand-strong hover:bg-brand-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring disabled:cursor-not-allowed disabled:opacity-50";
const control = "min-h-11 w-full min-w-0 rounded-md border border-line bg-control px-3 py-2 text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus-ring";
const pageSize = 20;

export function UploadMetadataFields({ id, metadata, disabled, onChange }: { id: string; metadata: UploadMetadata; disabled?: boolean; onChange: (metadata: UploadMetadata) => void }) {
  const update = <K extends keyof UploadMetadata>(key: K, value: UploadMetadata[K]) => {
    const next = { ...metadata };
    if (value === undefined) delete next[key]; else next[key] = value;
    onChange(next);
  };
  return <fieldset className="m-0 grid min-w-0 gap-4 border-0 p-0" disabled={disabled}>
    <legend className="mb-3 text-sm font-semibold">Video metadata</legend>
    <label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-title`}>Title template<input className={control} id={`${id}-title`} onChange={(event) => update("title", event.target.value || undefined)} placeholder="{filename}" value={metadata.title || ""} /><span className="text-xs font-normal leading-relaxed text-muted">Blank uses the filename. Supported tokens: {"{filename}"}, {"{folder}"}, {"{sequence}"}. Resolved titles must fit 100 characters.</span></label>
    <div className="grid gap-2"><label className="flex items-center gap-2 text-sm font-semibold"><input checked={metadata.description !== undefined} className="accent-brand" onChange={(event) => update("description", event.target.checked ? "" : undefined)} type="checkbox" />Set description</label>{metadata.description !== undefined ? <label className="grid gap-2 text-sm" htmlFor={`${id}-description`}>Description template<textarea className={`${control} min-h-32 resize-y leading-relaxed`} id={`${id}-description`} onChange={(event) => update("description", event.target.value)} value={metadata.description} /><span className="text-xs text-muted">An empty description is an explicit choice. Limit: 5,000 UTF-8 bytes after tokens resolve.</span></label> : <p className="m-0 text-xs text-muted">No description declaration in this metadata choice.</p>}</div>
    <div className="grid gap-2"><label className="flex items-center gap-2 text-sm font-semibold"><input checked={metadata.tags !== undefined} className="accent-brand" onChange={(event) => update("tags", event.target.checked ? [] : undefined)} type="checkbox" />Set tags</label>{metadata.tags !== undefined ? <label className="grid gap-2 text-sm" htmlFor={`${id}-tags`}>Tags, one per line<textarea className={`${control} min-h-24 resize-y`} id={`${id}-tags`} onChange={(event) => update("tags", event.target.value.split(/\r?\n/).filter((value) => value.trim().length > 0))} value={metadata.tags.join("\n")} /><span className="text-xs text-muted">500 characters total, including separators and quotes around tags with spaces.</span></label> : null}</div>
    <div className="grid min-w-0 gap-4 sm:grid-cols-2"><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-category`}>Category ID<input className={control} id={`${id}-category`} inputMode="numeric" onChange={(event) => update("categoryId", event.target.value || undefined)} placeholder="For example, 22" value={metadata.categoryId || ""} /><span className="text-xs font-normal text-muted">YouTube validates the category for the destination.</span></label><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-language`}>Title and description language<input className={control} id={`${id}-language`} onChange={(event) => update("language", event.target.value || undefined)} placeholder="en, fil, en-US" value={metadata.language || ""} /></label><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-audio-language`}>Spoken audio language<input className={control} id={`${id}-audio-language`} onChange={(event) => update("defaultAudioLanguage", event.target.value || undefined)} placeholder="en, fil, en-US" value={metadata.defaultAudioLanguage || ""} /></label><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-license`}>License declaration<select className={control} id={`${id}-license`} onChange={(event) => update("license", (event.target.value || undefined) as UploadMetadata["license"])} value={metadata.license || ""}><option value="">No declaration</option><option value="youtube">Standard YouTube license</option><option value="creativeCommon">Creative Commons attribution</option></select></label><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-notify`}>Subscriber notifications<select className={control} id={`${id}-notify`} onChange={(event) => update("notifySubscribers", metadataFromBooleanChoice(event.target.value))} value={metadataBooleanChoice(metadata.notifySubscribers)}><option value="">YouTube default</option><option value="true">Notify subscribers</option><option value="false">Do not notify subscribers</option></select><span className="text-xs font-normal text-muted">YouTube’s upload default is to notify subscribers.</span></label><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-altered`}>Realistic altered or synthetic content<select className={control} id={`${id}-altered`} onChange={(event) => update("containsSyntheticMedia", metadataFromBooleanChoice(event.target.value))} value={metadataBooleanChoice(metadata.containsSyntheticMedia)}><option value="">No declaration</option><option value="true">Yes, contains realistic altered content</option><option value="false">No, does not contain it</option></select></label></div>
    {metadata.publishAt ? <p className="m-0 rounded-md border border-line bg-subtle-surface p-3 text-sm leading-relaxed">Publication time already selected: <time dateTime={metadata.publishAt}>{metadata.publishAt}</time>. Adjust it in the publication calendar; private upload visibility is required.</p> : null}
  </fieldset>;
}

export function UploadMetadataPreview({ fileName, metadata, sequence, override }: { fileName: string; metadata: UploadMetadata; sequence: number; override?: boolean }) {
  const declarations = [
    ["Category", metadata.categoryId], ["Metadata language", metadata.language], ["Audio language", metadata.defaultAudioLanguage],
    ["License", metadata.license === "creativeCommon" ? "Creative Commons attribution" : metadata.license === "youtube" ? "Standard YouTube" : undefined],
    ["Subscriber notifications", metadata.notifySubscribers === undefined ? "YouTube default · notify" : metadata.notifySubscribers ? "Notify" : "Do not notify"],
    ["Altered content", metadata.containsSyntheticMedia === undefined ? "No declaration" : metadata.containsSyntheticMedia ? "Yes" : "No"],
    ["Publication time", metadata.publishAt],
  ];
  return <article className="min-w-0 rounded-lg border border-line bg-surface p-4"><div className="flex flex-wrap items-start justify-between gap-2"><div className="min-w-0"><h3 className="m-0 break-words text-base font-semibold">{sequence}. {metadata.title || fileName.replace(/\.[^.]+$/, "")}</h3><p className="mt-1 mb-0 break-all text-xs text-muted">{fileName}</p></div>{override ? <span className="rounded-full bg-subtle-surface px-3 py-1 text-xs font-semibold">Item override</span> : null}</div>
    <dl className="mt-4 mb-0 grid min-w-0 gap-3 text-sm"><div><dt className="text-xs text-muted">Description</dt><dd className="mt-1 ml-0 whitespace-pre-wrap break-words">{metadata.description === undefined ? "Not declared" : metadata.description || "Empty description"}</dd></div><div><dt className="text-xs text-muted">Tags</dt><dd className="mt-1 ml-0 break-words">{metadata.tags === undefined ? "Not declared" : metadata.tags.join(" · ") || "No tags"}</dd></div><div className="grid min-w-0 grid-cols-2 gap-x-4 gap-y-3">{declarations.map(([label, value]) => value !== undefined ? <div className="min-w-0" key={label}><dt className="text-xs text-muted">{label}</dt><dd className="mt-1 ml-0 break-words">{value}</dd></div> : null)}</div></dl>
  </article>;
}

export function UploadMetadataDialog({ expectedChannelId, paths, initialMetadata, initialItemMetadata, onApply, onClose }: UploadMetadataDialogProps) {
  const id = useId(); const dialog = useRef<HTMLDialogElement>(null); const heading = useRef<HTMLHeadingElement>(null);
  const callbacks = useRef({ onApply, onClose }); callbacks.current = { onApply, onClose };
  const opened = useRef({ channel: expectedChannelId, paths: JSON.stringify(paths) }); const closed = useRef(false); const pending = useRef(false);
  const fence = useWorkspaceRequestFence(expectedChannelId);
  const [step, setStep] = useState(0); const [busy, setBusy] = useState(false); const [error, setError] = useState(""); const [notice, setNotice] = useState("");
  const [presets, setPresets] = useState<UploadMetadataPreset[]>([]); const [loaded, setLoaded] = useState(false); const [presetId, setPresetId] = useState("");
  const [metadata, setMetadata] = useState<UploadMetadata>(initialMetadata || {}); const [itemMetadata, setItemMetadata] = useState<Record<string, UploadMetadata>>(initialItemMetadata || {});
  const [editTarget, setEditTarget] = useState(""); const [rootPath, setRootPath] = useState(""); const [manifestName, setManifestName] = useState(""); const [manifestIssues, setManifestIssues] = useState<string[]>([]);
  const [preview, setPreview] = useState<UploadMetadataMapping>(); const [offset, setOffset] = useState(0); const [approved, setApproved] = useState(false);
  const [saveMode, setSaveMode] = useState<"none" | "new" | "update">("none"); const [presetName, setPresetName] = useState(""); const [removePreset, setRemovePreset] = useState<UploadMetadataPreset>();
  const matchesScope = opened.current.channel === expectedChannelId && opened.current.paths === JSON.stringify(paths);
  const chosenPreset = presets.find((preset) => preset.id === presetId);
  const currentMetadata = editTarget ? itemMetadata[editTarget] || metadata : metadata;

  const finish = useCallback(() => { if (closed.current) return; closed.current = true; fence.invalidate(); dialog.current?.close(); callbacks.current.onClose(); }, [fence]);
  useEffect(() => { const element = dialog.current; const previous = document.activeElement instanceof HTMLElement ? document.activeElement : undefined; element?.showModal(); heading.current?.focus(); return () => { element?.close(); if (previous?.isConnected) previous.focus(); }; }, []);
  useEffect(() => { if (!matchesScope) finish(); }, [matchesScope, finish]);
  useEffect(() => { heading.current?.focus(); dialog.current?.scrollTo({ top: 0 }); }, [step]);
  const run = useCallback(async (action: (current: () => boolean) => Promise<void>) => {
    if (pending.current || closed.current || !matchesScope) return; pending.current = true; setBusy(true); setError(""); const current = fence.begin();
    try { assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId); if (current()) await action(current); }
    catch (problem) { if (current()) setError(workspaceErrorMessage(problem, "Metadata could not be prepared. Current choices were retained.")); }
    finally { pending.current = false; if (current()) setBusy(false); }
  }, [expectedChannelId, fence, matchesScope]);
  const load = useCallback(() => run(async (current) => {
    const result = await loadUploadPresets(expectedChannelId);
    if (result.some((preset) => !preset.id || preset.channelId !== expectedChannelId) || new Set(result.map((preset) => preset.id)).size !== result.length) throw new Error("Saved presets belong to another channel. Reopen the metadata editor.");
    assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
    if (current()) { setPresets(result); setLoaded(true); }
  }), [expectedChannelId, run]);
  useEffect(() => { let cancelled = false; queueMicrotask(() => { if (!cancelled) void load(); }); return () => { cancelled = true; }; }, [load]);

  const invalidate = () => { setPreview(undefined); setApproved(false); setOffset(0); };
  const changeMetadata = (next: UploadMetadata) => { if (editTarget) setItemMetadata((previous) => ({ ...previous, [editTarget]: next })); else setMetadata(next); invalidate(); };
  const chooseRoot = () => run(async (current) => { const { open } = await import("@tauri-apps/plugin-dialog"); const selected = await open({ title: "Choose the manifest matching root", directory: true, multiple: false }); if (current() && typeof selected === "string") { setRootPath(selected); setManifestName(""); setManifestIssues([]); } });
  const chooseManifest = () => run(async (current) => {
    const { open } = await import("@tauri-apps/plugin-dialog"); const selected = await open({ title: "Choose upload metadata manifest", multiple: false, filters: [{ name: "Metadata manifests", extensions: ["csv", "json"] }] });
    if (!current() || typeof selected !== "string") return;
    const result = await mapUploadManifest(expectedChannelId, selected, paths, rootPath); const mapped = metadataMappingToItems(result, paths);
    assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
    if (current()) {
      // A manifest never overwrites a deliberate item override. The operator can edit it.
      const retained = Object.keys(mapped).filter((path) => Object.hasOwn(itemMetadata, path));
      setItemMetadata({ ...mapped, ...itemMetadata }); setManifestIssues([...result.issues, ...retained.map((path) => `${metadataFileName(path)}: the existing item override was retained instead of its manifest row.`)]); setManifestName(metadataFileName(selected)); invalidate(); setNotice(`${result.matches.length} usable manifest row${result.matches.length === 1 ? "" : "s"}. Review every video before applying.`);
    }
  });
  const resolve = () => run(async (current) => {
    if (removePreset) { setApproved(false); setStep(2); return; }
    if (saveMode !== "none" && (!presetName.trim() || Array.from(presetName.trim()).length > 80)) throw new Error("Use a preset name of 1–80 characters.");
    const result = await resolveUploadMetadata(expectedChannelId, paths, metadata, itemMetadata); metadataMappingToItems(result, paths);
    assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
    if (current()) { setPreview(result); setOffset(0); setApproved(false); setStep(2); }
  });
  const apply = () => run(async (current) => {
    if (!approved) return;
    if (removePreset) {
      await deleteUploadPreset(expectedChannelId, removePreset.id); assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
      if (current()) { setPresets((previous) => previous.filter((preset) => preset.id !== removePreset.id)); setRemovePreset(undefined); setPresetId(""); setSaveMode("none"); setStep(0); setApproved(false); setNotice("Preset removed. Existing uploads and draft choices were retained."); }
      return;
    }
    if (!metadataPreviewReady(preview, paths.length)) return;
    // Fresh native preview invalidates a stale review instead of silently applying it.
    const fresh = await resolveUploadMetadata(expectedChannelId, paths, metadata, itemMetadata);
    if (JSON.stringify(fresh) !== JSON.stringify(preview)) { if (current()) { setPreview(fresh); setApproved(false); setError("Resolved metadata changed. Review it again before applying."); } return; }
    if (saveMode !== "none") {
      const saved = await saveUploadPreset(expectedChannelId, presetName.trim(), metadata, saveMode === "update" ? presetId : undefined);
      if (saved.channelId !== expectedChannelId) throw new Error("The saved preset belongs to another channel. Reopen the editor.");
    }
    assertWorkspaceChannel(await loadConnectionSettings(), expectedChannelId);
    if (current()) { callbacks.current.onApply({ metadata, itemMetadata }); finish(); }
  });

  const title = removePreset ? step === 1 ? "Confirm preset removal" : "Review preset removal" : ["Choose metadata sources", "Edit your video metadata", "Review resolved metadata"][step];
  return <dialog aria-describedby={`${id}-help`} aria-labelledby={`${id}-heading`} aria-modal="true" className="m-auto max-h-[calc(100dvh-2rem)] w-[calc(100%-2rem)] max-w-3xl overflow-y-auto rounded-xl border border-line bg-surface p-0 text-ink shadow-2xl backdrop:bg-black/55" onCancel={(event) => { event.preventDefault(); if (!pending.current) finish(); }} onClose={() => { if (!dialog.current?.open) finish(); }} ref={dialog}>
    <header className="flex items-start justify-between gap-4 border-b border-line p-4 sm:p-6"><div className="min-w-0"><p className="mt-0 mb-2 text-xs font-semibold tracking-wide text-muted uppercase">Upload metadata</p><h2 className="m-0 text-lg font-bold tracking-tight focus:outline-none" id={`${id}-heading`} ref={heading} tabIndex={-1}>{title}</h2><p className="mt-2 mb-0 max-w-xl text-sm leading-relaxed text-muted" id={`${id}-help`}>Prepare {paths.length} selected video{paths.length === 1 ? "" : "s"} for this channel. Metadata stays with the reviewed upload; changing a preset leaves existing queues intact.</p></div><button aria-label="Close metadata editor" className={`${button} shrink-0 px-2`} disabled={busy} onClick={finish} type="button"><X aria-hidden="true" size={20} /></button></header>
    <ol aria-label="Metadata steps" className="m-0 flex list-none gap-3 border-b border-line bg-subtle-surface px-4 py-3 text-xs sm:px-6">{["Preset or manifest", "Edit", "Review"].map((label, index) => <li aria-current={step === index ? "step" : undefined} className="flex min-w-0 flex-1 items-center gap-2" key={label}><span aria-hidden="true" className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full border border-line bg-surface">{index + 1}</span><span>{label}</span></li>)}</ol>
    <div aria-busy={busy} className="grid min-w-0 gap-5 p-4 sm:p-6">
      {error ? <p className="m-0 rounded-md bg-warning-surface p-3 text-sm leading-relaxed text-warning-ink" role="alert">{error}</p> : null}{notice ? <p className="m-0 text-sm leading-relaxed text-muted" role="status">{notice}</p> : null}{busy ? <p className="m-0 flex items-center gap-2 text-sm text-muted" role="status"><LoaderCircle aria-hidden="true" className="animate-spin" size={16} />Preparing this channel’s metadata…</p> : null}
      {removePreset ? <section className="grid gap-3 rounded-lg border border-line bg-subtle-surface p-4"><h3 className="m-0 break-words text-base font-semibold">{removePreset.name}</h3><p className="m-0 text-sm leading-relaxed">Remove this channel’s saved preset. Approved upload metadata, original files, queue entries and the current draft remain available.</p>{step === 2 ? <label className="flex items-start gap-3 text-sm leading-relaxed"><input checked={approved} className="mt-1 accent-brand" disabled={busy} onChange={(event) => setApproved(event.target.checked)} type="checkbox" /><span>I approve removal of this saved preset.</span></label> : null}</section> : step === 0 ? <>
        <section className="grid gap-3"><div className="flex flex-wrap items-center justify-between gap-3"><h3 className="m-0 text-base font-semibold">Channel presets</h3><button className={button} disabled={busy} onClick={() => void load()} type="button"><RefreshCw aria-hidden="true" size={16} />Refresh presets</button></div><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-preset`}>Saved metadata preset<select className={control} disabled={busy || !loaded} id={`${id}-preset`} onChange={(event) => { const chosen = presets.find((preset) => preset.id === event.target.value); setPresetId(event.target.value); setSaveMode("none"); if (chosen) { setMetadata({ ...chosen.metadata }); setPresetName(chosen.name); invalidate(); setNotice("Preset applied to common choices. Deliberate item overrides were retained."); } }} value={presetId}><option value="">Keep current choices</option>{presets.map((preset) => <option key={preset.id} value={preset.id}>{preset.name}</option>)}</select></label>{loaded && !presets.length ? <p className="m-0 text-sm text-muted">No presets saved for this channel yet. Create one after editing your metadata.</p> : null}{chosenPreset ? <button className={`${button} justify-self-start`} disabled={busy} onClick={() => { setRemovePreset(chosenPreset); setApproved(false); setError(""); setStep(1); }} type="button"><Trash2 aria-hidden="true" size={16} />Remove preset…</button> : null}</section>
        <section className="grid min-w-0 gap-3 rounded-lg border border-line bg-subtle-surface p-4"><h3 className="m-0 text-base font-semibold">Local CSV or JSON manifest</h3><p className="m-0 text-sm leading-relaxed text-muted">Choose the folder that contains your selected originals. Relative file references can match only these selected videos inside that root.</p><div className="flex flex-wrap gap-2"><button className={button} disabled={busy} onClick={() => void chooseRoot()} type="button"><FolderOpen aria-hidden="true" size={16} />Choose matching root</button><button className={button} disabled={busy || !rootPath} onClick={() => void chooseManifest()} type="button"><FileJson aria-hidden="true" size={16} />Choose manifest</button></div>{rootPath ? <p className="m-0 break-all text-xs text-muted">Matching root: {rootPath}</p> : null}{manifestName ? <p className="m-0 break-words text-sm font-semibold">{manifestName}</p> : null}<details className="text-xs leading-relaxed text-muted"><summary className="cursor-pointer font-semibold text-ink">Manifest format and limits</summary><p>UTF-8, at most 1 MiB and 512 video rows. CSV headers: file, title, description, tags, categoryId, language, defaultAudioLanguage, license, notifySubscribers, containsSyntheticMedia, publishAt. Tags use a JSON array or semicolons; booleans use true/false.</p><pre className="max-w-full overflow-x-auto rounded-md border border-line bg-control p-3 text-xs">{'{"schemaVersion":1,"videos":[{"file":"clip.mp4","title":"{filename}","description":"Session {sequence}"}]}'}</pre><p>Unknown fields, unmatched files, duplicate rows and invalid values are withheld and disclosed. No expressions or scripts run.</p></details></section>
        {manifestIssues.length ? <details className="rounded-lg border border-line p-3" open><summary className="cursor-pointer text-sm font-semibold">Manifest review notes ({manifestIssues.length})</summary><ul className="mb-0 grid gap-2 pl-5 text-sm leading-relaxed text-muted">{manifestIssues.map((issue, index) => <li className="break-words" key={index}>{issue}</li>)}</ul></details> : null}
      </> : step === 1 ? <>
        <label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-target`}>Edit metadata for<select className={control} disabled={busy} id={`${id}-target`} onChange={(event) => setEditTarget(event.target.value)} value={editTarget}><option value="">Common choices for this batch</option>{paths.map((path, index) => <option key={path} value={path}>{index + 1}. {metadataFileName(path)}{Object.hasOwn(itemMetadata, path) ? " · item override" : ""}</option>)}</select></label>
        {editTarget ? <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-subtle-surface p-3"><p className="m-0 text-sm leading-relaxed">Editing this video creates a complete item override. Changes to common choices keep it intact.</p>{Object.hasOwn(itemMetadata, editTarget) ? <button className={button} disabled={busy} onClick={() => { setItemMetadata((previous) => { const next = { ...previous }; delete next[editTarget]; return next; }); invalidate(); }} type="button">Use common choices</button> : null}</div> : null}
        <UploadMetadataFields disabled={busy} id={`${id}-fields`} metadata={currentMetadata} onChange={changeMetadata} />
        {!editTarget ? <section className="grid gap-3 rounded-lg border border-line bg-subtle-surface p-4"><label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-save`}>Save common choices as a preset<select className={control} disabled={busy} id={`${id}-save`} onChange={(event) => setSaveMode(event.target.value as typeof saveMode)} value={saveMode}><option value="none">Use for this batch only</option><option value="new">Create a new channel preset</option>{chosenPreset ? <option value="update">Update {chosenPreset.name}</option> : null}</select></label>{saveMode !== "none" ? <label className="grid gap-2 text-sm font-semibold" htmlFor={`${id}-name`}>Preset name<input className={control} disabled={busy} id={`${id}-name`} onChange={(event) => setPresetName(event.target.value)} value={presetName} /><span className="text-xs font-normal text-muted">Item overrides are not saved into the common preset.</span></label> : null}</section> : null}
      </> : <>
        <p className="m-0 rounded-lg bg-subtle-surface p-4 text-sm leading-relaxed"><strong>{preview?.matches.length || 0} of {paths.length} videos resolved.</strong> Destination channel: <span className="break-all">{expectedChannelId}</span>. {saveMode === "none" ? "Apply these choices to the current draft." : `${saveMode === "new" ? "Create" : "Update"} channel preset “${presetName.trim()}” and apply the draft choices.`}</p>
        {preview?.issues.length ? <div className="rounded-lg bg-warning-surface p-4 text-sm text-warning-ink" role="alert"><p className="mt-0 font-semibold">Correct these values before applying.</p><ul className="mb-0 grid gap-2 pl-5">{preview.issues.map((issue, index) => <li className="break-words" key={index}>{issue}</li>)}</ul></div> : null}
        {preview?.matches.slice(offset, offset + pageSize).map((match) => <UploadMetadataPreview fileName={match.fileName} key={match.index} metadata={match.metadata} override={Object.hasOwn(itemMetadata, paths[match.index])} sequence={match.index + 1} />)}
        {preview && preview.matches.length > pageSize ? <nav aria-label="Metadata preview pages" className="flex items-center justify-between gap-3"><button className={button} disabled={busy || !offset} onClick={() => setOffset((value) => Math.max(0, value - pageSize))} type="button">Previous page</button><span className="text-xs text-muted">{offset + 1}–{Math.min(offset + pageSize, preview.matches.length)} of {preview.matches.length}</span><button className={button} disabled={busy || offset + pageSize >= preview.matches.length} onClick={() => setOffset((value) => value + pageSize)} type="button">Next page</button></nav> : null}
        <label className="flex items-start gap-3 text-sm leading-relaxed"><input checked={approved} className="mt-1 accent-brand" disabled={busy || !metadataPreviewReady(preview, paths.length)} onChange={(event) => setApproved(event.target.checked)} type="checkbox" /><span>I reviewed the resolved metadata and optional declarations for these selected videos{saveMode !== "none" ? ", including the preset change" : ""}.</span></label>
      </>}
    </div>
    <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-line bg-subtle-surface p-4 sm:px-6"><div>{step > 0 ? <button className={button} disabled={busy} onClick={() => { setStep((value) => value - 1); setApproved(false); setError(""); if (step === 1) setRemovePreset(undefined); }} type="button"><ArrowLeft aria-hidden="true" size={16} />Back</button> : null}</div><div className="flex flex-wrap gap-2"><button className={button} disabled={busy} onClick={finish} type="button">Cancel</button>{step === 0 ? <button className={primary} disabled={busy || !loaded || !paths.length} onClick={() => { setStep(1); setError(""); }} type="button">Next: Edit<ArrowRight aria-hidden="true" size={16} /></button> : step === 1 ? <button className={primary} disabled={busy} onClick={() => void resolve()} type="button">Next: Review<ArrowRight aria-hidden="true" size={16} /></button> : <button className={primary} disabled={busy || !approved || (!removePreset && !metadataPreviewReady(preview, paths.length))} onClick={() => void apply()} type="button"><Check aria-hidden="true" size={16} />{removePreset ? "Remove preset" : saveMode === "none" ? "Apply metadata" : "Save preset and apply"}</button>}</div></footer>
  </dialog>;
}
