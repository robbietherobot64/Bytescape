# Systems/Infra Sprint v18 — Deploy Identity Resolver to bytescape.social

**Sprint**: v18 (Go-Live: Hosted Identity)  
**Track**: C (Systems/Infrastructure)  
**Date**: 2026-04-08  
**Previous sprint**: N/A (first infra sprint)  
**HEAD at prompt time**: `a12201a`  
**Decision**: Single-user launch first, single VM, Caddy auto-TLS

---

## Sprint Context

Track A (Backend) is building a new standalone axum server crate at `bytescape/server/`. This sprint provides the deployment wrapper: Docker, reverse proxy, TLS, DNS, secrets, and monitoring. The server binary will be built from the same monorepo.

### Architecture Reference

See `systems-dual-engine-architecture-2026-03-25.md` §Component Diagram for full layout. This sprint implements a **simplified subset** — only the Identity Resolver and Publish API.

```
Internet → Caddy (TLS, :443) → bytescape-server (:3000)
               ↓
         ┌─────────────────────┐
         │ /.well-known/*      │  Identity resolution
         │ /{username}/did.json│  DID document
         │ /@{username}        │  Profile page / AP Actor
         │ /api/publish/*      │  Publish API (authenticated)
         └─────────────────────┘
```

---

## P0: Deployment Configuration

### Directory structure

Create `bytescape/deploy/`:

```
deploy/
├── Dockerfile               # Multi-stage: build server binary + minimal runtime
├── docker-compose.yml        # server + caddy services
├── Caddyfile                 # Reverse proxy + auto-TLS
├── .env.example              # Template for secrets
├── systemd/
│   └── bytescape-server.service  # Optional: if deploying without Docker
└── README.md                 # Deployment guide
```

### Dockerfile

Multi-stage build:

```dockerfile
# Stage 1: Build
FROM rust:1.82-bookworm AS builder
WORKDIR /build
COPY server/ ./server/
COPY src-tauri/src/activitypub/ ./src-tauri/src/activitypub/
# Only copy what the server crate needs
RUN cd server && cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/server/target/release/bytescape-server /usr/local/bin/
EXPOSE 3000
ENV RUST_LOG=info
ENTRYPOINT ["bytescape-server"]
```

**Notes:**
- The server crate may share code with `src-tauri/src/activitypub/` — adjust COPY paths based on how Track A structures the dependencies
- If the server crate is fully standalone (no path deps into src-tauri), simplify to just copying `server/`
- Pin the Rust version to match `src-tauri/rust-toolchain.toml` if it exists

### docker-compose.yml

```yaml
version: '3.8'

services:
  server:
    build:
      context: ..
      dockerfile: deploy/Dockerfile
    restart: unless-stopped
    ports:
      - "127.0.0.1:3000:3000"
    env_file:
      - .env
    volumes:
      - server-data:/data
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s

  caddy:
    image: caddy:2-alpine
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy-data:/data
      - caddy-config:/config
    depends_on:
      server:
        condition: service_healthy

volumes:
  server-data:
  caddy-data:
  caddy-config:
```

### Caddyfile

```
bytescape.social {
    # Identity resolution — public, no auth
    handle /.well-known/webfinger {
        reverse_proxy server:3000
    }
    handle /.well-known/atproto-did {
        reverse_proxy server:3000
    }
    handle /*/did.json {
        reverse_proxy server:3000
    }
    handle /@* {
        reverse_proxy server:3000
    }

    # Publish API — authenticated, rate-limited
    handle /api/publish/* {
        reverse_proxy server:3000
    }

    # Health check
    handle /health {
        reverse_proxy server:3000
    }

    # Default — landing page or 404
    handle {
        respond "bytescape.social — decentralized identity" 200
    }

    # Security headers
    header {
        X-Content-Type-Options nosniff
        X-Frame-Options DENY
        Referrer-Policy strict-origin-when-cross-origin
        Strict-Transport-Security "max-age=63072000; includeSubDomains; preload"
    }

    # Logging
    log {
        output file /var/log/caddy/bytescape.log
        format json
    }
}
```

