# signin Specification

## Purpose

Sign-in expands from "Sign in with Bluesky" to three equivalent paths — Bluesky, Mastodon,
and email+password — each producing the same dual-protocol account (see `identity`). This
is how Mastodon users and Facebook-migrators (no Bluesky account) join Bytescape. The
existing Bluesky flow keeps working unchanged.

## Requirements

### Requirement: Three sign-in paths
The sign-in surface SHALL offer: (1) "Sign in with Bluesky" (OAuth), (2) "Sign in with
Mastodon" (OAuth), and (3) email + password (native). Each path SHALL produce the same account type defined in `identity`, carrying the provider-appropriate protocol identities: reuse where the user already has one, issue where they do not.

#### Scenario: Sign in with Bluesky
- **WHEN** a user chooses "Sign in with Bluesky"
- **THEN** they authenticate via Bluesky OAuth and land on the same account model as other paths (AT identity = their `did:plc`, AP actor minted)

#### Scenario: Sign in with Mastodon
- **WHEN** a user chooses "Sign in with Mastodon"
- **THEN** they authenticate via their Mastodon instance's OAuth and land on the account model where their existing home-instance actor is their ActivityPub identity and no local AP actor is created

#### Scenario: Sign up with email
- **WHEN** a user provides email + password
- **THEN** a native account is created with both an AT identity and an AP actor

### Requirement: Mastodon OAuth is instance-agnostic
"Sign in with Mastodon" SHALL accept a user@instance handle or instance URL and perform
OAuth against that user's own Mastodon server (not a single hardcoded instance).

#### Scenario: Sign in from a specific instance
- **WHEN** a user enters `@alice@mastodon.example`
- **THEN** OAuth runs against `mastodon.example` and their account there is linked

### Requirement: Existing Bluesky flow unchanged
The current "Sign in with Bluesky" path SHALL continue to work exactly as it does today
for users who do not opt into the AP surface. When `AP_ACTORS_ENABLED` is off, the sign-in
surface is Bluesky + email only (no Mastodon option), matching today's behavior.

#### Scenario: Flag off
- **WHEN** `AP_ACTORS_ENABLED` is false
- **THEN** the Mastodon sign-in option is hidden and Bluesky/email sign-in are unchanged

### Requirement: One identity per person across paths
If a user's email, Bluesky DID, or Mastodon actor already corresponds to an existing
account, sign-in SHALL link to that account rather than creating a duplicate.

#### Scenario: Same person, two sign-in methods
- **WHEN** a user who signed in with Bluesky later signs in with Mastodon using contact
  info that resolves to the same account
- **THEN** they access the same account, not a new one

#### Scenario: Re-keyed DID is the same person
- **WHEN** a user whose account was re-identified (its `did:web` AT identity swapped to a `did:plc` — see `identity`) signs in again
- **THEN** sign-in links to the same account via the handle mapping (handle resolution returns the live DID), and no duplicate account is created for the retired DID

### Requirement: Mastodon sign-in carries the home actor forward
Completing "Sign in with Mastodon" SHALL record the user's home-instance actor as the account's ActivityPub identity (so later interactions resolve to it), alongside the authorization needed to act as that actor.

#### Scenario: Account records its home actor
- **WHEN** a Mastodon OAuth sign-in completes
- **THEN** the account resolves its ActivityPub identity to the user's home-instance actor, and none to a local minted actor

#### Scenario: Returning Mastodon user
- **WHEN** a user with an existing linked Mastodon identity signs in again
- **THEN** they reach the same account, with the same home-instance actor identity (see `One identity per person across paths`)
