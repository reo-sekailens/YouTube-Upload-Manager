import { describe, expect, it } from "vitest";
import { assertWorkspaceChannel, createWorkspaceRequestFence, workspaceErrorMessage } from "./workspace-requests";

describe("workspace request isolation", () => {
  it("rejects a late old-channel response even after switching back to the same channel", async () => {
    const fence = createWorkspaceRequestFence("channel-a");
    let resolve!: (value: string) => void;
    const oldRead = new Promise<string>((done) => { resolve = done; });
    const accept = fence.begin();
    let displayed = "new channel";
    const pending = oldRead.then((value) => { if (accept()) displayed = value; });
    fence.setScope("channel-b");
    fence.setScope("channel-a");
    resolve("old account library");
    await pending;
    expect(displayed).toBe("new channel");
    expect(fence.begin()()).toBe(true);
  });

  it("keeps a mutation result when an earlier connection read settles after disconnect", async () => {
    const fence = createWorkspaceRequestFence("device-connection");
    const acceptOldRead = fence.begin();
    let displayed = { connected: true };
    fence.invalidate();
    displayed = { connected: false };
    await Promise.resolve().then(() => { if (acceptOldRead()) displayed = { connected: true }; });
    expect(displayed.connected).toBe(false);
  });

  it("rejects an obsolete same-channel refresh and a completion after unmount", () => {
    const fence = createWorkspaceRequestFence("channel-a");
    const first = fence.begin();
    const second = fence.begin();
    expect(first()).toBe(false);
    expect(second()).toBe(true);
    fence.invalidate();
    expect(second()).toBe(false);
  });

  it("requires the immutable connected channel before invoking provider work", () => {
    expect(() => assertWorkspaceChannel({ connected: true, activeChannel: "Same name", activeChannelId: "b" }, "a")).toThrow("connected channel changed");
    expect(() => assertWorkspaceChannel({ connected: false, activeChannelId: "a" }, "a")).toThrow();
    expect(() => assertWorkspaceChannel({ connected: true, activeChannelId: "a" }, undefined)).toThrow();
    expect(() => assertWorkspaceChannel({ connected: true, activeChannelId: "a" }, "a")).not.toThrow();
  });

  it("preserves the native failure detail shown for a rejected picker or operation", () => {
    expect(workspaceErrorMessage("The selected archive belongs to a different channel.", "Transfer failed")).toBe("The selected archive belongs to a different channel.");
    expect(workspaceErrorMessage({ message: "Picker access was denied." }, "Transfer failed")).toBe("Picker access was denied.");
    expect(workspaceErrorMessage(null, "Transfer failed")).toBe("Transfer failed");
  });
});
