# Design: Linked Identity Model (Fediverse → Profile)

**Status:** Draft for review — not yet implemented
**Date:** 2026-11-09
**Motivation:** A fediverse-signed-in user's profile is empty even after their
posts are imported. Root cause is an identity-resolution gap, not ingestion.

---

## 1. Problem

When a user signs in with a fediverse account (`@user@host`), Bytescape
backfills their AP post history into `appview_posts`, attributed to the
**remote** identity:

- `author_handle = @lovingisliving@indieweb.social`
- `author_did = https://indieweb.social/ap/users/116307373277390796`

But the profile renders by looking up the **local** `users.username`
(`lovingisliving`) and reading `posts` + linked Bluesky accounts. Nothing joins
the local account to the remote actor, so:

- The profile never queries `appview_posts` for the remote-attributed posts.
- Even if it did, it would match by local username, which never equals the
  remote handle (`@lovingisliving@indieweb.social`).

**Verified evidence** (live, before test reset): 16 ingested rows had
`author_handle=@lovingisliving@indieweb.social`; the profile query
`author_handle ILIKE '%<local username>%'` returned 0. The signature/backfill
itself works (`16 rows ingested`); only display is disconnected.

### Why this matters beyond the bug

The user's stated goal: robust identity linking that supports **migrating other
identities in** and **migrating an identity to other instances**. That requires
identity to be a *portable, linkable record* — not a baked-in username. This
design establishes that foundation.

---

## 2. Design principles

1. **Identity is a set of linked remote actors, not a username.** A Bytescape
   account is a local *profile*; its identity is the collection of remote
   actors (across instances/protocols) that resolve to it.
2. **Canonical key = remote actor URL** (e.g. `https://indieweb.social/ap/users/116307373277390796`).
   Handles (`user@host`) are mutable labels; actor URLs are stable. (User
   decision, 2026-11-09.)
3. **Local username is a display/collision label only.** Suggested from the
   primary handle; collision-resolved (`name`, `name1`, `name2`, …). Decoupled
   from identity resolution.
4. **Every content row resolves to an author through the linked-identity map**,
   never through a username string guess.

---

## 3. Data model

### 3.1 `linked_accounts` (existing table — reuse)

Existing columns: `id, user_id, provider, external_id, external_username,
token_type, access_token, refresh_token, token_expires_at, scope,
dpop_private_key, metadata, created_at, updated_at`.

**Repurpose for identity, keeping auth tokens separate:**

| Column | Role in identity model |
|---|---|
| `user_id` | Links remote identity → Bytescape account (many per user). |
| `provider` | `mastodon` / `bluesky` / `pixelfed` / … (which AP/AT flavor). |
| `external_id` | **Canonical key = remote actor URL.** (Currently holds the handle `me.acct`; migrate to actor URL — see 5.2.) |
| `external_username` | Cached handle label (`lovingisliving@indieweb.social`) for display + fallback lookup. |
| `metadata` | `{ "instance": "indieweb.social", "avatar": …, "display_name": … }`. |

Auth tokens (`access_token`, `refresh_token`, `dpop_private_key`) stay as-is —
they are per-linked-identity OAuth credentials, orthogonal to identity
resolution.

**New columns (migration):**

```sql
ALTER TABLE linked_accounts
  ADD COLUMN IF NOT EXISTS is_primary boolean NOT NULL DEFAULT false,
  ADD COLUMN IF NOT EXISTS actor_url text,          -- canonical key (see 5.2)
  ADD COLUMN IF NOT EXISTS verified_at timestamptz; -- when the actor URL was confirmed
CREATE UNIQUE INDEX IF NOT EXISTS idx_linked_actor_url
  ON linked_accounts (actor_url) WHERE actor_url IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_linked_user ON linked_accounts (user_id);
```

- `actor_url` is the **authoritative** key. `external_id` becomes a legacy
  mirror during migration (5.2) then is dropped.
- `is_primary` marks the user's headline identity (drives the suggested local
  username + default profile attribution).

### 3.2 `appview_posts` — no schema change

Rows already carry `author_did` (the remote actor URL) and `author_handle`.
Resolution goes through `linked_accounts.actor_url`, so no new column is needed.
(A `author_did` index is recommended for the profile query — see 6.)

---

## 4. Flows

### 4.1 Sign-in (fediverse): capture the identity

`mastodon_callback` / `oauth_complete` / `consume_temp_token` already receive
the remote identity (`me.acct`, `me.id` = actor URL). Change:

1. Store `actor_url` = the remote actor URL (from `verify_credentials.id`, the
   canonical `https://host/ap/users/...`), not the handle.
2. Upsert a `linked_accounts` row keyed on `actor_url` (idempotent — re-login
   reuses it, never duplicates).
3. Set `is_primary` if it's the user's first linked identity.
4. Trigger backfill as today (it already uses the identity to fetch history).

**This is the fix for the "identity thrown away" gap.**

### 4.2 Profile resolution (the display fix)

`public_feed` / `build_profile_response` resolve the author's posts through the
linked-identity map:

