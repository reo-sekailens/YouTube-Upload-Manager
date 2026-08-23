import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

export const NATIVE_CONFIGURATION_BUDGETS = Object.freeze({
  maximumConcurrentUploadsPerVolume: 4,
  startupResidentWorkers: 1,
  directThreadSpawns: 1,
});

const STARTUP_WORKERS = Object.freeze([
  "folder_monitor_deadline_loop",
  "quota_resume_deadline_loop",
]);

const FOCUSED_NATIVE_GATES = Object.freeze([
  "persistence::tests::hot_query_plans_use_channel_and_status_indexes_at_10000_records",
  "tests::current_schema_hot_open_executes_no_schema_or_journal_transition_sql",
  "tests::only_one_worker_can_claim_a_queued_upload",
  "tests::automatic_scheduler_leaves_the_queue_for_the_current_upload_worker",
]);

function occurrenceCount(source, pattern) {
  return [...source.matchAll(pattern)].length;
}

function withoutCfgTestModule(source) {
  const testModule = source.search(/#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*mod\s+tests\s*\{/);
  return testModule < 0 ? source : source.slice(0, testModule);
}

export function evaluateNativeConfiguration(sources) {
  const normalizedSources = Array.isArray(sources) ? sources : [sources];
  const productionSources = normalizedSources.map(withoutCfgTestModule);
  const source = productionSources.join("\n");
  const concurrencyMatch = source.match(
    /const\s+MAX_CONCURRENT_UPLOADS_PER_VOLUME\s*:\s*usize\s*=\s*(\d+)\s*;/,
  );
  const maximumConcurrentUploadsPerVolume = concurrencyMatch
    ? Number.parseInt(concurrencyMatch[1], 10)
    : null;
  const startupWorkerCounts = Object.fromEntries(
    STARTUP_WORKERS.map((worker) => [
      worker,
      occurrenceCount(source, new RegExp(`spawn_worker\\(move \\|\\| ${worker}\\(`, "g")),
    ]),
  );
  const setupBody = source.match(
    /\.setup\(\|app\|\s*\{([\s\S]*?)\n\s*\}\)\s*\n\s*\.invoke_handler/,
  )?.[1];
  const startupResidentWorkers = setupBody
    ? occurrenceCount(setupBody, /\bspawn_worker\s*\(/g) +
      occurrenceCount(setupBody, /\.state_events\s*\.start\s*\(/g)
    : null;
  const directThreadSpawns = occurrenceCount(source, /\bthread::spawn\s*\(/g);

  const checks = [
    {
      metric: "maximumConcurrentUploadsPerVolume",
      actual: maximumConcurrentUploadsPerVolume,
      maximum: NATIVE_CONFIGURATION_BUDGETS.maximumConcurrentUploadsPerVolume,
      passed:
        maximumConcurrentUploadsPerVolume !== null &&
        maximumConcurrentUploadsPerVolume >= 1 &&
        maximumConcurrentUploadsPerVolume <=
          NATIVE_CONFIGURATION_BUDGETS.maximumConcurrentUploadsPerVolume,
    },
    {
      metric: "startupResidentWorkers",
      actual: startupResidentWorkers,
      maximum: NATIVE_CONFIGURATION_BUDGETS.startupResidentWorkers,
      passed:
        startupResidentWorkers !== null &&
        startupResidentWorkers <= NATIVE_CONFIGURATION_BUDGETS.startupResidentWorkers &&
        Object.values(startupWorkerCounts).every((count) => count <= 1),
    },
    {
      metric: "directThreadSpawns",
      actual: directThreadSpawns,
      maximum: NATIVE_CONFIGURATION_BUDGETS.directThreadSpawns,
      passed: directThreadSpawns <= NATIVE_CONFIGURATION_BUDGETS.directThreadSpawns,
    },
  ];

  return {
    schemaVersion: 1,
    localOnly: true,
    timingMeasurements: false,
    budgets: {
      passed: checks.every((check) => check.passed),
      checks,
    },
    startupWorkerCounts,
  };
}

function runCargoGate(testName) {
  const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
  const result = spawnSync(
    cargo,
    [
      "test",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--features",
      "performance-harness",
      "--lib",
      testName,
      "--",
      "--exact",
      "--test-threads=1",
    ],
    { stdio: "inherit" },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`Native performance gate ${testName} failed with exit code ${result.status}.`);
  }
}

export async function runNativeBudgetCheck({ runCargoTests = true } = {}) {
  const sourceDirectory = path.resolve("src-tauri", "src");
  const sourceNames = (await readdir(sourceDirectory))
    .filter(
      (name) =>
        name.endsWith(".rs") && !name.endsWith("_performance_benchmarks.rs") &&
        name !== "performance_benchmarks.rs",
    )
    .sort();
  const sources = await Promise.all(
    sourceNames.map((name) => readFile(path.join(sourceDirectory, name), "utf8")),
  );
  const result = evaluateNativeConfiguration(sources);
  console.log(JSON.stringify(result, null, 2));
  if (!result.budgets.passed) {
    throw new Error("Deterministic native worker configuration exceeds its budget.");
  }
  if (runCargoTests) {
    for (const testName of FOCUSED_NATIVE_GATES) runCargoGate(testName);
  }
  return result;
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  runNativeBudgetCheck({
    runCargoTests: !process.argv.slice(2).includes("--configuration-only"),
  }).catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
