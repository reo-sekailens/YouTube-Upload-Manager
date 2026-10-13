import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { PublicationCalendarDialog, PublicationTimeCard } from "./PublicationCalendarDialog";
import type { PublicationRow } from "../lib/publication-schedule";
vi.mock("../lib/local", () => ({ loadConnectionSettings: vi.fn(), beginDeletionAuthorization: vi.fn(), enableDeletionSudoMode: vi.fn() }));
const row: PublicationRow = { index: 0, localTime: "2026-11-01T01:30", state: "ambiguous", preserved: false, candidates: [{ choice: "earlier", localTime: "2026-11-01T01:30:00-04:00", publishAt: "2026-11-01T05:30:00+00:00" }, { choice: "later", localTime: "2026-11-01T01:30:00-05:00", publishAt: "2026-11-01T06:30:00+00:00" }], detail: "Choose the earlier or later UTC instant explicitly." };
const noAction = () => undefined;
describe("stepped publication calendar", () => {
  it("opens with three steps, separate private upload and provider confirmation", () => {
    const markup = renderToStaticMarkup(<PublicationCalendarDialog expectedChannelId="channel-a" paths={["D:\\clip.mp4"]} onClose={noAction} />);
    for (const text of ["Start &amp; timezone", "Per-video times", "Review", 'aria-modal="true"', "Close publication calendar", "Upload privately", "Google must confirm the exact schedule", "Next: Per-video times"]) expect(markup).toContain(text);
    expect(markup).not.toContain("Apply private publication plan");
  });
  it("discloses both repeated wall-clock instants with accessible explicit radio choices", () => {
    const markup = renderToStaticMarkup(<PublicationTimeCard id="time" row={row} title="Video <original>" editable onTime={noAction} onChoice={noAction} onRestore={noAction} />);
    for (const text of ['id="time-local"', 'name="time-choice"', 'value="2026-11-01T01:30"', "earlier occurrence", "later occurrence", "05:30:00 UTC", "06:30:00 UTC", "-04:00", "-05:00", "Video &lt;original&gt;"]) expect(markup).toContain(text);
    expect(markup).not.toContain("checked=");
  });
  it("holds DST gaps for correction and displays a retained UTC instant without editable controls in review", () => {
    const markup = renderToStaticMarkup(<PublicationTimeCard id="time" row={{ ...row, state: "gap", candidates: [], detail: "This local time does not exist; choose another time." }} title="Clip" originalTime="2026-11-01T06:30:00+00:00" editable={false} onTime={noAction} onChoice={noAction} onRestore={noAction} />);
    expect(markup).toContain('role="alert"'); expect(markup).toContain("does not exist"); expect(markup).toContain("Previously approved: 2026-11-01 06:30:00 UTC"); expect(markup).not.toContain("datetime-local");
  });
  it("allows deliberate local changes to restore the previously approved instant", () => {
    const markup = renderToStaticMarkup(<PublicationTimeCard id="time" row={{ ...row, state: "ready", publishAt: row.candidates[1].publishAt }} title="Clip" originalTime={row.candidates[0].publishAt} editable onTime={noAction} onChoice={noAction} onRestore={noAction} />);
    expect(markup).toContain("Keep the approved instant"); expect(markup).toContain("Persisted UTC instant"); expect(markup).toContain("checked=");
  });
});
