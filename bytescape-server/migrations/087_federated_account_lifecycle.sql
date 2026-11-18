-- add-identity-realignment: federated artifacts must not outlive their account.
--
-- 1) ap_activities gains an owner column + ON DELETE CASCADE so a deleted
--    account can never orphan its federated activity rows (the tables carried
--    no owner reference at all; transient orphan rows were observed during the
--    2026-11-14 incident — the test DB was re-verified 2026-11-14 with zero).
--    Backfilled
--    from the owning user's issued actor id; rows whose actor no longer maps to
--    a live user (pre-existing orphans) keep user_id NULL and are left alone —
--    their cleanup is owned by the parallel quick-fix workstream (OQ2).
--    ap_delivery_queue already cascades with its activity row
--    (ap_delivery_queue_activity_id_fkey), so it is covered transitively.
--    Retirement staging rows (below) deliberately carry user_id NULL so they
--    outlive the account for the delivery drain window.
ALTER TABLE ap_activities ADD COLUMN IF NOT EXISTS user_id uuid;

UPDATE ap_activities a
SET user_id = u.id
FROM users u
WHERE a.user_id IS NULL AND a.actor_id = u.ap_actor_id;

-- Guarded with the repo's idempotent-constraint pattern (see 054): the
-- migration runner re-runs every migration at boot, so a plain ADD CONSTRAINT
-- panics the server on every restart after the first.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ap_activities_user_id_fkey'
          AND conrelid = 'ap_activities'::regclass
    ) THEN
        ALTER TABLE ap_activities
            ADD CONSTRAINT ap_activities_user_id_fkey
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_ap_activities_user ON ap_activities (user_id) WHERE user_id IS NOT NULL;

-- 2) Retirement staging: when an account dies (or a superseded minted actor is
--    retired), its interaction retractions and the actor Delete fan-out must
--    OUTLIVE the user row long enough to be delivered. Remote signature
--    verification fetches the signing actor's public key from its actor
--    document, so the retiring actor keeps serving its (public) key material
--    until the drain completes, then this row is dropped and the actor id
--    answers with a Tombstone. private_key_pem is the same encrypted-at-rest
--    form users.ap_private_key_pem uses (chacha20poly1305, JWT_SECRET-derived)
--    and exists only for the drain window.
--    delete_inboxes is the frozen Delete fan-out set, captured while followers
--    and delivery history still exist (both die with the account).
CREATE TABLE IF NOT EXISTS retired_ap_actors (
    actor_id            text PRIMARY KEY,
    user_id             uuid,            -- audit only, deliberately NO foreign key
    username            text NOT NULL DEFAULT '',
    public_key_pem      text,
    private_key_pem     text,
    delete_inboxes      jsonb NOT NULL DEFAULT '[]'::jsonb,
    delete_enqueued_at  timestamptz,
    retired_at          timestamptz NOT NULL DEFAULT now(),
    drained_at          timestamptz
);
