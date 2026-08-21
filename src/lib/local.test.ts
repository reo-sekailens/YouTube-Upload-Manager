import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("native OAuth client configuration", () => {
  beforeAll(() => {
    vi.stubGlobal("window", {});
  });

  afterAll(() => {
    vi.unstubAllGlobals();
  });

  beforeEach(() => {
    invoke.mockReset();
  });

  it("uses Tauri's camel-cased Rust command argument", async () => {
    const { saveOAuthClientId } = await import("./local");
    const clientId = "12345-example.apps.googleusercontent.com";

    await saveOAuthClientId(clientId);

    expect(invoke).toHaveBeenCalledWith("save_oauth_client_id", { oauthClientId: clientId });
  });
});
