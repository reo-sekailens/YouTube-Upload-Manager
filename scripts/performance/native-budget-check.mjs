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

function maskRustNonCode(source) {
  const characters = source.split("");
  const rawStringPrefix = /(?:b|c)?r(#{0,})"/y;
  const characterLiteral = /'(?:\\(?:u\{[\da-fA-F_]+\}|x[\da-fA-F]{2}|[^\r\n])|[^'\\\r\n])'/uy;
  let index = 0;
  while (index < source.length) {
    const start = index;
    if (source.startsWith("//", index)) {
      const newline = source.indexOf("\n", index + 2);
      index = newline < 0 ? source.length : newline;
    } else if (source.startsWith("/*", index)) {
      let depth = 1;
      index += 2;
      while (index < source.length && depth > 0) {
        if (source.startsWith("/*", index)) { depth += 1; index += 2; }
        else if (source.startsWith("*/", index)) { depth -= 1; index += 2; }
        else index += 1;
      }
    } else {
      rawStringPrefix.lastIndex = index;
      const rawString = /[rbc]/.test(source[index]) &&
        (index === 0 || !/[\w]/.test(source[index - 1])) && rawStringPrefix.exec(source);
      if (rawString) {
        const terminator = `"${rawString[1]}`;
        const end = source.indexOf(terminator, rawStringPrefix.lastIndex);
        index = end < 0 ? source.length : end + terminator.length;
      } else if (source[index] === '"') {
        index += 1;
        while (index < source.length) {
          if (source[index] === "\\") index += 2;
          else if (source[index++] === '"') break;
        }
      } else if (source[index] === "'") {
        // A lifetime such as 'static is code, while a quoted character can
        // contain a brace that must not affect module balancing.
        characterLiteral.lastIndex = index;
        const character = characterLiteral.exec(source);
        if (character) index = characterLiteral.lastIndex;
      }
    }
    if (index === start) { index += 1; continue; }
    for (let masked = start; masked < index && masked < source.length; masked += 1) {
      if (characters[masked] !== "\n" && characters[masked] !== "\r") characters[masked] = " ";
    }
  }
  return characters.join("");
}

function withoutCfgTestModules(source) {
  // Mask literals and comments before matching attributes or balancing braces.
  // Keep source positions/newlines intact, including production declarations
  // after a test module; test modules do not have to be named `tests` or be last.
  const code = maskRustNonCode(source);
  const testModule = /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*(?:#\s*\[[^\]]*\]\s*)*(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+(?:r#)?[A-Za-z_]\w*\s*\{/g;
  const characters = code.split("");
  for (let match = testModule.exec(code); match; match = testModule.exec(code)) {
    let depth = 1;
    let end = testModule.lastIndex;
    while (end < code.length && depth > 0) {
      if (code[end] === "{") depth += 1;
      else if (code[end] === "}") depth -= 1;
      end += 1;
    }
    // Malformed/unclosed modules cannot hide the remainder of production code.
    if (depth !== 0) continue;
    for (let index = match.index; index < end; index += 1) {
      if (characters[index] !== "\n" && characters[index] !== "\r") characters[index] = " ";
    }
    testModule.lastIndex = end;
  }
  return characters.join("");
}

export function evaluateNativeConfiguration(sources) {
  const normalizedSources = Array.isArray(sources) ? sources : [sources];
  const productionSources = normalizedSources.map(withoutCfgTestModules);
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

export function runCargoGate(testName, {
  spawnImpl = spawnSync,
  writeOutput = (output) => process.stdout.write(output),
} = {}) {
  const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
  const result = spawnImpl(
    cargo,
    [
      "test",
      "--locked",
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
    {
      stdio: ["ignore", "pipe", "inherit"],
      encoding: "utf8",
      maxBuffer: 8 * 1024 * 1024,
      windowsHide: true,
    },
  );
  if (result.stdout) writeOutput(result.stdout);
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`Native performance gate ${testName} failed with exit code ${result.status}.`);
  }
  // Cargo exits successfully when an exact filter matches no test. Require
  // both the named success and one executed, non-ignored test before accepting
  // the gate, so a renamed or cfg-disabled fixture cannot silently pass.
  const outputLines = (result.stdout ?? "").split(/\r?\n/);
  const summary = result.stdout?.match(
    /^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;/m,
  );
  if (!outputLines.includes(`test ${testName} ... ok`) ||
      !summary || summary[1] !== "1" || summary[2] !== "0" || summary[3] !== "0") {
    throw new Error(`Native performance gate ${testName} did not execute exactly one passing test.`);
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
