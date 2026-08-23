import { invoke, isTauri } from "@tauri-apps/api/core";

export const performanceHarnessEnabled =
  import.meta.env.TAURI_ENV_PERFORMANCE_HARNESS === "1" && isTauri();
const batchContentSelector = '[data-performance-batch-content="ready"]';
const safeShellSelector = "[data-performance-shell]";
const safeShellMark = safeShellSelector;
const queueSearchSelector = "#upload-queue-title-search";
let initialized = false;
let safeShellPaintScheduled = false;
let safeShellPaintRecorded = false;
let safeShellNativeRequest = false;
let safeShellReceipt: Promise<void> | undefined;
let resolveSafeShellReceipt: (() => void) | undefined;
let rejectSafeShellReceipt: ((error: unknown) => void) | undefined;
let batchPaintScheduled = false;
let batchPaintRecorded = false;
let interactionPaintScheduled = false;
let firstInteractionRecorded = false;
let longTasks = 0;
let maxLongTaskMs = 0;

type FrontendMilestone =
  | "safe_shell_paint"
  | "first_batch_paint"
  | "first_interaction"
  | "settled_idle"
  | "idle_sample_end";

type FrontendMetrics = {
  reactCommits: null;
  longTasks: number;
  maxLongTaskMs: number;
  safeShellPaintMs?: number;
  firstInteractionResponseMs?: number;
  firstInteractionLatencyMs?: number;
  firstInteractionKind?: string;
};

type PendingInteraction = {
  batch: Element;
  kind: string;
  query?: string;
  startedAtMs: number;
  totalItems?: number;
};

export type PaintedResponseReceipt = {
  responsePaintedMs: number;
  latencyMs: number;
  longTasks: number;
  maxLongTaskMs: number;
};

export type QueueSearchPaintState = {
  expectedQuery: string;
  inputValue: string;
  inputBusy: boolean;
  regionBusy: boolean;
  totalItems: number;
  visibleTitles: string[];
  noMatches: boolean;
};

let pendingInteraction: PendingInteraction | undefined;
let pendingSearchKey:
  | { input: HTMLInputElement; startedAtMs: number }
  | undefined;

export function hasPaintableBatchContent(
  root: Pick<ParentNode, "querySelector">,
) {
  return root.querySelector(batchContentSelector) !== null;
}

export function hasPaintableSafeShell(
  root: Pick<ParentNode, "querySelector">,
) {
  return root.querySelector(safeShellSelector) !== null;
}

export function createPaintedResponseReceipt(
  startedAtMs: number,
  responsePaintedMs: number,
  observedLongTasks: number,
  observedMaxLongTaskMs: number,
): PaintedResponseReceipt {
  return {
    responsePaintedMs: Math.round(responsePaintedMs),
    latencyMs: Math.max(0, Math.round(responsePaintedMs - startedAtMs)),
    longTasks: observedLongTasks,
    maxLongTaskMs: Math.round(observedMaxLongTaskMs),
  };
}

export function isPaintedQueueSearchResponse(state: QueueSearchPaintState) {
  const query = state.expectedQuery.trim().toLocaleLowerCase();
  if (
    state.totalItems < 10_000 ||
    !query ||
    state.inputValue !== state.expectedQuery ||
    state.inputBusy ||
    state.regionBusy
  )
    return false;
  return (
    state.noMatches ||
    (state.visibleTitles.length > 0 &&
      state.visibleTitles.every((title) =>
        title.toLocaleLowerCase().includes(query),
      ))
  );
}

function sendMilestone(
  milestone: FrontendMilestone,
  metrics: Partial<FrontendMetrics> = {},
) {
  return invoke("mark_performance_milestone", {
    milestone,
    metrics: {
      // Default production React intentionally omits Profiler timers. Reporting
      // null is truthful; substituting a profiling build would distort startup.
      reactCommits: null,
      longTasks,
      maxLongTaskMs: Math.round(maxLongTaskMs),
      ...metrics,
    },
  }).then(() => undefined);
}

