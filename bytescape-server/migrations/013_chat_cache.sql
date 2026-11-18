CREATE TABLE IF NOT EXISTS chat_messages (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    platform        TEXT NOT NULL,      -- "bluesky", "matrix", "mastodon"
    conversation_id TEXT NOT NULL,      -- Bluesky convoId, Matrix roomId, etc.
    provider_account_id TEXT,           -- linked_accounts.id for Bluesky/Mastodon
    message_id      TEXT NOT NULL,      -- Bluesky message id, Matrix event_id
    sender_id       TEXT NOT NULL,      -- Bluesky DID, Matrix MXID
    sender_name     TEXT,               -- display name
    sender_avatar   TEXT,
    content         TEXT NOT NULL,      -- message body
    sent_at         TIMESTAMPTZ NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (platform, message_id)
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_convo ON chat_messages(platform, conversation_id, sent_at DESC);
CREATE INDEX IF NOT EXISTS idx_chat_messages_provider ON chat_messages(provider_account_id);
