import { describe, expect, it } from "vitest";
import { createSaveQueue } from "./saveQueue";

/** A backend call that finishes only when the test says so. */
function deferred<T>() {
  let resolve!: (v: T) => void, reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => ((resolve = res), (reject = rej)));
  return { promise, resolve, reject };
}

describe("createSaveQueue", () => {
  it("starts each save only after the one before has finished", async () => {
    const q = createSaveQueue();
    const log: string[] = [];
    const first = deferred<{ id: number }[]>();
    const a = q.save([1], () => (log.push("pick starts"), first.promise));
    const b = q.save([1], async () => (log.push("reject starts"), [{ id: 1 }]));
    await Promise.resolve();
    expect(log).toEqual(["pick starts"]);
    first.resolve([{ id: 1 }]);
    await Promise.all([a, b]);
    expect(log).toEqual(["pick starts", "reject starts"]);
  });

  it("drops an older reply for a photo that has a newer save queued", async () => {
    const q = createSaveQueue();
    const a = q.save([1, 2], async () => [{ id: 1, flag: 1 }, { id: 2, flag: 1 }]);
    const b = q.save([1], async () => [{ id: 1, flag: -1 }]);
    expect(await a).toEqual([{ id: 2, flag: 1 }]);
    expect(await b).toEqual([{ id: 1, flag: -1 }]);
  });

  it("keeps going after a failed save, and settles after everything queued", async () => {
    const q = createSaveQueue();
    const done: number[] = [];
    const bad = q.save([1], async () => Promise.reject(new Error("disk full")));
    const good = q.save([1], async () => (done.push(1), [{ id: 1 }]));
    const move = q.serial(async () => done.push(2));
    await expect(bad).rejects.toThrow("disk full");
    expect(await good).toEqual([{ id: 1 }]);
    await q.settled();
    expect(done).toEqual([1, 2]);
    await move;
  });
});
