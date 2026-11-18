//! Application configuration loaded from environment variables.

use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub port: u16,
    pub s3_endpoint: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub s3_bucket: String,
    /// S3/R2 media storage is used only when explicitly configured via env;
    /// otherwise media is written to the local uploads/ directory.
    pub s3_enabled: bool,
    pub s3_public_base_url: String,
    pub domain: String,
    pub jwt_secret: String,
    pub log_level: String,
    pub google_client_id: String,
    pub google_client_secret: String,

    // Bluesky OAuth — registered client for the AT Protocol OAuth flow
    pub bluesky_client_id: String,
    pub bluesky_redirect_uri: String,

    // Frontend URL for OAuth callback redirects
    pub frontend_url: String,

    // SMTP for email verification
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_from_address: String,

    // hCaptcha verification
    pub hcaptcha_secret: String,
    pub hcaptcha_site_key: String,

    // Invite codes — when set, signup requires a valid invite code
    pub require_invite_code: bool,

    // Admin token for protected endpoints (log viewer, etc.). `None` (the
    // default — ADMIN_TOKEN unset) disables the admin interface entirely;
    // every admin route fails closed with 401.
    pub admin_token: Option<String>,

    // Extra apexes the on-demand TLS ask endpoint vouches for (comma
    // separated) beyond DOMAIN — e.g. the staging domain on the shared box.
    pub tls_ask_extra_domains: Vec<String>,

    // PLC directory base URL for minting did:plc DIDs (see `pds::plc`).
    pub plc_directory: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            database_url: env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set".to_string())?,
            port: env::var("PORT").unwrap_or_else(|_| "8080".to_string()).parse().unwrap_or(8080),
            s3_endpoint: env::var("S3_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string()),
            s3_access_key: env::var("S3_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string()),
            s3_secret_key: env::var("S3_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string()),
            s3_bucket: env::var("S3_BUCKET").unwrap_or_else(|_| "bytescape-media".to_string()),
            s3_enabled: env::var("S3_BUCKET").is_ok() || env::var("S3_ENDPOINT").is_ok(),
            s3_public_base_url: env::var("S3_PUBLIC_BASE_URL").unwrap_or_default(),
            domain: env::var("DOMAIN").unwrap_or_else(|_| "bytescape.social".to_string()),
            jwt_secret: env::var("JWT_SECRET").map_err(|_| "JWT_SECRET must be set".to_string())?,
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            google_client_id: env::var("GOOGLE_CLIENT_ID").unwrap_or_default(),
            google_client_secret: env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default(),

            bluesky_client_id: env::var("BLUESKY_CLIENT_ID")
                .unwrap_or_else(|_| "http://127.0.0.1:8080/oauth/client-metadata.json".to_string()),
            bluesky_redirect_uri: env::var("BLUESKY_REDIRECT_URI")
                .unwrap_or_else(|_| "http://127.0.0.1:8080/api/auth/bluesky/callback".to_string()),

            frontend_url: env::var("FRONTEND_URL")
                .unwrap_or_else(|_| "http://localhost:1420".to_string()),

            smtp_host: env::var("SMTP_HOST").unwrap_or_else(|_| "localhost".to_string()),
            smtp_port: env::var("SMTP_PORT").unwrap_or_else(|_| "587".to_string()).parse().unwrap_or(587),
            smtp_username: env::var("SMTP_USERNAME").unwrap_or_default(),
            smtp_password: env::var("SMTP_PASSWORD").unwrap_or_default(),
            smtp_from_address: env::var("SMTP_FROM_ADDRESS").unwrap_or_else(|_| "noreply@bytescape.social".to_string()),

            hcaptcha_secret: env::var("HCAPTCHA_SECRET").unwrap_or_default(),
            hcaptcha_site_key: env::var("HCAPTCHA_SITE_KEY").unwrap_or_default(),
            require_invite_code: env::var("REQUIRE_INVITE_CODE").unwrap_or_else(|_| "true".to_string()) == "true",
            admin_token: env::var("ADMIN_TOKEN").ok().filter(|t| !t.is_empty()),
            tls_ask_extra_domains: env::var("TLS_ASK_EXTRA_DOMAINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            plc_directory: env::var("PLC_DIRECTORY")
                .unwrap_or_else(|_| "https://plc.directory".to_string()),
        })
    }
}
