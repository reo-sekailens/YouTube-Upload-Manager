import { describe, expect, it } from "vitest";
import {
  NATIVE_CONFIGURATION_BUDGETS,
  evaluateNativeConfiguration,
} from "../scripts/performance/native-budget-check.mjs";

function fixture({
  concurrency = 4,
  extraThread = false,
  extraStartupWorker = false,
  omitQuotaWorker = false,
} = {}) {
  const library = `
const MAX_CONCURRENT_UPLOADS_PER_VOLUME: usize = ${concurrency};
fn spawn_worker() { thread::spawn(worker); }
fn start_workers() {
  spawn_worker(move || folder_monitor_deadline_loop(monitor_state));
  ${omitQuotaWorker ? "" : "spawn_worker(move || quota_resume_deadline_loop(state));"}
}
builder
  .setup(|app| {
    ${extraStartupWorker ? "spawn_worker(move || another_poll_loop(state));" : ""}
    ${extraThread ? "thread::spawn(other_worker);" : ""}
    state.state_events.start(app.handle(), state.database_path);
    Ok(())
  })
  .invoke_handler(handler);
#[cfg(test)]
mod tests {
  fn test_helper() { thread::spawn(test_worker); }
}
`;
  return [library, "fn persistence_read() {}"];
}

describe("deterministic native performance budgets", () => {
  it("accepts the bounded upload and resident-worker configuration", () => {
    const result = evaluateNativeConfiguration(fixture());

    expect(result.budgets.passed).toBe(true);
    expect(result.budgets.checks).toEqual([
      {
        metric: "maximumConcurrentUploadsPerVolume",
        actual: 4,
        maximum: NATIVE_CONFIGURATION_BUDGETS.maximumConcurrentUploadsPerVolume,
        passed: true,
      },
      {
        metric: "startupResidentWorkers",
        actual: 1,
        maximum: NATIVE_CONFIGURATION_BUDGETS.startupResidentWorkers,
        passed: true,
      },
      {
        metric: "directThreadSpawns",
        actual: 1,
        maximum: NATIVE_CONFIGURATION_BUDGETS.directThreadSpawns,
        passed: true,
      },
    ]);
  });

  it("rejects excess concurrency, direct spawning, and an extra resident worker", () => {
    const result = evaluateNativeConfiguration(
      fixture({
        concurrency: 5,
        extraThread: true,
        extraStartupWorker: true,
      }),
    );

    expect(result.budgets.passed).toBe(false);
    expect(result.budgets.checks.map((check) => check.passed)).toEqual([
      false,
      false,
      false,
    ]);
  });

  it("ignores cfg-test thread spawns while accepting removed conditional workers", () => {
    const source = fixture({ omitQuotaWorker: true });
    source[0] = source[0]
      .replace("spawn_worker(move || folder_monitor_deadline_loop(monitor_state));", "")
      .replace("fn spawn_worker() { thread::spawn(worker); }", "");

    expect(evaluateNativeConfiguration(source).budgets.passed).toBe(true);
  });

  it("counts direct spawns in every production module", () => {
    const source = fixture();
    source.push("fn extra_module_worker() { thread::spawn(worker); }");

    expect(evaluateNativeConfiguration(source).budgets.passed).toBe(false);
  });
});
