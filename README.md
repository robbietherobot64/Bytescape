# Bytescape

Import your history from Facebook and Instagram, reconstruct your social graph on
AT Protocol, and keep one account that also federates over ActivityPub.

Bytescape is an open social platform for people leaving walled gardens. You bring
your Meta GDPR data archive; Bytescape rebuilds your posts, photos, videos, and
stories as AT Protocol records — the same network as Bluesky — and helps you find
the people from your old network who are already here. Your account is also an
ActivityPub actor, so the fediverse (Mastodon, Pixelfed, and friends) can follow
you too. The feed is chronological and the profile is yours to design. Not built
to be addictive: utility over habit.

**Status:** pre-alpha. See [ROADMAP.md](ROADMAP.md) for the gates that must pass
before the public alpha opens, and [VISION.md](VISION.md) for what we're building
and why.

## Architecture

A Rust monolith and two React SPAs:

- **`bytescape-server/`** — one Rust binary with four roles:
  - an AT Protocol **PDS**: hosted repositories, XRPC endpoints, and a firehose
    (`com.atproto.sync.subscribeRepos`) other clients can consume;
  - a **firehose consumer** that indexes the Bluesky network into a local,
    queryable copy;
  - a **derived index** (AppView) powering feeds, search, and graph lookups
    without waiting on remote services;
  - a **REST API** for the web apps (posts, import, chat, events, stories, …).

  Records follow AT Protocol Lexicon schemas; the Postgres schema lives in
  `bytescape-server/migrations/` and is applied on startup.
- **`apps/web/`** — the main Bytescape SPA (React + Vite).
- **`apps/events/`** — a standalone events site; the shared events UI is
  `packages/events-ui/`.
- **`openspec/specs/`** — behavior contracts for the major subsystems (identity,
  import, feed ingestion, and more): the most precise statement of what the code
  is supposed to do.

Storage is PostgreSQL plus S3-compatible object storage — MinIO works out of the
box, and without S3 configuration the server falls back to local disk.

More depth: [`wiki/`](wiki/) (architecture, database map, screen designs; also
published at bytescape.social/wiki) and [`docs/`](docs/) (deployment, testing,
project notes).

## Quickstart

Prerequisites: a Rust toolchain (the vendored crates pin 1.86; the Docker build
uses 1.82), Node.js 20+, and Docker — or your own Postgres and S3.

`docker compose up` is the first step for local development **and** for a real
instance: it starts the backing services (Postgres on `:5432`, MinIO on `:9000`).
The app tier is then run from source (below) or deployed per
[docs/self-hosting.md](docs/self-hosting.md).

### 1. Backing services

```sh
docker compose up -d
```

### 2. Environment

```sh
cp .env.example .env
```

`.env.example` documents every variable: database, JWT, OAuth, SMTP, captcha,
access control. Set `JWT_SECRET` to a random string of at least 32 characters
before anything else. The server loads `.env` from its working directory or any
parent; `bytescape-server/.env.example` shows the server-local toggles (such as
`REQUIRE_INVITE_CODE`) if you want to override them just for `bytescape-server/`.

### 3. Server

```sh
cd bytescape-server
cargo run          # REST + XRPC API on :8080
```

### 4. Web apps

```sh
npm ci
npm run dev:web     # main app on http://localhost:1420
npm run dev:events  # events site on http://localhost:1421
```

### Tests

```sh
cd bytescape-server && cargo test   # backend suite
npm test -w apps/web                # web suite (vitest)
```

Database-backed backend tests read `DATABASE_URL` (the compose Postgres works)
and skip when it's unset. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full
verification bar.

## Running a real instance

The self-host quickstart is [docs/self-hosting.md](docs/self-hosting.md): what to
run (Postgres, MinIO/S3, the server, the frontend behind Caddy), how the pieces
fit, and the honest caveats. Three of them matter before you open an instance to
other people:

- **Relay dependency** — network feed ingestion rides the Bluesky-operated relay
  today; its availability and policies are outside your control.
- **Moderation is yours** — every instance operator is responsible for the
  content on and through their instance.
- **Updates need care** — migrations are applied at startup; back up before
  upgrading.

## Contributing

Issue first, then a PR — see [CONTRIBUTING.md](CONTRIBUTING.md) for the flow,
test conventions, and review expectations. AI-assisted contributions are welcome
under the mandatory verification rules described there.

Security issues: report privately per [SECURITY.md](SECURITY.md), not in a public
issue.

## License

[AGPL-3.0-only](LICENSE). One honest reason: if you run a modified Bytescape as a
service, you must share your changes — the point of the project is portable,
user-owned data, and closed forks would turn it into yet another walled garden.