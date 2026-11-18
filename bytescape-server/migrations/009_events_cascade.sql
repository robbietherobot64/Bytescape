-- Fix events table FK to cascade on user deletion
ALTER TABLE events DROP CONSTRAINT IF EXISTS events_received_by_fkey;
ALTER TABLE events ADD CONSTRAINT events_received_by_fkey
    FOREIGN KEY (received_by) REFERENCES users(id) ON DELETE CASCADE;
