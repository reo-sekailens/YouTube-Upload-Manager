import assert from "node:assert/strict";
import test from "node:test";
import { runCargoGate } from "./native-budget-check.mjs";

const gate = "tests::only_one_worker_can_claim_a_queued_upload";

function resultWith(output, overrides = {}) {
  return { status: 0, stdout: output, ...overrides };
}

function runFixture(result) {
  return () => runCargoGate(gate, { spawnImpl: () => result, writeOutput() {} });
}

test("accepts only the requested exact native gate with one passing test", () => {
  let invocation;
  let printed = "";
  const output = `running 1 test\r\ntest ${gate} ... ok\r\n\r\n` +
    "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 149 filtered out; finished in 0.01s\r\n";
  runCargoGate(gate, {
    spawnImpl(command, args, options) {
      invocation = { command, args, options };
      return resultWith(output);
    },
    writeOutput(value) { printed += value; },
  });
  assert.equal(printed, output);
  assert.equal(invocation.args[invocation.args.indexOf("--") - 1], gate);
  assert.ok(invocation.args.includes("--locked"));
  assert.ok(invocation.args.includes("--exact"));
  assert.deepEqual(invocation.options.stdio, ["ignore", "pipe", "inherit"]);
  assert.equal(invocation.options.windowsHide, true);
});

test("rejects a successful Cargo exit that executes zero tests", () => {
  assert.throws(runFixture(resultWith(
    "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 150 filtered out; finished in 0.00s\n",
  )), /did not execute exactly one passing test/);
});

test("rejects an ignored gate or success for a different named test", () => {
  assert.throws(runFixture(resultWith(
    `running 1 test\ntest ${gate} ... ignored\n\n` +
    "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 149 filtered out; finished in 0.00s\n",
  )), /did not execute exactly one passing test/);
  assert.throws(runFixture(resultWith(
    "running 1 test\ntest tests::another_gate ... ok\n\n" +
    "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 149 filtered out; finished in 0.01s\n",
  )), /did not execute exactly one passing test/);
});

test("rejects missing or ambiguous execution receipts", () => {
  assert.throws(runFixture(resultWith("")), /did not execute exactly one passing test/);
  assert.throws(runFixture(resultWith(
    `test ${gate} ... ok\n\n` +
    "test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 148 filtered out; finished in 0.01s\n",
  )), /did not execute exactly one passing test/);
});

test("preserves failed execution and process launch errors", () => {
  assert.throws(runFixture(resultWith("", { status: 101 })), /exit code 101/);
  const error = new Error("Cargo is unavailable");
  assert.throws(runFixture(resultWith("", { status: null, error })), (actual) => actual === error);
});