```
user.id
  → linked_accounts WHERE user_id = user.id  (set of actor_urls)
  → appview_posts WHERE author_did = ANY(actor_urls)
```

Concretely, add to `public_feed` (and any profile-posts source):

```sql
SELECT … FROM appview_posts
WHERE author_did = ANY($linked_actor_urls)
  AND reply_parent IS NULL
ORDER BY created_at DESC
LIMIT …
```

This replaces the current username-string guess (`author_handle ILIKE '%name%'`)
with the authoritative actor-URL match. Existing local (`posts`) + Bluesky
sources remain; `appview_posts` becomes a third, identity-resolved source.

### 4.3 Collision-resolved local username (at signup)

The local username is a *label*. Suggested from the primary handle's
`preferredUsername` (`lovingisliving`). If taken, resolve by appending a
counter:

```
lovingisliving  → taken → lovingisliving1 → taken → lovingisliving2 …
```

Because identity resolution does **not** depend on the username (it uses
`linked_accounts`), a collision-resolved username is purely cosmetic and never
breaks post attribution.

### 4.4 Migration

Because identity = portable `linked_accounts` rows keyed by actor URL:

- **Migrate identity in** (add another handle/instance): insert a new
  `linked_accounts` row (`actor_url` = the new remote actor). Its posts flow
  into the same profile automatically (4.2 matches any linked actor).
- **Migrate identity out / to another instance**: the user adds the actor URL on
  the new instance as a linked identity; the old actor URL can be marked
  `verified_at = NULL` (unverified/legacy) or detached. Content attribution
  follows the actor URLs, so it's a data operation, not a rewrite.
- **Merge identities**: multiple `linked_accounts` rows → one `user_id` already
  merges them at the profile (4.2 unions all actor URLs).

---

## 5. Migration plan (existing data)

### 5.1 Backfill the missing link for the affected account
For the test account, upsert the linked identity explicitly (SQL or a one-shot):
```
linked_accounts(user_id=<user>, provider='mastodon',
  actor_url='https://indieweb.social/ap/users/116307373277390796',
  external_username='lovingisliving@indieweb.social',
  is_primary=true, metadata.instance='indieweb.social')
```

### 5.2 `external_id` → `actor_url`
- Add `actor_url` (3.1).
- Backfill: for `provider='mastodon'`, set `actor_url` = the resolved actor URL
  for `external_id` (WebFinger `self` link). For `bluesky`, `actor_url` = the
  `did:` (already stable).
- New writes set `actor_url` directly; `external_id` mirrors it until dropped.

---

## 6. Performance

The profile query `author_did = ANY(actor_urls)` needs an index on
`appview_posts.author_did` (currently unindexed → full scan on 3.7M rows, the
`ILIKE` we saw time out). Add:

```sql
CREATE INDEX IF NOT EXISTS idx_appview_posts_author_did ON appview_posts (author_did);
CREATE INDEX IF NOT EXISTS idx_appview_posts_author_handle ON appview_posts (author_handle);
```

---

## 7. Edge cases

| Case | Handling |
|---|---|
| Username collision | Counter-suffix (4.3); identity unaffected. |
| Handle renamed on remote instance | `actor_url` is the key (unchanged); `external_username` refreshed on next fetch. |
| User has identities on multiple instances | Union of actor URLs in 4.2. |
| Same remote actor linked to two Bytescape accounts | `idx_linked_actor_url` unique → second link rejected (prevents split attribution). |
| Remote actor URL changes (instance migration on the remote side) | Old URL kept as historical key; new URL added as a linked identity; both resolve to the profile. |
| Local-only user (no remote identity) | Profile reads `posts` + local sources as today; `appview_posts` source matches an empty actor set → no-op. |

---

## 8. What this does NOT change

- The HTTP-signature / backfill ingestion path (already fixed and working).
- AT (Bluesky) rows and the AT read path (untouched, per ingest isolation).
- Auth/OAuth token handling (orthogonal to identity resolution).

---

## 9. Implementation slices

**Slice 1 — Identity linkage + profile visibility** (this review's scope):
1. Migration: add `actor_url`, `is_primary`, `verified_at`, indexes (3.1, 6).
2. Sign-in captures + upserts the linked identity by `actor_url` (4.1).
3. Profile resolves imported posts via `linked_accounts.actor_url` (4.2).
4. Backfill the test account's link (5.1).
5. Verify: profile shows imported posts (once the user re-imports).

**Slice 2 — Migration + UI** (after Slice 1 verified):
1. Collision-resolved username suggestion (4.3).
2. Link additional identities / migrate across instances (4.4).
3. Identity-management UI.

---

## 10. Open questions for review

1. **Canonical key confirmation** — actor URL is chosen; confirm `did:` is
   acceptable as the Bluesky actor URL equivalent (stable, already used).
2. **Primary identity semantics** — is `is_primary` the identity whose handle
   drives the suggested username + default attribution, or should there be a
   separate "display identity"?
3. **Merge policy** — when two Bytescape accounts claim the same remote actor,
   reject the second (proposed) vs. allow a user-initiated merge?
4. **Username migration** — if a user renames their local username later, do we
   keep the old one as a redirect (profile URL stability)?
