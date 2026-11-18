# Test Server — Quick Reference

## Hosts (each has its own DB and invite codes!)

| Host | Purpose | Invite gate |
|---|---|---|
| `https://test.bytescape.social` | **Test server (use this one)** | `REQUIRE_INVITE_CODE=true` |
| `https://bytescape.social` | Prod/other deployment | `REQUIRE_INVITE_CODE=true` |

Codes are **per-host** — a code minted on one host is invalid on the other.
The user-facing test server is `test.bytescape.social`.

## Invite codes (signup gate)

> **All known codes live in [`docs/invite-codes.md`](invite-codes.md)** —
> check there first instead of re-querying the API.

Enter the code in the "invite code" field on the Bluesky signup landing page.

### test.bytescape.social (minted 2026-11-07)

| Code | Uses | Notes |
|---|---|---|
| `3UGTW5X2QSWAPJZH` | 25 | reusable — use this one |
| `G5HV6LJ8MSCE6M6Z` | 5 | unused spare |
| `CY2KP8ATQ3Y49DT2` | 5 | partially used |
| `S2H5TN7QC2BCSU2F` | 5 | partially used |
| `T4Y4XCM6BSC67KLY` | 5 | partially used |

### bytescape.social (minted 2026-11-07)

| Code | Uses | Notes |
|---|---|---|
| `3YATPK4QECFHDTCD` | 25 | reusable |
| `Q9VH27FR28A69GPX` | 1 | single-use spare |

## Mint a new code (per host)

```sh
# test server
curl -X POST "https://test.bytescape.social/api/admin/invite-codes?max_uses=25"

# bytescape.social
curl -X POST "https://bytescape.social/api/admin/invite-codes?max_uses=25"
# → {"code":"...","max_uses":25}
```

Gotchas:
- `max_uses` is a **query param**, not JSON body (a body value is silently ignored → defaults to 1 use).
- List existing codes: `GET https://<host>/api/admin/invite-codes`
- ⚠️ The admin endpoint is **unauthenticated** (flagged in `wiki/product/code-audit.md`).
  Must get admin-token protection before the URL is public. Fine for private testing.

## Server facts

- Production env: `REQUIRE_INVITE_CODE=true`; Bluesky/Mastodon OAuth + `FRONTEND_URL`
  point at the deployment's own host.
- Invite codes live in the deployed server's Postgres (`invite_codes` table,
  migration 042) — not in this repo. Regenerate via the endpoint above; never
  commit live codes to source control other than this file.

## Events site (staging) — `events.test.bytescape.social`

Live since 2026-11-08. Same box, same test backend (`:8081`), its own static
root — so it deploys independently of the test web frontend.

| Piece | Where |
|---|---|
| Static build | `/srv/bytescape-test/events/` (rsync via `npm run deploy:test:events`) |
| Web sibling | `/srv/bytescape-test/frontend/` (`npm run deploy:test:web`) |
| Server binary | `/opt/bytescape-test/bytescape-server` (scp the local release build, then `systemctl restart bytescape-test`) |
| Caddy block | `events.test.bytescape.social` in the box's `/etc/caddy/Caddyfile` (exact-host block ahead of the `*.test` wildcard; validate + `systemctl reload caddy`) |
| TLS ask | Covered: `DOMAIN=test.bytescape.social` vouches single-label subdomains at `/caddy/ask` |

**Sign-in on the events site is an SSO hop** to `test.bytescape.social/login`
(family `?next=` round-trip) — invite codes are still per-host (table above);
the events site itself mints none.

**Backfill (launch/historical content):** run `ops/backfill-calendar-*` against
this host's `DATABASE_URL` (bounded author pull + idempotent re-key/scan).

**Prod deploy (owner, when ready):** apply the `events.bytescape.social` block
from `deploy/Caddyfile` to the box's Caddyfile, rsync `npm run build:events`
output to `/srv/bytescape/events/`, ship the release binary to
`/opt/bytescape/`, and run the `ops/backfill-calendar-*` scripts against the
prod DB.
