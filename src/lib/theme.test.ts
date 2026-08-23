import { describe, expect, it } from "vitest";
import { resolveTheme } from "./theme";

describe("resolveTheme", () => {
  it("keeps an explicit device-local appearance preference", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
  });

  it("uses the system preference when no valid choice was saved", () => {
    expect(resolveTheme(null, true)).toBe("dark");
    expect(resolveTheme("unexpected", false)).toBe("light");
  });
});
