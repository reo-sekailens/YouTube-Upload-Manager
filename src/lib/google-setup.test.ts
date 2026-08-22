import { describe, expect, it } from "vitest";
import { googleSetupSteps, setupStepProgress } from "./google-setup";

describe("first-open Google setup guidance", () => {
  it("covers the required operator-owned configuration in order", () => {
    expect(googleSetupSteps.map((step) => step.title)).toEqual([
      "Use your Google account",
      "Create a Cloud project",
      "Enable YouTube Data API v3",
      "Configure Google Auth Platform",
      "Create a Desktop OAuth client",
      "Import the downloaded JSON",
    ]);
  });

  it("keeps progress within the guided flow", () => {
    expect(setupStepProgress(-4)).toBe(1);
    expect(setupStepProgress(2)).toBe(3);
    expect(setupStepProgress(999)).toBe(googleSetupSteps.length);
  });
});
