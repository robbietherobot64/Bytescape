-- Add custom_profile_fields to users table for editable widget header field visibility
ALTER TABLE users ADD COLUMN IF NOT EXISTS custom_profile_fields JSONB NOT NULL DEFAULT '[]';
