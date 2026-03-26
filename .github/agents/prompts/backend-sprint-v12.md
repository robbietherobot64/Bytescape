# Backend Sprint v12 — Phase 4.5 Infrastructure Deployment (Track A)

**Sprint Goal:** Stand up the `bytescape.social` server infrastructure: Docker Compose scaffold, Official PDS deployment, identity resolution. This is the **#1 user priority** — hosting data on a self-owned PDS and federating with the fediverse.

**Context:** Sprint v11 completed the client-side identity migration (A-S1 ✅), relay/server command stubs (A-S2/A-S3/A-S4 🔶 stubs). The stubs in `commands/relay.rs` now need real infrastructure behind them. The self-hosting guide (`docs/SELF-HOSTING-GUIDE.md`) and systems architecture doc (`.github/agents/prompts/systems-dual-engine-architecture-2026-03-25.md`) document the target deployment. **IMPORTANT: "Tranquil PDS" does not exist — it was a hallucination.** Use the **Official Bluesky PDS** (`ghcr.io/bluesky-social/pds`, Node.js, SQLite).

**HEAD:** `06cf97a` on `main` (clean tree). **Build:** 0 tsc errors, 272/272 frontend tests, 207 Rust tests pass.

---

## P1: Docker Compose Scaffold (I-1)

**Priority:** 🔥 Critical — prerequisite for all server infrastructure.

**Task:** Create the Docker Compose project that will run the `bytescape.social` stack.

1. **Create `server/` directory** at the repo root (alongside `bytescape/`):
   ```
   server/
     docker-compose.yml
     Caddyfile
     .env.example
     README.md
   ```
2. **`docker-compose.yml`** with these services:
   - **`caddy`** — Reverse proxy with automatic TLS. Mount `Caddyfile`. Ports 80, 443. Wildcard cert for `*.bytescape.social` via Cloudflare DNS challenge (or HTTP challenge for initial setup).
   - **`postgres`** — PostgreSQL 16. Used by AP server and sync bridge (NOT the PDS — PDS uses SQLite). Volume for data persistence.
   - **`pds`** — `ghcr.io/bluesky-social/pds:latest`. Environment variables: `PDS_HOSTNAME`, `PDS_DATA_DIRECTORY`, `PDS_BLOBSTORE_DISK_LOCATION`, `PDS_DID_PLC_URL`, `PDS_BSKY_APP_VIEW_URL`, `PDS_BSKY_APP_VIEW_DID`, `PDS_REPORT_SERVICE_URL`, `PDS_REPORT_SERVICE_DID`, `PDS_CRAWLERS`, `PDS_INVITE_REQUIRED=true`, `PDS_JWT_SECRET`, `PDS_ADMIN_PASSWORD`. Internal port 3000.
   - Empty service slots (commented out) for: `ap-server`, `iroh-relay`, `sync-bridge`.
3. **`Caddyfile`** — Route `*.bytescape.social` → PDS (port 3000). Route `bytescape.social/.well-known/did.json` and `bytescape.social/.well-known/webfinger` → identity resolver (placeholder for I-4). Reserve `/ap/` path prefix for AP server.
4. **`.env.example`** — All configurable values with sensible defaults and comments.
5. **`README.md`** — Quick start instructions. Reference `docs/SELF-HOSTING-GUIDE.md` for full guide.

**Key decisions:**
- PDS runs on SQLite (its default). No Postgres dependency for PDS.
- Caddy handles TLS automatically. For local dev, use `--internal-tls` or HTTP only.
- `PDS_INVITE_REQUIRED=true` for launch — invite-only access.
- Generate `PDS_JWT_SECRET` and `PDS_ADMIN_PASSWORD` in the `.env.example` with placeholder instructions.

**Reference:** Read `docs/SELF-HOSTING-GUIDE.md` for the full architecture. The Official PDS Docker setup guide: https://github.com/bluesky-social/pds (check their `README.md` for canonical environment variables).

**Verification:** `docker compose config` validates the YAML without errors.

---

## P2: PDS Configuration & Health Verification (I-2)

**Priority:** 🔥 Critical — proves the PDS actually works.

**Task:** Configure and validate the Official PDS deployment.

1. **PDS environment tuning:**
   - Set `PDS_HOSTNAME=pds.bytescape.social` (or `PDS_HOSTNAME=bytescape.social` if using subdomain routing — check PDS docs for the correct pattern)
   - Ensure PDS data volume persists across restarts
   - Configure `PDS_CRAWLERS=https://bsky.network` to register with the Bluesky Relay
2. **Create a health check script** (`server/scripts/verify-pds.sh`):
   - Test XRPC health endpoint: `curl https://pds.bytescape.social/xrpc/_health`
   - Test account creation (using admin credentials): `curl -X POST .../xrpc/com.atproto.server.createAccount` with invite code
   - Test WebSocket firehose: `websocat wss://pds.bytescape.social/xrpc/com.atproto.sync.subscribeRepos` (verify stream starts)
   - Test `describeServer`: `curl .../xrpc/com.atproto.server.describeServer` — verify `availableUserDomains` includes `bytescape.social`
3. **Document the verification results** in `server/README.md`.

**Note:** This may require actual DNS and network setup that can't be fully automated in code. Focus on creating the configuration files and verification scripts that are ready to run once DNS is pointed. If you can test locally with `localhost` substitutions, do so.

