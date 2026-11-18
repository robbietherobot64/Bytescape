# feed-ingestion Specification

## Purpose

ActivityPub content (Mastodon `Note` posts, Pixelfed `Story` objects) enters the existing
feed and stories rail as an additive source — no changes to the AT read path. This is the
first visible AP content (the owner's "first content population"). Behind
`AP_INGEST_ENABLED`; the shipped AT feed is unchanged when the flag is off.

## Requirements

### Requirement: Mastodon Note posts reach the feed
Mastodon `Note` objects ingested from followed AP accounts SHALL be written to the derived
post index and surface in the home feed with `source = "mastodon"` and a provenance marker
showing their origin.

#### Scenario: Followed Mastodon account posts
- **WHEN** a followed Mastodon account publishes a `Note`
- **THEN** it appears in the home feed with `source = "mastodon"` and an origin marker

### Requirement: Pixelfed stories reach the stories rail
Pixelfed `Story` objects ingested from followed AP accounts SHALL be written to the story
index and appear in the stories rail alongside existing stories.

#### Scenario: Followed Pixelfed account posts a story
- **WHEN** a followed Pixelfed account publishes a story
- **THEN** it appears in the stories rail

### Requirement: Ingestion via inbox receiver backed by a poller
AP content SHALL be ingested through an AP inbox receiver (pushed activities) with a
poller for backfill of followed accounts' outboxes. Ingestion is additive: it writes to the
same derived tables the feed already reads; the AT relay consumer is unchanged.

#### Scenario: Pushed activity
- **WHEN** a remote AP server pushes a `Create(Note)` activity to our inbox
- **THEN** the note is indexed and appears in the feed without a poll cycle

#### Scenario: Poll backfill
- **WHEN** the poller runs over a followed account's outbox
- **THEN** recent notes/stories not yet indexed are added

### Requirement: Non-regression and bail
AP ingestion SHALL NOT alter existing AT feed behavior. With `AP_INGEST_ENABLED` off, the
feed and stories rail are identical to the pre-change AT-only build; disabling the flag
stops AP content from appearing without breaking existing content.

#### Scenario: Flag off
- **WHEN** `AP_INGEST_ENABLED` is false
- **THEN** no AP content is ingested and the AT feed/stories are unchanged

#### Scenario: Bail on regression
- **WHEN** AP ingestion is suspected of regressing the feed
- **THEN** setting `AP_INGEST_ENABLED` false removes AP content and restores AT-only
  behavior without data loss

### Requirement: Source rendering and provenance
Ingested AP content SHALL render with `source = "mastodon"` or `"pixelfed"`, a provenance
marker (which account/hashtag it came from), and the correct author attribution (the remote
AP actor — never presented as ours).

#### Scenario: Provenance visible
- **WHEN** an ingested AP post is shown in the feed
- **THEN** it shows its Mastodon/Pixelfed origin and the source account, distinct from a
  native Bytescape post

### Requirement: Interests surface AP content
Ingestion SHALL honor interest subscriptions: AP posts/stories tagged with a subscribed
interest (e.g. `#cats`) SHALL appear for that interest, independent of whether the user
follows the originating account (see `follows`).

#### Scenario: Interest-tagged AP content
- **WHEN** a user is subscribed to `#cats` and a Mastodon post is tagged `#cats`
- **THEN** it surfaces for that interest even if the user does not follow the account
