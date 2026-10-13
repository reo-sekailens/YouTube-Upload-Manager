import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { createDesktopRuntime, DesktopRouteBanner, DesktopRuntimeIntegration, type DesktopRuntimeDependencies } from "./DesktopRuntimeIntegration";
import type { DesktopRoute } from "../lib/desktop-presence";

vi.mock("../lib/local", () => ({ isTauri: false, resumeQueuedUploads: vi.fn() }));
const routeId = "1056d589-8a69-4613-a6f6-ae28130fb87a";
function route(overrides: Partial<DesktopRoute> = {}): DesktopRoute {
  return { routeId, channelId: "channel-a", kind: "needs_attention", itemIds: ["item-a", "item-b"], count: 2, needsChannelSwitch: false, ...overrides };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}
function fixture(initial?: DesktopRoute) {
  let channel: string | undefined = "channel-a";
  let recovery = false;
  let close: ((event: { preventDefault: () => void }) => void) | undefined;
  const events = new Map<string, () => void>();
  const stops: ReturnType<typeof vi.fn>[] = [];
  const native = {
    listen: vi.fn(async (name: string, listener: () => void) => { events.set(name, listener); const stop = vi.fn(); stops.push(stop); return stop; }),
    onClose: vi.fn(async (listener: (event: { preventDefault: () => void }) => void) => { close = listener; const stop = vi.fn(); stops.push(stop); return stop; }),
    handleClose: vi.fn(async () => false),
    loadRoute: vi.fn(async () => initial ?? null),
    acknowledge: vi.fn(async () => undefined),
    dismiss: vi.fn(async () => undefined),
    resume: vi.fn(async () => 3),
  } satisfies DesktopRuntimeDependencies;
  const options = {
    getChannelId: () => channel,
    getRecoveryMode: () => recovery,
    onExitRequest: vi.fn(),
    onShowUploads: vi.fn(),
    onNotice: vi.fn(),
    onRefresh: vi.fn(async () => undefined),
    onRoute: vi.fn(),
  };
  const controller = createDesktopRuntime(options, native);
  return { controller, native, options, events, stops, setChannel: (value: string | undefined) => { channel = value; }, setRecovery: (value: boolean) => { recovery = value; }, close: (event: { preventDefault: () => void }) => close?.(event) };
}

