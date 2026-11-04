-- Bounded keyset pages over durable pending tombstones, including after a
-- verifier restart. Completed or unrelated hidden snapshots are excluded.
create index source_snapshots_pending_rejection_cleanup
  on source_snapshots (snapshot_hash collate "C")
  where hidden_at is not null
    and starts_with(hidden_reason, 'verification_rejected_pending_delete:');
