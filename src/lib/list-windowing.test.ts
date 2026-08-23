import { describe, expect, it } from "vitest";
import { DEFAULT_DATA_WINDOW_SIZE, windowItems } from "./list-windowing";

describe("windowItems", () => {
  it("keeps a 10,000-record surface below 100 mounted records", () => {
    const fixture = Array.from({ length: 10_000 }, (_, index) => index);
    const result = windowItems(fixture, 1);

    expect(result.items).toHaveLength(DEFAULT_DATA_WINDOW_SIZE);
    expect(result.items.length).toBeLessThan(100);
    expect(result.total).toBe(10_000);
    expect(result.pageCount).toBe(209);
  });

  it("clamps stale pages after a search narrows the result set", () => {
    const result = windowItems(["match"], 99, 24);

    expect(result.page).toBe(1);
    expect(result.items).toEqual(["match"]);
    expect(result.start).toBe(0);
    expect(result.end).toBe(1);
  });

  it("uses stable, non-overlapping page boundaries", () => {
    const fixture = Array.from({ length: 120 }, (_, index) => index);
    const first = windowItems(fixture, 1);
    const second = windowItems(fixture, 2);

    expect(first.items.at(-1)).toBe(47);
    expect(second.items.at(0)).toBe(48);
    expect(new Set([...first.items, ...second.items])).toHaveLength(96);
  });
});
