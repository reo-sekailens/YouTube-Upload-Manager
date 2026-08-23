import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import App, { prefetchBatchWorkspace } from "../App";
import type { StartupBootstrap } from "../lib/types";

const readyStartup: StartupBootstrap = {
  crashRecovery: { crashDetected: false },
  connection: { connected: false },
  snapshot: {
    revision: 0,
    items: [],
    duplicates: [],
    pendingTitleDuplicates: [],
  },
  readiness: {
    classificationComplete: true,
    safeShellRendered: true,
    deferredRecoveryState: "complete",
    queueActionsEnabled: true,
    detail: "Ready.",
  },
};

describe("workspace render isolation", () => {
  it("renders the Batch workspace without mounting any inactive tab panel", () => {
    const markup = renderToStaticMarkup(<App initialStartup={readyStartup} />);

    expect(markup).toContain('id="workspace-tab-batch"');
    expect(markup).toContain('aria-selected="true"');
    expect(markup.match(/role="tab"/g)).toHaveLength(8);
    expect(markup.match(/role="tabpanel"/g)).toHaveLength(1);
    for (const inactive of [
      "monitor",
      "dedupe",
      "rename",
      "deletion",
      "transfer",
      "account",
      "about",
    ]) {
      expect(markup).not.toContain(`id="workspace-tab-${inactive}"`);
    }
  });

  it("renders only the branded safe shell before native startup is known", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toContain("SAFE STARTUP");
    expect(markup).toContain("YouTube Upload Manager");
    expect(markup).toContain('data-performance-shell="holding"');
    expect(markup).not.toContain('role="tab"');
    expect(markup).not.toContain('role="tabpanel"');
    expect(markup).not.toContain("data-performance-batch-content");
  });

  it("does not expose a safe-shell marker from the crash Suspense fallback", () => {
    const markup = renderToStaticMarkup(
      <App
        initialStartup={{
          ...readyStartup,
          crashRecovery: { crashDetected: true },
        }}
      />,
    );

    expect(markup).toContain('role="status"');
    expect(markup).not.toContain("data-performance-shell");
    expect(markup).not.toContain("data-performance-batch-content");
  });

  it("can prefetch Batch code without mounting Batch DOM before readiness", async () => {
    await prefetchBatchWorkspace();

    const markup = renderToStaticMarkup(<App />);
    expect(markup).toContain("SAFE STARTUP");
    expect(markup).not.toContain('role="tabpanel"');
    expect(markup).not.toContain("data-performance-batch-content");
  });

  it("exposes the paint marker only with the resolved ready Batch content", async () => {
    await prefetchBatchWorkspace();

    const markup = renderToStaticMarkup(<App initialStartup={readyStartup} />);
    expect(markup).toContain('data-performance-batch-content="ready"');
    expect(markup.match(/data-performance-batch-content/g)).toHaveLength(1);
    expect(markup).toContain("Your upload queue");
    expect(markup).not.toContain("data-performance-shell");
  });
});
