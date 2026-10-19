import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ConnectionPanel, deliverReviewedProfileSettings, profileDialogScope, retainedChannelLimit } from "./ConnectionPanel";
import type { ConnectionSettings } from "../lib/types";

describe("account entry for saved channel profiles", () => {
  it("removes a previous project's projection while retaining independent channel pauses after JSON replacement", () => {
    expect(retainedChannelLimit({ kind: "api_quota", resumesAt: "old-project-retry" })).toEqual({ kind: "none" });
    expect(retainedChannelLimit({ kind: "api_quota", resumesAt: "old-project-retry", channelResumesAt: "channel-retry" })).toEqual({ kind: "channel_upload", resumesAt: "channel-retry" });
    expect(retainedChannelLimit({ kind: "channel_upload", resumesAt: "channel-retry" })).toEqual({ kind: "channel_upload", resumesAt: "channel-retry" });
  });
  it("exposes the profile entry on a disconnected device while preserving existing account setup", () => {
    const markup = renderToStaticMarkup(<ConnectionPanel />);
    for (const text of ["Saved channel profiles", "Connect YouTube", "Import Desktop OAuth JSON", "Refresh connection", "YouTube is not connected"]) expect(markup).toContain(text);
    expect(markup).not.toContain('aria-label="Channel profile steps"');
    expect(markup).not.toContain("Save channel profile");
  });
  it("delivers an explicitly reviewed switch to the new channel instead of rejecting its changed identity", () => {
    const accept = vi.fn();
    const switched: ConnectionSettings = { connected: true, activeChannelId: "channel-b", activeChannel: "Reviewed channel B" };
    expect(deliverReviewedProfileSettings(switched, () => true, accept)).toBe(true);
    expect(accept).toHaveBeenCalledExactlyOnceWith(switched);
  });
  it("delivers reviewed active-profile removal as disconnected while retaining parent ownership of the refresh", () => {
    const accept = vi.fn(); const removed: ConnectionSettings = { connected: false };
    expect(deliverReviewedProfileSettings(removed, () => true, accept)).toBe(true);
    expect(accept).toHaveBeenCalledExactlyOnceWith(removed);
  });
  it("normalizes native null channel fields so a disconnected device can open saved profiles", () => {
    const native = { connected: false, activeChannelId: null, activeChannel: null } as unknown as ConnectionSettings;
    expect(profileDialogScope(native)).toEqual({ channelId: undefined, channelName: undefined });
    expect(profileDialogScope({ connected: true, activeChannelId: "a", activeChannel: "A" })).toEqual({ channelId: "a", channelName: "A" });
  });
  it("discards a callback from a dismissed or externally changed profile dialog", () => {
    const accept = vi.fn();
    expect(deliverReviewedProfileSettings({ connected: true, activeChannelId: "channel-b" }, () => false, accept)).toBe(false);
    expect(accept).not.toHaveBeenCalled();
  });
});
