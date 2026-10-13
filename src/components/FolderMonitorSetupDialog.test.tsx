import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FolderMonitorSettings } from "../lib/types";
import { FolderMonitorApprovalReview, FolderMonitorSetupDialog, saveFolderMonitorForChannel, type FolderMonitorChoices } from "./FolderMonitorSetupDialog";

const mocks = vi.hoisted(() => ({ connection: vi.fn(), enable: vi.fn(), playlists: vi.fn() }));
vi.mock("../lib/local", () => ({ loadConnectionSettings: mocks.connection, listYouTubePlaylists: mocks.playlists }));
vi.mock("../lib/folder-monitor", () => ({ enableFolderMonitor: mocks.enable }));
const choices: FolderMonitorChoices = { folderPath: "D:\\Event videos", visibility: "unlisted", madeForKids: true, deleteSourceAfterUpload: true, playlistId: "playlist-a", playlistTitle: "Highlights" };
const saved: FolderMonitorSettings = { id: "monitor-a", channelId: "channel-a", channelName: "Event channel", enabled: true, folderPath: choices.folderPath, visibility: choices.visibility, madeForKids: choices.madeForKids, deleteSourceAfterUpload: choices.deleteSourceAfterUpload, playlistId: choices.playlistId, playlistTitle: choices.playlistTitle, status: "watching", detail: "Watching this folder." };

describe("folder monitor setup", () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.connection.mockResolvedValue({ connected: true, activeChannelId: "channel-a" }); mocks.enable.mockResolvedValue(saved); });

  it("opens at Folder without enabling monitoring or reading provider playlists", () => {
    const markup = renderToStaticMarkup(<FolderMonitorSetupDialog activeChannel="Event channel" expectedChannelId="channel-a" onClose={() => undefined} onSaved={() => undefined} />);
    expect(markup).toContain("<dialog");
    expect(markup).toContain('aria-modal="true"');
    expect(markup).toContain('aria-label="Folder monitoring steps"');
    expect(markup).toContain('aria-current="step"');
    expect(markup).toContain("Next: Upload choices");
    expect(markup).toContain("Existing and newly added supported direct-child videos upload after two unchanged scans.");
    expect(markup).toContain("Choose folder");
    expect(markup).not.toContain("baseline");
    expect(markup).not.toContain("Folder monitoring details");
    expect(mocks.enable).not.toHaveBeenCalled();
    expect(mocks.playlists).not.toHaveBeenCalled();
  });

  it("keeps the original folder fixed when editing its choices", () => {
    const markup = renderToStaticMarkup(<FolderMonitorSetupDialog expectedChannelId="channel-a" monitor={{ ...saved, enabled: false }} onClose={() => undefined} onSaved={() => undefined} />);
    expect(markup).toContain("Edit watched folder");
    expect(markup).toContain("D:\\Event videos");
    expect(markup).not.toContain("Choose another folder");
    expect(markup).not.toContain('type="text"');
    expect(mocks.enable).not.toHaveBeenCalled();
  });

  it("reviews the exact folder, destination and all automatic upload choices safely", () => {
    const markup = renderToStaticMarkup(<FolderMonitorApprovalReview channelName="<Event channel>" choices={choices} />);
    expect(markup).toContain('aria-label="Folder monitoring details"');
    expect(markup).toContain("D:\\Event videos");
    expect(markup).toContain("&lt;Event channel&gt;");
    expect(markup).toContain("unlisted");
    expect(markup).toContain("Made for kids");
    expect(markup).toContain("Highlights");
    expect(markup).toContain("Delete automatically only after YouTube processing succeeds");
    expect(markup).not.toContain("<Event channel>");
  });

  it("states No playlist and Keep original files when those options are selected", () => {
    const markup = renderToStaticMarkup(<FolderMonitorApprovalReview choices={{ folderPath: "/videos", visibility: "private", madeForKids: false, deleteSourceAfterUpload: false }} />);
    expect(markup).toContain("No playlist");
    expect(markup).toContain("Keep original files");
    expect(markup).toContain("Not made for kids");
  });

  it("requires a chosen folder and recurring approval before any privileged read or mutation", async () => {
    await expect(saveFolderMonitorForChannel({ ...choices, folderPath: " " }, undefined, "channel-a", true, () => true)).rejects.toThrow("Choose a local folder.");
    await expect(saveFolderMonitorForChannel(choices, undefined, "channel-a", false, () => true)).rejects.toThrow("Approve recurring automatic uploads");
    expect(mocks.connection).not.toHaveBeenCalled();
    expect(mocks.enable).not.toHaveBeenCalled();
  });

  it("adds a folder with every reviewed choice and immutable channel scope", async () => {
    expect(await saveFolderMonitorForChannel(choices, undefined, "channel-a", true, () => true)).toEqual(saved);
    expect(mocks.enable).toHaveBeenCalledExactlyOnceWith(choices.folderPath, "unlisted", true, true, "playlist-a", "Highlights", undefined, "channel-a");
  });

  it("edits only the specified folder and retains its channel scope", async () => {
    await saveFolderMonitorForChannel(choices, "monitor-a", "channel-a", true, () => true);
    expect(mocks.enable).toHaveBeenCalledExactlyOnceWith(choices.folderPath, "unlisted", true, true, "playlist-a", "Highlights", "monitor-a", "channel-a");
  });

  it("stops before enabling after a disconnected or changed channel", async () => {
    for (const connection of [{ connected: false, activeChannelId: "channel-a" }, { connected: true, activeChannelId: "channel-b" }]) {
      mocks.connection.mockResolvedValueOnce(connection);
      await expect(saveFolderMonitorForChannel(choices, undefined, "channel-a", true, () => true)).rejects.toThrow("connected channel changed");
    }
    expect(mocks.enable).not.toHaveBeenCalled();
  });

  it("suppresses a stale request before the folder can be enabled", async () => {
    expect(await saveFolderMonitorForChannel(choices, undefined, "channel-a", true, () => false)).toBeUndefined();
    expect(mocks.enable).not.toHaveBeenCalled();
  });

  it("suppresses a late result after the dialog closes or changes channel", async () => {
    let resolve!: (value: FolderMonitorSettings) => void;
    mocks.enable.mockReturnValue(new Promise<FolderMonitorSettings>((done) => { resolve = done; }));
    let current = true;
    const request = saveFolderMonitorForChannel(choices, undefined, "channel-a", true, () => current);
    await vi.waitFor(() => expect(mocks.enable).toHaveBeenCalledOnce());
    current = false;
    resolve(saved);
    expect(await request).toBeUndefined();
  });

  it("never accepts a receipt without a folder identity or for another channel", async () => {
    for (const outcome of [{ ...saved, channelId: "channel-b" }, { ...saved, id: undefined }]) {
      mocks.enable.mockResolvedValueOnce(outcome);
      await expect(saveFolderMonitorForChannel(choices, undefined, "channel-a", true, () => true)).rejects.toThrow("did not match the reviewed channel");
    }
  });
});
