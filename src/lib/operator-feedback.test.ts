import { describe, expect, it } from "vitest";
import { operatorFailureMessage } from "./operator-feedback";

describe("operator failure feedback", () => {
  it("retains safe native string and Error failure details", () => {
    expect(operatorFailureMessage("Switch back to this item's reviewed channel.", "queue"))
      .toBe("Switch back to this item's reviewed channel.");
    expect(operatorFailureMessage(new Error("The source changed."), "import")).toBe("The source changed.");
  });

  it("uses actionable local recovery feedback for empty and unknown failures", () => {
    expect(operatorFailureMessage(new Error(""), "queue")).toContain("recovery evidence were retained");
    expect(operatorFailureMessage(undefined, "import")).toContain("Recovery requires the original files to remain available and unchanged");
    expect(operatorFailureMessage(" ", "library")).toContain("last complete local library was kept");
  });
});
