import type { PendingRejectedSnapshotPurge, RegistryStore } from "./store";

/** Bounded rotating pages prevent one failing object from starving later work.
 * The durable tombstone owns retry identity; restarting needs no local queue. */
export class RejectedSnapshotCleanup {
  private cursor = "";

  constructor(
    private readonly store: RegistryStore,
    private readonly deleteObject: (key: string) => Promise<void>,
    private readonly onFailure: (candidate: PendingRejectedSnapshotPurge, error: unknown) => void,
    private readonly batchSize = 20,
  ) {}

  async runBatch(): Promise<{ examined: number; completed: number }> {
    const candidates = await this.store.listPendingRejectedSnapshotPurges({ limit: this.batchSize, after_snapshot_hash: this.cursor });
    let completed = 0;
    for (const candidate of candidates) {
      try {
        if (await this.store.purgeRejectedSnapshot({ ...candidate, delete_object: this.deleteObject })) completed += 1;
      } catch (error) {
        this.onFailure(candidate, error);
      }
    }
    this.cursor = candidates.length === this.batchSize ? candidates[candidates.length - 1]!.snapshot_hash : "";
    return { examined: candidates.length, completed };
  }
}