**Caddy auto-TLS**: Caddy automatically provisions Let's Encrypt certificates. No manual cert config needed. Just point DNS and Caddy handles it.

### .env.example

```bash
# Bytescape Server Configuration
# Copy to .env and fill in values

# Server bind address
BIND_ADDR=0.0.0.0:3000

# Domain for DID resolution
DOMAIN=bytescape.social

# Ed25519 signing key (base64-encoded private key bytes)
# Generate with: openssl rand -base64 32
SIGNING_KEY=

# Publish API secret (shared with Tauri app)
# Generate with: openssl rand -hex 32
PUBLISH_SECRET=

# Data directory (inside container)
DATA_DIR=/data

# Logging level
RUST_LOG=info

# Optional: Prometheus metrics port
# METRICS_PORT=9090
```

### Files to create

| File | Purpose |
|------|---------|
| `deploy/Dockerfile` | Multi-stage build for server binary |
| `deploy/docker-compose.yml` | Orchestrate server + Caddy |
| `deploy/Caddyfile` | Reverse proxy with auto-TLS |
| `deploy/.env.example` | Secrets template |
| `deploy/README.md` | Deployment guide |

---

## P1: Deployment Guide

Write `deploy/README.md` with step-by-step deployment:

### Contents

```markdown
# Deploying Bytescape Identity Resolver

## Prerequisites
- A VM or VPS with Docker and Docker Compose installed
- A domain (bytescape.social) with DNS pointed to the VM
- SSH access to the server

## Quick Start

1. Clone the repo on your server:
   git clone https://github.com/your-org/bytescape.git
   cd bytescape/deploy

2. Create .env from template:
   cp .env.example .env
   # Edit .env — set SIGNING_KEY and PUBLISH_SECRET

3. Generate secrets:
   # Signing key (Ed25519 seed)
   openssl rand -base64 32
   # Publish secret
   openssl rand -hex 32

4. Start services:
   docker compose up -d

5. Verify:
   # Health check
   curl https://bytescape.social/health

   # After publishing from the app:
   curl https://bytescape.social/.well-known/webfinger?resource=acct:alice@bytescape.social
   curl https://bytescape.social/alice/did.json
   curl https://bytescape.social/.well-known/atproto-did
   curl -H "Accept: application/activity+json" https://bytescape.social/@alice

## DNS Setup

Point these records to your server IP:
- A record: bytescape.social → <server-ip>
- AAAA record: bytescape.social → <server-ipv6> (optional)

Caddy handles TLS automatically via Let's Encrypt.

## Updating

git pull
docker compose build
docker compose up -d

## Monitoring

docker compose logs -f server
docker compose logs -f caddy

## Backup

The server stores data in a Docker volume. Back up:
docker compose exec server cp /data/identities.json /backup/
# Or mount a host volume in docker-compose.yml
```

---

## P2: Systemd Service (Alternative to Docker)

For operators who prefer bare-metal deployment:

### `deploy/systemd/bytescape-server.service`

```ini
[Unit]
Description=Bytescape Identity Resolver
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=bytescape
Group=bytescape
WorkingDirectory=/opt/bytescape
ExecStart=/opt/bytescape/bytescape-server
EnvironmentFile=/opt/bytescape/.env
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

# Hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/bytescape/data
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

### Files to create

| File | Purpose |
|------|---------|
| `deploy/systemd/bytescape-server.service` | Systemd unit file |

---

## P3: Health Check and Monitoring

### Server health endpoint

The server (built by Track A) must expose `GET /health` returning:

```json
{
    "status": "ok",
    "version": "0.1.0",
    "identities_count": 1,
    "uptime_seconds": 3600
}
```

### Docker healthcheck

Already included in `docker-compose.yml` via `healthcheck` directive.

### External monitoring (document, don't implement)

In `deploy/README.md`, document these options for the operator:
- **UptimeRobot** or **Uptime Kuma**: Monitor `https://bytescape.social/health`
- **Caddy metrics**: Caddy has built-in Prometheus metrics at `:2019/metrics`
- **Log rotation**: Caddy logs rotate automatically. Server logs via Docker `json-file` driver with `max-size: 10m, max-file: 3`

