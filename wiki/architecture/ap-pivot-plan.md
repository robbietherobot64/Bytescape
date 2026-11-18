# ActivityPub Pivot — Pre-Task Plan (Wafrn as reference model)

**Status:** direction decided 2026-11-02; early phases now in active build as
`openspec/changes/add-activitypub-integration` (identity first: unified AT+AP
accounts with Bluesky / Mastodon / email sign-in, then flag-gated feed
ingestion). Resolved there on 2026-11-08: identity model, sign-in paths,
follow surface, ingestion model, ordering, and events dual-write. Still open:
the five questions at the bottom of this page.

**Decision:** Bytescape becomes a **dual-protocol platform** — native ActivityPub
alongside AT Protocol. Rationale: Bluesky-class features (posts, likes, follows,
feeds) work fine on AT Protocol, but the unique sections (stories, events,
classifieds, groups) need the fediverse as their content network — Pixelfed has
stories, Mobilizon/Gancio have events, Lemmy/Mobilizon have groups. Being
AT-Protocol-only means building every content niche ourselves against zero
audience; being ActivityPub-native lets users follow existing fediverse accounts
to populate those sections from day one.

## The Wafrn model (what we copy)

Wafrn runs **both protocols natively in one codebase** — not a bridge, not
cross-posting. Reference: [deep dive](https://rmendes.net/articles/2026/02/26/deep-dive-wafrn-dual-protocol-federation/),
[codeberg.org/wafrn/wafrn](https://codeberg.org/wafrn/wafrn).

1. **Native AP server** — WebFinger/NodeInfo, actor + inbox/outbox, HTTP
   Signatures, ~16 activity processors (Accept, Create, Follow, Like, Announce,
   Undo, Delete, Block, …). Inbox returns 200 fast, work happens on a queue
   (retry + backoff) — federation reliability depends on this.
2. **Official Bluesky PDS as a sidecar** — stock `ghcr.io/bluesky-social/pds`
   image, never reimplemented. Handles = `user.pds.domain` via wildcard TLS.
   Coupling only through stable interfaces (Jetstream firehose + ATProto HTTP
   API). PDS upgrades = bump the image tag.
3. **Unified user model** — one row carries AP identity (`remoteId`, inbox,
   RSA keypair) and ATProto identity (`bskyDid`, app password); `alsoKnownAs`
   cross-links the two (`at://{did}` in the AP actor). Local vs remote is just
   field presence.
4. **Dual ingestion** — AP is push (inbox), ATProto is pull (Jetstream, filtered
   by a cached set of followed DIDs + mentions + hashtags before hitting a
   queue).
5. **Ordered dual-write with anti-spoof dedup** — publish to Bluesky first, save
   `bskyUri`/`bskyCid`, then federate via AP with those cross-references
   embedded; inbound posts carry `fediverseId`/`blueskyUri` claims that are
   verified **bidirectionally** before a duplicate is suppressed.
6. **Bridgy Fed awareness** — a DID whose PDS is `*.brid.gy` is resolved back to
   its AP actor via `alsoKnownAs` and the two accounts are merged, not shown
   twice.

## Feature → fediverse source map

| Section | Fediverse source | Protocol object | Status |
|---|---|---|---|
| Stories | Pixelfed (incl. Instagram-mirroring accounts) | `Story` object (PAE-06) | mapping already specced in `stories-cross-protocol.md` |
| Events | Mobilizon / Gancio | AP `Event` | ingest + publish later |
| Groups | Lemmy / Mobilizon / Guppe | `Group` actor, FEP-1b12 announce | ingest + publish later |
| Classifieds | *none exists* | open question | needs a custom AP vocab + a seeding plan |
| Posts/likes/follows | Mastodon, Misskey, Akkoma, etc. | `Note`, `Like`, `Follow`, `Announce` | core AP surface |

## What we keep from the current build

- The whole frontend surface (rail, viewer, composer, explorer, vault) — it is
  protocol-agnostic; the AP ingestion arms feed the same `appview_*` tables.
- The ATProto leg of stories (`com.bytescape.story` + `app.bsky.actor.status`) —
  becomes one of two write paths, exactly like Wafrn's `net.wafrn.feed.bite`
  custom lexicon proves out.
- The firehose consumer and appview index — extended, not replaced.
- `wiki/architecture/stories-cross-protocol.md` — becomes the stories
  translation spec (PAE-06 ↔ `com.bytescape.story` ↔ internal row).

## Phasing

- **Phase 0 — live-service fixes** (done 2026-11-02): the aws-sdk-s3
  `behavior_version` panic that 502'd every media upload on prod.
- **Phase 1 — AP ingestion, read-only (the bootstrap milestone):** remote actor
  resolution (WebFinger + actor fetch), a signed inbox receiver with
  Follow/Accept processors, and an ingestion arm that maps Pixelfed `Story`
  objects into `appview_stories`. Success test: **follow a Pixelfed account and
  see its stories in the rail** — the "Instagram stories" test. Same pattern
  bootstraps events (follow a Gancio/Mobilizon account) and groups.
- **Phase 2 — AP identity + outbound:** our users as AP actors (WebFinger,
  actor docs, outbox), dual-write publishing (ATProto first, then AP with
  cross-references, per Wafrn's ordering), inbound Like/Announce/Follow parity,
  unified notifications.
- **Phase 3 — unique features federated:** events and groups as first-class
  federated objects in both directions; classifieds vocab (custom) once its
  open question is settled; dedup/merge (Bridgy Fed awareness).
- **Phase 4 — identity consolidation:** the PDS decision (below), handle/DID
  migration for existing accounts.

## Open questions (user decisions)

1. **PDS strategy:** adopt the official Bluesky PDS as a sidecar (Wafrn's model:
   zero ATProto-protocol code to maintain, sovereign identities, trivial
   upgrades) and retire our custom `pds/` module — or keep the PDS-lite because
   it already carries our custom lexicons and `did:web` accounts? Migration
   cost lands in Phase 4 either way.
2. **Identity anchor:** do Bytescape accounts present primarily as
   `@user@bytescape.social` (AP-first) with the ATProto handle secondary, or the
   reverse? Wafrn exposes both, with per-user opt-in for the Bluesky leg.
3. **Write ordering:** Wafrn does Bluesky-first for cross-reference integrity.
   Given our pivot motivation is the fediverse audience, do we invert to
   AP-first, or keep Wafrn's order and accept the ~5s AP delay?
4. **Classifieds:** no fediverse standard — we invent an AP vocabulary
   (`bytescape` context terms) or defer the section. Where does its content
   network come from?
5. **Existing accounts:** migration story for the current `did:web:bytescape.social`
   users if we adopt the PDS sidecar (`did:plc` + subdomain handles).
