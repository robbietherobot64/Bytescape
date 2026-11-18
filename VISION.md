---
title: Bytescape VISION
created: 2026-10-10
status: canonical — highest-precedence product document
---

# Bytescape — Vision

**Precedence:** This document > `wiki/product/mvp-scope.md` (ship gates + shelf) >
`wiki/product/prd.md` (archived reference). `docs/archive/brainstorm.md` is the source material
this distills — direction-setting input, not canon. Decisions are logged one line at a
time in `DECISIONS.md`; current phase and next actions live in `STATUS.md`.

## What Bytescape is

A social platform on **AT Protocol** for people leaving walled gardens, differentiated
on three axes:

1. **Migration** — GDPR archive import (Instagram first) with two-way social-graph
   matching. The cold-start wedge.  Discover previous followers who have already uploaded their archives, and get notified when new ones do.
2. **Expression** — Profile customization through drag and drop widgets, app/web theme editor, custom css, etc.  Overall reminiscent of myspace/geocities in the array of aesthetics and layouts that are available, with a more modern and user friendly approach, as well as mobile friendly views.
3. Interest based combined feeds** — User selects interests through interest picker menu, interest grouped feeds and hashtags are subscribed and populate the feed, blended into a single chronological feed by default and filtered/sorted on demand. No black box algorithm, just user driven curation.

**Explicitly not designed to be addictive.** Utility over habit.

**Structure:** three sections — **Profile / Feed / Community** (be / consume / participate).

## Why it exists (prioritization context)

Decentralization alone doesn't retain users: Bluesky DAU −40% YoY, Mastodon −70%
post-peak, Farcaster/Lens never reached viral growth. The recurring causes: cold-start
(empty room), culture shock, no revenue certainty, fragmented feed UX. Bytescape attacks
cold-start with migration, fragmentation with a blended feed, and "why stay" with
expressive ownership (profiles).

## Core product decisions (distilled)

### Feed — blend by default, filter on demand
- One blended **Home**: chronological default, all formats, all subscribed sources.
- **Source chips** on every post: truthful provenance, subscribed sources only, ordered
  by specificity (niche → hashtag → following). Tap a chip → **ephemeral tab** (all
  content types matching that source, eg. "#ska"); pin → **saved lens tab in style of bluesky feeds** (persistent, shareable, possibly even a real feed per bluesky definitions?).
- Three tab tiers: Home (always) / Saved lenses / Ephemeral (transient, light, closing).
- **Stars**— starred sources are prioritized in the feed.
- **Suggested Accounts**— Accounts who are followed by people you follow or have similar interest selections suggested.  Horizontal feed section that shows up at the top of the feed in the first load and disappears when you scroll.
- *Design session pending: map chips/lenses/stars onto the existing implementation
  (two-phase loading, dedup, media-first PostCard, source plumbing survive; category
  sidebar + Popular/New tabs are replaced).*

### Onboarding & interests
- Sign in with an existing ATProto account (bring your own
   PDS). *Phase 1: existing credentials only; no-account users are directed to Bluesky.
   Native bytescape.social signups are deferred till later.*
- Interest questionnaire ("What are you here for?").
- Two-tier interest selector (8–12 categories × 6–10 sub-interests). Interests are
  **bundles**: hashtags + feeds + community categories.
- Interest selection will influence suggested accounts on the feed who have similar interests.

### Migration
- One source first (**Instagram**); media pipeline scoping gates volume (STATUS open item).
- Initial import detects those in your archive who have already uploaded their archives from that platform, and suggests them as followers automatically post upload.  
- For future follower matching: both sides import → mutual notification → suggested follow.

### Community — a hub, not a stream
- Four top-level types: **Classifieds, Events, Groups, Local** (Local may become a lens);
  everything else is a category *within* one — prevents Facebook's junk-drawer failure.
- **Classifieds**:  Post title, location (map integration), price, communication options (phone number, dm), description, custom user added fields, self-expiring listings. *Categories* - Buy/Sell/Trade, Offer/Find Services, Hire/Get Hired, Find People, Housing, Local.  Each of these have subcategories that we will flesh out.  Monetization (resolved 2026-10-10): the platform never processes transaction payments — users settle payment themselves. Any post carrying a *price field* pays a posting fee at posting time (greater of $1 or 5% of declared price, capped at $25, owed whether or not the item sells); posts without a price field are free. Paid promotion boosts placement only within Classifieds surfaces, labeled Promoted — it never enters the Home feed. Full requirements: `docs/plans/2026-10-10-0935-feat-open-forks-resolution-plan.md` (R1–R5).
- **Events**: read existing `community.lexicon.calendar.event`/`.rsvp` records (interop
  with Smoke Signal, atmo.rsvp); index the network before creating our own.
