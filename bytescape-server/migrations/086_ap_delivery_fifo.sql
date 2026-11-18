-- ap_delivery_queue FIFO ordering: the worker used to claim due rows ordered
-- by (next_attempt_at, id) where id = gen_random_uuid(), so rows inserted in
-- the same instant (e.g. a rapid Like → Undo) were claimed in random order —
-- an Undo could leapfrog its Like and get dropped as unresolvable upstream.
-- A monotonic BIGSERIAL restores insertion order within a due batch; retries
-- keep using next_attempt_at for their due time and simply come later in the
-- sequence (stable, never starving new rows for long).
ALTER TABLE ap_delivery_queue
    ADD COLUMN IF NOT EXISTS seq BIGSERIAL;

-- Backfill ordering ONLY for rows that were never sequenced (`seq IS NULL`).
-- The migration runner re-runs every migration at every boot (no version
-- table), and the old `AND q.seq IS DISTINCT FROM s.rn` guard re-ranked EVERY
-- row on each pass — with `row_number() OVER (ORDER BY created_at, id)`
-- tiebreaking same-created_at rows (the same-instant Like → Undo pair this
-- migration exists to protect) on the RANDOM `gen_random_uuid()` queue id, so
-- a boot could re-invert the pair and resurrect the pre-086 leapfrog bug.
-- The `IS NULL` guard restricts the backfill to never-sequenced rows, making
-- every re-run after the first a strict no-op (mirror of 087's guarded
-- idempotence).
UPDATE ap_delivery_queue q
SET seq = s.rn
FROM (
    SELECT id, row_number() OVER (ORDER BY created_at, id) AS rn
    FROM ap_delivery_queue
) s
WHERE q.id = s.id AND q.seq IS NULL;

-- Claim scan: due rows in insertion order.
CREATE INDEX IF NOT EXISTS idx_ap_delivery_queue_due_seq
    ON ap_delivery_queue (seq) WHERE status = 'pending';
