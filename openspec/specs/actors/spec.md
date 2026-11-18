# actors Specification

## Purpose

ActivityPub actor issuance and resolution — the protocol mechanics that make each
Bytescape account a real, addressable fediverse actor. WebFinger for discovery, actor
documents for identity, `alsoKnownAs` cross-links to the AT identity, and per-user RSA
keypairs so signed delivery is possible in Phase C. Feature-flagged so the shipped AT app
is unaffected when disabled.

## Requirements

### Requirement: WebFinger discovery
The server SHALL answer WebFinger (`/.well-known/webfinger`) for each Bytescape account's
AP actor, resolving a `user@domain` handle to the actor's `self` link (ActivityPub
`application/activity+json`).

#### Scenario: Remote server resolves our user
- **WHEN** a remote ActivityPub server looks up `@alice@bytescape.social` via WebFinger
- **THEN** it receives a JRD whose `self` link points to Alice's AP actor document

### Requirement: AP actor document
Each account with AP enabled SHALL have an actor document (`application/activity+json`)
carrying `id`, `inbox`, `outbox`, `preferredUsername`, `alsoKnownAs` (containing the AT
identity `at://...`), and public key material. The actor `id` is stable and dereferenceable.

#### Scenario: Actor document dereference
- **WHEN** a remote server fetches a Bytescape actor's `id`
- **THEN** it receives a valid actor document with inbox, outbox, and `alsoKnownAs`

### Requirement: Per-user keypair
Each AP actor SHALL have an RSA keypair used to sign outbound activities and (in Phase C)
HTTP signatures. The private key is stored server-side and never exposed.

#### Scenario: Keypair present
- **WHEN** an AP actor is created
- **THEN** it has an RSA keypair with a published public key in its actor document

### Requirement: alsoKnownAs cross-link
The AP actor's `alsoKnownAs` SHALL include the account's AT identity (`at://did:...`), so
the two protocol identities are discoverably the same person (the Wafrn unified model).

#### Scenario: Cross-link present
- **WHEN** an AP actor document is fetched
- **THEN** `alsoKnownAs` contains the account's `at://` identity

### Requirement: Feature-flagged issuance
AP actor issuance and resolution SHALL be gated by `AP_ACTORS_ENABLED`, and SHALL issue actors only for accounts without an existing AP identity: Bluesky and email sign-ups get an issued actor; Mastodon sign-ups (who bring their home-instance actor) get none. When the flag is off, no WebFinger/actor endpoints are served and no keypairs are minted; the AT surface is unchanged.

#### Scenario: Flag off
- **WHEN** `AP_ACTORS_ENABLED` is false
- **THEN** WebFinger and actor-document endpoints do not resolve Bytescape accounts

#### Scenario: Flag on
- **WHEN** `AP_ACTORS_ENABLED` is true
- **THEN** each dual-protocol account without an existing AP identity resolves via WebFinger to a valid actor document

#### Scenario: Mastodon sign-up requests issuance
- **WHEN** account creation runs for a Mastodon-signed-up user
- **THEN** no local AP actor and no keypair are created for them

### Requirement: Remote actor resolution
The server SHALL resolve a remote `user@instance` (Mastodon/Pixelfed) to its AP actor
document via WebFinger, for use by Phase B feed population and follow surfaces.

#### Scenario: Resolve a Mastodon handle
- **WHEN** the server resolves `@bob@mastodon.example`
- **THEN** it obtains Bob's AP actor document (inbox, outbox, public key) from that instance

### Requirement: Retired actor serving
An actor id that has been retired (superseded by an existing home-instance actor) SHALL serve a tombstone/deletion document instead of a live actor document, and its WebFinger entry SHALL no longer resolve it as a live identity, while already-published activity URLs under that actor remain dereferenceable for retraction processing.

#### Scenario: Actor document after retirement
- **WHEN** the retired actor id is fetched with ActivityPub content negotiation
- **THEN** the response is a tombstone/deletion form carrying the actor id, not a live actor

#### Scenario: WebFinger after retirement
- **WHEN** the retired handle is looked up via WebFinger
- **THEN** it does not resolve to the retired actor as a live identity

#### Scenario: Published activities remain dereferenceable
- **WHEN** a remote dereferences an activity URL previously published under a retired actor (to process its retraction)
- **THEN** the stored activity payload is still served
