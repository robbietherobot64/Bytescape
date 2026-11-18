-- Migration 038: Add RSA key columns to ap_actors for HTTP Signatures compatibility.
-- Mastodon and most AP implementations use RSA-PKCS#1-v1.5 for HTTP Signatures.
-- Bytescape needs RSA keys alongside existing Ed25519 keys.
-- NOTE: ap_actors table was dropped in migration 053 (AP federation removed).
-- This migration is kept for schema history but is now a no-op.

DO $$
BEGIN
    IF EXISTS (SELECT FROM pg_tables WHERE tablename = 'ap_actors') THEN
        ALTER TABLE ap_actors ADD COLUMN IF NOT EXISTS rsa_private_key TEXT;
        ALTER TABLE ap_actors ADD COLUMN IF NOT EXISTS rsa_public_key_pem TEXT;
    END IF;
END $$;
