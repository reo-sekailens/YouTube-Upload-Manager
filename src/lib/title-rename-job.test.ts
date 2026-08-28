import { describe, expect, it } from "vitest";
import {
  getTitleRenameJobSnapshot,
  startTitleRenameJob,
  subscribeTitleRenameJob,
} from "./title-rename-job";

describe("title rename jobs", () => {
  it("keeps a running progress log available to a later subscriber", async () => {
    let releaseFirst!: () => void;
    const firstRequest = new Promise<void>((resolve) => { releaseFirst = resolve; });
    const run = startTitleRenameJob(
      [
        { videoId: "one", previousTitle: "Before one", title: "After one" },
        { videoId: "two", previousTitle: "Before two", title: "After two" },
      ],
      async ([change]) => {
        if (change.videoId === "one") await firstRequest;
      },
      () => undefined,
    );

    expect(getTitleRenameJobSnapshot()).toMatchObject({
      applying: true,
      activity: [
        { videoId: "one", status: "running" },
        { videoId: "two", status: "pending" },
      ],
    });
    let observed = 0;
    const unsubscribe = subscribeTitleRenameJob(() => { observed += 1; });
    releaseFirst();
    await expect(run).resolves.toBe(true);
    unsubscribe();
    expect(observed).toBeGreaterThan(0);
    expect(getTitleRenameJobSnapshot()).toMatchObject({
      applying: false,
      activity: [
        { videoId: "one", status: "completed" },
        { videoId: "two", status: "completed" },
      ],
    });
  });

  it("keeps an actionable native string rejection in the activity log", async () => {
    await expect(startTitleRenameJob(
      [{ videoId: "one", previousTitle: "Before", title: "After" }],
      async () => Promise.reject("YouTube did not confirm the requested title for video one."),
      () => undefined,
    )).resolves.toBe(false);
    expect(getTitleRenameJobSnapshot().activity).toMatchObject([
      { status: "failed", detail: "YouTube did not confirm the requested title for video one." },
    ]);
  });
});