function mark(milestone: FrontendMilestone, metrics: Partial<FrontendMetrics> = {}) {
  return sendMilestone(milestone, metrics).catch(() => {
      // A performance marker must never interfere with the operator workspace.
    });
}

function recordSafeShellPaint(paintedAtMs: number) {
  if (safeShellPaintRecorded || safeShellNativeRequest) return;
  safeShellReceiptPromise();
  safeShellNativeRequest = true;
  void sendMilestone("safe_shell_paint", {
    safeShellPaintMs: Math.round(paintedAtMs),
  })
    .then(() => {
      safeShellPaintRecorded = true;
      resolveSafeShellReceipt?.();
    })
    .catch((error) => {
      rejectSafeShellReceipt?.(error);
    });
}

function safeShellReceiptPromise() {
  safeShellReceipt ??= new Promise<void>((resolve, reject) => {
    resolveSafeShellReceipt = resolve;
    rejectSafeShellReceipt = reject;
  });
  return safeShellReceipt;
}

function scheduleSafeShellPaintReceipt() {
  if (safeShellPaintRecorded || safeShellPaintScheduled) return;
  const existingMark = performance.getEntriesByName(safeShellMark)[0];
  if (existingMark) {
    recordSafeShellPaint(existingMark.startTime);
    return;
  }
  if (!hasPaintableSafeShell(document)) return;
  safeShellPaintScheduled = true;
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      safeShellPaintScheduled = false;
      if (!hasPaintableSafeShell(document)) return;
      performance.mark(safeShellMark);
      recordSafeShellPaint(
        performance.getEntriesByName(safeShellMark)[0]?.startTime ??
          performance.now(),
      );
    });
  });
}

export function waitForSafeShellPaintReceipt() {
  const receipt = safeShellReceiptPromise();
  scheduleSafeShellPaintReceipt();
  return receipt;
}

export async function completeStartupAfterSafeShellPaint<T>(
  complete: () => Promise<T>,
  receipt: () => Promise<void> = waitForSafeShellPaintReceipt,
) {
  await receipt();
  return complete();
}

function scheduleBatchPaintReceipt() {
  if (
    batchPaintRecorded ||
    batchPaintScheduled ||
    !hasPaintableBatchContent(document)
  )
    return;
  batchPaintScheduled = true;
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      batchPaintScheduled = false;
      if (!hasPaintableBatchContent(document)) return;
      batchPaintRecorded = true;
      void mark("first_batch_paint");
      window.setTimeout(() => {
        // Start the exact idle window only after native has persisted its
        // baseline counters; otherwise two floating invokes can shorten it.
        void mark("settled_idle").then(() => {
          window.setTimeout(() => void mark("idle_sample_end"), 2_000);
        });
      }, 1_000);
    });
  });
}

function readQueueItemCount(batch: Element) {
  const match = batch.querySelector(".item-count")?.textContent?.match(/[\d,]+/);
  return match ? Number(match[0].replaceAll(",", "")) : 0;
}

function searchResponseIsReady(interaction: PendingInteraction) {
  const input = interaction.batch.querySelector<HTMLInputElement>(
    queueSearchSelector,
  );
  const region = input?.closest(".queue-table-wrap");
  if (!input || !region || interaction.query === undefined) return false;
  return isPaintedQueueSearchResponse({
    expectedQuery: interaction.query,
    inputValue: input.value,
    inputBusy: input.getAttribute("aria-busy") !== "false",
    regionBusy: region.getAttribute("aria-busy") !== "false",
    totalItems: interaction.totalItems ?? 0,
    visibleTitles: Array.from(
      interaction.batch.querySelectorAll("[data-queue-record] strong"),
      (title) => title.textContent ?? "",
    ),
    noMatches:
      interaction.batch.querySelectorAll("[data-queue-record]").length === 0 &&
      interaction.batch.textContent?.includes("No video titles match") === true,
  });
}

