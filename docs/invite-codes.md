# Invite codes — quick reference

All known invite codes, per host. **Codes are per-host — a code minted on one
host is invalid on the other.** Enter on the "invite code" field of the
Bluesky signup landing page.

> **Refresh this file from the API** — it is a snapshot (2026-11-12):
> `curl -s "https://<host>/api/admin/invite-codes"`. Usage counts drift.

## test.bytescape.social

| Code | Max uses | Used | Status | Notes |
|---|---|---|---|---|
| `SBTPHJ8AM55F27YN` | 25 | 0 | active | fresh — prefer this one |
| `U1CNR86M1DDWN` | 100 | 3 | active | high-capacity |
| `9P2AJSRU44JKN27D` | 50 | 11 | active | |
| `3UGTW5X2QSWAPJZH` | 25 | 6 | active | the original "use this one" |
| `CY2KP8ATQ3Y49DT2` | 5 | 2 | active | |
| `G5HV6LJ8MSCE6M6Z` | 5 | 0 | active | unused spare |
| `S2H5TN7QC2BCSU2F` | 5 | 2 | active | |
| `ZTXB9M57KMZFQEX7` | 5 | 5 | exhausted | |
| `T4Y4XCM6BSC67KLY` | 5 | 5 | exhausted | |
| `C9D2A9DD220F6B0E` | 1 | 1 | exhausted | |
| `18B732F4032BD77D` | 1 | 1 | exhausted | |
| `7F5728F19EB2F0EE` | 1 | 1 | exhausted | |

## bytescape.social

| Code | Max uses | Used | Status | Notes |
|---|---|---|---|---|
| `3YATPK4QECFHDTCD` | 25 | 0 | active | reusable |
| `TMYYETZPXUPK4Z3K` | 5 | 0 | active | spare |
| `founder-9q4x7k2m` | 10 | 1 | active | named code |
| `Q9VH27FR28A69GPX` | 1 | 0 | active | single-use spare |

## Mint / manage

```sh
# mint (per host) — max_uses is a QUERY param; a body value is silently
# ignored and the code defaults to 1 use
curl -X POST "https://test.bytescape.social/api/admin/invite-codes?max_uses=25"
curl -X POST "https://bytescape.social/api/admin/invite-codes?max_uses=25"
# → {"code":"...","max_uses":25}

# list (source of truth for refreshing this file)
curl -s "https://test.bytescape.social/api/admin/invite-codes"
curl -s "https://bytescape.social/api/admin/invite-codes"
```

Gotchas (from the [test-server runbook](test-server.md)):
- Codes live in each host's own Postgres (`invite_codes` table) — never shared.
- ⚠️ The admin endpoint is **unauthenticated** (flagged in
  `wiki/product/code-audit.md`); needs admin-token protection before the URL
  is public. Fine for private testing.
- The events site mints no codes of its own.
