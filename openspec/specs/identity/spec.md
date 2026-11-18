# identity Specification

## Purpose

Every Bytescape account is a citizen of both networks at once: an AT Protocol identity
(Bluesky-compatible) and an ActivityPub actor (fediverse-compatible), cross-linked. This
is the foundation sign-in, feed population, and later federation all build on. Modeled on
Wafrn's unified account: one account row carries both protocol identities; local vs
remote is just a routing detail.

## Requirements

### Requirement: Dual-protocol account
A Bytescape account SHALL carry the identities of the protocols it participates in, cross-referenced so each resolves to the same person — with the ActivityPub side determined by the sign-up provider: a user who signed up with an existing ActivityPub presence (Mastodon) SHALL use that existing actor as their ActivityPub identity, while users without one (Bluesky, email) SHALL have an ActivityPub actor issued by Bytescape. The account's cross-links SHALL make both sides discoverably the same person.

#### Scenario: Account created via any sign-up path
- **WHEN** a user signs up via Bluesky, Mastodon, or email+password
- **THEN** the resulting account has both an AT identity and an ActivityPub identity, cross-linked so each resolves to the one account

#### Scenario: Bluesky sign-up links to existing DID
- **WHEN** a user signs in with Bluesky
- **THEN** their existing `did:plc` is their AT identity, and an AP actor is minted whose `alsoKnownAs` contains `at://did:plc:...`

#### Scenario: Mastodon sign-up reuses the existing actor
- **WHEN** a user signs in with Mastodon
- **THEN** their existing home-instance actor is their ActivityPub identity, NO local AP actor is created for them, and an AT identity (minted `did:plc` — the OQ1 verdict, 2026-11-14) is minted whose reference is cross-linked to that home-instance actor

#### Scenario: Email sign-up mints both
- **WHEN** a user signs up with email+password
- **THEN** a native AT identity (`did:plc`) and a fresh AP actor are both minted and cross-linked

#### Scenario: Existing did:web account is re-identified
- **WHEN** an account created before the did:plc verdict is migrated (the pre-realignment `did:web` cohort)
- **THEN** its AT identity becomes a per-user `did:plc` for the same account and handle, its content is preserved (see `Re-identify preserves the account's content`), and the cross-links resolve to the new identity

### Requirement: One account, one person
A single user SHALL NOT appear as two separate people across the two protocols. When multiple protocol identities are present, content and interactions attributed to any of them SHALL resolve to the one Bytescape account, and the user SHALL act on each network as exactly one actor: their existing actor where they already had one, otherwise their issued actor.

#### Scenario: Identity resolution across protocols
- **WHEN** a follow, post, or story references either the AT identity or the AP identity of a Bytescape user
- **THEN** it resolves to the same account, not two

#### Scenario: No second fediverse identity for Mastodon users
- **WHEN** a Mastodon-signed-up user performs an ActivityPub-visible interaction
- **THEN** it is attributed to their home-instance actor, never to a second, Bytescape-minted actor

#### Scenario: Re-keyed DID continuity
- **WHEN** an account's AT identity is re-keyed and swapped (the `did:web` → `did:plc` re-identify)
- **THEN** the account, handle, and content continue as the same person — no duplicate account is created and identity resolution across protocols still converges on the one account

### Requirement: Migration path is non-destructive
Adding or realigning the ActivityPub side of an existing (already-shipped) account SHALL NOT change, re-key, or lose the account's existing AT identity, posts, follows, or stories. Retiring a superseded minted AP actor SHALL NOT affect the account's AT side or its reused home-instance actor.

