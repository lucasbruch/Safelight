/**
 * Sends saves to the backend one at a time, in the order they were made: two
 * quick keys on one photo must land in that order in the database and its XMP.
 */
export function createSaveQueue() {
  let tail: Promise<unknown> = Promise.resolve();
  // Saves still on their way, per photo id.
  const queued = new Map<number, number>();

  const serial = <T,>(call: () => Promise<T>): Promise<T> => {
    const run = tail.then(call);
    tail = run.catch(() => {});
    return run;
  };

  return {
    serial,
    /** Resolves once every save queued so far has finished. */
    settled: () => tail.then(() => {}),
    /**
     * Queues a save of `ids` and resolves to the stored items it returns, minus
     * photos with a later save still queued: that older reply would flip a newer
     * key press back on screen, and the later save's reply covers them anyway.
     */
    async save<T extends { id: number }>(ids: number[], call: () => Promise<T[]>): Promise<T[]> {
      for (const id of ids) queued.set(id, (queued.get(id) ?? 0) + 1);
      let stored: T[];
      try {
        stored = await serial(call);
      } finally {
        for (const id of ids) {
          const n = (queued.get(id) ?? 1) - 1;
          if (n > 0) queued.set(id, n);
          else queued.delete(id);
        }
      }
      return stored.filter((i) => !queued.has(i.id));
    },
  };
}
