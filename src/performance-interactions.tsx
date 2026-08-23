import { StrictMode, useEffect, useMemo } from "react";
import { createRoot } from "react-dom/client";
import { QueueTable } from "./components/QueueTable";
import type { UploadItem } from "./lib/types";
import {
  interactionCertificationFailures,
  summarizeInteractionSamples,
} from "./lib/performance-interaction-report";
import type {
  BrowserInteractionReport,
  InteractionKind,
  InteractionSample,
} from "./lib/performance-interaction-report";
import "./styles.css";

declare global {
  interface Window {
    __YUM_PERFORMANCE_INTERACTION_REPORT__?: BrowserInteractionReport;
  }
}

const fixtureSize = 10_000;
const mountedRecordLimit = 32;
const warmupPairs = 5;
const measuredPairs = 40;
const queryPrefix = "certification-target-";
let driverStarted = false;

type LongTaskReceipt = { startTime: number; duration: number };

function fixtureItems(): UploadItem[] {
  return Array.from({ length: fixtureSize }, (_, index) => {
    const ordinal = String(index + 1).padStart(5, "0");
    return {
      id: `synthetic-${ordinal}`,
      title: `Synthetic video ${ordinal} ${queryPrefix}${ordinal}`,
      fileName: `synthetic-${ordinal}.mp4`,
      sizeBytes: 1_048_576,
      digest: ordinal.padStart(64, "0"),
      status: "uploaded",
      confirmedBytes: 1_048_576,
      totalBytes: 1_048_576,
      visibility: "private",
      madeForKids: false,
      updatedAt: "1970-01-01T00:00:00.000Z",
    };
  });
}

function nextFrame() {
  return new Promise<number>((resolve) => requestAnimationFrame(resolve));
}

async function afterPaint() {
  await nextFrame();
  await nextFrame();
  return performance.now();
}

async function waitFor(
  predicate: () => boolean,
  description: string,
  timeoutMs = 5_000,
) {
  if (predicate()) return;
  await new Promise<void>((resolve, reject) => {
    const observer = new MutationObserver(() => {
      if (!predicate()) return;
      window.clearTimeout(timeout);
      observer.disconnect();
      resolve();
    });
    const timeout = window.setTimeout(() => {
      observer.disconnect();
      reject(new Error(`Timed out waiting for ${description}.`));
    }, timeoutMs);
    observer.observe(document.documentElement, {
      attributes: true,
      characterData: true,
      childList: true,
      subtree: true,
    });
  });
}

function setReactInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )?.set;
  if (!setter) throw new Error("The browser did not expose the input value setter.");
  setter.call(input, value);
}

function visibleTitles(root: Element) {
  return Array.from(
    root.querySelectorAll<HTMLElement>("[data-queue-record] strong"),
    ({ textContent }) => textContent ?? "",
  );
}

function sampleLongTasks(
  entries: readonly LongTaskReceipt[],
  startedAtMs: number,
  paintedAtMs: number,
) {
  const matching = entries.filter(
    ({ startTime }) => startTime >= startedAtMs && startTime <= paintedAtMs,
  );
  return {
    longTasks: matching.length,
    maxLongTaskMs: Math.round(
      matching.reduce((maximum, entry) => Math.max(maximum, entry.duration), 0),
    ),
  };
}

