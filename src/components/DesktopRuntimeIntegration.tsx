import { lazy, Suspense, useCallback, useEffect, useRef, useState, type ComponentType, type RefObject } from "react";
import { isTauri, resumeQueuedUploads } from "../lib/local";
import { acknowledgeDesktopRoute, dismissDesktopRoute, handleDesktopClose, loadPendingDesktopRoute, type DesktopRoute } from "../lib/desktop-presence";
import type { UploadCompletionDialogProps } from "./UploadCompletionDialog";

export type DesktopRuntimeIntegrationProps = {
  recoveryModeRef: RefObject<boolean>;
  channelId?: string;
  onExitRequest: () => void;
  onShowAccount: () => void;
  onShowUploads: () => void;
  onNotice: (message: string) => void;
  onRefresh: () => void | Promise<void>;
};

type Unlisten = () => void;
type RuntimeOptions = {
  getChannelId: () => string | undefined;
  getRecoveryMode: () => boolean;
  onExitRequest: () => void;
  onShowUploads: () => void;
  onNotice: (message: string) => void;
  onRefresh: () => void | Promise<void>;
  onRoute: (route: DesktopRoute | undefined) => void;
};
export type DesktopRuntimeDependencies = {
  listen: (name: string, listener: () => void) => Promise<Unlisten>;
  onClose: (listener: (event: { preventDefault: () => void }) => void) => Promise<Unlisten>;
  handleClose: () => Promise<boolean>;
  loadRoute: () => Promise<DesktopRoute | null>;
  acknowledge: (routeId: string, channelId: string) => Promise<void>;
  dismiss: (routeId: string) => Promise<void>;
  resume: () => Promise<number>;
};
const dependencies: DesktopRuntimeDependencies = {
  listen: async (name, listener) => (await import("@tauri-apps/api/event")).listen(name, listener),
  onClose: async (listener) => (await import("@tauri-apps/api/window")).getCurrentWindow().onCloseRequested(listener),
  handleClose: handleDesktopClose,
  loadRoute: loadPendingDesktopRoute,
  acknowledge: acknowledgeDesktopRoute,
  dismiss: dismissDesktopRoute,
  resume: resumeQueuedUploads,
};

function validRoute(route: DesktopRoute, channelId: string | undefined) {
  const validId = (id: string) => typeof id === "string" && /^[a-zA-Z0-9_-]{1,128}$/.test(id);
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(route.routeId)
    && validId(route.channelId) && ["batch_complete", "needs_attention", "quota_pause"].includes(route.kind)
    && Number.isSafeInteger(route.count) && route.count > 0 && route.count <= 1_000_000
    && Array.isArray(route.itemIds) && route.itemIds.length <= 200 && route.itemIds.every(validId)
    && new Set(route.itemIds).size === route.itemIds.length
    && route.needsChannelSwitch === (route.channelId !== channelId);
}
function sameRoute(left: DesktopRoute, right: DesktopRoute) {
  return left.routeId === right.routeId && left.channelId === right.channelId && left.kind === right.kind
    && left.count === right.count && left.itemIds.join("\n") === right.itemIds.join("\n");
}