function scheduleInteractionPaintReceipt() {
  const interaction = pendingInteraction;
  if (
    !interaction ||
    firstInteractionRecorded ||
    interactionPaintScheduled ||
    (interaction.query !== undefined && !searchResponseIsReady(interaction))
  )
    return;
  interactionPaintScheduled = true;
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      interactionPaintScheduled = false;
      if (pendingInteraction !== interaction) {
        scheduleInteractionPaintReceipt();
        return;
      }
      if (
        !interaction.batch.isConnected ||
        (interaction.query !== undefined && !searchResponseIsReady(interaction))
      )
        return;
      const receipt = createPaintedResponseReceipt(
        interaction.startedAtMs,
        performance.now(),
        longTasks,
        maxLongTaskMs,
      );
      firstInteractionRecorded = true;
      pendingInteraction = undefined;
      void mark("first_interaction", {
        firstInteractionResponseMs: receipt.responsePaintedMs,
        firstInteractionLatencyMs: receipt.latencyMs,
        firstInteractionKind: interaction.kind,
      });
    });
  });
}

function beginBatchInteraction(event: Event) {
  if (firstInteractionRecorded || !(event.target instanceof Element)) return;
  const batch = event.target.closest(batchContentSelector);
  if (!batch) return;
  const searchInput = event.target.closest<HTMLInputElement>(queueSearchSelector);
  if (event.type === "input" && searchInput) {
    const totalItems = readQueueItemCount(batch);
    pendingInteraction = {
      batch,
      kind: totalItems >= 10_000 ? "batch_search_10k" : "batch_search",
      query: totalItems >= 10_000 ? searchInput.value : undefined,
      startedAtMs:
        pendingSearchKey?.input === searchInput
          ? pendingSearchKey.startedAtMs
          : performance.now(),
      totalItems,
    };
    pendingSearchKey = undefined;
    queueMicrotask(scheduleInteractionPaintReceipt);
    return;
  }
  pendingInteraction = {
    batch,
    kind: `batch_${event.type}`,
    startedAtMs: performance.now(),
  };
}

function beginBatchSearchKey(event: KeyboardEvent) {
  if (firstInteractionRecorded || !(event.target instanceof HTMLInputElement))
    return;
  if (!event.target.matches(queueSearchSelector)) return;
  pendingSearchKey = { input: event.target, startedAtMs: performance.now() };
}

/**
 * Adds harness-only safe-shell, interaction, long-task, and real Batch-content
 * milestones. Ordinary builds never import this module. Paint receipts require
 * two animation frames and selectors that crash/Suspense fallbacks cannot
 * satisfy. The 10k search receipt additionally waits for the deferred result
 * set to match the requested query while keeping the bounded row window.
 */
export function initializePerformanceHarness() {
  if (!performanceHarnessEnabled || initialized) return;
  initialized = true;

  if (
    "PerformanceObserver" in window &&
    PerformanceObserver.supportedEntryTypes.includes("longtask")
  ) {
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        longTasks += 1;
        maxLongTaskMs = Math.max(maxLongTaskMs, entry.duration);
      }
    });
    observer.observe({ type: "longtask", buffered: true });
  }

  const paintObserver = new MutationObserver(() => {
    scheduleSafeShellPaintReceipt();
    scheduleBatchPaintReceipt();
    scheduleInteractionPaintReceipt();
  });
  paintObserver.observe(document.documentElement, {
    attributes: true,
    characterData: true,
    childList: true,
    subtree: true,
  });
  scheduleSafeShellPaintReceipt();
  scheduleBatchPaintReceipt();

  document.addEventListener("click", beginBatchInteraction, true);
  document.addEventListener("change", beginBatchInteraction, true);
  document.addEventListener("input", beginBatchInteraction, true);
  document.addEventListener("keydown", beginBatchSearchKey, true);
}
