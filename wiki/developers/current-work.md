# Current work

**Snapshot: 2026-11-08.** This page is a dated picture of the sprints; the
living truth is always [STATUS.md](../STATUS.md) and the change folders
themselves in `openspec/changes/` (each has a proposal and task list).

## Complete

| Sprint | What it delivered |
|---|---|
| `add-events-standalone-site` (36/36) | Standalone Events site + `community.lexicon.calendar` records on user PDSes; live on `events.test.bytescape.social`, production deploy pending |
| `add-stories` (21/21) | Stories end to end: rail, viewer, composer, explorer, Vault archive, cross-instance sync |
| `import-pipeline-verification` (19/19) | Instagram import fully verified against real exports (photos, videos, Reels, stories, origin badge) |
| `feed-refocus` (20/25) | Home with source chips, saved lenses, star tiers — then the feed was **frozen at MVP** by owner call (2026-11-06); remaining tasks are verification only |
| `add-hosted-wiki` (16/17) | This wiki, published on a daily schedule from GitLab |
| `profile-import-rework` (archived 2026-11-07) | Profile default layout + import signup integration |

## Active

| Sprint | What it's building |
|---|---|
| `add-activitypub-integration` | **The dual-protocol pivot, in code.** Phase A: every account becomes an AT + ActivityPub actor (Wafrn unified-account model) with three sign-in paths — Bluesky, Mastodon, email. Phase B: ingest Mastodon notes and Pixelfed stories into the feed and stories rail, feature-flagged (`AP_ACTORS_ENABLED`, `AP_INGEST_ENABLED`) so the shipped AT app never regresses. Sign-in rebuild and outbox backfill are landing now. |

## Next up (from STATUS)

1. ActivityPub integration (above) — identity first, then feed population
2. Verify Facebook/Threads imports against real exports
3. Stand up UI test coverage (the standing quality gap)
4. Backlog: dead-code cleanup, security-stub hardening
5. Events production deploy (block ready; see `docs/test-server.md`)

## Reading a sprint

```text
openspec/changes/<name>/
├── proposal.md   why + what + non-goals
├── design.md     technical decisions
├── tasks.md      checklist with verify criteria (progress = [x] count)
└── specs/        behavior contracts (promoted to openspec/specs/ at archive)
```
