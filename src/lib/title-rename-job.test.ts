import { describe, expect, it } from "vitest";
import {
  getTitleRenameJobSnapshot,
  getScopedTitleRenameJobSnapshot,
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
      "channel-a",
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
      "channel-a",
    )).resolves.toBe(false);
    expect(getTitleRenameJobSnapshot().activity).toMatchObject([
      { status: "failed", detail: "YouTube did not confirm the requested title for video one." },
    ]);
  });

  it("hides a running and completed log from another same-named channel while retaining the job lock", async () => {
    let release!: () => void;
    const wait = new Promise<void>((resolve) => { release = resolve; });
    const run = startTitleRenameJob(
      [{ videoId: "private-a", previousTitle: "Private A", title: "Edited A" }],
      async () => wait,
      () => undefined,
      "channel-a",
    );
    expect(getScopedTitleRenameJobSnapshot("channel-b")).toEqual({ activity: [], applying: true });
    expect(getScopedTitleRenameJobSnapshot("channel-a").activity[0].previousTitle).toBe("Private A");
    await expect(startTitleRenameJob([], async () => undefined, () => undefined, "channel-b")).rejects.toThrow("already running");
    release();
    await run;
    expect(getScopedTitleRenameJobSnapshot("channel-b")).toEqual({ activity: [], applying: false });
    expect(getScopedTitleRenameJobSnapshot()).toEqual({ activity: [], applying: false });
  });

  it("leaves later changes pending when a channel guard rejects the next request", async () => {
    let channel = "channel-a";
    const invoked: string[] = [];
    await expect(startTitleRenameJob(
      [
        { videoId: "one", previousTitle: "Before one", title: "After one" },
        { videoId: "two", previousTitle: "Before two", title: "After two" },
        { videoId: "three", previousTitle: "Before three", title: "After three" },
      ],
      async ([change]) => {
        if (channel !== "channel-a") throw new Error("The connected channel changed.");
        invoked.push(change.videoId);
        channel = "channel-b";
      },
      () => undefined,
      "channel-a",
    )).resolves.toBe(false);
    expect(invoked).toEqual(["one"]);
    expect(getScopedTitleRenameJobSnapshot("channel-a").activity.map((item) => item.status)).toEqual(["completed", "failed", "pending"]);
    expect(getScopedTitleRenameJobSnapshot("channel-b").activity).toEqual([]);
  });
});
