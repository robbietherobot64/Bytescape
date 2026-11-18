use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform { Bytescape, Bluesky }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility { Public, Followers, Friends }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCategory { Following, Feeds, Hashtags, Subscriptions, Rss }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol { AtProto }

impl Platform {
    pub fn to_protocol(&self) -> Protocol {
        match self {
            Platform::Bluesky => Protocol::AtProto,
            Platform::Bytescape => Protocol::AtProto,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Bytescape => "bytescape",
            Platform::Bluesky => "bluesky",
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::AtProto => "atproto",
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Visibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Followers => "followers",
            Visibility::Friends => "friends",
        }
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl SourceCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceCategory::Following => "following",
            SourceCategory::Feeds => "feeds",
            SourceCategory::Hashtags => "hashtags",
            SourceCategory::Subscriptions => "subscriptions",
            SourceCategory::Rss => "rss",
        }
    }
}

impl fmt::Display for SourceCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
