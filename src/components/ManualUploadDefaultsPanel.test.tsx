import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionSettings } from "../lib/types";
import { loadDefaultPlaylistsForChannel, ManualUploadDefaultsPanel } from "./ManualUploadDefaultsPanel";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), playlists: vi.fn(), defaults: vi.fn(), save: vi.fn() }));
vi.mock("../lib/local", () => ({ isTauri: true, loadConnectionSettings: mocks.connection, listYouTubePlaylists: mocks.playlists, loadManualUploadDefaults: mocks.defaults, saveManualUploadDefaults: mocks.save }));
vi.mock("../lib/manual-upload-defaults", () => ({ loadManualUploadDefaults: mocks.defaults, saveManualUploadDefaults: mocks.save }));

const channel: ConnectionSettings = { connected: true, activeChannelId: "channel-a" };

describe("manual upload defaults", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.connection.mockResolvedValue(channel);
    mocks.playlists.mockResolvedValue([{ id: "playlist-a", title: "Event highlights" }]);
  });

  it("shows three labeled defaults and retains private and no-playlist fallbacks while loading", () => {
    const markup = renderToStaticMarkup(<ManualUploadDefaultsPanel activeChannel="Event channel" activeChannelId="channel-a" />);
    expect(markup).toContain("Upload defaults");
    expect(markup).toContain("Made for Kids");
    expect(markup).toContain("Default visibility");
    expect(markup).toContain("Default playlist");
    expect(markup).toContain('value="private" selected=""');
    expect(markup).toContain('value="unlisted"');
    expect(markup).toContain('value="public"');
    expect(markup).toContain("No playlist");
    expect(markup).toContain("For every channel on this device.");
    expect(markup).toContain("For Event channel on this device.");
    expect(markup).toContain("You can change these choices for each batch.");
    expect(markup).toContain("Loading saved upload defaults…");
    expect(markup.match(/<select[^>]*disabled=""/g)).toHaveLength(3);
    expect(mocks.save).not.toHaveBeenCalled();
  });

  it("explains the playlist's channel scope when disconnected", () => {
    const markup = renderToStaticMarkup(<ManualUploadDefaultsPanel />);
    expect(markup).toContain("Connect a YouTube channel to choose its playlist default.");
    expect(markup).not.toContain("Refresh playlists");
    expect(markup).not.toContain("Loading playlists…");
  });

  it("reads playlists only after checking and rechecking the connected channel", async () => {
    const playlists = await loadDefaultPlaylistsForChannel("channel-a", () => true);
    expect(playlists).toEqual([{ id: "playlist-a", title: "Event highlights" }]);
    expect(mocks.connection).toHaveBeenCalledTimes(2);
    expect(mocks.playlists).toHaveBeenCalledTimes(1);
  });

  it("blocks playlist reads for a different connected channel", async () => {
    mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-b" });
    await expect(loadDefaultPlaylistsForChannel("channel-a", () => true)).rejects.toThrow("connected channel changed");
    expect(mocks.playlists).not.toHaveBeenCalled();
  });

  it("discards stale reads before contacting YouTube", async () => {
    expect(await loadDefaultPlaylistsForChannel("channel-a", () => false)).toBeUndefined();
    expect(mocks.playlists).not.toHaveBeenCalled();
  });

  it("discards playlists returned after the request became stale", async () => {
    let current = true;
    mocks.playlists.mockImplementation(async () => { current = false; return [{ id: "playlist-a", title: "Event highlights" }]; });
    expect(await loadDefaultPlaylistsForChannel("channel-a", () => current)).toBeUndefined();
    expect(mocks.connection).toHaveBeenCalledTimes(1);
  });

  it("rejects a channel switch discovered after the playlist response", async () => {
    mocks.connection.mockResolvedValueOnce(channel).mockResolvedValueOnce({ connected: true, activeChannelId: "channel-b" });
    await expect(loadDefaultPlaylistsForChannel("channel-a", () => true)).rejects.toThrow("connected channel changed");
  });
});