> **Superseded 2026-11-14 (in part) — see `add-at-identity-reidentify` design.md
> "Spec question: non-destructive migration".** For the pre-realignment `did:web` cohort
> only, the migration re-keys and swaps the AT identity (retroactive `did:web` →
> `did:plc` re-identify), by dated decision: those identities never worked on the real
> Atmosphere (the AppView drops their events — bluesky-social/atproto#4379), so the
> non-destructive guarantee protected identities with no live remote state to lose. The
> surviving guarantees — record rkeys + values preserved verbatim, exactly one public
> mint per account, honest tombstone for the old identity, Update fan-out so cached
> remotes converge — are pinned by `Re-identify preserves the account's content` below.
> The requirement's original text is preserved above as history.

#### Scenario: Existing Bluesky account gains AP actor
- **WHEN** an already-deployed AT-only account is upgraded to dual-protocol
- **THEN** its `did`, posts, follows, and stories are unchanged and it additionally has an AP actor

#### Scenario: Existing Mastodon-signed-up account is realigned
- **WHEN** an account that was given a minted AP actor at Mastodon sign-up is realigned
- **THEN** its minted actor is retired (see `Retirement of superseded minted actors`) while its AT identity and its home-instance actor remain unchanged

#### Scenario: Re-identify of a did:web account (superseded clause, by decision)
- **WHEN** the pre-realignment `did:web` cohort is migrated to `did:plc`
- **THEN** the account's posts, follows, and stories survive verbatim (record rkeys + values preserved) while the AT identity itself is re-keyed and swapped, per the dated supersession note above

### Requirement: Feature-flagged rollout
The AP actor issuance SHALL be gated by `AP_ACTORS_ENABLED`. When the flag is off, accounts
are AT-only exactly as today (no AP actor is minted, no AP surface is exposed), and the
shipped app behaves identically to the pre-change build.

#### Scenario: Flag off
- **WHEN** `AP_ACTORS_ENABLED` is false
- **THEN** no AP actors are created and the existing AT sign-in/feed/stories are unchanged

#### Scenario: Flag on, rollback
- **WHEN** `AP_ACTORS_ENABLED` is switched from true to false
- **THEN** AP actor issuance stops and the app continues on its AT surface without
  regression (existing AP actor records may remain inert)

### Requirement: Stored home-instance authorization
For a Mastodon-signed-up account, the authorization granted to their home instance SHALL be retained server-side so the account can act as its home-instance actor without re-authentication, and SHALL be refreshed so acting continues across token expiry.

#### Scenario: Acting long after sign-up
- **WHEN** a Mastodon-signed-up user performs an interaction days after linking
- **THEN** it is performed as their home-instance actor using valid authorization

#### Scenario: Revoked authorization
- **WHEN** the home instance rejects the stored authorization and it cannot be refreshed
- **THEN** the interaction fails with a visible "re-link" error, and is never silently attributed to another identity

### Requirement: Retirement of superseded minted actors
A minted AP actor that is superseded (its user acts via an existing home-instance actor instead) SHALL be retired so remotes no longer treat it as a live person: its actor id SHALL serve a deletion/tombstone form instead of a live actor document, and its WebFinger/actor discovery SHALL stop resolving it as live.

#### Scenario: Cross-links after retirement
- **WHEN** a superseded minted actor is retired
- **THEN** the account's cross-links point at the surviving identities only

#### Scenario: Remote fetches a retired actor
- **WHEN** a remote server dereferences a retired minted actor's id
- **THEN** it receives a tombstone/deletion form, not a live actor document

#### Scenario: Remote looks up the retired handle
- **WHEN** a remote server WebFingers the retired actor's handle
- **THEN** resolution no longer yields the retired actor as a live identity

### Requirement: Retirement preserves activity dereference
Retiring an actor SHALL NOT break dereference of its already-published activities, because undo retraction depends on them.

#### Scenario: Retraction remains possible during retirement
- **WHEN** a standing interaction of the retired actor must be undone
- **THEN** the original activity it references remains dereferenceable long enough for the undo to be processed

### Requirement: Federated artifacts do not outlive their account
When a Bytescape account stops existing (deleted or expired), no federated artifact SHALL remain attributable to it as a live person: no standing remote interactions, and no locally retained federated activity or delivery state (see `account-lifecycle` for the deletion behavior).

#### Scenario: Account is deleted
- **WHEN** a Bytescape account with federated interactions is deleted
- **THEN** its remote standing interactions are retracted and its federated activity/delivery state is purged with the account

### Requirement: Re-identify preserves the account's content
Re-keying and swapping an account's AT identity (the `did:web` → `did:plc` re-identify) SHALL preserve the account's content exactly and mint at most one public identity: every record SHALL keep its rkey and value verbatim (only the repo commit and its signature are new), the account SHALL NOT lose records — the old root is dropped only with a walk-completeness proof and a rev check, and writes racing the migration SHALL be refused with a retryable error rather than lost — and exactly one public DID SHALL ever be minted per account (the minted identity is staged before the public mint so a retry resumes with the same identity).

#### Scenario: Records preserved verbatim
- **WHEN** an account with repo records is re-identified
- **THEN** every record survives under the new identity with the same rkey and byte-identical value (including nested-map records), and the account remains the same person (see `One account, one person`)

#### Scenario: No second public mint
- **WHEN** a re-identify crashes or loses the mint response and is retried (or two runners race)
- **THEN** exactly one public DID is ever minted for the account — the retry resumes with the staged identity — and at most one runner performs the cutover

#### Scenario: A write racing the migration is refused, not lost
- **WHEN** a record write lands during the walk→cutover window of the account's re-identify
- **THEN** it is refused with a retryable error and succeeds after the migration, never silently destroyed

#### Scenario: Migration aborts rather than losing records
- **WHEN** the record walk cannot prove completeness (untraversable record, count mismatch, or the repo's rev moved past the walk)
- **THEN** the migration aborts before any drop, leaving the old identity intact and the account retryable

### Requirement: Superseded AT identity is honestly tombstoned
When an account's `did:web` AT identity is superseded by its re-identified `did:plc` identity, the old identity SHALL stop serving as a live identity — its DID-document routes answer an honest tombstone (not a document describing a different DID) and its repo endpoints answer not-found — while handle resolution keeps returning the account's live identity, and the account's ActivityPub `alsoKnownAs` cross-link change is fanned out so cached remotes converge.

#### Scenario: Remote fetches the retired identity's DID document
- **WHEN** a fetcher resolves the retired `did:web` identity's DID-document routes
- **THEN** they receive an honest tombstone (404), never a document whose `id` is a different DID

#### Scenario: Handle resolution stays live
- **WHEN** a resolver looks up the account's handle (`/.well-known/atproto-did`)
- **THEN** it receives the account's live DID (`did:plc:` after re-identification)

#### Scenario: Cached remotes converge on the new cross-link
- **WHEN** an account's AP `alsoKnownAs` cross-link swaps to the new AT identity at cutover
- **THEN** an Update of the actor document is fanned out to the account's known inboxes so cached remotes resolve to the new identity (a no-op for accounts with no AP actor)
