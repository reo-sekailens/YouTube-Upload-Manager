import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionSettings, YouTubePlaylist } from "../lib/types";
import { createPlaylistForChannel, hasPlaylistCreationPermission, PlaylistCreateDialog, PlaylistCreationReview, playlistCreationNeedsRefresh, playlistTitleError, PlaylistVisibilityChoices } from "./PlaylistCreateDialog";

const mocks = vi.hoisted(() => ({ create: vi.fn(), connection: vi.fn(), grant: vi.fn(), enter: vi.fn(), open: vi.fn() }));
vi.mock("../lib/local", () => ({ createYouTubePlaylist: mocks.create, loadConnectionSettings: mocks.connection, beginDeletionAuthorization: mocks.grant, enableDeletionSudoMode: mocks.enter }));
vi.mock("../lib/google-authorization", () => ({ openAndCopyGoogleAuthorization: mocks.open }));

const channel: ConnectionSettings = { connected: true, activeChannelId: "channel-a", deletionAuthorized: true, deletionSudoActive: true };
const playlist: YouTubePlaylist = { id: "playlist-a", title: "Event highlights" };

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("playlist creation review", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.connection.mockResolvedValue(channel);
    mocks.create.mockResolvedValue(playlist);
  });

  it("opens at the named first step with next navigation and no provider creation action", () => {
    const markup = renderToStaticMarkup(<PlaylistCreateDialog expectedChannelId="channel-a" onClose={() => undefined} onCreated={() => undefined} />);
    expect(markup).toContain("<dialog");
    expect(markup).toContain('aria-modal="true"');
    expect(markup).toContain("aria-labelledby=");
    expect(markup).toContain("aria-describedby=");
    expect(markup).toContain("Playlist name");
    expect(markup).toContain('required=""');
    expect(markup).toContain("Name your playlist");
    expect(markup).toContain('aria-label="Playlist creation steps"');
    expect(markup).toContain('aria-current="step"');
    expect(markup).toContain("Next: Visibility");
    expect(markup).not.toContain('type="radio"');
    expect(markup).not.toContain("Playlist details to create");
    expect(markup).not.toContain("maxLength=");
    expect(mocks.create).not.toHaveBeenCalled();
    expect(mocks.grant).not.toHaveBeenCalled();
    expect(mocks.enter).not.toHaveBeenCalled();
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("offers all privacy explanations with private selected in the visibility step", () => {
    const markup = renderToStaticMarkup(<PlaylistVisibilityChoices id="wizard" onChange={() => undefined} privacy="private" />);
    expect(markup).toMatch(/type="radio"[^>]*checked=""[^>]*value="private"/);
    expect(markup).toContain("Only you can view this playlist.");
    expect(markup).toContain("Anyone with the playlist link can view it.");
    expect(markup).toContain("Anyone can find and view this playlist.");
    expect(mocks.create).not.toHaveBeenCalled();
  });

  it("reviews the exact trimmed name, visibility and connected-channel destination safely", () => {
    const markup = renderToStaticMarkup(<PlaylistCreationReview privacy="unlisted" title="  <Event highlights>  " />);
    expect(markup).toContain('aria-label="Playlist details to create"');
    expect(markup).toContain("&lt;Event highlights&gt;");
    expect(markup).not.toContain("<Event highlights>");
    expect(markup).not.toContain("  &lt;Event highlights&gt;  ");
    expect(markup).toContain("Unlisted");
    expect(markup).toContain("Anyone with the playlist link can view it.");
    expect(markup).toContain("Destination");
    expect(markup).toContain("Connected channel");
    expect(mocks.create).not.toHaveBeenCalled();
  });

  it("shows the reviewed connected-channel name when available", () => {
    const markup = renderToStaticMarkup(<PlaylistCreationReview destinationName="Event channel" privacy="private" title="Highlights" />);
    expect(markup).toContain("Event channel");
    expect(markup).not.toContain("Connected channel");
    expect(markup).not.toContain("channel-a");
  });

  it("explains why a channel is needed before a playlist can be created", () => {
    const markup = renderToStaticMarkup(<PlaylistCreateDialog onClose={() => undefined} onCreated={() => undefined} />);
    expect(markup).toContain("Connect a YouTube channel before creating a playlist.");
    expect(markup).toMatch(/<button[^>]*disabled=""[^>]*type="submit"/);
  });

  it("requires a trimmed name and accepts the same 150 Unicode scalar limit as native creation", () => {
    expect(playlistTitleError(" \n\t ")).toBe("Enter a playlist name.");
    expect(playlistTitleError(`  ${"🎬".repeat(150)}  `)).toBeUndefined();
    expect(playlistTitleError("🎬".repeat(151))).toBe("Use 150 characters or fewer for the playlist name.");
    expect(playlistTitleError("a".repeat(150))).toBeUndefined();
    expect(playlistTitleError("a".repeat(151))).toBeDefined();
  });

  it("creates exactly the trimmed reviewed title, visibility and channel", async () => {
    expect(await createPlaylistForChannel("  Event highlights  ", "unlisted", "channel-a", () => true)).toEqual(playlist);
    expect(mocks.create).toHaveBeenCalledOnce();
    expect(mocks.create).toHaveBeenCalledWith("Event highlights", "unlisted", "channel-a");
    expect(mocks.connection).toHaveBeenCalledTimes(2);
    expect(mocks.grant).not.toHaveBeenCalled();
    expect(mocks.enter).not.toHaveBeenCalled();
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("requires both management consent and active temporary mode before any creation request", async () => {
    for (const flags of [
      { deletionAuthorized: false, deletionSudoActive: false },
      { deletionAuthorized: true, deletionSudoActive: false },
      { deletionAuthorized: false, deletionSudoActive: true },
    ]) {
      mocks.connection.mockResolvedValueOnce({ ...channel, ...flags });
      await expect(createPlaylistForChannel("Name", "private", "channel-a", () => true)).rejects.toThrow("Grant playlist-management permission and enter its temporary 15-minute mode before creating a playlist.");
    }
    expect(mocks.create).not.toHaveBeenCalled();
    expect(mocks.grant).not.toHaveBeenCalled();
    expect(mocks.enter).not.toHaveBeenCalled();
    expect(mocks.open).not.toHaveBeenCalled();
    expect(hasPlaylistCreationPermission(channel)).toBe(true);
    expect(hasPlaylistCreationPermission()).toBe(false);
  });

  it("rejects an invalid name before reading the connection or sending a request", async () => {
    await expect(createPlaylistForChannel(" ", "private", "channel-a", () => true)).rejects.toThrow("Enter a playlist name.");
    expect(mocks.create).not.toHaveBeenCalled();
    expect(mocks.connection).not.toHaveBeenCalled();
  });

  it("never creates a playlist for a disconnected, missing or different reviewed channel", async () => {
    await expect(createPlaylistForChannel("Name", "private", undefined, () => true)).rejects.toThrow("connected channel changed");
    mocks.connection.mockResolvedValueOnce({ connected: false, activeChannelId: "channel-a" });
    await expect(createPlaylistForChannel("Name", "private", "channel-a", () => true)).rejects.toThrow("connected channel changed");
    mocks.connection.mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(createPlaylistForChannel("Name", "private", "channel-a", () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.create).not.toHaveBeenCalled();
  });

  it("suppresses a stale request before sending native creation", async () => {
    expect(await createPlaylistForChannel("Name", "private", "channel-a", () => false)).toBeUndefined();
    expect(mocks.create).not.toHaveBeenCalled();
  });

  it("suppresses a late provider result after the dialog closes or changes channel", async () => {
    const pending = deferred<YouTubePlaylist>();
    mocks.create.mockReturnValue(pending.promise);
    let active = true;
    const request = createPlaylistForChannel("Name", "public", "channel-a", () => active);
    await vi.waitFor(() => expect(mocks.create).toHaveBeenCalledOnce());
    active = false;
    pending.resolve(playlist);
    expect(await request).toBeUndefined();
    expect(mocks.connection).toHaveBeenCalledOnce();
  });

  it("does not publish a returned playlist if the connected channel changed during creation", async () => {
    mocks.connection.mockResolvedValueOnce(channel).mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(createPlaylistForChannel("Name", "private", "channel-a", () => true)).rejects.toThrow("connected channel could not be rechecked");
    expect(mocks.create).toHaveBeenCalledOnce();
  });

  it("retains native error guidance for a request whose outcome is unknown", async () => {
    const message = "YouTube may have created the playlist, but the app could not confirm the result. Refresh playlists and check for it before creating again.";
    mocks.create.mockRejectedValue(message);
    await expect(createPlaylistForChannel("Name", "private", "channel-a", () => true)).rejects.toBe(message);
    expect(mocks.create).toHaveBeenCalledOnce();
    expect(mocks.connection).toHaveBeenCalledOnce();
  });

  it("requires checking playlists for exact uncertain outcomes while allowing retries after confirmed rejection", () => {
    expect(playlistCreationNeedsRefresh("YouTube may have created the playlist, but the app could not confirm the result. Refresh playlists and check for it before creating again.")).toBe(true);
    expect(playlistCreationNeedsRefresh("YouTube returned a playlist, but its channel or visibility did not match the review. Refresh playlists and check it before creating again.")).toBe(true);
    expect(playlistCreationNeedsRefresh("Playlist creation stopped unexpectedly. Refresh playlists and check for the reviewed name before creating again.")).toBe(true);
    expect(playlistCreationNeedsRefresh("YouTube returned a playlist, but the connected channel could not be rechecked. Refresh playlists and check it before creating again.")).toBe(true);
    expect(playlistCreationNeedsRefresh("YouTube playlist creation was denied. Reconnect and try again.")).toBe(false);
    expect(playlistCreationNeedsRefresh("YouTube rate limit reached. Wait and try again.")).toBe(false);
  });
});
