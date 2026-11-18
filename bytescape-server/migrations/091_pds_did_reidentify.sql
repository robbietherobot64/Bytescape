-- Migration 091: did:web -> did:plc re-identity claim registry.
--
-- Backs the one-shot gated runner `pds::reidentify::supersede_did_web`
-- (REIDENTIFY_DID_WEB, same posture as RETIRE_MASTODON_MINTED_ACTORS): every
-- pre-realignment minted `did:web` AT identity is re-identified onto a per-user
-- `did:plc`, because a `did:web` identity is invisible to the real Atmosphere
-- (the AppView drops its events — bluesky-social/atproto#4379) and both
-- production bridges ship per-user `did:plc`.
--
-- One row per superseded identity. `new_did` + `signing_key_private` are
-- written BEFORE the public PLC mint (stage-before-mint: the key and its
-- deterministic derived DID are persisted first, so a crash at ANY point —
-- including between the directory accepting the genesis op and any later
-- write — resumes with the same identity rather than burn a second one).
-- `signing_key_private` holds the `crate::secret` AEAD envelope BYTES
-- (`enc:v1:…`, same pattern as retired_ap_actors' key material; column stays
-- bytea so existing installs migrate with no type change) and is cleared the
-- moment the migration finalizes (review #2 R2-5: no second key copy survives
-- the drain window). `new_did` is retained at done as the audit record of
-- which public identity was minted. `user_id` is audit-only, deliberately
-- without a foreign key (same as retired_ap_actors). `state` machine:
-- pending | applying | done | failed (a terminal failure never re-mints;
-- stale applying rows are re-enlisted after a lease window — review #2
-- R2-10).
CREATE TABLE IF NOT EXISTS pds_did_reidentify (
    old_did             text PRIMARY KEY,
    new_did             text,
    signing_key_private bytea,  -- crate::secret envelope bytes (enc:v1:…); NULL at done
    user_id             uuid,
    username            text NOT NULL DEFAULT '',
    state               text NOT NULL DEFAULT 'pending',  -- pending | applying | done | failed
    started_at          timestamp with time zone,
    finished_at          timestamp with time zone,
    created_at          timestamp with time zone NOT NULL DEFAULT now()
);

-- R2-5 residue sweep (idempotent, re-run every boot like every migration):
-- releases the staged key copy of already-finalized migrations — including
-- the pre-hardening cohort whose keys were staged in plaintext.
UPDATE pds_did_reidentify
SET signing_key_private = NULL
WHERE state = 'done' AND signing_key_private IS NOT NULL;
