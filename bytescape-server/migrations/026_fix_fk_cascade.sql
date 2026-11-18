-- migration name: fix_fk_cascade
-- purpose: add ON DELETE CASCADE to media.user_id and events.received_by
-- these were missing CASCADE, causing 500 errors on account deletion

-- Drop and recreate media.user_id FK with CASCADE
ALTER TABLE media DROP CONSTRAINT IF EXISTS media_user_id_fkey;
ALTER TABLE media ADD CONSTRAINT media_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

-- Drop and recreate events.received_by FK with CASCADE
ALTER TABLE events DROP CONSTRAINT IF EXISTS events_received_by_fkey;
ALTER TABLE events ADD CONSTRAINT events_received_by_fkey
    FOREIGN KEY (received_by) REFERENCES users(id) ON DELETE CASCADE;
