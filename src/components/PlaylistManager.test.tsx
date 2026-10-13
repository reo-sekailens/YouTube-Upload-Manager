import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import PlaylistManager from "./PlaylistManager";

describe("playlist workspace structure", () => {
  it("keeps destination, bulk selection and reviewed addition in distinct steps", () => {
    const markup = renderToStaticMarkup(<PlaylistManager activeChannel="Test channel" activeChannelId="channel-a" onNotice={() => undefined} />);
    expect(markup).toContain("1. Choose where to add videos");
    expect(markup).toContain("Destination playlist");
    expect(markup).toContain("2. Select videos to add");
    expect(markup).toContain("Search videos");
    expect(markup).toContain("Video visibility");
    expect(markup).toContain("0 of 200 selected");
    expect(markup).toContain("Select this page (0)");
    expect(markup).toContain("Select matching (0)");
    expect(markup).toContain("Show selected");
    expect(markup).toContain("Shift-click or Shift-Space");
    expect(markup).toContain("3. Review &amp; add");
    expect(markup).toContain("Videos already in the playlist are skipped");
    expect(markup).toContain('aria-controls="playlist-panel-manage"');
    expect(markup).toContain("Manage &amp; sort playlist");
    expect(markup).not.toContain("New playlist name");
    expect(markup).not.toContain("<dialog");
  });

  it("offers connection guidance before exposing playlist mutations", () => {
    const markup = renderToStaticMarkup(<PlaylistManager onNotice={() => undefined} />);
    expect(markup).toContain("Connect and sync a YouTube channel first");
    expect(markup).not.toContain("Create playlist");
  });
});