async function measureSearch(
  ordinal: number,
  root: Element,
  longTaskEntries: LongTaskReceipt[],
): Promise<InteractionSample> {
  const input = root.querySelector<HTMLInputElement>("#upload-queue-title-search");
  const region = input?.closest<HTMLElement>("[data-queue-table]");
  if (!input || !region) throw new Error("The real queue search control is missing.");
  const target = String(((ordinal * 239) % fixtureSize) + 1).padStart(5, "0");
  const query = `${queryPrefix}${target}`;
  const startedAtMs = performance.now();
  input.dispatchEvent(
    new KeyboardEvent("keydown", { bubbles: true, cancelable: true, key: query[0] }),
  );
  setReactInputValue(input, query);
  input.dispatchEvent(
    new InputEvent("input", {
      bubbles: true,
      cancelable: false,
      data: query,
      inputType: "insertText",
    }),
  );
  await waitFor(() => {
    const titles = visibleTitles(root);
    return (
      input.value === query &&
      input.getAttribute("aria-busy") === "false" &&
      region.getAttribute("aria-busy") === "false" &&
      titles.length === 1 &&
      titles.every((title) => title.toLocaleLowerCase().includes(query))
    );
  }, "the painted 10k-title search response");
  const responsePaintedMs = await afterPaint();
  return {
    ordinal,
    kind: "batch_search_10k",
    latencyMs: Math.round(responsePaintedMs - startedAtMs),
    responsePaintedMs: Math.round(responsePaintedMs),
    ...sampleLongTasks(longTaskEntries, startedAtMs, responsePaintedMs),
    totalItems: fixtureSize,
    visibleRecords: visibleTitles(root).length,
  };
}

async function measureClear(
  ordinal: number,
  root: Element,
  longTaskEntries: LongTaskReceipt[],
): Promise<InteractionSample> {
  const input = root.querySelector<HTMLInputElement>("#upload-queue-title-search");
  const region = input?.closest<HTMLElement>("[data-queue-table]");
  const button = Array.from(root.querySelectorAll<HTMLButtonElement>("button")).find(
    ({ textContent }) => textContent?.trim() === "Clear search",
  );
  if (!input || !region || !button)
    throw new Error("The real Batch clear-search control is missing.");
  const startedAtMs = performance.now();
  button.click();
  await waitFor(
    () =>
      input.value === "" &&
      input.getAttribute("aria-busy") === "false" &&
      region.getAttribute("aria-busy") === "false" &&
      visibleTitles(root).length === mountedRecordLimit,
    "the painted Batch clear response",
  );
  const responsePaintedMs = await afterPaint();
  return {
    ordinal,
    kind: "batch_clear",
    latencyMs: Math.round(responsePaintedMs - startedAtMs),
    responsePaintedMs: Math.round(responsePaintedMs),
    ...sampleLongTasks(longTaskEntries, startedAtMs, responsePaintedMs),
    totalItems: fixtureSize,
    visibleRecords: visibleTitles(root).length,
  };
}

async function runInteractionCertification(root: Element) {
  const longTaskEntries: LongTaskReceipt[] = [];
  const longTaskObserverSupported =
    "PerformanceObserver" in window &&
    PerformanceObserver.supportedEntryTypes.includes("longtask");
  const observer = longTaskObserverSupported
    ? new PerformanceObserver((list) => {
        for (const entry of list.getEntries())
          longTaskEntries.push({ startTime: entry.startTime, duration: entry.duration });
      })
    : undefined;
  observer?.observe({ type: "longtask", buffered: true });

  await waitFor(
    () => visibleTitles(root).length === mountedRecordLimit,
    "the bounded 10k-item Batch fixture",
  );
  await afterPaint();

  for (let ordinal = 1; ordinal <= warmupPairs; ordinal += 1) {
    await measureSearch(ordinal, root, longTaskEntries);
    await measureClear(ordinal, root, longTaskEntries);
  }
  longTaskEntries.length = 0;

  const samples: InteractionSample[] = [];
  for (let ordinal = 1; ordinal <= measuredPairs; ordinal += 1) {
    samples.push(await measureSearch(ordinal, root, longTaskEntries));
    samples.push(await measureClear(ordinal, root, longTaskEntries));
  }
  for (const entry of observer?.takeRecords() ?? [])
    longTaskEntries.push({ startTime: entry.startTime, duration: entry.duration });
  observer?.disconnect();

  const searches = samples.filter(({ kind }) => kind === "batch_search_10k");
  const clears = samples.filter(({ kind }) => kind === "batch_clear");
  const maximumInteractionLongTaskMs = samples.reduce(
    (maximum, sample) => Math.max(maximum, sample.maxLongTaskMs),
    0,
  );
  const report: BrowserInteractionReport = {
    schemaVersion: 1,
    evidenceBoundary: "local-browser-performance-harness",
    localOnly: true,
    containsSensitiveData: false,
    fixture: {
      synthetic: true,
      uploadItems: fixtureSize,
      mountedRecordLimit,
      warmupPairs,
      measuredPairs,
    },
    browser: {
      engine: "chromium",
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      longTaskObserverSupported,
    },
    runtimeErrors: Number(document.documentElement.dataset.runtimeErrors ?? "0"),
    search10k: summarizeInteractionSamples(searches),
    batchClear: summarizeInteractionSamples(clears),
    maximumInteractionLongTaskMs,
    samples,
    gates: {
      searchP95Under100Ms: searches.length === 40 && summarizeInteractionSamples(searches).p95 < 100,
      batchP95Under100Ms: clears.length === 40 && summarizeInteractionSamples(clears).p95 < 100,
      noInteractionLongTaskOver50Ms: maximumInteractionLongTaskMs <= 50,
      boundedMountedRows: samples.every(({ visibleRecords }) => visibleRecords <= mountedRecordLimit),
      fortySearches: searches.length === 40,
      fortyBatchInteractions: clears.length === 40,
      passed: false,
    },
  };
  report.gates.passed = interactionCertificationFailures(report).length === 0;
  window.__YUM_PERFORMANCE_INTERACTION_REPORT__ = report;
  document.documentElement.dataset.certification = report.gates.passed
    ? "passed"
    : "failed";
}

