-- 055: restore posts columns/types the v1 code depends on.
--
-- The hand-rolled migration runner in db/mod.rs skips a block of files
-- (032-035, 037, 042-049), so two posts-table changes that the v1 code
-- depends on never ran against this database:
--
--   * migration 032 — `reshared_post_id` (native reshares). Symptom: every
--     archive-import insert failed with `column "reshared_post_id" of
--     relation "posts" does not exist` (import_sessions landed `failed`,
--     0 posts imported).
--   * migration 043 — `original_platform` must be TEXT (PostRow maps it as
--     String); the column is still the `post_platform` enum, so decoding
--     the RETURNING row of every insert failed with `mismatched types`.
--
-- Plus the event-shaped columns PostRow maps via `SELECT *`
-- (start_time / end_time / location).

ALTER TABLE posts ADD COLUMN IF NOT EXISTS reshared_post_id UUID REFERENCES posts(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_posts_reshared_post_id ON posts(reshared_post_id);

ALTER TABLE posts ADD COLUMN IF NOT EXISTS start_time TIMESTAMPTZ;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS end_time TIMESTAMPTZ;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS location TEXT;

ALTER TABLE posts ALTER COLUMN original_platform TYPE text USING original_platform::text;
ALTER TABLE posts ALTER COLUMN original_platform SET DEFAULT 'bytescape';

DROP INDEX IF EXISTS idx_posts_user_platform_ts;
CREATE INDEX IF NOT EXISTS idx_posts_user_platform_ts ON posts(user_id, original_platform, original_ts DESC);
