# Post-Profile Phase Architecture (PM Source Document)

## Purpose

This document defines the architecture and delivery boundaries for the phase that starts **after profile-hosted identity (v20)** is accepted. PM should use this file as the single source for writing track prompts.

## Phase goal

Deliver an RSS-reader ingestion layer that can:
- ingest native RSS/Atom feeds,
- ingest approved candidates inferred from archive follow graphs (Instagram/Facebook exports),
- render normalized items in existing feed UI contracts,
- enforce policy, safety, and bounded performance constraints.

## Start gate

Do not start this phase until v20 acceptance criteria are complete:
- hosted identity publish path works,
- profile hosting path works,
- PM acceptance checks passed.

## Architecture overview

### 1) Ingestion lanes

1. Native lane: RSS/Atom and protocol-native sources.
2. Archive-derived lane: subscription candidates inferred from imported social graphs.
3. Link-only lane: non-ingestable sources stored as watchlist references.

### 2) Core backend components

1. Subscription registry
- Stores subscriptions, state, poll metadata, health status.

2. Candidate registry
- Stores inferred candidates with confidence and provenance.
- Requires explicit approval before activation.

3. Scheduler and worker pool
- Bounded concurrency.
- Retry with exponential backoff.
- Circuit-breaker pause on repeated failures.

4. Fetch + parse pipeline
- Fetch feed source.
- Parse RSS/Atom payload.
- Produce normalized internal feed entries.

5. Normalization + dedupe
- Canonical URL and timestamp keying.
- Fallback content/media hash keying.
- Source provenance retained per item.

6. Feed projection
- Produces tab-ready result sets with bounded window and cursor semantics.

### 3) Frontend components

1. RSS tab data wiring
- Replace placeholder data path with backend-provided feed page.

2. Subscription controls
- Add/remove subscriptions.
- Health visibility.

3. Candidate review UX
- Show inferred candidates.
- Show confidence and provenance.
- Approve/reject actions.

4. Source clarity
- Badge/source labeling for item origin (`rss`, `archive-derived`, etc.).

## Data contracts (minimum)

1. `RssSubscription`
- id, url, title, status, poll cadence, etag, last_modified, last_fetch_at, error_count

2. `SubscriptionCandidate`
- id, source, identity_hint, discovered_url, confidence, provenance, created_at

3. `NormalizedFeedItem`
- id, source, author, title, summary/content, canonical_url, published_at, media, provenance

4. `FeedPage`
- items, cursor, has_more

## Risk controls (mandatory)

1. No unsupported scraping paths.
2. Source policy gates (allow/deny per source type).
3. Polling budgets and hard caps for memory-safe operation.
4. Sanitization before renderable content persistence.
5. Dead-letter capture for parse/fetch failures.
6. Audit trail for candidate approval/rejection.

## Platform viability rules

1. Instagram/Facebook
- Use exports to infer subscriptions/candidates.
- Do not implement direct unsupported live timeline scraping.

2. RSS/Atom-native sources
- Full support path.

3. Federated/API-native sources
- Use protocol-native connectors where available.

## PM prompt-building guidance

When creating the next sprint prompts, PM should split work by tracks:

1. Track A (Backend)
- Subscription + candidate persistence.
- Scheduler/worker pipeline.
- Parse/normalize/dedupe/projection path.

2. Track B (Frontend)
- RSS tab live data wiring.
- Subscription management UI.
- Candidate review UI with provenance/confidence.

3. Shared acceptance checks
- End-to-end candidate approval to feed visibility.
- Bounded memory behavior under sustained polling.
- Build/tests/lint pass.

## Acceptance criteria for this phase

1. RSS tab returns non-placeholder entries from active subscriptions.
2. Archive-derived candidates are generated and require explicit approval.
3. Approved candidates can become active subscriptions.
4. Feed stays bounded and does not regress memory stability.
5. No unsupported ingestion paths are introduced.

## Out of scope for this phase

1. New full visual redesign.
2. Full multi-tenant control plane.
3. Unsupported source scraping workflows.
