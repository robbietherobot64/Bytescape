# Getting started

Bytescape is one repository: a Rust backend, two React frontends, and a
shared UI package, all coordinated through an npm workspace.

## Repository layout

| Path | What it is |
|---|---|
| `bytescape-server/` | Rust binary — PDS, AppView, REST API (`src/pds/`, `src/appview/`, `src/api/`, `src/db/`) |
| `bytescape-server/migrations/` | Postgres schema — the source of truth for the database |
| `apps/web/` | Main SPA (React + Vite) — feed, profile, import, stories, chat |
| `apps/events/` | Standalone Events site (React + Vite) |
| `packages/events-ui/` | Events component tree shared by both frontends |
| `deploy/` | Server bootstrap: Caddy, systemd, OAuth metadata, wiki publish tooling |
| `openspec/` | Planning layer: per-change proposals, specs, tasks |
| `wiki/` | This public wiki (published by `deploy/wiki-build.py`) |
| `docs/` | Internal working docs: plans, learning, archives |

The full map (tools, process, environments): the
[Project guide](../docs/README.md).

## Run it locally

Prerequisites: **Rust** (1.85+), **Node.js** (20+), **Postgres** — or
`docker compose up -d` for a containerized Postgres.

```bash
# 1. Backend — applies migrations on startup, serves :8080
cd bytescape-server
cp .env.example .env        # set DATABASE_URL
cargo run

# 2. Main app — http://localhost:1420
npm install
npm run dev:web

# 3. Events site (optional) — http://localhost:1421
npm run dev:events
```

## Tests and checks

```bash
cargo test                  # needs DATABASE_URL pointing at a test database
npm run build:web           # typecheck + build the main SPA
npm run build:events        # typecheck + build the Events site
```

CI runs the same three as `backend-check`, `frontend-check`, and
`events-check` on every merge request. Verification bar for any change:
builds green **and** exercised evidence for the changed surface
([Contributing](contributing.md)).

## Live environments

| Host | What |
|---|---|
| `bytescape.social` | Production |
| `test.bytescape.social` | Primary working instance — deploy first, verify, then prod |
| `events.test.bytescape.social` | Standalone Events site (test) |
| `feeds.bytescape.social` | Topic feed service (publishes real, followable AT feeds) |

Deployment is systemd + Caddy on a Hetzner VPS; the server binary and static
builds ship per host (`docs/test-server.md` in the repo has the per-host
deploy steps and invite-code table).

## Next

- How changes are proposed, verified, and shipped: [Contributing](contributing.md)
- What's being worked on right now: [Current work](current-work.md)