### Docker log rotation

Add to `docker-compose.yml` server service:

```yaml
logging:
  driver: json-file
  options:
    max-size: "10m"
    max-file: "3"
```

---

## P4: Security Checklist

Document in `deploy/README.md` security section:

### Pre-launch

- [ ] `.env` file is `chmod 600` and not committed to git
- [ ] `PUBLISH_SECRET` is a cryptographically random 256-bit value
- [ ] `SIGNING_KEY` is generated fresh, not reused from dev
- [ ] Docker containers run as non-root user
- [ ] Firewall allows only ports 80, 443, and SSH
- [ ] SSH uses key authentication only (no passwords)

### Post-launch

- [ ] Verify TLS certificate: `curl -vI https://bytescape.social 2>&1 | grep -i "SSL certificate"`
- [ ] Verify HSTS header: `curl -I https://bytescape.social | grep Strict`
- [ ] WebFinger resolves correctly
- [ ] DID document resolves correctly
- [ ] Profile page loads correctly
- [ ] Publish API rejects unauthenticated requests: `curl -X POST https://bytescape.social/api/publish/did` → 401

### Ongoing

- Rotate `PUBLISH_SECRET` every 90 days
- Update Docker images monthly: `docker compose pull && docker compose up -d`
- Review Caddy access logs for anomalies

---

## Acceptance Criteria

After deployment + identity publish from the app:

```bash
# 1. WebFinger resolves
curl -s "https://bytescape.social/.well-known/webfinger?resource=acct:{username}@bytescape.social" | jq .subject
# → "acct:{username}@bytescape.social"

# 2. DID Document resolves
curl -s "https://bytescape.social/{username}/did.json" | jq .id
# → "did:web:bytescape.social:{username}"

# 3. AT Protocol DID resolves
curl -s "https://bytescape.social/.well-known/atproto-did"
# → "did:web:bytescape.social:{username}"

# 4. Profile page works
curl -s "https://bytescape.social/@{username}" | grep "<title>"
# → contains username

# 5. AP Actor works
curl -s -H "Accept: application/activity+json" "https://bytescape.social/@{username}" | jq .type
# → "Person"

# 6. Health check
curl -s "https://bytescape.social/health" | jq .status
# → "ok"

# 7. Unauthenticated publish rejected
curl -s -o /dev/null -w "%{http_code}" -X POST "https://bytescape.social/api/publish/did"
# → "401"
```

---

## Track Boundaries

- **This track owns**: `deploy/` (all deployment config, Docker, Caddy, systemd)
- **Track A owns**: `server/` (the actual server binary this deploys), `src-tauri/src/` (Rust app)
- **Track B owns**: `src/` (React/TypeScript)
- **Coordination**: The Dockerfile must match Track A's server crate structure. If Track A changes the crate layout, update Dockerfile accordingly.

---

## Important Notes

1. **Single-user launch**: The compose file runs ONE server with ONE signing key. Multi-user (registration, email verification) is a future sprint.
2. **Data persistence**: Identity data is stored in a Docker volume. For durability, add host-mounted volume or backup cron.
3. **No database**: For single-user, the server stores identities in a JSON file. Database (redb/SQLite) is a future sprint.
4. **Caddy handles TLS**: No manual cert management. Just point DNS and Caddy auto-provisions via Let's Encrypt/ZeroSSL.
5. **The `.env` file is the only secret store**: Never commit it. The `.env.example` documents all required vars.
