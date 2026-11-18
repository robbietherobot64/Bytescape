//! PDS Account management — create accounts with DID + secp256k1 signing keys.

use anyhow::Result;
use secp256k1::{Keypair, Secp256k1};
use secp256k1::rand::rngs::OsRng;
use sqlx::PgPool;

const RELAY_HOST: &str = "https://bsky.network";

/// POST `{relay_base}/xrpc/com.atproto.sync.requestCrawl` with `{"hostname": ...}`.
/// Returns `Ok(status)` for any HTTP response — the relay answers non-2xx with a
/// body whose error name maps to a known cause (see `_ref/sprints/
/// atproto-research-2026-11-14.md` c.1): `400 HostNotFound` = the PDS XRPC
/// surface (`describeServer`) is unreachable at the host — the blocking
/// prerequisite; `401` = DomainBan; `403` = the relay has public requestCrawl
/// disabled; other `400` = often `ErrNewHostsDisabled` (the relay's
/// new-hosts-per-day limiter — retry later). `Err` means no response arrived
/// (transport failure).
pub(crate) async fn request_crawl_to(relay_base: &str, hostname: &str) -> Result<u16, String> {
    let url = format!("{relay_base}/xrpc/com.atproto.sync.requestCrawl");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client build failed: {e}"))?;
    match client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, "Bytescape/1.0")
        .body(format!(r#"{{"hostname":"{hostname}"}}"#))
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status().as_u16();
            if resp.status().is_success() {
                tracing::info!("[PDS] requestCrawl registered {hostname} with {relay_base} (HTTP {status})");
            } else {
                let body = resp.text().await.unwrap_or_default();
                tracing::warn!(
                    "[PDS] requestCrawl {hostname}: HTTP {status} body: {body} \
                     (HostNotFound = describeServer unreachable at host; 401 = DomainBan; \
                     403 = requestCrawl disabled at relay; other 400 = often ErrNewHostsDisabled, retry later)"
                );
            }
            Ok(status)
        }
        Err(e) => Err(format!("requestCrawl request failed: {e}")),
    }
}

/// Ask the relay to crawl this PDS (`com.atproto.sync.requestCrawl`). Without
/// this a brand-new `did:web` account is invisible to the network — the relay
/// never learns the repo exists, so `getLatestCommit`/`getProfile` report
/// `RepoNotSynchronized` / "Profile not found" and the handle is not searchable
/// on the Atmosphere. Fire-and-forget: never blocks account creation.
pub async fn request_crawl(hostname: &str) {
    if let Err(e) = request_crawl_to(RELAY_HOST, hostname).await {
        tracing::warn!("[PDS] requestCrawl {hostname} failed: {e}");
    }
}

#[derive(Debug, Clone)]
pub struct PdsAccount {
    pub did: String,
    pub handle: String,
    pub keypair: Keypair,
    pub status: String,
}

impl PdsAccount {
    /// Create a new PDS account: [`Self::mint`] plus the best-effort relay
    /// registration ping (`request_crawl`).
    pub async fn create(
        pool: &PgPool,
        handle: &str,
        domain: &str,
        user_id: Option<uuid::Uuid>,
        plc_base: &str,
    ) -> Result<Self> {
        let account = Self::mint(pool, handle, domain, user_id, plc_base).await?;
        // Register with the relay so the account is crawled and becomes
        // searchable on the Atmosphere (see `request_crawl`). Best-effort.
        request_crawl(domain).await;
        Ok(account)
    }

