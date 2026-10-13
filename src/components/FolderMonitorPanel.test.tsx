import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { FolderMonitorSettings } from "../lib/types";
import { FolderMonitorCards, FolderMonitorConfirmationDialog, FolderMonitorPanel, folderMonitorStatusLabel } from "./FolderMonitorPanel";

vi.mock("../lib/local", () => ({ isTauri: true, loadConnectionSettings: vi.fn(), listYouTubePlaylists: vi.fn() }));
vi.mock("../lib/folder-monitor", () => ({ disableFolderMonitor: vi.fn(), enableFolderMonitor: vi.fn(), loadFolderMonitors: vi.fn(), loadFolderMonitorOverview: vi.fn(), requeueCancelledFolderMonitorFiles: vi.fn(), reconcileFolderMonitorUploads: vi.fn(), deleteFolderMonitorUploadedSource: vi.fn(), deleteFolderMonitorUploadedSources: vi.fn(), removeFolderMonitor: vi.fn(), scanFolderMonitorNow: vi.fn() }));
vi.mock("../lib/state-events", () => ({ subscribeLocalStateChanges: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const monitors: FolderMonitorSettings[] = [
  { id: "folder-a", channelId: "channel-a", channelName: "Event channel", folderPath: "D:\\Event footage", enabled: true, visibility: "unlisted", madeForKids: false, playlistTitle: "Highlights", status: "watching", detail: "Watching" },
  { id: "folder-b", channelId: "channel-a", channelName: "Event channel", folderPath: "/home/videos/Backstage", enabled: false, visibility: "private", madeForKids: true, status: "disabled", detail: "Paused" },
];
const handlers = { onSelect: () => undefined, onToggle: () => undefined, onEdit: () => undefined, onRemove: () => undefined };

describe("multiple folder monitoring", () => {
  it("renders independent upload choices, states and actions for every folder", () => {
    const markup = renderToStaticMarkup(<FolderMonitorCards {...handlers} busy={false} monitors={monitors} selectedId="folder-a" />);
    expect(markup).toContain('aria-label="Watched folders"');
    expect(markup).toContain('aria-label="View activity for D:\\Event footage" aria-pressed="true"');
    expect(markup).toContain('aria-label="View activity for /home/videos/Backstage" aria-pressed="false"');
    expect(markup).toContain("Watching");
    expect(markup).toContain("Paused");
    expect(markup).toContain("Highlights");
    expect(markup).toContain("No playlist");
    expect(markup).toContain("Not made for kids");
    expect(markup).toContain("Made for kids");
    expect(markup).toContain('aria-label="Pause Event footage"');
    expect(markup).toContain('aria-label="Resume Backstage"');
    expect(markup).toContain('aria-label="Edit Event footage"');
    expect(markup).toContain('aria-label="Remove Backstage"');
    expect(markup).toContain("Viewing this folder’s activity");
  });

  it("disables every targeted card action while a mutation is pending", () => {
    const markup = renderToStaticMarkup(<FolderMonitorCards {...handlers} busy monitors={monitors} />);
    expect((markup.match(/disabled=""/g) || []).length).toBe(8);
  });

  it("does not expose a folder list or Add action for an unconnected channel", () => {
    const markup = renderToStaticMarkup(<FolderMonitorPanel onNotice={() => undefined} onQueueRefresh={async () => undefined} />);
    expect(markup).toContain("Connect a YouTube channel before adding watched folders.");
    expect(markup).not.toContain('aria-label="Watched folders"');
    expect(markup).toMatch(/<button[^>]*disabled=""[^>]*type="button"[^>]*>/);
    expect(markup).not.toContain("Monitor one local folder");
  });

  it("requires Folder and Review steps before offering folder removal", () => {
    const confirm = vi.fn();
    const markup = renderToStaticMarkup(<FolderMonitorConfirmationDialog monitor={monitors[0]} onClose={() => undefined} onConfirm={confirm} />);
    expect(markup).toContain("<dialog");
    expect(markup).toContain('aria-modal="true"');
    expect(markup).toContain('aria-label="Folder action steps"');
    expect(markup).toContain("Next: Review");
    expect(markup).toContain("D:\\Event footage");
    expect(markup).not.toContain(">Remove folder<");
    expect(confirm).not.toHaveBeenCalled();
  });

  it("gives each monitor its own readable status regardless of another folder’s status", () => {
    expect(folderMonitorStatusLabel(monitors[0])).toBe("Watching");
    expect(folderMonitorStatusLabel(monitors[1])).toBe("Paused");
    expect(folderMonitorStatusLabel({ ...monitors[0], status: "error" })).toBe("Needs attention");
    expect(folderMonitorStatusLabel({ ...monitors[0], status: "scanning" })).toBe("Scanning");
  });
});
