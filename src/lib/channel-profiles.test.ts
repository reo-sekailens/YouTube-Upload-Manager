import { beforeEach, describe, expect, it, vi } from "vitest";
import { beginChannelProfileAuthorization, cancelChannelProfileReview, commitChannelProfile, loadChannelProfileReview, loadChannelProfiles, prepareChannelProfile, profileWorkspaceMatches, removeChannelProfile, switchChannelProfile, verifiedProfileReview, type ChannelProfileWorkspace } from "./channel-profiles";
const mocks = vi.hoisted(() => ({ desktop: vi.fn(() => true), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: mocks.desktop, invoke: mocks.invoke }));
describe("protected channel profile bridge", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.desktop.mockReturnValue(true); });
  it("passes explicit current channel fences and exact immutable profile identities", async () => {
    await loadChannelProfiles("channel-a"); await switchChannelProfile("profile-b", "channel-a", true); await removeChannelProfile("profile-a", "channel-a", true);
    expect(mocks.invoke).toHaveBeenNthCalledWith(1, "load_channel_profiles", { expectedChannelId: "channel-a" });
    expect(mocks.invoke).toHaveBeenNthCalledWith(2, "switch_channel_profile", { profileId: "profile-b", expectedChannelId: "channel-a", approval: true });
    expect(mocks.invoke).toHaveBeenNthCalledWith(3, "remove_channel_profile", { profileId: "profile-a", expectedChannelId: "channel-a", approval: true });
  });
  it("stages and verifies OAuth before a separate approval commit, including disconnected devices", async () => {
    await prepareChannelProfile("D:/account.json"); await beginChannelProfileAuthorization("review-a"); await loadChannelProfileReview("review-a"); await commitChannelProfile("review-a", undefined, true); await cancelChannelProfileReview("review-a");
    expect(mocks.invoke.mock.calls).toEqual([
      ["prepare_channel_profile", { path: "D:/account.json", expectedChannelId: null }],
      ["begin_channel_profile_authorization", { reviewId: "review-a", expectedChannelId: null }],
      ["load_channel_profile_review", { reviewId: "review-a" }],
      ["commit_channel_profile", { reviewId: "review-a", expectedChannelId: null, approval: true }],
      ["cancel_channel_profile_review", { reviewId: "review-a" }],
    ]);
  });
  it("does not allow browser-only privilege calls", () => {
    mocks.desktop.mockReturnValue(false);
    for (const call of [() => loadChannelProfiles(), () => switchChannelProfile("p", undefined, true), () => removeChannelProfile("p", undefined, true), () => prepareChannelProfile("x"), () => beginChannelProfileAuthorization("r"), () => loadChannelProfileReview("r"), () => commitChannelProfile("r", undefined, true), () => cancelChannelProfileReview("r")]) expect(call).toThrow("desktop app");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
  it("rejects profile lists with stale active channels, duplicate identities or wrong selection", () => {
    const workspace: ChannelProfileWorkspace = { activeChannelId: "a", canSwitch: true, profiles: [{ profileId: "p", channelId: "a", channelName: "A", selected: true, removed: false, managementAuthorized: false }] };
    expect(profileWorkspaceMatches(workspace, "a")).toBe(true);
    expect(profileWorkspaceMatches(workspace, "b")).toBe(false);
    expect(profileWorkspaceMatches({ ...workspace, profiles: [...workspace.profiles, workspace.profiles[0]] }, "a")).toBe(false);
    expect(profileWorkspaceMatches({ ...workspace, profiles: [{ ...workspace.profiles[0], selected: false }] }, "a")).toBe(false);
    expect(profileWorkspaceMatches({ activeChannelId: null, canSwitch: true, profiles: [] })).toBe(true);
  });
  it("requires official verification state and channel identity before review approval", () => {
    expect(verifiedProfileReview({ reviewId: "r", state: "verified", channelId: "a", channelName: "A" })).toBe(true);
    expect(verifiedProfileReview({ reviewId: "r", state: "authorizing", channelId: "a", channelName: "A" })).toBe(false);
    expect(verifiedProfileReview({ reviewId: "r", state: "verified", channelName: "A" })).toBe(false);
  });
});