    /// Mint a new PDS account: a `did:plc` DID via the PLC directory
    /// (`crate::pds::plc::create_did`) and a secp256k1 signing key, plus an
    /// initial `app.bsky.actor.profile` record so the user appears on Bluesky
    /// search immediately. Everything `create` does except the relay ping —
    /// the seam tests exercise so a mint never writes to the public relay.
    /// Pre-existing `did:web` rows in `pds_accounts` are re-identified to
    /// `did:plc` by the 091 runner (`pds::reidentify`, `REIDENTIFY_DID_WEB`)
    /// and the old identity is tombstoned — they are NOT kept loading/serving
    /// (the pre-091 comment here claimed they were "untouched and keep
    /// loading/serving"; corrected 2026-11-14). New mints use `did:plc` only
    /// (no `did:web` fallback: PLC minting failure is an error).
    async fn mint(
        pool: &PgPool,
        handle: &str,
        domain: &str,
        user_id: Option<uuid::Uuid>,
        plc_base: &str,
    ) -> Result<Self> {
        // Every non-instance account row must belong to a user
        // (`ck_pds_accounts_user_id`) — and a mint is an irreversible PUBLIC
        // write, so refuse BEFORE touching the PLC directory rather than
        // burning a did:plc for a row we could not store.
        let Some(user_id) = user_id else {
            return Err(anyhow::anyhow!(
                "refusing to mint for handle {handle}: a PDS account must belong to a user"
            ));
        };
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let keypair = Keypair::new(&secp, &mut rng);

        let pds_endpoint = format!("https://{}", domain);
        // The genesis `alsoKnownAs` handle must be the RESOLVABLE one —
        // `{user}.{domain}` (our wildcard well-known answers it both ways). A
        // bare username is not a handle: the AppView would reject it
        // (bluesky-social/atproto#4379) and a mint is irreversible.
        let at_handle = if handle.ends_with(&format!(".{domain}")) {
            handle.to_string()
        } else {
            format!("{handle}.{domain}")
        };
        let did = crate::pds::plc::create_did(
            &reqwest::Client::new(),
            plc_base,
            &keypair,
            &at_handle,
            &pds_endpoint,
        )
        .await
        .map_err(|e| anyhow::anyhow!("did:plc mint failed: {e}"))?;
        let public_key_multibase = encode_public_key_multibase(&keypair);
        let private_key_bytes = keypair.secret_key().secret_bytes().to_vec();

        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id)
             VALUES ($1, $2, $3, $4, 'active', $5)",
        )
        .bind(&did)
        .bind(handle)
        .bind(&private_key_bytes)
        .bind(&public_key_multibase)
        .bind(user_id)
        .execute(pool)
        .await?;

        // Create initial profile record so the user shows up on Bluesky
        let profile_record = serde_json::json!({
            "$type": "app.bsky.actor.profile",
            "displayName": handle,
            "description": "",
            "createdAt": chrono::Utc::now().to_rfc3339(),
        });

        if let Err(e) = crate::pds::create_record_internal(
            pool,
            &did,
            "app.bsky.actor.profile",
            Some("self"),
            &profile_record,
            Some(domain),
        ).await {
            tracing::warn!("[PDS] Failed to create initial profile record for {}: {}", handle, e);
        }

        Ok(Self {
            did,
            handle: handle.to_string(),
            keypair,
            status: "active".to_string(),
        })
    }

    /// The reserved instance identity row (handle `_instance`): deterministic
    /// `did:web:{domain}` — the HOST identity, not a user mint, so it never
    /// goes through the PLC directory (a minted `did:plc` here would be a
    /// second, wrong identity for the host, and it would fail anyway: the
    /// directory only accepts identities whose handle we can prove). Key
    /// generation and persistence match [`Self::create`]; no profile record or
    /// crawl ping — the instance has no repo persona, and `request_crawl` runs
    /// from the startup loop.
    pub async fn create_instance(pool: &PgPool, domain: &str) -> Result<Self> {
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let keypair = Keypair::new(&secp, &mut rng);

        let did = format!("did:web:{}", domain);
        let public_key_multibase = encode_public_key_multibase(&keypair);
        let private_key_bytes = keypair.secret_key().secret_bytes().to_vec();

        sqlx::query(
            "INSERT INTO pds_accounts (did, handle, signing_key_private, signing_key_public, status, user_id) \
             VALUES ($1, '_instance', $2, $3, 'active', NULL)",
        )
        .bind(&did)
        .bind(&private_key_bytes)
        .bind(&public_key_multibase)
        .execute(pool)
        .await?;

        Ok(Self {
            did,
            handle: "_instance".to_string(),
            keypair,
            status: "active".to_string(),
        })
    }

    /// Load an account by DID.
    pub async fn load(pool: &PgPool, did: &str) -> Result<Option<Self>> {
        let row = sqlx::query_as::<_, (String, String, Vec<u8>, String, String)>(
            "SELECT did, handle, signing_key_private, signing_key_public, status
             FROM pds_accounts WHERE did = $1",
        )
        .bind(did)
        .fetch_optional(pool)
        .await?;

        match row {
            None => Ok(None),
            Some((did, handle, priv_bytes, _pub_multibase, status)) => {
                let secp = Secp256k1::new();
                let secret_key = secp256k1::SecretKey::from_slice(&priv_bytes)?;
                let keypair = Keypair::from_secret_key(&secp, &secret_key);
                Ok(Some(Self {
                    did,
                    handle,
                    keypair,
                    status,
                }))
            }
        }
    }

    /// Load an account by handle.
    pub async fn load_by_handle(pool: &PgPool, handle: &str) -> Result<Option<Self>> {
        let row = sqlx::query_as::<_, (String, String, Vec<u8>, String, String)>(
            "SELECT did, handle, signing_key_private, signing_key_public, status
             FROM pds_accounts WHERE handle = $1",
        )
        .bind(handle)
        .fetch_optional(pool)
        .await?;

        match row {
            None => Ok(None),
            Some((did, handle, priv_bytes, _pub_multibase, status)) => {
                let secp = Secp256k1::new();
                let secret_key = secp256k1::SecretKey::from_slice(&priv_bytes)?;
                let keypair = Keypair::from_secret_key(&secp, &secret_key);
                Ok(Some(Self {
                    did,
                    handle,
                    keypair,
                    status,
                }))
            }
        }
    }
}

