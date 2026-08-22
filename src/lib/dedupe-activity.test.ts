import { describe, expect, it } from "vitest";
import { dedupeProgressLabel, dedupeProgressStep, dedupeProgressStepCount, maxDedupeActivityEntries, recordDedupeActivity } from "./dedupe-activity";

describe("recordDedupeActivity", () => {
  it("keeps the most recent activity entries in chronological order", () => {
    const entries = Array.from({ length: maxDedupeActivityEntries + 2 }, (_, id) => ({
      id,
      state: "running" as const,
      message: `Step ${id}`,
    })).reduce(recordDedupeActivity, []);

    expect(entries).toHaveLength(maxDedupeActivityEntries);
    expect(entries.map((entry) => entry.id)).toEqual([2, 3, 4, 5, 6, 7, 8, 9]);
  });
});

describe("dedupe progress", () => {
  it("represents only the three known command phases", () => {
    expect(dedupeProgressStep("syncing")).toBe(1);
    expect(dedupeProgressStep("rebuilding")).toBe(2);
    expect(dedupeProgressStep("complete")).toBe(dedupeProgressStepCount);
    expect(dedupeProgressLabel("rebuilding")).toContain("Step 2 of 3");
    expect(dedupeProgressLabel("error")).toContain("stopped");
  });
});