function InteractionFixture() {
  const items = useMemo(fixtureItems, []);
  useEffect(() => {
    if (driverStarted) return;
    driverStarted = true;
    const root = document.querySelector('[data-performance-batch-content="ready"]');
    if (!root) return;
    void runInteractionCertification(root).catch((error) => {
      document.documentElement.dataset.runtimeErrors = String(
        Number(document.documentElement.dataset.runtimeErrors ?? "0") + 1,
      );
      document.documentElement.dataset.certificationFailure =
        error instanceof Error ? error.message : "Unknown interaction driver failure.";
      document.documentElement.dataset.certification = "failed";
    });
  }, []);

  return (
    <main className="mx-auto max-w-[1240px] px-8 pt-8 pb-20">
      <section
        className="mt-4"
        data-performance-batch-content="ready"
        id="workspace-tab-batch"
      >
        <section className="rounded-xl border border-line bg-surface p-6 shadow-[0_12px_30px_rgba(25,35,56,0.045)]" aria-labelledby="queue-heading">
          <div className="mb-5 flex items-start justify-between gap-4">
            <div>
              <p className="mb-2 text-[0.67rem] font-bold uppercase tracking-[0.1em] leading-[1.2] text-muted">LOCAL PERFORMANCE HARNESS</p>
              <h1 id="queue-heading" className="m-0 text-[1.2rem] font-bold tracking-[-0.035em] text-ink">10,000-title interaction certification</h1>
              <p className="mt-2 max-w-[40rem] text-[0.82rem] leading-[1.5] text-muted">
                Synthetic device-local records exercise the real Batch queue component.
              </p>
            </div>
            <span className="whitespace-nowrap rounded-full bg-[#f2f4f7] px-2.5 py-1.5 text-[0.73rem] font-semibold text-[#58657a]">10,000 saved items</span>
          </div>
          <QueueTable
            busy={false}
            items={items}
            onCancel={() => undefined}
            onDeleteSourceAfterUploadChange={() => undefined}
            onDeleteUploadedSource={() => undefined}
            onQueue={() => undefined}
            onVisibilityChange={() => undefined}
          />
        </section>
      </section>
    </main>
  );
}

let runtimeErrors = 0;
const recordRuntimeError = () => {
  runtimeErrors += 1;
  document.documentElement.dataset.runtimeErrors = String(runtimeErrors);
};
window.addEventListener("error", recordRuntimeError);
window.addEventListener("unhandledrejection", recordRuntimeError);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <InteractionFixture />
  </StrictMode>,
);