/** Commit/event driven desktop wiring, with no timer or provider request. */
export function createDesktopRuntime(options: RuntimeOptions, native: DesktopRuntimeDependencies = dependencies) {
  let disposed = false;
  let routeRevision = 0;
  let route: DesktopRoute | undefined;
  let closePending = false;
  let admissionPending = false;
  let admissionAgain = false;
  const unlisteners = new Set<Unlisten>();
  const visibilityPending = new Set<number>();
  const active = () => !disposed && !options.getRecoveryMode();
  const current = (revision: number, channelId: string | undefined) => active() && routeRevision === revision && options.getChannelId() === channelId;
  const notice = (message: string) => { if (!disposed) options.onNotice(message); };
  const showRoute = (value: DesktopRoute | undefined) => { route = value; if (!disposed) options.onRoute(value); };

  async function refreshRoute() {
    const revision = ++routeRevision;
    const channelId = options.getChannelId();
    if (!active()) { showRoute(undefined); return; }
    try {
      const pending = await native.loadRoute();
      if (!current(revision, channelId)) return;
      if (pending && !validRoute(pending, channelId)) throw new Error("invalid route");
      showRoute(pending ?? undefined);
      if (pending && !pending.needsChannelSwitch) options.onShowUploads();
    } catch {
      if (current(revision, channelId)) {
        showRoute(undefined);
        notice("This desktop activity could not be opened. Its saved receipt remains available in Desktop activity.");
      }
    }
  }

  function requestClose(event: { preventDefault: () => void }) {
    if (disposed || options.getRecoveryMode()) return;
    event.preventDefault();
    if (closePending) return;
    closePending = true;
    void native.handleClose().then((hidden) => {
      if (!disposed && !hidden) options.onExitRequest();
    }).catch(() => { if (!disposed) options.onExitRequest(); })
      .finally(() => { closePending = false; });
  }

  async function admissionChanged() {
    if (!active()) return;
    if (admissionPending) { admissionAgain = true; return; }
    admissionPending = true;
    try {
      do {
        admissionAgain = false;
        // Native admission rechecks the saved device pause, channel, quota and
        // source gates. This event cannot grant a new upload approval.
        try { await native.resume(); }
        catch { if (active()) notice("New work could not resume. Review the saved queue and device pause setting."); }
        if (active()) await options.onRefresh();
      } while (active() && admissionAgain);
    } catch {
      if (active()) notice("Desktop activity changed. Refresh the saved queue to see its current state.");
    } finally { admissionPending = false; }
  }

  async function retainListener(pending: Promise<Unlisten>) {
    try {
      const stop = await pending;
      if (disposed) stop();
      else unlisteners.add(stop);
    } catch { notice("Some desktop controls could not connect. Your saved queue remains available."); }
  }

  async function visible(reviewed: DesktopRoute) {
    if (!route || !sameRoute(route, reviewed) || reviewed.needsChannelSwitch) return;
    const revision = routeRevision;
    const channelId = options.getChannelId();
    if (!current(revision, channelId) || channelId !== reviewed.channelId || visibilityPending.has(revision)) return;
    visibilityPending.add(revision);
    try {
      const pending = await native.loadRoute();
      if (!current(revision, channelId)) return;
      if (!pending || !validRoute(pending, channelId) || pending.needsChannelSwitch || !sameRoute(pending, reviewed)) {
        await refreshRoute();
        return;
      }
      await native.acknowledge(reviewed.routeId, reviewed.channelId);
    } catch {
      if (current(revision, channelId)) notice("The activity is open, but its route could not be acknowledged. Its saved receipt remains available.");
    } finally { visibilityPending.delete(revision); }
  }

  async function dismiss(reviewed: DesktopRoute) {
    if (!route || !sameRoute(route, reviewed) || !active()) return;
    const revision = ++routeRevision;
    const channelId = options.getChannelId();
    try {
      await native.dismiss(reviewed.routeId);
      if (current(revision, channelId)) showRoute(undefined);
    } catch { if (current(revision, channelId)) notice("The activity could not be dismissed. Its saved route remains available."); }
  }

  return {
    async start() {
      if (disposed) return;
      await Promise.all([
        retainListener(native.onClose(requestClose)),
        retainListener(native.listen("desktop-exit-requested", () => { if (!disposed) options.onExitRequest(); })),
        retainListener(native.listen("desktop-presence-route", () => { void refreshRoute(); })),
        retainListener(native.listen("desktop-presence-admission-changed", () => { void admissionChanged(); })),
        retainListener(native.listen("desktop-presence-changed", () => { if (active()) void Promise.resolve(options.onRefresh()).catch(() => notice("Refresh Desktop activity to see the saved choices.")); })),
        refreshRoute(),
      ]);
    },
    scopeChanged() { showRoute(undefined); void refreshRoute(); },
    visible,
    dismiss,
    closeRoute() { ++routeRevision; showRoute(undefined); },
    dispose() { disposed = true; ++routeRevision; for (const stop of unlisteners) { try { stop(); } catch { /* Continue releasing the other native listeners. */ } } unlisteners.clear(); },
  };
}

