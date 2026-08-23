import { afterEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  isTauri: () => false,
}));

import {
  completeStartupAfterSafeShellPaint,
  createPaintedResponseReceipt,
  hasPaintableBatchContent,
  hasPaintableSafeShell,
  isPaintedQueueSearchResponse,
} from "./performance-harness";

afterEach(() => {
  invoke.mockReset();
  vi.unstubAllGlobals();
  performance.clearMarks("[data-performance-shell]");
});

describe("packaged performance Batch paint receipt", () => {
  it("accepts only the ready Batch workspace marker, never the startup fallback", () => {
    const ready = { querySelector: vi.fn(() => ({})) };
    const fallback = { querySelector: vi.fn(() => null) };

    expect(hasPaintableBatchContent(ready)).toBe(true);
    expect(hasPaintableBatchContent(fallback)).toBe(false);
    expect(ready.querySelector).toHaveBeenCalledWith(
      '[data-performance-batch-content="ready"]',
    );
  });

  it("accepts only an explicit holding/recovery shell marker", () => {
    const shell = { querySelector: vi.fn(() => ({})) };
    const crashOrSuspenseFallback = { querySelector: vi.fn(() => null) };

    expect(hasPaintableSafeShell(shell)).toBe(true);
    expect(hasPaintableSafeShell(crashOrSuspenseFallback)).toBe(false);
    expect(shell.querySelector).toHaveBeenCalledWith(
      "[data-performance-shell]",
    );
  });

  it("creates a non-null request-to-painted receipt with long-task evidence", () => {
    expect(createPaintedResponseReceipt(100.2, 142.8, 2, 60.6)).toEqual({
      responsePaintedMs: 143,
      latencyMs: 43,
      longTasks: 2,
      maxLongTaskMs: 61,
    });
  });

  it("keeps the real holding shell mounted through two paints and the native receipt", async () => {
    const events: string[] = ["bootstrap-holding"];
    let holdingShellMounted = true;
    performance.clearMarks("[data-performance-shell]");
    vi.stubGlobal("document", {
      querySelector: vi.fn(() => (holdingShellMounted ? {} : null)),
    });
    vi.stubGlobal(
      "requestAnimationFrame",
      (callback: FrameRequestCallback) => {
        expect(holdingShellMounted).toBe(true);
        events.push("paint-frame");
        callback(performance.now());
        return events.length;
      },
    );
    invoke.mockImplementation(async (_command, payload) => {
      expect(holdingShellMounted).toBe(true);
      expect(payload).toMatchObject({
        milestone: "safe_shell_paint",
        metrics: { safeShellPaintMs: expect.any(Number) },
      });
      events.push("native-mark-request", "native-mark-receipt");
    });

    const result = await completeStartupAfterSafeShellPaint(async () => {
      expect(holdingShellMounted).toBe(true);
      events.push("complete-startup");
      holdingShellMounted = false;
      return "ready";
    });

    expect(result).toBe("ready");
    expect(events).toEqual([
      "bootstrap-holding",
      "paint-frame",
      "paint-frame",
      "native-mark-request",
      "native-mark-receipt",
      "complete-startup",
    ]);
    expect(holdingShellMounted).toBe(false);
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("accepts a 10k search only after the requested bounded results are ready", () => {
    const ready = {
      expectedQuery: "Target",
      inputValue: "Target",
      inputBusy: false,
      regionBusy: false,
      totalItems: 10_000,
      visibleTitles: ["Target 1", "Another target"],
      noMatches: false,
    };

    expect(isPaintedQueueSearchResponse(ready)).toBe(true);
    expect(
      isPaintedQueueSearchResponse({ ...ready, regionBusy: true }),
    ).toBe(false);
    expect(
      isPaintedQueueSearchResponse({
        ...ready,
        visibleTitles: ["Stale result"],
      }),
    ).toBe(false);
    expect(
      isPaintedQueueSearchResponse({ ...ready, totalItems: 9_999 }),
    ).toBe(false);
  });
});
