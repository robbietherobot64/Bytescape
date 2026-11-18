-- Tracks whether an account has completed onboarding server-side. The frontend
-- gates the onboarding flow on this flag instead of browser localStorage so a
-- re-created account (or a fresh browser) always onboards correctly.
ALTER TABLE users ADD COLUMN IF NOT EXISTS onboarded boolean NOT NULL DEFAULT false;
