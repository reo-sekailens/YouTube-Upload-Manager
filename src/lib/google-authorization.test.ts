import { describe, expect, it } from "vitest";
import { validateGoogleAuthorizationUrl } from "./google-authorization";

describe("Google authorization URLs", () => {
  it("accepts the native Google OAuth consent endpoint", () => {
    expect(validateGoogleAuthorizationUrl("https://accounts.google.com/o/oauth2/v2/auth?state=local")).toContain("accounts.google.com");
  });

  it("rejects non-Google or insecure URLs before opening or copying", () => {
    expect(() => validateGoogleAuthorizationUrl("http://accounts.google.com/o/oauth2/v2/auth")).toThrow();
    expect(() => validateGoogleAuthorizationUrl("https://example.test/oauth")).toThrow();
  });
});
