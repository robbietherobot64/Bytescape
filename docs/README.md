# Bytescape — Project Guide

Living map of what this project is, how work happens, and where everything lives.
Current state and next actions: `STATUS.md`. Product truth: `VISION.md`. Decision
history: `DECISIONS.md`. Everything here is repo-root-relative.

## What we're building

An AT Protocol social platform for people leaving walled gardens: import your
history, find your friends, own a profile worth showing. Not designed to be
addictive. Full vision, scope gates, and success ladder: `VISION.md` and
`wiki/product/mvp-scope.md`.

## Where things are

| What | Where |
|---|---|
| Vision, status, decisions (canon) | `VISION.md`, `STATUS.md`, `DECISIONS.md` |
| Server (Rust: PDS + AppView + API) | `bytescape-server/` |
| Frontend (React + Vite) | `apps/web/` (npm workspace) |
| Standalone Events site (React + Vite) | `apps/events/` → live at `events.bytescape.social` (test: `events.test.bytescape.social`) |
| Shared section UI (Events) | `packages/events-ui/` — mounted by both hosts |
| Backfill helpers (events) | `ops/backfill-calendar-*` |
| Per-change specs (OpenSpec) | `openspec/changes/<name>/`, canonical specs in `openspec/specs/` |
| Requirements/plans (Compound Engineering) | `docs/plans/`, learnings in `docs/solutions/` |
| Enforcement rules (pack) | `compound-packs/bytescape/` |
| Product reference (ship gates, screen specs) | `wiki/product/`, `wiki/design/` |
| Tutoring curriculum | `docs/learning/` |
| Test server quick ref (invite codes, minting) | `docs/test-server.md` |
| Server deployment (Caddy, systemd, OAuth metadata) | `deploy/` |
| Hosted public wiki — source, tooling, route, schedule | `wiki/` → `deploy/wiki-build.py` + `deploy/wiki-publish.sh` → `/wiki` on both hosts, daily GitLab pipeline (04:00 UTC) + main pushes touching `wiki/`; `wiki/product/code-audit.md` excluded |
| Historical artifacts (completed migrations, old research) | `docs/archive/` |
| v1 reference material (sprints, old tooling) | `_ref/` |

## How work happens

One precedence chain: **`VISION.md` > `wiki/product/mvp-scope.md` (ship gates) >
`wiki/product/prd.md` (archived reference)**. Design decisions come from the user —
never inferred.

Per non-trivial change:

1. Shape the idea (`ce-brainstorm` if it needs dialogue).
2. `openspec-propose` → proposal + specs + design + tasks in `openspec/changes/<name>/`.
3. `ce-plan` enriches the change folder with guardrails and test scenarios.
4. Implement with `ce-work` / `ce-simplify-code`; `ce-code-review` before showing results.
5. Verify with exercised evidence: builds green **and** real-browser proof of the
   changed surface.
6. `ce-compound` captures learnings (they ground future planning and can be promoted
   into pack rules); `openspec sync-specs` then `openspec archive-change`.

No stages, checkpoints, or memlogs. Small fixes and investigation skip the spec flow.

## Tools (live, and what each is for)

- **omp** — the agent harness this project runs in.
- **OpenSpec** — the planning layer: durable behavior contracts per change.
- **Compound Engineering** — the implementation layer: work, review, and the
  compounding loop (`docs/solutions/`). `compound-packs/bytescape/` = prescriptive
  rules planning grounds in and review enforces.
- **CodeDebrief** (optional) — flow explanations and release-time model refresh; not
  a per-commit obligation.
- **Obsidian** — the repo root is the vault; these markdown files are the whole
  documentation surface.

Retired deliberately (see `DECISIONS.md`, 2026-10-10): BMAD, GitNexus, context-mode,
the old kanban/plane stack.

## Running the stack

- **Local:** Postgres on `:5432` (see `docker-compose.yml` for a containerized one) +
  `cargo run` in `bytescape-server/` (`:8080`) + `npm run dev:web` at the repo root
  (`:1420`); the standalone Events site is `npm run dev:events` (`:1421`).
- **Production:** Hetzner VPS `46.62.132.214`, provisioned by `deploy/setup-server.sh`;
  Caddy terminates TLS and serves the OAuth client metadata statically (it survives app
  downtime); app runs as the `bytescape` systemd unit. Sign-in is Bluesky OAuth;
  native email sign-up exists behind the invite gate, and fediverse (Mastodon)
  sign-in is landing with the ActivityPub integration.
- Schema note: the migration runner skips a block of files on fresh databases —
  the manual patch list is tracked internally until real migration tooling
  lands (roadmap item in `STATUS.md`).