function VisibleCompletion({ Component, onVisible, ...props }: UploadCompletionDialogProps & { Component: ComponentType<UploadCompletionDialogProps>; onVisible: () => void }) {
  useEffect(() => { onVisible(); }, [onVisible]);
  return <Component {...props} />;
}
const CompletionDialog = lazy(async () => {
  const { UploadCompletionDialog } = await import("./UploadCompletionDialog");
  return { default: (props: UploadCompletionDialogProps & { onVisible: () => void }) => <VisibleCompletion Component={UploadCompletionDialog} {...props} /> };
});

export function DesktopRouteBanner({ route, onAccount, onDismiss }: { route: DesktopRoute; onAccount: () => void; onDismiss: () => void }) {
  return <section aria-label="Desktop activity for another channel" className="ui-panel" role="status">
    <h2 className="ui-section-heading">Desktop activity needs its saved channel</h2>
    <p className="ui-section-copy">Channel: {route.channelId}. Open Accounts and explicitly choose this saved channel to view {route.count} activity item{route.count === 1 ? "" : "s"}. The activity will wait here while you change accounts.</p>
    <div className="ui-action-row"><button className="ui-button-secondary" onClick={onDismiss} type="button">Dismiss activity</button><button className="ui-button-primary" onClick={onAccount} type="button">Open Accounts</button></div>
  </section>;
}

export function DesktopRuntimeIntegration(props: DesktopRuntimeIntegrationProps) {
  const latest = useRef(props); latest.current = props;
  const runtime = useRef<ReturnType<typeof createDesktopRuntime>>(null);
  const previousScope = useRef(`${props.channelId ?? ""}:${props.recoveryModeRef.current}`);
  const [route, setRoute] = useState<DesktopRoute>();
  useEffect(() => {
    if (!isTauri) return;
    const controller = createDesktopRuntime({
      getChannelId: () => latest.current.channelId,
      getRecoveryMode: () => latest.current.recoveryModeRef.current,
      onExitRequest: () => latest.current.onExitRequest(),
      onShowUploads: () => latest.current.onShowUploads(),
      onNotice: (message) => latest.current.onNotice(message),
      onRefresh: () => latest.current.onRefresh(),
      onRoute: setRoute,
    });
    runtime.current = controller;
    void controller.start();
    return () => { controller.dispose(); if (runtime.current === controller) runtime.current = null; };
  }, []);
  const scope = `${props.channelId ?? ""}:${props.recoveryModeRef.current}`;
  useEffect(() => { if (previousScope.current !== scope) { previousScope.current = scope; runtime.current?.scopeChanged(); } }, [scope]);
  const acknowledgeVisible = useCallback(() => { if (route) void runtime.current?.visible(route); }, [route]);
  if (!route || props.recoveryModeRef.current) return null;
  if (route.needsChannelSwitch || route.channelId !== props.channelId) return <DesktopRouteBanner route={route} onAccount={props.onShowAccount} onDismiss={() => { void runtime.current?.dismiss(route); }} />;
  return <Suspense fallback={<p className="ui-notice" role="status">Opening the saved activity checklist…</p>}><CompletionDialog key={route.routeId} expectedChannelId={route.channelId} itemIds={route.itemIds} onClose={() => runtime.current?.closeRoute()} onChanged={props.onRefresh} onVisible={acknowledgeVisible} /></Suspense>;
}

export default DesktopRuntimeIntegration;
