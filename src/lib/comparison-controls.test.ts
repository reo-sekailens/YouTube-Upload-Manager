import { describe, expect, it } from "vitest";
import { clampComparisonPosition, comparisonPositionMaximum, moveComparisonPosition } from "./comparison-controls";

describe("comparison playback positions", () => {
  it("keeps synchronized seek positions within the permitted range", () => {
    expect(moveComparisonPosition(12, -10)).toBe(2);
    expect(moveComparisonPosition(2, -10)).toBe(0);
    expect(moveComparisonPosition(comparisonPositionMaximum - 5, 10)).toBe(comparisonPositionMaximum);
    expect(clampComparisonPosition(42.5)).toBe(42.5);
  });
});
