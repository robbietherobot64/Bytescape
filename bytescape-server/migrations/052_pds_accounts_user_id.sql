-- Add user_id foreign key to pds_accounts table.
-- Previously, pds_accounts had no user_id column — all lookups were by string match on handle.
-- This migration adds the FK constraint so lookups can use user_id directly.

-- Step 1: Add user_id column (nullable initially)
ALTER TABLE pds_accounts ADD COLUMN IF NOT EXISTS user_id UUID;

-- Step 2: Backfill by matching handle = username
UPDATE pds_accounts p
SET user_id = u.id
FROM users u
WHERE p.handle = u.username
  AND p.user_id IS NULL;

-- Step 3: Delete orphan accounts that have no matching user
-- (excluding the special '_instance' system account which has no user)
DELETE FROM pds_accounts WHERE user_id IS NULL AND handle != '_instance';

-- Step 4: Add CHECK constraint — all non-instance accounts must have user_id
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'ck_pds_accounts_user_id'
    ) THEN
        ALTER TABLE pds_accounts ADD CONSTRAINT ck_pds_accounts_user_id
            CHECK (user_id IS NOT NULL OR handle = '_instance');
    END IF;
END $$;

-- Step 5: Add FK constraint (only applies to non-NULL user_id values)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fk_pds_accounts_user'
    ) THEN
        ALTER TABLE pds_accounts ADD CONSTRAINT fk_pds_accounts_user
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
    END IF;
END $$;

-- Step 6: Create index for lookups
CREATE INDEX IF NOT EXISTS idx_pds_accounts_user_id ON pds_accounts(user_id);
