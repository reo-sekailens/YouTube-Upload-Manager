import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { authorizeProfileForScope, changeReviewedProfile, ChannelProfileIdentity, ChannelProfileList, ChannelProfilesDialog, loadProfilesForScope, prepareProfileForScope, readProfileVerification, saveProfileForScope } from "./ChannelProfilesDialog";
import type { ChannelProfileReview, ChannelProfileWorkspace } from "../lib/channel-profiles";
const mocks = vi.hoisted(() => ({ connection: vi.fn(), load: vi.fn(), prepare: vi.fn(), authorize: vi.fn(), read: vi.fn(), commit: vi.fn(), cancel: vi.fn(), switch: vi.fn(), remove: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection }));
vi.mock("../lib/channel-profiles", async (load) => ({ ...await load<typeof import("../lib/channel-profiles")>(), loadChannelProfiles: mocks.load, prepareChannelProfile: mocks.prepare, beginChannelProfileAuthorization: mocks.authorize, loadChannelProfileReview: mocks.read, commitChannelProfile: mocks.commit, cancelChannelProfileReview: mocks.cancel, switchChannelProfile: mocks.switch, removeChannelProfile: mocks.remove }));
const workspace: ChannelProfileWorkspace = { activeChannelId: "a", canSwitch: true, profiles: [{ profileId: "pa", channelId: "a", channelName: "Channel A", projectId: "project-a", selected: true, removed: false, managementAuthorized: false }, { profileId: "pb", channelId: "b", channelName: "Channel B", selected: false, removed: false, managementAuthorized: true }] };
const ready: ChannelProfileReview = { reviewId: "r", state: "ready", projectId: "project-a" };
const verified: ChannelProfileReview = { ...ready, state: "verified", channelId: "c", channelName: "Channel C" };
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((done) => { resolve = done; }); return { promise, resolve }; }
describe("stepped channel profile management", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "a" }); mocks.load.mockResolvedValue(workspace); mocks.prepare.mockResolvedValue(ready); mocks.authorize.mockResolvedValue({ authorizationUrl: "https://accounts.google.com/o/oauth2/v2/auth", attemptId: "oauth-r" }); mocks.read.mockResolvedValue(verified); mocks.commit.mockResolvedValue({ ...workspace, profiles: [...workspace.profiles, { profileId: "pc", channelId: "c", channelName: "Channel C", selected: false, managementAuthorized: false, removed: false }] }); mocks.cancel.mockResolvedValue({ connected: true, activeChannelId: "a" }); mocks.switch.mockResolvedValue({ connected: true, activeChannelId: "b" }); mocks.remove.mockResolvedValue({ connected: false }); });
  it("starts with safe device profiles and makes no OAuth or mutation during rendering", () => {
    const markup = renderToStaticMarkup(<ChannelProfilesDialog expectedChannelId="a" onChanged={() => undefined} onClose={() => undefined} />);
    for (const text of ["Saved YouTube channels", 'aria-label="Channel profile steps"', "Choose profile", "Review", "Add account", "Only the selected channel dispatches"]) expect(markup).toContain(text);
    expect(mocks.authorize).not.toHaveBeenCalled(); expect(mocks.commit).not.toHaveBeenCalled(); expect(mocks.switch).not.toHaveBeenCalled();
  });
  it("completes normal Add → Google verify → Review save without switching the channel", async () => {
    expect(await loadProfilesForScope("a", () => true)).toEqual(workspace);
    expect(await prepareProfileForScope("D:/account.json", "a", () => true)).toEqual(ready);
    expect(await authorizeProfileForScope(ready, "a", () => true)).toMatchObject({ attemptId: "oauth-r" });
    expect(await readProfileVerification("r", "a", () => true)).toEqual(verified);
    const saved = await saveProfileForScope(verified, "a", true, () => true);
    expect(saved?.activeChannelId).toBe("a"); expect(saved?.profiles.some((profile) => profile.channelId === "c")).toBe(true);
    expect(mocks.commit).toHaveBeenCalledWith("r", "a", true); expect(mocks.switch).not.toHaveBeenCalled();
  });
  it("blocks unverified and unapproved commits", async () => {
    await expect(saveProfileForScope(ready, "a", true, () => true)).rejects.toThrow("Verify the channel");
    await expect(saveProfileForScope(verified, "a", false, () => true)).rejects.toThrow("approve");
    expect(mocks.commit).not.toHaveBeenCalled();
  });
  it("rejects an untrusted consent URL and releases the staged native attempt", async () => {
    mocks.authorize.mockResolvedValue({ authorizationUrl: "https://example.test/oauth", attemptId: "oauth-r" });
    await expect(authorizeProfileForScope(ready, "a", () => true)).rejects.toThrow("accounts.google.com");
    expect(mocks.cancel).toHaveBeenCalledWith("r");
  });
  it("switches and removes only the exact reviewed profile with approval", async () => {
    expect(await changeReviewedProfile(workspace, workspace.profiles[1], "switch", "a", true, () => true)).toMatchObject({ activeChannelId: "b" });
    expect(mocks.switch).toHaveBeenCalledWith("pb", "a", true);
    expect(await changeReviewedProfile(workspace, workspace.profiles[0], "remove", "a", true, () => true)).toMatchObject({ connected: false });
    expect(mocks.remove).toHaveBeenCalledWith("pa", "a", true);
  });
  it("blocks active transfers, absent profiles, unchanged switching and missing approval", async () => {
    for (const [saved, target, action, approval] of [[{ ...workspace, canSwitch: false }, workspace.profiles[1], "switch", true], [workspace, { ...workspace.profiles[1], profileId: "foreign" }, "switch", true], [workspace, workspace.profiles[0], "switch", true], [workspace, workspace.profiles[1], "remove", false]] as const)
      await expect(changeReviewedProfile(saved, target, action, "a", approval, () => true)).rejects.toThrow("approve its review");
    expect(mocks.switch).not.toHaveBeenCalled(); expect(mocks.remove).not.toHaveBeenCalled();
  });
  it("fences stale channels before preparation, verification and review writes", async () => {
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "b" });
    for (const operation of [loadProfilesForScope("a", () => true), prepareProfileForScope("x", "a", () => true), readProfileVerification("r", "a", () => true), saveProfileForScope(verified, "a", true, () => true), changeReviewedProfile(workspace, workspace.profiles[1], "switch", "a", true, () => true)]) await expect(operation).rejects.toThrow("selected channel changed");
    for (const mock of [mocks.load, mocks.prepare, mocks.read, mocks.commit, mocks.switch]) expect(mock).not.toHaveBeenCalled();
  });
  it("releases a late preparation instead of retaining an orphan review", async () => {
    let current = true; const native = deferred<ChannelProfileReview>(); mocks.prepare.mockReturnValue(native.promise);
    const operation = prepareProfileForScope("x", "a", () => current); await Promise.resolve(); await Promise.resolve(); current = false; native.resolve(ready);
    expect(await operation).toBeUndefined(); expect(mocks.cancel).toHaveBeenCalledWith("r");
  });
  it("ignores late saves and rejects wrong native verification/profile identities", async () => {
    mocks.read.mockResolvedValue({ ...verified, reviewId: "foreign" }); await expect(readProfileVerification("r", "a", () => true)).rejects.toThrow("did not match");
    mocks.commit.mockResolvedValue(workspace); await expect(saveProfileForScope(verified, "a", true, () => true)).rejects.toThrow("not confirmed");
    mocks.switch.mockResolvedValue({ connected: true, activeChannelId: "foreign" }); await expect(changeReviewedProfile(workspace, workspace.profiles[1], "switch", "a", true, () => true)).rejects.toThrow("not confirmed");
    let current = true; const native = deferred<ChannelProfileWorkspace>(); mocks.commit.mockReturnValue(native.promise); const operation = saveProfileForScope(verified, "a", true, () => current); await Promise.resolve(); await Promise.resolve(); current = false; native.resolve(workspace); expect(await operation).toBeUndefined();
  });
  it("supports a disconnected scope without silently binding to a saved channel", async () => {
    mocks.connection.mockResolvedValue({ connected: false }); const saved = { ...workspace, activeChannelId: null, profiles: workspace.profiles.map((profile) => ({ ...profile, selected: false })) }; mocks.load.mockResolvedValue(saved);
    expect(await loadProfilesForScope(undefined, () => true)).toEqual(saved); expect(mocks.load).toHaveBeenCalledWith(undefined);
  });
  it("paginates profile lists, escapes supplied names and communicates quota and permission scope", () => {
    const many = { ...workspace, profiles: Array.from({ length: 41 }, (_, index) => ({ ...workspace.profiles[1], profileId: `p${index}`, channelId: `c${index}`, channelName: `<Channel ${index}>` })) };
    const markup = renderToStaticMarkup(<ChannelProfileList workspace={many} page={2} busy={false} onPage={() => undefined} onSelect={() => undefined} />);
    expect(markup).toContain("&lt;Channel 20&gt;"); expect(markup).not.toContain("&lt;Channel 0&gt;"); expect(markup).not.toContain("&lt;Channel 40&gt;"); expect(markup).toContain("Page 2 of 3"); expect(markup).toContain("Separately authorized for this profile");
    expect(renderToStaticMarkup(<ChannelProfileIdentity channelName="<Name>" channelId="a" />)).toContain("Unknown project identity");
  });
});
