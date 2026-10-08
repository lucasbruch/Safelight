import { describe, expect, it } from "vitest";
import type { Ai, Item } from "../types";
import { activeExtraCount, applyFilters, emptyFilters, filterOptions } from "./filters";

function item(id: number, over: Partial<Item> = {}, ai?: Partial<Ai>): Item {
  return {
    id,
    relPath: `2026-05-01/Cam/IMG_${id}.CR3`,
    kind: "photo",
    fileName: `IMG_${id}.CR3`,
    size: 1,
    capturedAt: "2026-05-01T10:00:00.000",
    camera: "Canon EOS R5",
    meta: {},
    rating: 0,
    flag: 0,
    movedToRejected: false,
    previewState: 1,
    aiState: 1,
    ai: ai ? { reasons: [], tags: [], people: [], ...ai } : null,
    video: null,
    tags: [],
    ...over,
  };
}

const ids = (items: Item[]) => items.map((i) => i.id);

describe("applyFilters", () => {
  it("hides moved rejects everywhere except under Rejects", () => {
    const items = [item(1), item(2, { flag: -1 }), item(3, { flag: -1, movedToRejected: true })];
    expect(ids(applyFilters(items, emptyFilters))).toEqual([1, 2]);
    expect(ids(applyFilters(items, { ...emptyFilters, flag: "rejects" }))).toEqual([2, 3]);
  });

  it("filters by flag and minimum stars", () => {
    const items = [item(1, { flag: 1, rating: 2 }), item(2, { rating: 4 }), item(3, { flag: 1, rating: 5 })];
    expect(ids(applyFilters(items, { ...emptyFilters, flag: "picks" }))).toEqual([1, 3]);
    expect(ids(applyFilters(items, { ...emptyFilters, flag: "unflagged" }))).toEqual([2]);
    expect(ids(applyFilters(items, { ...emptyFilters, minRating: 4 }))).toEqual([2, 3]);
  });

  it("matches camera, day, lens and flash", () => {
    const items = [
      item(1, { meta: { lens: "RF 50mm", flash: true } }),
      item(2, { camera: "Sony ILCE-7M4", capturedAt: "2026-05-02T09:00:00.000", meta: { flash: false } }),
      item(3, { capturedAt: null }),
    ];
    expect(ids(applyFilters(items, { ...emptyFilters, cameras: ["Sony ILCE-7M4"] }))).toEqual([2]);
    expect(ids(applyFilters(items, { ...emptyFilters, dates: ["2026-05-01", "Unknown"] }))).toEqual([1, 3]);
    expect(ids(applyFilters(items, { ...emptyFilters, lenses: ["RF 50mm"] }))).toEqual([1]);
    expect(ids(applyFilters(items, { ...emptyFilters, flash: "yes" }))).toEqual([1]);
    // "No flash" means the camera said so; unknown isn't the same as off.
    expect(ids(applyFilters(items, { ...emptyFilters, flash: "no" }))).toEqual([2]);
  });

  it("ORs the AI flags and needs AI results to match", () => {
    const items = [
      item(1, {}, { blurLevel: 2 }),
      item(2, {}, { eyesClosed: 1 }),
      item(3, {}, { overExposed: 0.01, underExposed: 0.1 }),
      item(4),
    ];
    expect(ids(applyFilters(items, { ...emptyFilters, ai: ["blurry", "eyes"] }))).toEqual([1, 2]);
    expect(ids(applyFilters(items, { ...emptyFilters, ai: ["exposure"] }))).toEqual([]);
  });

  it("matches your tags and the AI's tags, and people", () => {
    const items = [item(1, { tags: ["bride"] }), item(2, {}, { tags: ["bride"], people: [7] }), item(3, {}, { people: [8] })];
    expect(ids(applyFilters(items, { ...emptyFilters, tags: ["bride"] }))).toEqual([1, 2]);
    expect(ids(applyFilters(items, { ...emptyFilters, people: [7, 8] }))).toEqual([2, 3]);
  });

  it("collapses a burst to its best shot, or its first before the AI has chosen", () => {
    const items = [
      item(1, {}, { burstId: 1 }),
      item(2, {}, { burstId: 1, burstBest: true }),
      item(3, {}, { burstId: 2 }),
      item(4, {}, { burstId: 2 }),
      item(5),
    ];
    expect(ids(applyFilters(items, { ...emptyFilters, collapseBursts: true }))).toEqual([2, 3, 5]);
  });
});

describe("filterOptions", () => {
  it("lists what's in the project, with the AI's tags by frequency and not repeating yours", () => {
    const items = [
      item(1, { tags: ["groom", "Bride"], meta: { lens: "RF 50mm" } }, { tags: ["dress", "cake"] }),
      item(2, { kind: "video", meta: { flash: true } }, { tags: ["cake", "groom"], people: [3, 1] }),
    ];
    const o = filterOptions(items);
    expect(o.userTags).toEqual(["Bride", "groom"]);
    expect(o.tags).toEqual(["cake", "dress"]);
    expect(o.people).toEqual([1, 3]);
    expect(o.lenses).toEqual(["RF 50mm"]);
    expect(o.hasVideo && o.hasFlash).toBe(true);
  });
});

describe("activeExtraCount", () => {
  it("counts every non-default extra filter", () => {
    expect(activeExtraCount(emptyFilters)).toBe(0);
    expect(activeExtraCount({ ...emptyFilters, minRating: 3, cameras: ["A", "B"], flash: "no", people: [1] })).toBe(5);
  });
});
