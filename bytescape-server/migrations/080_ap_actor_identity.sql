-- ActivityPub actor identity (Phase A of add-activitypub-integration).
--
-- Every Bytescape account becomes a dual-protocol citizen: an AT Protocol
-- identity (did:web / did:plc, Ed25519) AND an ActivityPub actor (RSA, for
-- HTTP-signatures), cross-linked via `alsoKnownAs`. These columns are ADDITIVE
-- and nullable — existing accounts are untouched until an AP actor is minted
-- for them (reclaim / issuance behind AP_ACTORS_ENABLED).
--
-- The AP actor needs its own RSA keypair; the existing users.public_key /
-- encrypted_privkey are Ed25519 (AT Protocol) and must NOT be reused for AP
-- signing. We store the AP RSA private key encrypted (same chacha20poly1305
-- wrapping convention as users.encrypted_privkey).

ALTER TABLE users ADD COLUMN IF NOT EXISTS ap_actor_id text;        -- canonical AP actor URL, e.g. https://bytescape.social/@alice
ALTER TABLE users ADD COLUMN IF NOT EXISTS ap_inbox text;           -- AP inbox URL (delivery target)
ALTER TABLE users ADD COLUMN IF NOT EXISTS ap_outbox text;          -- AP outbox URL
ALTER TABLE users ADD COLUMN IF NOT EXISTS ap_private_key_pem text; -- encrypted RSA private key (PEM, wrapped)
ALTER TABLE users ADD COLUMN IF NOT EXISTS ap_public_key_pem text;  -- RSA public key (PEM) for the actor doc
ALTER TABLE users ADD COLUMN IF NOT EXISTS ap_enabled boolean NOT NULL DEFAULT false; -- actor issued & served

-- Reclaim index: fast lookup of an actor by its canonical id (WebFinger,
-- actor-doc fetch, alias reconciliation). Partial index keeps the table lean
-- (most rows have no AP actor yet).
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_ap_actor_id ON users (ap_actor_id) WHERE ap_actor_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_ap_enabled ON users (username) WHERE ap_enabled;
