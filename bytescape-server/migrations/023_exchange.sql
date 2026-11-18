CREATE TABLE IF NOT EXISTS exchange_listings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    listing_type TEXT NOT NULL DEFAULT 'offer',
    category TEXT NOT NULL DEFAULT 'other',
    price_cents BIGINT,
    currency TEXT DEFAULT 'USD',
    location TEXT,
    tags TEXT[] DEFAULT '{}',
    image_urls TEXT[] DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'active',
    view_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS exchange_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id UUID NOT NULL REFERENCES exchange_listings(id) ON DELETE CASCADE,
    sender_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    read BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_exchange_listings_category ON exchange_listings(category);
CREATE INDEX IF NOT EXISTS idx_exchange_listings_type ON exchange_listings(listing_type);
CREATE INDEX IF NOT EXISTS idx_exchange_listings_status ON exchange_listings(status);
CREATE INDEX IF NOT EXISTS idx_exchange_listings_created ON exchange_listings(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_exchange_msgs_listing ON exchange_messages(listing_id);
