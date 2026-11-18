# follows Specification

## Purpose

How ActivityPub sources enter the feed for population. Per the owner (2026-11-08): (1)
imported Mastodon/Pixelfed contacts (from the user's archive / linked identity) and (2)
interest hashtags — a user subscribed to `#cats` gets Mastodon/Pixelfed `#cats` content.
Not contacts-only: interests are a first-class feed signal.

## Requirements

### Requirement: Imported AP contacts populate the feed
Mastodon/Pixelfed accounts present in the user's imported contacts (from an archive import
or a linked Mastodon identity) SHALL become AP feed sources whose notes/stories are ingested
and shown in the feed/stories rail.

#### Scenario: Archive with Mastodon contacts
- **WHEN** a user imports an archive whose contacts include Mastodon accounts
- **THEN** those accounts become AP sources and their content populates the feed

### Requirement: Interest hashtags surface AP content
A user subscribed to an interest hashtag (e.g. `#cats`) SHALL see AP content (Mastodon
`Note`s, Pixelfed posts/stories) tagged with that hashtag in their feed/stories, without
needing to follow each originating account.

#### Scenario: Interest without follow
- **WHEN** a user is subscribed to `#cats` and does not follow a given Mastodon account
- **THEN** that account's `#cats` notes still surface for the user

### Requirement: Follows are additive and reversible
Adding AP sources (via imported contacts or interests) SHALL NOT change existing AT
follows or subscriptions. Removing an AP source stops its content without affecting other
sources.

#### Scenario: Remove an AP source
- **WHEN** a user removes a Mastodon source or unsubscribes from an interest
- **THEN** its AP content stops appearing; other sources and AT follows are unaffected

### Requirement: Interest hashtags apply to both protocols
An interest subscription SHALL cover both AT and AP content tagged with that hashtag — the
same `#cats` interest pulls Bluesky `#cats` posts AND Mastodon/Pixelfed `#cats` content.

#### Scenario: Cross-protocol interest
- **WHEN** a user is subscribed to `#cats`
- **THEN** the feed shows both AT and AP `#cats` content, each with its own provenance
