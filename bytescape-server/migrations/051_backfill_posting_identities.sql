BEGIN;
INSERT INTO posting_identities (user_id, protocol, identity_type)
SELECT id, 'atproto', 'bytescape' FROM users WHERE deleted_at IS NULL
ON CONFLICT DO NOTHING;
COMMIT;
