# account-lifecycle Specification

## Purpose

Federated account retirement: when a Bytescape account stops existing, its federated presence dies with it. Deletion retracts what the account did on remote servers and purges its federated state locally, so no favourite, follow, actor, or queued activity from a dead account lingers anywhere.

## Requirements

### Requirement: Deletion federates actor deletion
When a Bytescape account that has a federated presence is deleted (immediately or at soft-delete expiry), a deletion of its federated identity SHALL be delivered to every inbox that account has ever delivered to, so remote servers stop treating the identity as live.

#### Scenario: Account with federated history is deleted
- **WHEN** an account that delivered activities to remote inboxes is deleted
- **THEN** a Delete/Tombstone of its actor is delivered to each known inbox (followers and previously-delivered inboxes, deduplicated by shared inbox)

#### Scenario: Delivery failure during deletion
- **WHEN** a deletion delivery fails or is never acknowledged by a remote
- **THEN** the local account deletion still completes, and the failure is recorded for audit rather than blocking deletion

### Requirement: Standing federated interactions are retracted on deletion
Before a federated identity dies, its standing outbound interactions on remote servers (favourites/likes and follows) SHALL be undone, so remote servers never keep an interaction attributed to a dead account.

#### Scenario: Deleted account had favourited a remote post
- **WHEN** an account with a standing remote favourite is deleted
- **THEN** the favourite is undone on the identity that created it — a full-embed `Undo` referencing the exact original activity for minted-actor interactions, the home instance's unfavourite API as the home actor for home-REST favourites — before/with the actor deletion, and the remote favourite is removed

#### Scenario: Original activity needed for retraction
- **WHEN** a retraction must reference the exact original interaction
- **THEN** the original's stored payload is available until the retraction is delivered, then purged with the account

### Requirement: Federated state is purged with the account
Deleting an account SHALL remove its locally retained federated state — its AP actor material, stored activities, and delivery queue rows — so no orphan federated rows survive their account.

#### Scenario: No orphan rows after deletion
- **WHEN** an account with stored AP activities and delivery queue rows is deleted
- **THEN** no `ap_activities`/`ap_delivery_queue` rows attributable to that account remain

#### Scenario: Orphan class regression
- **WHEN** deletion completes for an account that had federated interactions
- **THEN** remote `favourited_by`/follower lists show no entries for that account's identities after retraction is processed

### Requirement: Identity supersession federates the cross-link change
When an account's federated cross-link changes because one of its identities is superseded (the `did:web` → `did:plc` AT re-identity), the change SHALL be federated before/with the supersession completing: an Update of the account's ActivityPub actor document — carrying the new cross-link — SHALL be delivered to every inbox the account has delivered to, so remote caches converge and never resolve the account to a tombstoned identity.

#### Scenario: Cross-link swap at re-identity
- **WHEN** an account with a federated presence is re-identified and its `alsoKnownAs` target swaps to the new AT identity
- **THEN** an Update(actor) carrying the re-rendered document is delivered to the account's known inboxes (deduplicated by shared inbox)

#### Scenario: Account with no federated presence
- **WHEN** an account with no AP actor (or whose AP identity is its home-instance actor / linked Bluesky account) is re-identified
- **THEN** nothing is fanned out — the swap is a local identity change only
