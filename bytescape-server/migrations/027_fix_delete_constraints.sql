-- migration name: fix_delete_cascades_and_constraints
-- purpose: fix FK constraints that block account deletion
--   1. posts.reply_parent_id: RESTRICT -> SET NULL (orphaned replies become top-level)
--   2. posts.import_session_id: RESTRICT -> SET NULL (posts survive session deletion)

-- Fix reply_parent_id: when a parent post is deleted (via user cascade),
-- child replies from OTHER users should not block the deletion.
-- SET NULL makes orphaned replies become top-level posts.
ALTER TABLE posts DROP CONSTRAINT IF EXISTS posts_reply_parent_id_fkey;
ALTER TABLE posts ADD CONSTRAINT posts_reply_parent_id_fkey
    FOREIGN KEY (reply_parent_id) REFERENCES posts(id) ON DELETE SET NULL;

-- Fix import_session_id: when an import session is deleted (via user cascade),
-- the posts themselves should survive with import_session_id set to NULL.
ALTER TABLE posts DROP CONSTRAINT IF EXISTS posts_import_session_id_fkey;
ALTER TABLE posts ADD CONSTRAINT posts_import_session_id_fkey
    FOREIGN KEY (import_session_id) REFERENCES import_sessions(id) ON DELETE SET NULL;