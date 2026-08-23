import { describe, expect, it } from "vitest";
import type {
  BrowserInteractionReport,
  InteractionSample,
} from "./performance-interaction-report";
import {
  interactionCertificationFailures,
  summarizeInteractionSamples,
} from "./performance-interaction-report";

function sample(
  ordinal: number,
  kind: InteractionSample["kind"],
  latencyMs: number,
): InteractionSample {
  return {
    ordinal,
    kind,
    latencyMs,
    responsePaintedMs: latencyMs + 1_000,
    longTasks: 0,
    maxLongTaskMs: 0,
    totalItems: 10_000,
    visibleRecords: kind === "batch_clear" ? 32 : 1,
  };
}

function report(): BrowserInteractionReport {
  const searches = Array.from({ length: 40 }, (_, index) =>
    sample(index + 1, "batch_search_10k", index + 1),
  );
  const clears = Array.from({ length: 40 }, (_, index) =>
    sample(index + 1, "batch_clear", index + 2),
  );
  const samples = searches.flatMap((search, index) => [search, clears[index]]);
  return {
    schemaVersion: 1,
    evidenceBoundary: "local-browser-performance-harness",
    localOnly: true,
    containsSensitiveData: false,
    fixture: {
      synthetic: true,
      uploadItems: 10_000,
      mountedRecordLimit: 32,
      warmupPairs: 5,
      measuredPairs: 40,
    },
    browser: {
      engine: "chromium",
      viewportWidth: 1440,
      viewportHeight: 1200,
      longTaskObserverSupported: true,
    },
    runtimeErrors: 0,
    search10k: summarizeInteractionSamples(searches),
    batchClear: summarizeInteractionSamples(clears),
    maximumInteractionLongTaskMs: 0,
    samples,
    gates: {
      searchP95Under100Ms: true,
      batchP95Under100Ms: true,
      noInteractionLongTaskOver50Ms: true,
      boundedMountedRows: true,
      fortySearches: true,
      fortyBatchInteractions: true,
      passed: true,
    },
  };
}

describe("browser interaction performance report", () => {
  it("uses nearest-rank distributions for the complete 40-sample population", () => {
    const summary = report().search10k;
    expect(summary).toEqual({ count: 40, p50: 20, p90: 36, p95: 38, maximum: 40 });
  });

  it("accepts only a redacted 40-pair run that satisfies every response gate", () => {
    expect(interactionCertificationFailures(report())).toEqual([]);
  });

  it("rejects missing samples, slow responses, excess rows, and long tasks", () => {
    const invalid = report();
    invalid.search10k = { count: 39, p50: 20, p90: 90, p95: 101, maximum: 101 };
    invalid.maximumInteractionLongTaskMs = 51;
    invalid.samples[0].visibleRecords = 33;
    expect(interactionCertificationFailures(invalid)).toEqual([
      "Exactly 40 measured 10k-title searches are required.",
      "The 10k-title search p95 must remain below 100 ms.",
      "No measured interaction may contain a long task over 50 ms.",
      "The bounded queue window mounted more than 32 rows.",
    ]);
  });
});
