-- Fix: original_platform column was a custom enum type (post_platform) that 
-- caused sqlx decode errors. Change to text for compatibility.
ALTER TABLE posts ALTER COLUMN original_platform TYPE text USING original_platform::text;