---

## P3: Identity Resolver Stub (I-4, scaffold)

**Priority:** High — needed for `did:web` resolution.

**Task:** Create a lightweight identity resolution service.

1. **Create `server/identity-resolver/`** — a small Rust or Node.js service that:
   - `GET /.well-known/did.json` → Returns a `did:web` DID document for `did:web:bytescape.social`
   - `GET /.well-known/webfinger?resource=acct:username@bytescape.social` → Routes to PDS WebFinger or AP server WebFinger depending on the account type
   - `GET /.well-known/atproto-did?handle=username.bytescape.social` → Returns the `did:plc:` for AT Proto handle resolution
2. **The DID document** should include:
   - `id`: `did:web:bytescape.social:{username}`
   - `alsoKnownAs`: AT Proto handle (`at://username.bytescape.social`), AP actor URL
   - `verificationMethod`: Ed25519 public key from user's Iroh keypair
   - `service`: PDS endpoint, AP inbox endpoint
3. **For now:** This can be a simple static file server or a script that generates the JSON. The full dynamic resolution (querying user database) comes in a later sprint.
4. **Add to `docker-compose.yml`** as the `identity-resolver` service. Update Caddyfile routing.

**Reference:** AT Proto DID Web spec: https://atproto.com/specs/did#did-web — check the exact format required.

---

## P4: Wire `provision_pds_account` Stub to XRPC (A-S3, partial)

**Priority:** Medium — connects the desktop client to the real PDS.

**Task:** Advance the `provision_pds_account` stub in `commands/relay.rs` toward real functionality.

1. **Add a configuration mechanism** for the server URL:
   - New field in `AppState` (or in the encrypted keychain settings): `server_config: Option<ServerConfig>` with `pds_url`, `admin_token`
   - Tauri command: `configure_server(pds_url: String)` — stores the PDS base URL for subsequent operations
2. **Implement the PDS account creation flow** (behind a `server_config.is_some()` check):
   - Call `com.atproto.server.createInviteCode` (admin endpoint) — or accept user-provided invite code
   - Call `com.atproto.server.createAccount` with `{ email, handle, password, inviteCode }`
   - Store the returned `did`, `handle`, `accessJwt`, `refreshJwt` in the keychain as a new `LinkedAccount`
   - Update `get_owner_did()` — the PDS-minted `did:plc:` should now appear in the hierarchy
3. **If no server is configured** — keep the current error message ("PDS not configured").
4. **Test with the local PDS** from P1/P2 if it's running.

**Constraints:**
- Don't hardcode `bytescape.social` — use the configured PDS URL.
- The XRPC calls need proper HTTP client setup (existing `reqwest` usage patterns in the codebase).
- Store credentials securely in the encrypted keychain, not in plaintext config.

**Verification:** `cargo check` + `cargo test` + `pnpm test -- --run`.

---

## P5: Invite Code Management (server admin utility)

**Priority:** Medium — needed for invite-only launch.

**Task:** Create a simple utility for managing PDS invite codes.

1. **Create `server/scripts/manage-invites.sh`:**
   - `./manage-invites.sh create` — calls PDS admin API to generate an invite code, prints it
   - `./manage-invites.sh list` — lists existing invite codes and their usage status
   - `./manage-invites.sh revoke <code>` — disables an invite code
2. **Add Tauri command** `generate_invite_code` (admin-only, behind server config check) — calls PDS XRPC endpoint to create invite code from the desktop app.
3. **Document the invite flow** in `server/README.md`: admin generates code → shares with user → user enters code in Settings → `provision_pds_account` creates their account.

---

## P6: A1.4 Actor Metadata (FEP-fb2a) — stretch goal

**Priority:** Low — federation polish, only if P1-P5 are complete.

**Task:** Map Bytescape profile fields to/from the `attachment` array on AP actor records.

1. **In the AP module** (`src-tauri/src/activitypub/`): When constructing an AP actor JSON, populate the `attachment` array with profile fields (bio, location, website, pronouns).
2. **When receiving AP actors** (followers/following): Parse the `attachment` array and store relevant fields.
3. **Field mapping:**
   - `name` → `attachment[type=PropertyValue, name="Website", value="..."]`
   - Bio → `summary` (already done?)
   - Location → `attachment[type=PropertyValue, name="Location", value="..."]`

**Reference:** FEP-fb2a spec, Mastodon's actor metadata format.

---

## Constraints

- **Track boundary:** Backend files: `src-tauri/src/`, `server/` (new). Do NOT touch `src/` (frontend files).
- **Infrastructure files** (`server/`): New directory at repo root for Docker Compose and deployment configuration.
- **Build verification:** Run `cargo check` + `pnpm test -- --run` before EVERY commit. Zero errors, zero test regressions.
- **Shared files:** Before editing `lib.rs`, `db.rs`, or `keychain_encrypted.rs`, run `git log --oneline -5 -- <filepath>` to check for recent cross-agent modifications.
- **Commit format:** `feat(track-a): description` or `infra(track-a): description`. One logical change per commit.
- **PDS choice:** Official Bluesky PDS (`ghcr.io/bluesky-social/pds`), Node.js, SQLite. NOT "Tranquil PDS" (doesn't exist). Upgrade path: `rsky-pds` when mature.
- **Update `progress.md`** with your deliverables when done.
