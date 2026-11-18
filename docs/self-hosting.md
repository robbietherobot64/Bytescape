# Self-hosting Bytescape

This is the self-host quickstart: the honest, docs-only path to running a real
Bytescape instance. There is no turnkey installer — this document describes what
to run, using the artifacts already in this repository.

## The stack

A Bytescape instance is five pieces:

| Piece | What | Where it comes from |
|---|---|---|
| PostgreSQL | all state | `docker-compose.yml` (`postgres` service) or a system/managed Postgres |
| Object storage | media blobs (optional) | `docker-compose.yml` (`minio` service) or any S3; without it the server stores files on local disk |
| Server | PDS + firehose consumer + derived index + REST API | `bytescape-server/` (binary or `Dockerfile`), configured by `.env` |
| Frontend | static SPA build | `apps/web/` (`npm run build:web` → `apps/web/dist`) |
| TLS + reverse proxy | HTTPS, static OAuth metadata, SPA fallback, API routing | Caddy with `deploy/Caddyfile` as the reference config |

The reference production setup (what `deploy/` is written for) runs the server as
a systemd unit (`deploy/bytescape.service`) with Caddy in front. Everything below
follows that shape; a Docker deployment of the server (`bytescape-server/Dockerfile`)
is equivalent if you prefer containers — the pieces and caveats are the same.

## 0. Requirements

- A Linux VPS with your domain's DNS pointing at it. Small boxes are fine but
  need swap (the reference bootstrap adds 2G for this reason — compiling Rust or
  a firehose backlog replay will OOM a swapless 2 GB box).
- To build: a Rust toolchain (the vendored crates pin 1.86) and Node.js 20+.
- Ports 80/443 reachable for Caddy.

## 1. Backing services

```sh
docker compose up -d
```

That starts Postgres (`:5432`) and MinIO (`:9000` API, `:9001` console). The
credentials in `docker-compose.yml` are development defaults (`bytescape` /
`minioadmin`) — **change them for any instance that leaves your machine, and do
not expose those ports publicly.**

Postgres migrations are embedded in the server binary and applied idempotently
at startup; you do not run them by hand.

## 2. Configure

```sh
cp .env.example .env
```

The variables are documented in `.env.example`. For a real instance, at minimum:

- `JWT_SECRET` — random, 32+ characters. Required.
- `DATABASE_URL` — pointing at your Postgres.
- `DOMAIN` / `FRONTEND_URL` — your public domain(s).
- `SMTP_*` — real email delivery. Without SMTP the server returns verification
  codes inline, which is fine for dev and wrong for anyone else.
- `REQUIRE_INVITE_CODE=true` — keep registration invite-only until you have a
  moderation story (see below). `ADMIN_TOKEN` guards admin surfaces; change it.
- `S3_*` — if you use S3/MinIO for media; omit to fall back to local disk under
  the server's working directory.

**OAuth client metadata:** Bluesky sign-in validates
`/oauth/client-metadata.json` against your client registration. The copy in
`deploy/client-metadata.json` is the bytescape.social instance's — its
`client_id` and `redirect_uris` are domain-specific, so **edit it for your
domain** (and serve it statically per `deploy/Caddyfile`) or Bluesky sign-in
will fail. `deploy/did.json` is the feed-generator DID document for
`feeds.bytescape.social`; adapt or drop it unless you run the feed generator.

## 3. Server

```sh
cd bytescape-server
cargo build --release        # or: docker build -t bytescape-server .
```

Run the binary with your `.env` loaded (the systemd unit
`deploy/bytescape.service` reads `EnvironmentFile=/opt/bytescape/.env` — adjust
paths/user to your layout). The API listens on `:8080` by default (`PORT`).

On first start the server creates its schema (embedded migrations) and, if
`APPVIEW_CONSUMER=true`, starts the network firehose consumer. Turn the consumer
on if this instance should populate feeds from the wider network; leave it off
for an account-only instance.

## 4. Frontend

```sh
npm ci
npm run build:web            # → apps/web/dist
```

Serve `apps/web/dist` as static files with SPA fallback (the reference Caddyfile
serves it from `/srv/bytescape/frontend`). Optionally build the standalone events
site too (`npm run build:events` → serve `apps/events/dist` at its own host/path).

## 5. TLS + routing (Caddy)

`deploy/Caddyfile` is the reference config. Copy it, change the domain names and
roots to yours. What it encodes that matters:

- `/oauth/client-metadata.json` is served **statically, first**, so Bluesky
  OAuth validation survives app downtime.
- `/api`, `/oauth`, `/uploads`, `/xrpc`, `/.well-known`, `/nodeinfo`, `/users`,
  `/actors`, `/inbox` go to the backend on `127.0.0.1:8080`.
- `/@*` (ActivityPub actor surface) does content negotiation: AP clients get
  the actor document, browsers get the SPA profile page; non-GET requests
  (inbox deliveries) always reach the backend.
- Everything else is the SPA with `index.html` fallback.

Then point DNS at the box, get certificates (Caddy does this automatically), and
smoke-test: sign in, post, load the feed.

## Operating it — read before opening registration

### Relay dependency (honest caveat)

Network feed ingestion subscribes to the Bluesky-operated relay
(`wss://bsky.network/xrpc/com.atproto.sync.subscribeRepos`), and new accounts
register for crawling there too (`com.atproto.sync.requestCrawl`). Today this
dependency is real and not configurable: if that relay is down, throttling you,
or refusing new hosts, your instance's view of (and visibility to) the wider AT
Protocol network degrades through no fault of your own. Your own PDS firehose,
local index, and REST API keep working regardless — but plan for the relay being
a shared piece of infrastructure someone else runs.

### Moderation is your responsibility (honest caveat)

Every instance operator is responsible for the content on and through their
instance. The app ships moderation building blocks (labeling, report flows), but
no policy writes itself and no legal shield comes with the code. If you open
registration: publish terms/privacy/contact, decide what your instance allows,
act on reports, and keep `REQUIRE_INVITE_CODE=true` until you can. Bytescape's
product stance — moderation as labeling, proactive moderation, notice-based
takedown — is described in `wiki/`; it is a stance, not a service level.

### Updates and migrations (honest caveat)

- **Back up Postgres before every upgrade.** Migrations run automatically at
  startup and there is no automatic downgrade path.
- The API surface is pre-alpha and still moving. Pin a commit, read the diff,
  then upgrade — not the other way around.
- Known fresh-database caveat: the migration runner skips a block of historical
  migration files on fresh databases (they patch states that only ever existed
  on long-lived instances). If a brand-new instance hits a schema error on
  startup, open an issue — and expect this caveat to disappear once real
  migration tooling lands.
- There is no built-in backup, monitoring, or update automation. `pg_dump` on a
  cron is the minimum; the rest is yours.

### What this document does not cover

- Email deliverability (bring working SMTP, set up DKIM/SPF).
- Backups and restore drills.
- Multi-instance or HA setups — the design is a single-node instance.
- The feed generator (`feeds.bytescape.social`) and the hosted wiki — both are
  optional extras with their own config in `deploy/`.