describe("native desktop runtime integration", () => {
  it("installs commit-driven events and does no upload admission or acknowledgement on boot", async () => {
    const f = fixture(); await f.controller.start();
    expect([...f.events.keys()]).toEqual(["desktop-exit-requested", "desktop-presence-route", "desktop-presence-admission-changed", "desktop-presence-changed"]);
    expect(f.native.onClose).toHaveBeenCalledOnce(); expect(f.native.loadRoute).toHaveBeenCalledOnce();
    expect(f.native.resume).not.toHaveBeenCalled(); expect(f.native.acknowledge).not.toHaveBeenCalled();
    f.controller.dispose(); expect(f.stops).toHaveLength(5); for (const stop of f.stops) expect(stop).toHaveBeenCalledOnce();
  });

  it("prevents a normal close synchronously and uses the approved native tray hide", async () => {
    const f = fixture(); await f.controller.start();
    const hide = deferred<boolean>(); f.native.handleClose.mockReturnValueOnce(hide.promise);
    const event = { preventDefault: vi.fn() }; f.close(event);
    expect(event.preventDefault).toHaveBeenCalledOnce(); expect(f.native.handleClose).toHaveBeenCalledOnce(); expect(f.options.onExitRequest).not.toHaveBeenCalled();
    hide.resolve(true); await vi.waitFor(() => expect(f.native.handleClose).toHaveBeenCalledOnce()); await Promise.resolve();
    expect(f.options.onExitRequest).not.toHaveBeenCalled(); f.controller.dispose();
  });

  it("falls back to existing exit review when hiding is unavailable or fails", async () => {
    for (const fail of [false, true]) {
      const f = fixture(); await f.controller.start();
      if (fail) f.native.handleClose.mockRejectedValueOnce(new Error("unavailable"));
      f.close({ preventDefault: vi.fn() });
      await vi.waitFor(() => expect(f.options.onExitRequest).toHaveBeenCalledOnce()); f.controller.dispose();
    }
  });

  it("leaves recovery-mode OS close alone and tray Exit opens review directly", async () => {
    const f = fixture(); await f.controller.start(); f.setRecovery(true);
    const event = { preventDefault: vi.fn() }; f.close(event);
    expect(event.preventDefault).not.toHaveBeenCalled(); expect(f.native.handleClose).not.toHaveBeenCalled();
    f.events.get("desktop-exit-requested")?.(); expect(f.options.onExitRequest).toHaveBeenCalledOnce();
    expect(f.native.handleClose).not.toHaveBeenCalled(); f.controller.dispose();
  });

  it("coalesces repeated close requests and ignores a late hide result after unmount", async () => {
    const f = fixture(); await f.controller.start(); const hide = deferred<boolean>(); f.native.handleClose.mockReturnValueOnce(hide.promise);
    const event = { preventDefault: vi.fn() }; f.close(event); f.close(event);
    expect(event.preventDefault).toHaveBeenCalledTimes(2); expect(f.native.handleClose).toHaveBeenCalledOnce();
    f.controller.dispose(); hide.resolve(false); await Promise.resolve(); await Promise.resolve(); expect(f.options.onExitRequest).not.toHaveBeenCalled();
  });

  it("opens same-channel saved item IDs and acknowledges only after the checklist is visible", async () => {
    const saved = route(); const f = fixture(saved); await f.controller.start();
    expect(f.options.onRoute).toHaveBeenLastCalledWith(saved); expect(f.options.onShowUploads).toHaveBeenCalledOnce(); expect(f.native.acknowledge).not.toHaveBeenCalled();
    await f.controller.visible(saved);
    expect(f.native.loadRoute).toHaveBeenCalledTimes(2); expect(f.native.acknowledge).toHaveBeenCalledExactlyOnceWith(routeId, "channel-a"); f.controller.dispose();
  });

  it("keeps another channel's activity pending without selecting or acknowledging it", async () => {
    const saved = route({ channelId: "channel-b", needsChannelSwitch: true }); const f = fixture(saved); await f.controller.start();
    expect(f.options.onRoute).toHaveBeenLastCalledWith(saved); expect(f.options.onShowUploads).not.toHaveBeenCalled();
    await f.controller.visible(saved); expect(f.native.acknowledge).not.toHaveBeenCalled(); expect(f.native.dismiss).not.toHaveBeenCalled(); f.controller.dispose();
  });

  it("reloads after an explicit channel change and withholds stale route reads", async () => {
    const f = fixture(); const pending = deferred<DesktopRoute | null>(); f.native.loadRoute.mockReturnValueOnce(pending.promise);
    const start = f.controller.start(); f.setChannel("channel-b");
    const cross = route({ needsChannelSwitch: true }); f.native.loadRoute.mockResolvedValueOnce(cross); f.controller.scopeChanged();
    await vi.waitFor(() => expect(f.options.onRoute).toHaveBeenLastCalledWith(cross));
    pending.resolve(route()); await start;
    expect(f.options.onRoute).toHaveBeenLastCalledWith(cross); expect(f.options.onShowUploads).not.toHaveBeenCalled(); expect(f.native.acknowledge).not.toHaveBeenCalled(); f.controller.dispose();
  });

  it("does not acknowledge a route if the connected channel changes while checking visibility", async () => {
    const saved = route(); const f = fixture(saved); await f.controller.start();
    const pending = deferred<DesktopRoute | null>(); f.native.loadRoute.mockReturnValueOnce(pending.promise);
    const visible = f.controller.visible(saved); f.setChannel("channel-b"); pending.resolve(saved); await visible;
    expect(f.native.acknowledge).not.toHaveBeenCalled(); f.controller.dispose();
  });

  it("reconciles expired or superseded pending routes before acknowledging", async () => {
    for (const replacement of [null, route({ routeId: "355838ec-7c1a-4fe2-9533-b8f6f4c1ea20", itemIds: ["new-item"], count: 1 })]) {
      const saved = route(); const f = fixture(saved); await f.controller.start(); f.native.loadRoute.mockResolvedValue(replacement);
      await f.controller.visible(saved);
      expect(f.native.acknowledge).not.toHaveBeenCalled(); expect(f.options.onRoute).toHaveBeenLastCalledWith(replacement ?? undefined); f.controller.dispose();
    }
  });

  it("deduplicates concurrent visibility effects such as React StrictMode", async () => {
    const saved = route(); const f = fixture(saved); await f.controller.start(); const pending = deferred<DesktopRoute | null>(); f.native.loadRoute.mockReturnValueOnce(pending.promise);
    const first = f.controller.visible(saved); const second = f.controller.visible(saved); pending.resolve(saved); await Promise.all([first, second]);
    expect(f.native.acknowledge).toHaveBeenCalledOnce(); f.controller.dispose();
  });

  it("rejects malformed, expanded or incorrectly scoped routes", async () => {
    for (const saved of [route({ routeId: "unsafe" }), route({ itemIds: ["item-a", "item-a"] }), route({ itemIds: Array.from({ length: 201 }, (_, i) => `item-${i}`) }), route({ channelId: "channel-b", needsChannelSwitch: false }), route({ count: 0 }), route({ kind: "unknown" })]) {
      const f = fixture(saved); await f.controller.start(); expect(f.options.onRoute).toHaveBeenLastCalledWith(undefined); expect(f.options.onShowUploads).not.toHaveBeenCalled(); expect(f.options.onNotice).toHaveBeenCalledOnce(); expect(f.native.acknowledge).not.toHaveBeenCalled(); f.controller.dispose();
    }
  });

  it("keeps a failed acknowledgement visible and reports its retained receipt", async () => {
    const saved = route(); const f = fixture(saved); await f.controller.start(); f.native.acknowledge.mockRejectedValueOnce(new Error("changed"));
    await f.controller.visible(saved); expect(f.options.onRoute).toHaveBeenLastCalledWith(saved); expect(f.options.onNotice).toHaveBeenCalledWith(expect.stringContaining("could not be acknowledged")); f.controller.dispose();
  });

  it("dismisses only the displayed opaque route by explicit action", async () => {
    const saved = route({ channelId: "channel-b", needsChannelSwitch: true }); const f = fixture(saved); await f.controller.start();
    await f.controller.dismiss(route({ routeId: "355838ec-7c1a-4fe2-9533-b8f6f4c1ea20" })); expect(f.native.dismiss).not.toHaveBeenCalled();
    await f.controller.dismiss(saved); expect(f.native.dismiss).toHaveBeenCalledExactlyOnceWith(routeId); expect(f.options.onRoute).toHaveBeenLastCalledWith(undefined); f.controller.dispose();
  });

  it("loads fresh authoritative routing on a toast event rather than acting on event data", async () => {
    const f = fixture(); await f.controller.start(); const saved = route(); f.native.loadRoute.mockResolvedValue(saved);
    f.events.get("desktop-presence-route")?.(); await vi.waitFor(() => expect(f.options.onRoute).toHaveBeenLastCalledWith(saved));
    expect(f.native.acknowledge).not.toHaveBeenCalled(); f.controller.dispose();
  });

  it("rechecks native admission only on changes and refreshes after each coalesced request", async () => {
    const f = fixture(); await f.controller.start(); const pending = deferred<number>(); f.native.resume.mockReturnValueOnce(pending.promise);
    f.events.get("desktop-presence-admission-changed")?.(); f.events.get("desktop-presence-admission-changed")?.(); f.events.get("desktop-presence-admission-changed")?.();
    expect(f.native.resume).toHaveBeenCalledOnce(); pending.resolve(0);
    await vi.waitFor(() => expect(f.native.resume).toHaveBeenCalledTimes(2)); expect(f.options.onRefresh).toHaveBeenCalledTimes(2); f.controller.dispose();
  });

  it("refreshes actual admission failures and ignores admission while recovery is holding", async () => {
    const f = fixture(); await f.controller.start(); f.native.resume.mockRejectedValueOnce(new Error("paused"));
    f.events.get("desktop-presence-admission-changed")?.(); await vi.waitFor(() => expect(f.options.onRefresh).toHaveBeenCalledOnce()); expect(f.options.onNotice).toHaveBeenCalledWith(expect.stringContaining("could not resume"));
    f.setRecovery(true); f.events.get("desktop-presence-admission-changed")?.(); expect(f.native.resume).toHaveBeenCalledOnce(); f.controller.dispose();
  });

  it("releases late listener registrations and ignores late routes after disposal", async () => {
    const f = fixture(route()); const registration = deferred<ReturnType<typeof vi.fn>>(); const lateStop = vi.fn(); f.native.listen.mockReturnValueOnce(registration.promise);
    const start = f.controller.start(); f.controller.dispose(); registration.resolve(lateStop); await start;
    expect(lateStop).toHaveBeenCalledOnce(); expect(f.options.onRoute).not.toHaveBeenCalled(); expect(f.options.onShowUploads).not.toHaveBeenCalled(); expect(f.native.acknowledge).not.toHaveBeenCalled();
    for (const stop of f.stops) expect(stop).toHaveBeenCalledOnce();
  });

  it("continues listener disposal if one native unlisten fails", async () => {
    const f = fixture(); await f.controller.start(); f.stops[0].mockImplementation(() => { throw new Error("gone"); }); expect(() => f.controller.dispose()).not.toThrow();
    for (const stop of f.stops) expect(stop).toHaveBeenCalledOnce();
  });

  it("renders an explicit Account action and escaped saved channel, while browser preview installs nothing", () => {
    const markup = renderToStaticMarkup(<DesktopRouteBanner route={route({ channelId: "<other>", needsChannelSwitch: true })} onAccount={() => undefined} onDismiss={() => undefined} />);
    expect(markup).toContain('aria-label="Desktop activity for another channel"'); expect(markup).toContain("Open Accounts"); expect(markup).toContain("&lt;other&gt;"); expect(markup).not.toContain("<other>"); expect(markup).toContain("will wait here");
    expect(renderToStaticMarkup(<DesktopRuntimeIntegration recoveryModeRef={{ current: false }} channelId="channel-a" onExitRequest={() => undefined} onShowAccount={() => undefined} onShowUploads={() => undefined} onNotice={() => undefined} onRefresh={() => undefined} />)).toBe("");
  });
});
