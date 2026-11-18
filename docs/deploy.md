# Deploy topology and rules (authoritative)

This file exists because a wrong `rsync` target once overwrote the **production**
frontend while deploying a **test** build. Read it before deploying anything.

## Host map (verified 2026-11-16 against the VPS Caddyfile)

`bytescape.social` (prod) and `test.bytescape.social` (test) are served from the
**same box** by one Caddy process, with separate backend processes, env files,
static roots and databases. They are *not* separate machines.

| Piece | Production | Test |
|---|---|---|
| Public host | `bytescape.social` | `test.bytescape.social` |
| SPA static root | `/srv/bytescape/frontend/` | `/srv/bytescape-test/frontend/` |
| Backend binary | `/opt/bytescape/bytescape-server` | `/opt/bytescape-test/bytescape-server` |
| Backend env | `/opt/bytescape/.env` | `/opt/bytescape-test/.env` |
| Backend port (loopback) | `8080` | `8081` |
| systemd unit | `bytescape.service` | `bytescape-test.service` |
| OAuth metadata | `/srv/bytescape/static/` | `/srv/bytescape-test/static/` |
| Events site | `/srv/bytescape/events/` | `/srv/bytescape-test/events/` |
| Wiki | `/srv/bytescape/wiki/` | (same Caddy block, prod root) |

Caddy source of truth in-repo: [`deploy/Caddyfile`](../deploy/Caddyfile). The
live file is `/etc/caddy/Caddyfile` on the box; keep them in sync. Note: the
repo copy currently documents the prod/feeds/events blocks only — the live file
also carries the `test.bytescape.social` block and the `*.bytescape.social`
wildcard used above for the test roots.

## The rule: never raw-rsync a frontend

**NEVER run `rsync .../dist/ root@<host>:/srv/...` by hand.** The two roots are
one path segment apart and a copy-paste lands a test build on prod.

Use the npm scripts — they own the target paths:

```sh
# Test frontend (safe, the normal loop)
npm run deploy:test:web      # builds apps/web then rsync → /srv/bytescape-test/frontend/

# Production frontend (guarded)
PROD_DEPLOY=1 npm run deploy:prod:web
```

`deploy:prod:web` refuses unless `PROD_DEPLOY=1` is set, prints the SSH host and
root it is about to write, and requires the operator to type the target host
name before it builds or copies anything. Do not bypass it.

## Backend deploy (per environment)

Never replace a binary in place. Back up, copy to `.new`, `mv`, restart:

```sh
# test
ssh root@test.bytescape.social \
  'cp -a /opt/bytescape-test/bytescape-server /opt/bytescape-test/bytescape-server.bak.$(date +%Y%m%d-%H%M%S)'
cd bytescape-server && cargo build --release
scp target/release/bytescape-server root@test.bytescape.social:/opt/bytescape-test/bytescape-server.new
ssh root@test.bytescape.social \
  'chmod +x /opt/bytescape-test/bytescape-server.new && \
   mv /opt/bytescape-test/bytescape-server.new /opt/bytescape-test/bytescape-server && \
   systemctl restart bytescape-test.service'
```

Prod uses the same pattern against `/opt/bytescape/bytescape-server` and
`bytescape.service`, and is an explicit owner decision — do not deploy prod
because a task said "deploy".

## Verification after a deploy

- Test backend: `curl -s localhost:8081/api/health` **on the box** (the public
  host goes through Caddy; `localhost:8080` is prod, not test).
- Test frontend: the served `index.html` asset hash must match the locally built
  `apps/web/dist/index.html` **and** the referenced asset must answer with a
  real JavaScript content type (an SPA-fallback `text/html` means the asset is
  missing — the copy raced). `scripts/deploy-web-test.sh` runs exactly this
  check automatically after every deploy and fails the deploy on mismatch.
- Prod frontend: `curl -s https://bytescape.social/ | grep -o 'index-[^"]*'`
  (the same served-asset check runs in `scripts/deploy-web-prod.sh`).

## Asset deploy ordering — do not "simplify" it away

`deploy:test:web` and `deploy:prod:web` copy **`assets/` before `index.html`**,
and the assets copy is deliberately additive (no `--delete` before the entry
flip). Why: Caddy serves hashed `/assets/*` with
`Cache-Control: max-age=31536000, immutable` (see `@spa_assets` in the
Caddyfile). A single non-atomic `rsync dist/` has a window where the served
`index.html` already references NEW asset hashes whose files have not landed;
a browser that fetches one in that window receives the SPA fallback
(`index.html` as `text/html`) and caches it **as immutable for a year** — a
permanently black app for that user until a cache-bypassing hard reload. (This
happened on 2026-11-18 during the identity-graph deploys.)

Assets-first ordering removes the window in both directions: new hashes exist
before anything can reference them, and old hashes keep serving to clients
holding the old entry document. The cost is that orphaned asset files
accumulate until pruned.

**Follow-ups (not implemented — owner decision):**
1. Versioned deploy dirs + a symlink flip (`/srv/…/frontend-<build>` +
   `ln -sfn`) — removes the race structurally and makes rollback a link flip;
   prune old versions on a schedule.
2. Drop `immutable` for SPA-fallback responses in Caddy (keep it only when
   `file_server` actually matched a file) — turns any residual race from
   "year-long blackout" into "one reload".

**Prod deploys share this exact race** — the ordering above must be (and is)
present in `scripts/deploy-web-prod.sh` before any prod sync.

## History

- 2026-11-16: a test FE deploy rsynced to `/srv/bytescape/frontend` (prod) by
  mistake. Prod static assets were replaced; the pre-change tree (`edbf13e`) was
  rebuilt and restored. Prod's exact prior artifact had no backup. This doc and
  `deploy:prod:web` exist to make that impossible.
- 2026-11-18: the immutable-cache × non-atomic-rsync race (documented above)
  blacked out a verification browser for a year-long cache TTL. Both web deploy
  scripts now copy `assets/` before `index.html` and self-verify the served
  asset content type.
