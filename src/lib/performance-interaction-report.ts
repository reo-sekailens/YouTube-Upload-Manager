export type InteractionKind = "batch_clear" | "batch_search_10k";

export type InteractionSample = {
  ordinal: number;
  kind: InteractionKind;
  latencyMs: number;
  responsePaintedMs: number;
  longTasks: number;
  maxLongTaskMs: number;
  totalItems: number;
  visibleRecords: number;
};

export type InteractionDistribution = {
  count: number;
  p50: number;
  p90: number;
  p95: number;
  maximum: number;
};

export type BrowserInteractionReport = {
  schemaVersion: 1;
  evidenceBoundary: "local-browser-performance-harness";
  localOnly: true;
  containsSensitiveData: false;
  fixture: {
    synthetic: true;
    uploadItems: 10_000;
    mountedRecordLimit: 32;
    warmupPairs: number;
    measuredPairs: 40;
  };
  browser: {
    engine: "chromium";
    viewportWidth: number;
    viewportHeight: number;
    longTaskObserverSupported: boolean;
  };
  runtimeErrors: number;
  search10k: InteractionDistribution;
  batchClear: InteractionDistribution;
  maximumInteractionLongTaskMs: number;
  samples: InteractionSample[];
  gates: {
    searchP95Under100Ms: boolean;
    batchP95Under100Ms: boolean;
    noInteractionLongTaskOver50Ms: boolean;
    boundedMountedRows: boolean;
    fortySearches: boolean;
    fortyBatchInteractions: boolean;
    passed: boolean;
  };
};

function nearestRank(sorted: number[], percentile: number) {
  if (sorted.length === 0) return 0;
  return sorted[Math.max(0, Math.ceil(percentile * sorted.length) - 1)];
}

export function summarizeInteractionSamples(
  samples: readonly InteractionSample[],
): InteractionDistribution {
  const values = samples.map(({ latencyMs }) => latencyMs).sort((a, b) => a - b);
  return {
    count: values.length,
    p50: nearestRank(values, 0.5),
    p90: nearestRank(values, 0.9),
    p95: nearestRank(values, 0.95),
    maximum: values.at(-1) ?? 0,
  };
}

export function interactionCertificationFailures(
  report: BrowserInteractionReport,
) {
  const failures: string[] = [];
  if (!report.localOnly || report.containsSensitiveData)
    failures.push("The report does not satisfy its local-only redaction contract.");
  if (!report.browser.longTaskObserverSupported)
    failures.push("The browser did not expose the Long Tasks API.");
  if (report.search10k.count !== 40)
    failures.push("Exactly 40 measured 10k-title searches are required.");
  if (report.batchClear.count !== 40)
    failures.push("Exactly 40 measured Batch clear interactions are required.");
  if (report.search10k.p95 >= 100)
    failures.push("The 10k-title search p95 must remain below 100 ms.");
  if (report.batchClear.p95 >= 100)
    failures.push("The Batch clear p95 must remain below 100 ms.");
  if (report.maximumInteractionLongTaskMs > 50)
    failures.push("No measured interaction may contain a long task over 50 ms.");
  if (report.samples.some(({ visibleRecords }) => visibleRecords > 32))
    failures.push("The bounded queue window mounted more than 32 rows.");
  if (report.runtimeErrors !== 0)
    failures.push("The browser reported a runtime error during the run.");
  return failures;
}
