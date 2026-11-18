-- Migration 041: OAuth server tables

-- PAR requests (short-lived, 60s)
CREATE TABLE IF NOT EXISTS oauth_par_requests (
    request_uri TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    response_type TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    code_challenge_method TEXT NOT NULL,
    state TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    scope TEXT NOT NULL,
    login_hint TEXT,
    dpop_jkt TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

-- Auth codes (short-lived, 10 min)
CREATE TABLE IF NOT EXISTS oauth_auth_codes (
    code TEXT PRIMARY KEY,
    request_uri TEXT NOT NULL,
    did TEXT NOT NULL,
    scope TEXT NOT NULL,
    dpop_jkt TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    used BOOLEAN DEFAULT FALSE
);

-- OAuth sessions (long-lived)
CREATE TABLE IF NOT EXISTS oauth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    did TEXT NOT NULL,
    client_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    dpop_jkt TEXT NOT NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
    access_expires_at TIMESTAMPTZ NOT NULL,
    refresh_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked BOOLEAN DEFAULT FALSE
);
CREATE INDEX IF NOT EXISTS idx_oauth_sessions_token ON oauth_sessions(access_token);
CREATE INDEX IF NOT EXISTS idx_oauth_sessions_refresh ON oauth_sessions(refresh_token);
CREATE INDEX IF NOT EXISTS idx_oauth_sessions_did ON oauth_sessions(did);

-- DPoP nonces
CREATE TABLE IF NOT EXISTS oauth_dpop_nonces (
    nonce TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);