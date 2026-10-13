import { lazy, Suspense } from "react";
import type { ManualUploadSettings } from "../lib/types";
import type { KeyboardEvent } from "react";

const workspaceTabs = [
  ["batch", "Batch uploads"], ["monitor", "Folder monitor"],
  ["dedupe", "Duplicate review"], ["transfer", "Export and import"],
  ["rename", "Rename videos"], ["playlists", "Playlists"],
  ["deletion", "Video deletion"], ["account", "Connected account"],
  ["about", "About and support"],
] as const;
export type WorkspaceTab = (typeof workspaceTabs)[number][0];

export function WorkspaceNavigation({ active, dedupeBusy, onSelect }: {
  active: WorkspaceTab; dedupeBusy: boolean; onSelect: (tab: WorkspaceTab) => void;
}) {
  const handleKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    const index = workspaceTabs.findIndex(([id]) => id === active);
    const next = event.key === "Home" ? 0 : event.key === "End" ? workspaceTabs.length - 1
      : ["ArrowDown", "ArrowRight"].includes(event.key) ? (index + 1) % workspaceTabs.length
      : ["ArrowUp", "ArrowLeft"].includes(event.key) ? (index - 1 + workspaceTabs.length) % workspaceTabs.length
      : undefined;
    if (next === undefined) return;
    event.preventDefault();
    const tab = workspaceTabs[next][0];
    onSelect(tab);
    window.requestAnimationFrame(() => document.getElementById(`workspace-tab-button-${tab}`)?.focus());
  };
  return <nav aria-label="Workspace sections" className="ui-sidebar" role="tablist" aria-orientation="vertical">
    <span className="ui-sidebar-label">Workspace</span>
    {workspaceTabs.map(([id, label]) => <button aria-controls={`workspace-tab-${id}`} aria-selected={active === id}
      className={`ui-sidebar-tab ${active === id ? "ui-sidebar-tab-active" : ""}`} id={`workspace-tab-button-${id}`}
      key={id} onClick={() => onSelect(id)} onKeyDown={handleKey} role="tab" tabIndex={active === id ? 0 : -1} type="button">
      {label}
      {id === "dedupe" && dedupeBusy && <span className="size-1.5 animate-pulse rounded-full bg-current" aria-label="Duplicate detection in progress" />}
    </button>)}
  </nav>;
}

const GoogleSetupWizard = lazy(() => import("./GoogleSetupWizard").then((module) => ({ default: module.GoogleSetupWizard })));
const UploadIntakeReview = lazy(() => import("./UploadIntakeReview").then((module) => ({ default: module.UploadIntakeReview })));
const AppActionIcon = lazy(() => import("./AppActionIcon"));

export function WorkspaceHeader({ channel, dark, busy, onTheme, onRefresh, onImport }: {
  channel?: string; dark: boolean; busy: boolean;
  onTheme: () => void; onRefresh: () => void; onImport: () => void;
}) {
  return <header className="ui-topbar">
    <div className="ui-brand-lockup">
      <img className="ui-brand-mark" src="/favicon.svg" alt="" />
      <div>
        <p className="ui-eyebrow">UPLOAD WORKSPACE</p>
        <h1 className="ui-title">YouTube Upload Manager</h1>
        <p className="ui-subtle">{channel ? `Active channel: ${channel}` : "Your files stay on this device until you start an upload."}</p>
      </div>
    </div>
    <div className="flex flex-wrap justify-end gap-2">
      <button aria-label={`Switch to ${dark ? "light" : "dark"} appearance`} aria-pressed={dark}
        className="ui-button-secondary min-h-9" onClick={onTheme}
        title={`Switch to ${dark ? "light" : "dark"} appearance`} type="button">{dark ? "Light mode" : "Dark mode"}</button>
      <button aria-label="Refresh library" className="ui-button-secondary inline-flex size-9 items-center justify-center p-0"
        disabled={!channel || busy} onClick={onRefresh} title="Refresh library" type="button">
        <Suspense fallback={null}><AppActionIcon name="refresh" /></Suspense>
      </button>
      <button className="ui-button-primary" disabled={busy} onClick={onImport}>Import videos</button>
    </div>
  </header>;
}

type DialogProps = {
  activeChannelId?: string;
  setupOpen: boolean;
  exitOpen: boolean;
  importPaths?: string[];
  onOpenAccount: () => void;
  onDismissSetup: () => void;
  onKeepOpen: () => void;
  onExit: () => void;
  onCancelImport: () => void;
  onConfirmImport: (settings: ManualUploadSettings) => void;
};

/** Operator dialogs are loaded after the database-only startup holding shell. */
export function AppDialogs(props: DialogProps) {
  return <>
    {props.setupOpen && <>
      <div aria-hidden="true" className="ui-backdrop z-[19]" />
      <Suspense fallback={<p role="status">Loading setup…</p>}>
        <GoogleSetupWizard onOpenConnectedAccount={props.onOpenAccount} onDismiss={props.onDismissSetup} />
      </Suspense>
    </>}
    {props.exitOpen && <>
      <div aria-hidden="true" className="ui-backdrop z-[29]" />
      <section aria-labelledby="exit-confirmation-heading" aria-modal="true" className="ui-exit-dialog" role="dialog">
        <p className="ui-eyebrow">EXIT APPLICATION</p>
        <h2 className="ui-card-heading" id="exit-confirmation-heading">Exit YouTube Upload Manager?</h2>
        <p className="ui-modal-copy">Your queue and duplicate-review progress are saved locally. On the next launch, the app checks saved upload sessions before resuming. Keep each original file available and unchanged until its upload completes.</p>
        <div className="ui-modal-actions">
          <button autoFocus className="ui-button-secondary" onClick={props.onKeepOpen} type="button">Keep app open</button>
          <button className="ui-danger-button" onClick={props.onExit} type="button">Exit app</button>
        </div>
      </section>
    </>}
    {props.importPaths && <>
      <div aria-hidden="true" className="ui-backdrop z-[9] bg-slate-950/30" />
      <Suspense fallback={<p role="status">Loading import review…</p>}>
        <UploadIntakeReview activeChannelId={props.activeChannelId} paths={props.importPaths} onCancel={props.onCancelImport} onConfirm={props.onConfirmImport} />
      </Suspense>
    </>}
  </>;
}

export function BatchImportArea({ active, busy, onChoose }: { active: boolean; busy: boolean; onChoose: () => void }) {
  return <div className={`ui-drop-zone ${active ? "border-brand bg-blue-50" : ""}`}>
    <div className="ui-drop-copy">
      <strong>Drag and drop videos here</strong>
      <p className="ui-drop-description">The app uploads each original file from its current location. Keep it available and unchanged until upload or recovery completes. Saved queue and session records contain upload state, not another video file. Files over YouTube’s 256 GB or 12-hour limits are stopped before they are added. You can also choose multiple files.</p>
    </div>
    <button className="ui-button-secondary" disabled={busy} onClick={onChoose} type="button">Choose videos</button>
  </div>;
}

export function BrowserPreviewNotice() {
  return <p className="ui-warning">Browser preview mode: local video selection and upload are available in the Tauri desktop app.</p>;
}

export function WorkspaceFooter() {
  return <footer className="ui-disclaimer">YouTube Upload Manager is an independent project and is not affiliated with, endorsed by, sponsored by, or provided by Google or YouTube. Google and YouTube are trademarks of Google LLC; all other names, logos, and trademarks belong to their respective owners.</footer>;
}