/// Encode a secp256k1 public key as multibase (did:key format).
/// secp256k1 multicodec prefix: 0xe7 0x01, then 33-byte compressed public key.
pub fn encode_public_key_multibase(keypair: &Keypair) -> String {
    let serialized = keypair.public_key().serialize();
    let mut multicodec = Vec::with_capacity(2 + serialized.len());
    multicodec.push(0xe7);
    multicodec.push(0x01);
    multicodec.extend_from_slice(&serialized);
    let b58 = bs58::encode(&multicodec).into_string();
    format!("z{}", b58)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use parking_lot::Mutex;
    use std::sync::Arc;

    /// Accepts `calls` connections, capturing each full request (head + body)
    /// into one string, and answers each with the same `status_line`/`body`
    /// (correct Content-Length, `Connection: close`). Returns `(base_url,
    /// captured_requests, thread)`.
    fn stub_http(
        status_line: &str,
        body: &str,
        calls: usize,
    ) -> (String, Arc<Mutex<String>>, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind stub listener");
        let base = format!("http://{}", listener.local_addr().expect("stub addr"));
        let captured = Arc::new(Mutex::new(String::new()));
        let cap = Arc::clone(&captured);

        let body = body.to_string();
        let status_line = status_line.to_string();
        let thread = std::thread::spawn(move || {
            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            for _ in 0..calls {
                let Ok((mut stream, _)) = listener.accept() else { return };
                // Read the request head, then the body sized by Content-Length.
                let mut head = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let Ok(n) = stream.read(&mut buf) else { break };
                    if n == 0 {
                        break;
                    }
                    head.extend_from_slice(&buf[..n]);
                    if head.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let head_text = String::from_utf8_lossy(&head).to_string();
                let content_length: usize = head_text
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        k.eq_ignore_ascii_case("content-length")
                            .then(|| v.trim().parse().ok())?
                    })
                    .unwrap_or(0);
                // Body bytes may already have arrived in the head's segment.
                let boundary = head
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map_or(head.len(), |i| i + 4);
                let mut body_buf = head.split_off(boundary);
                while body_buf.len() < content_length {
                    let Ok(n) = stream.read(&mut buf) else { break };
                    if n == 0 {
                        break;
                    }
                    body_buf.extend_from_slice(&buf[..n]);
                }
                cap.lock().push_str(&head_text);
                cap.lock().push_str(&String::from_utf8_lossy(&body_buf));
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (base, captured, thread)
    }

    #[tokio::test]
    async fn request_crawl_posts_hostname_and_reports_success() {
        let (base, captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);
        let status = request_crawl_to(&base, "test.example")
            .await
            .expect("requestCrawl");
        assert_eq!(status, 200);
        let req = captured.lock().clone();
        assert!(
            req.contains("POST /xrpc/com.atproto.sync.requestCrawl"),
            "request head: {req}"
        );
        assert!(
            req.to_lowercase().contains("content-type: application/json"),
            "request: {req}"
        );
        assert!(req.contains("\"hostname\":\"test.example\""), "body: {req}");
        let _ = thread.join();
    }

    #[tokio::test]
    async fn request_crawl_surfaces_relay_rejection() {
        let (base, captured, thread) =
            stub_http("HTTP/1.1 400 Bad Request", r#"{"error":"HostNotFound"}"#, 1);
        let status = request_crawl_to(&base, "test.example")
            .await
            .expect("non-2xx is still a response");
        assert_eq!(status, 400);
        assert!(
            captured.lock().contains("test.example"),
            "request: {}",
            captured.lock()
        );
        let _ = thread.join();
    }

    #[tokio::test]
    async fn request_crawl_transport_error_is_an_err() {
        // Bind then drop the listener so the port is closed.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        drop(listener);
        assert!(request_crawl_to(&base, "test.example").await.is_err());
    }

    /// 5.4 acceptance (DB half): a fresh mint stores a `did:plc:` DID whose
    /// stored key material is exactly the minted keypair's, and the genesis op
    /// goes to the directory in the accepted form (resolvable
    /// `at://{user}.{domain}` handle claim, method-prefixed `did:key:z…` key).
    /// The directory here is a local stub — a real mint is an irreversible
    /// public write, and derivation + encoding are grounded offline by
    /// `pds::plc`'s real-fixture tests.
    #[tokio::test]
    async fn fresh_mint_stores_did_plc_with_same_key_material() {
        let Some(pool) = crate::ap::testutil::connect_pool().await else { return };
        let (base, captured, thread) = stub_http("HTTP/1.1 200 OK", "{}", 1);
        let suffix = crate::ap::testutil::unique();
        let handle = format!("mint_{suffix}");
        let domain = "bytescape.test";

        // A mint must belong to a user (`ck_pds_accounts_user_id`) — same as
        // every real signup path.
        let user = crate::db::users::UserRow::create_active(&pool, &handle, "", "x", "x")
            .await
            .expect("create user");

        let acct = PdsAccount::mint(&pool, &handle, domain, Some(user.id), &base)
            .await
            .expect("mint against stub directory");

        // (1) did:plc, never did:web — no fallback exists.
        assert!(acct.did.starts_with("did:plc:"), "minted {}", acct.did);

        // (2) the stored row carries the SAME key material as the minted keypair.
        let (stored_did, stored_pub): (String, String) = sqlx::query_as(
            "SELECT did, signing_key_public FROM pds_accounts WHERE handle = $1",
        )
        .bind(&handle)
        .fetch_one(&pool)
        .await
        .expect("minted row");
        assert_eq!(stored_did, acct.did, "row did is the minted did");
        let minted_pub = encode_public_key_multibase(&acct.keypair);
        assert_eq!(stored_pub, minted_pub, "row public key is the minted key's");
        let loaded = PdsAccount::load(&pool, &acct.did)
            .await
            .expect("reload")
            .expect("row reloads");
        assert_eq!(
            encode_public_key_multibase(&loaded.keypair),
            minted_pub,
            "stored private bytes reload to the same key pair"
        );

        // (3) the genesis op reached the directory's CREATE route in the
        // accepted form (`POST /{did}` — the bare base 404s "Cannot POST /").
        let request = captured.lock().clone();
        assert!(
            request.starts_with(&format!("POST /{} ", acct.did)),
            "op POSTed to the create route /{{did}}: {request}"
        );
        assert!(
            request.contains(&format!(r#""at://{handle}.{domain}""#)),
            "genesis claims the resolvable handle: {request}"
        );
        assert!(request.contains("did:key:z"), "did:key method prefix: {request}");
        assert!(request.contains("plc_operation"), "genesis op type: {request}");
        assert!(
            request.contains(&format!(r#""endpoint":"https://{domain}""#)),
            "atproto_pds service: {request}"
        );

        // Clean the mint's local rows (the stub directory holds nothing).
        for table in ["pds_firehose_seq", "pds_indexed_records", "pds_repo_roots", "pds_accounts"] {
            let _ = sqlx::query(&format!("DELETE FROM {table} WHERE did = $1"))
                .bind(&acct.did)
                .execute(&pool)
                .await;
        }
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await;
        let _ = thread.join();
    }
}