- **Groups: last** — hardest moderation surface, blocked on ATProto Spaces (alpha).
- Dual surfaces: every Community record is browsable in Community and consumable in Feed based on hashtag and feed subscriptions that these posts can be included in. I.e., a classified post or event with "#cats" would show up in my feed if I'm subscribed to that hashtag.

### Messaging — phased (resolved 2026-10-10)
- 1:1 DMs ride Bluesky's chat network: clean DMs with any reachable AT Protocol account, no new protocol surface.
- Friend-circle group chats ship early at small scale on Bluesky's native group chat.
- Later phase: channels-and-roles spaces (Roomy is the gold standard). The record model —
  adopt Roomy's lexicons (interop) vs. Bytescape-owned (control) — is decided after
  researching Roomy's actual lexicon model; the portability axis governs. (plan R6–R8)

### Platform (resolved 2026-10-10)
- Web-first: the web app is the primary, full-feature surface, including profile customization.
- Mobile: clean, legible, responsive web where profile customization adapts seamlessly to
  small screens — no separate app. (plan R11–R12)

### Moderation & legal (gate before public launch)
- Moderation layer precedes commerce; validate on tagged community posts → classifieds → events → groups. Classifieds are enumerable/bounded/expiring — groups are norms-based, persistent, contested. Groups are never "done."
- On AT Protocol, moderation = **labels** + presentation-layer filtering (records can't be
  deleted from others' repos).
- Labeler posture (resolved 2026-10-10): subscribe to established labelers and render
  their labels from launch; our reports produce **client-internal labels** at launch; a
  **published network labeler** ships only after moderation is validated on tagged
  community posts. (plan R9–R10)
- Legal: Section 230 shield; position as **facilitator** — "neutral party" is the wrong
  test: facilitate, don't sell; moderate proactively; notice-based takedown processes.
  Processing payments triggers marketplace-facilitator tax duties (45 states) — a strong
  structural reason not to process payments early. ToS, privacy policy, and DMCA agent
  required before public launch.

## Acquisition loops — product-embedded, no campaign dependency

Distribution must live in the product (the owner runs no marketing campaigns):

1. **Shareable profile pages** — public, decorated, indexable; "firstname lastname"
   searches land on personal pages. Compounds via SEO while asleep.
2. **The rescue ritual** — a completed import + friend-find is inherently showable
   ("I found you here!"); shareable links; the press story writes itself.
3. **Fun as engine** — a profile worth showing recruits the next user by word of mouth
   (MySpace's growth mechanic, zero ad budget).
4. **Matching flywheel** — every archive import improves friend-finding for everyone:
   the design's one true network effect.

## Success ladder (retention without addiction)

- **L0 — mission:** data liberation + friend reconnection works end-to-end. Success
  regardless of user counts.
- **L1 — ~100** non-friend users with weekly activity.
- **L2 — ~1,000+** compounding via the loops above (Pixelfed-shaped trajectory);
  contributor inbound is the signal to open the shelf.
- Friendica-scale (a few thousand loyal users) is the floor worth having, not failure.
- Per-feature ship gates: density gates in `mvp-scope.md`.

## Sequence (corrected)

1. Migration loop (importer + matching) — *prototype working*
2. Home feed — *built; needs refocus toward "where migrated life and matched friends converge"*
3. Auto-generated profile from import — *semi-working widget prototype*
4. Moderation layer — reports, labels, queues, appeals
5. Classifieds infrastructure → 6. Find People > Musicians as first classified section → 7. Events → 8. Groups (dependent on Spaces-stable)

Rejected/deferred inventory: `docs/archive/brainstorm.md` §12. Open forks: `STATUS.md`.

## Design principles

Chronological default = trust. Transparency about *why* content appears. Legibility over
cleverness. User-defined algorithms, not corporate ones. Utility over habit. Community
over scale.
