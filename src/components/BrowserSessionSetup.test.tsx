import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { BrowserSessionReview, BrowserSessionSetup, browserSessionDraftError } from "./BrowserSessionSetup";

describe("comparison browser session setup", () => {
  it("starts an optional stepped flow with an explicit Chrome profile and no credential fields", () => {
    const markup = renderToStaticMarkup(<BrowserSessionSetup expectedChannelId="channel-a" onClose={vi.fn()} onSaved={vi.fn()} />);
    expect(markup).toContain('aria-label="Set up browser session for comparisons"');
    expect(markup).toContain("Step 1 of 2");
    expect(markup).toContain("Choose your signed-in browser profile");
    expect(markup).toContain('value="chrome" selected=""');
    expect(markup).toContain('value="Default"');
    expect(markup).toContain("Browser profile name or folder");
    expect(markup).toContain("Choose profile folder");
    expect(markup).toContain("Review access");
    expect(markup).toContain("private videos that account can watch");
    expect(markup).not.toContain("<dialog");
    expect(markup).not.toContain('type="password"');
    expect(markup).not.toContain("textarea");
    expect(markup).not.toContain("Disable browser session");
    expect(markup).not.toContain("Enable and retry comparison");
  });

  it("retains the channel's saved profile and offers explicit disable", () => {
    const markup = renderToStaticMarkup(<BrowserSessionSetup expectedChannelId="channel-a" initialSession={{ browser: "edge", profile: "Profile 2" }} onClose={vi.fn()} onSaved={vi.fn()} />);
    expect(markup).toContain('value="edge" selected=""');
    expect(markup).toContain('value="Profile 2"');
    expect(markup).toContain("Enabled for this channel: Edge");
    expect(markup).toContain("Disable browser session");
  });

  it("requires an explicit Firefox profile rather than selecting its first discovered profile", () => {
    const markup = renderToStaticMarkup(<BrowserSessionSetup expectedChannelId="channel-a" initialSession={{ browser: "firefox", profile: "" }} onClose={vi.fn()} onSaved={vi.fn()} />);
    expect(markup).toContain("Firefox requires the exact profile name or its profile folder");
    expect(markup).toContain("about:profiles");
    expect(markup).toMatch(/disabled=""[^>]*>Review access/);
    expect(markup).not.toContain('value="Default"');
  });

  it("explains browser cookie access and requires separate unchecked approval", () => {
    const markup = renderToStaticMarkup(<BrowserSessionReview session={{ browser: "chrome", profile: "Profile 1" }} approved={false} busy={false} approvalId="approve-session" onApprovalChange={vi.fn()} />);
    expect(markup).toContain("Chrome");
    expect(markup).toContain("Profile 1");
    expect(markup).toContain("yt-dlp reads the selected browser session");
    expect(markup).toContain("may temporarily copy the browser cookie database");
    expect(markup).toContain("does not save exported cookies or reuse the app’s Google login tokens");
    expect(markup).toContain("Allow yt-dlp to read this browser session for comparisons on this channel");
    expect(markup).toContain('type="checkbox"');
    expect(markup).not.toContain('checked=""');
    expect(markup).toContain("An expired session or browser protection can still prevent access");
    expect(markup).toContain("YouTube embed remains available as fallback");
  });

  it("does not silently replace a saved less common browser", () => {
    const markup = renderToStaticMarkup(<BrowserSessionSetup expectedChannelId="channel-a" initialSession={{ browser: "vivaldi", profile: "Default" }} onClose={vi.fn()} onSaved={vi.fn()} />);
    expect(markup).toContain('value="vivaldi" selected=""');
    expect(markup).toContain("Enabled for this channel: Vivaldi");
  });
});

describe("browser profile input boundary", () => {
  it("accepts explicit browser profile names and Windows or POSIX folders", () => {
    for (const profile of ["Default", "Profile 2", "abcd.default-release", "C:\\Browser Profiles\\Profile 1", "/home/operator/.mozilla/firefox/abcd.default-release"]) expect(browserSessionDraftError(profile)).toBe("");
  });

  it("requires a profile and rejects option grammar or control characters", () => {
    expect(browserSessionDraftError("  ")).toContain("Choose a browser profile");
    expect(browserSessionDraftError("Default::container")).toContain("without control characters");
    expect(browserSessionDraftError("Profile\n2")).toContain("without control characters");
    expect(browserSessionDraftError("Default\u0000secret")).toContain("without control characters");
  });

  it("enforces the native byte limit rather than counting only characters", () => {
    expect(browserSessionDraftError("a".repeat(1024))).toBe("");
    expect(browserSessionDraftError("a".repeat(1025))).toContain("too long");
    expect(browserSessionDraftError("あ".repeat(342))).toContain("too long");
  });
});
