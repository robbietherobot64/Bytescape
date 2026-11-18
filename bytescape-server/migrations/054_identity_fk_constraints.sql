-- Migration 054: Finalize identity FK constraints (Sprint 5).
-- FK constraints already exist from prior migrations:
--   - linked_accounts.user_id → users(id) ON DELETE CASCADE (migration 002)
--   - posting_identities.user_id → users(id) ON DELETE CASCADE (migration 050)
--   - pds_accounts.user_id → users(id) ON DELETE CASCADE (migration 052)
--
-- This migration adds the missing unique constraint on pds_accounts.user_id
-- to enforce the 1:1 relationship (each user has at most one PDS account).

-- Step 1: Add unique constraint on pds_accounts.user_id (1:1 relationship)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'uq_pds_accounts_user_id'
    ) THEN
        ALTER TABLE pds_accounts ADD CONSTRAINT uq_pds_accounts_user_id
            UNIQUE (user_id);
    END IF;
END $$;

-- Step 2: Clean up posting_identities.protocol constraint
-- With AT-only (AP removed), the 'activitypub' protocol value is no longer valid.
-- Drop old CHECK constraint and add new one that only allows 'atproto'.
DO $$
BEGIN
    -- Drop old protocol CHECK constraint if it exists
    IF EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'posting_identities_protocol_check'
        AND conrelid = 'posting_identities'::regclass
    ) THEN
        ALTER TABLE posting_identities DROP CONSTRAINT posting_identities_protocol_check;
    END IF;

    -- Add new CHECK constraint that only allows 'atproto'
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_posting_identities_protocol_atproto'
        AND conrelid = 'posting_identities'::regclass
    ) THEN
        ALTER TABLE posting_identities ADD CONSTRAINT ck_posting_identities_protocol_atproto
            CHECK (protocol IN ('atproto'));
    END IF;
END $$;

-- Step 3: Remove any activitypub posting identities (they're dead code now)
DELETE FROM posting_identities WHERE protocol = 'activitypub';
