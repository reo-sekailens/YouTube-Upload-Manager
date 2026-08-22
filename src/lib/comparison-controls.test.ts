import { describe, expect, it } from "vitest";
import { clampComparisonPosition, comparisonPositionMaximum, moveComparisonPosition, sharedComparisonDuration, shouldLoadComparisonPlayers, youtubeComparisonEmbedUrl, youtubeComparisonOrigin } from "./comparison-controls";

describe("comparison playback positions", () => {
  it("uses the signed-in YouTube embed origin and safely encodes a video identifier", () => {
    expect(youtubeComparisonOrigin).toBe("https://www.youtube.com");
    expect(youtubeComparisonEmbedUrl("video id/with?")).toBe("https://www.youtube.com/embed/video%20id%2Fwith%3F?enablejsapi=1&playsinline=1&rel=0");
  });

  it("does not mount comparison frames until the operator presses Play", () => {
    expect(shouldLoadComparisonPlayers(false, false)).toBe(false);
    expect(shouldLoadComparisonPlayers(true, false)).toBe(true);
    expect(shouldLoadComparisonPlayers(false, true)).toBe(true);
  });

  it("keeps synchronized seek positions within the permitted range", () => {
    expect(moveComparisonPosition(12, -10)).toBe(2);
    expect(moveComparisonPosition(2, -10)).toBe(0);
    expect(moveComparisonPosition(comparisonPositionMaximum - 5, 10)).toBe(comparisonPositionMaximum);
    expect(clampComparisonPosition(42.5)).toBe(42.5);
  });

  it("uses the shorter reported player duration for shared seeks", () => {
    expect(sharedComparisonDuration([120, 95])).toBe(95);
    expect(sharedComparisonDuration([0, 95])).toBe(95);
    expect(moveComparisonPosition(90, 10, 95)).toBe(95);
    expect(clampComparisonPosition(96, 95)).toBe(95);
  });
});
