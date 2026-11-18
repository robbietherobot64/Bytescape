# Stories — cross-protocol mapping (Pixelfed / ActivityPub)

**Status:** translation reference for the ActivityPub leg of stories — now
active work: Pixelfed `Story` ingest is Phase B of
`openspec/changes/add-activitypub-integration`, and this mapping is the
contract between the wire object and `com.bytescape.story`. Written when the
project was AT-only (2026-08); the "does not implement ActivityPub" framing
below is historical. Target vocabulary: Pixelfed's federated `Story` object
(PAE-06) and the FEP for Ephemeral Stories that Pixelfed Engineering has in
preparation.

Sources verified against `pixelfed/pixelfed` dev:
`app/Util/ActivityPub/Validator/StoryValidator.php` (wire schema),
`app/Transformer/ActivityPub/Verb/CreateStory.php` (`Add` wrapper),
`app/Jobs/StoryPipeline/StoryExpire.php` (expiry semantics).

## Record mapping

| Semantic | Pixelfed AP `Story` | `com.bytescape.story` | Notes |
|---|---|---|---|
| Author | `attributedTo` | repo DID | adapter maps DID ↔ actor URI |
| Created | `published` | `createdAt` | direct |
| Scheduled start | `startTime` | `activeFrom` (optional) | direct |
| Expiry | `expiresAt` (required) | `expiresAt` nullable | Never-stories excluded from federation or mapped per documented policy — never a fake expiry |
| Pin ordering | absent | `pinned` (optional) | presentation-only; adapter ignores outbound |
| Item display length | `duration` (1–300s) | `items[].duration` (optional) | direct |
| Interaction flags | `can_react`, `can_reply` | `canReact`, `canReply` (optional, default true) | direct |
| Media | one `attachment` per record | `items[1..6]` blob refs | outbound: N Story objects; inbound: attachments grouped into one record |
| Reactions | `StoryReaction` (federated) | emoji reactions (strongRef targets) | semantics match |
| Replies | federated story replies | reply posts, story parent | semantics match |
| Reshare | absent | quote-post embedding the record | our superset; adapter ignores outbound |
| Audience | `to: followers` + bear-cap fetch URLs | active records are public | adapter sets audience; bear-cap is adapter-side |

## Expiry semantics

Pixelfed deactivates the story, rotates media paths to kill old URLs, and
federates a `Delete` (retaining an `is_archived` copy). Bytescape copies media
to app-controlled private storage, deletes the public record, and reduces the
index row to metadata. Both end public access; both keep an owner-side
archive.

## Status cards

`app.bsky.actor.status` is an official Bluesky record (self-keyed, duration in
minutes). It is Bytescape's text-card story format and has no AP Story
equivalent; an adapter would map it to a text-only AP object or leave it out.

*Recorded deviation (implementation):* story reshares create a quote-post
embedding the story record rather than an `app.bsky.feed.repost`, so the
reshare rides every existing feed path. The consumer counts quote embeds of
story records as the record's repost. `app.bsky.feed.repost` subjects still
count normally via the firehose path.
