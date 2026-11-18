-- settings-account-ia: NSFW three-state + theme roster + feed prefs.
--
-- 1. users.nsfw_pref TEXT replaces users.show_nsfw BOOLEAN:
--      'exclude' (default, hidden entirely) | 'blur' (shown blurred + reveal) | 'show' (unflagged)
--    Backfill: false -> 'exclude', true -> 'show'. The boolean column is
--    dropped here (clean cutover; both server and frontend migrate together).
-- 2. users.theme: roster extension is a value-domain change only (free TEXT,
--    no constraint added); new valid values: retrofuturistic, light, dark
--    (1999 keeps its id with a redone palette). No data change needed.
-- 3. users.feed_prefs JSONB: saved default sort + source mix
--    ({"sort": "new", "source_mix": "..."}). NULL = use client defaults.

ALTER TABLE users ADD COLUMN IF NOT EXISTS nsfw_pref TEXT NOT NULL DEFAULT 'exclude';

UPDATE users SET nsfw_pref = CASE WHEN show_nsfw THEN 'show' ELSE 'exclude' END
WHERE nsfw_pref = 'exclude';

ALTER TABLE users DROP COLUMN IF EXISTS show_nsfw;

ALTER TABLE users ADD COLUMN IF NOT EXISTS feed_prefs JSONB;
