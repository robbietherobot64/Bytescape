---
title: Vision Open Forks Resolution - Plan
type: feat
date: 2026-10-10
topic: open-forks-resolution
artifact_contract: ce-unified-plan/v1
product_contract_source: ce-brainstorm
execution: code
---

# Vision Open Forks Resolution - Plan

## Goal Capsule

- **Objective:** The product vision's four open forks — classifieds monetization, messaging strategy, labeler posture, and platform priority — are resolved into committed product behavior, so downstream planning proceeds without re-opening product direction.
- **Product authority:** `VISION.md` (precedence: VISION > `wiki/product/mvp-scope.md` > `wiki/product/prd.md`).
- **Open blockers:** None. All four forks are resolved; the Roomy lexicon research noted below gates a later phase, not this plan.

## Product Contract

### Summary

The four open forks of the vision are decided: classifieds monetize through posting fees on priced posts without any platform payment processing; messaging ships Bluesky-native chat first and Roomy-grade spaces later; moderation consumes labels at launch and publishes a network labeler only after validation; the platform is web-first with a mobile-optimized responsive web.

### Key Decisions

- **Classifieds monetize through posting fees on priced posts; the platform never processes transaction payments.** (session-settled: user-directed — chosen over platform-processed payments and over a pure 5%-only model: preserves the facilitator legal posture while a price-scaled fee deters junk; the user set the $1 floor as "low enough for most items, high enough to deter spam"). Governs R1, R2, R3, R4.
- **Paid placement exists only inside classifieds surfaces and never enters the Home feed.** (session-settled: user-directed — chosen over feed-injected promoted posts: intent surfaces tolerate promotion, the attention stream does not). Governs R5.
- **Messaging ships phased: Bluesky-native chat for 1:1 and friend circles first; channels-and-roles spaces later, with the interop-vs-control record-model decision made on Roomy lexicon research.** (session-settled: user-directed — chosen over committing immediately to Roomy's lexicons or a private schema: preserves the portability axis without freezing group semantics early). Governs R6, R7, R8.
- **Moderation consumes labels from day one and publishes a network labeler only after moderation is validated.** (session-settled: user-directed — chosen over publishing from day one and over never publishing: reversible posture without early accuracy and appeals obligations). Governs R9, R10.
- **Web-first, mobile-optimized responsive web.** (session-settled: user-directed — chosen over native-app-first: full web functionality first, with profile customization adapting seamlessly to small screens). Governs R11, R12.

### Requirements

**Classifieds monetization**

- R1. A transaction post is any post carrying a price field regardless of category, and it pays a posting fee equal to the greater of $1 or 5% of the declared price, capped at $25. (pack: bytescape, density-gates-before-commerce.md)
- R2. The posting fee is charged at posting time and is owed whether or not the item sells.
- R3. Posts without a price field post free, so community categories such as Find People remain free.
- R4. The poster sees the fee amount before committing, and the post publishes only once the fee is paid.

**Promotion and feed integrity**

- R5. Paid promotion raises placement only within classifieds browsing and search, is labeled Promoted, and never appears in the Home feed or any subscribed-source feed. (pack: bytescape, provenance-transparency.md)

**Messaging**

- R6. One-to-one direct messages use Bluesky's chat network so any reachable AT Protocol account can be messaged from Bytescape.
- R7. Friend-circle group chats are available early at small scale on Bluesky's native group chat.
- R8. A later phase provides group spaces with channels and roles for community-scale conversation. (pack: bytescape, portability-first.md)

**Moderation**

- R9. Bytescape subscribes to established labelers and renders their labels (blur, hide, warn) from first launch. (pack: bytescape, moderation-is-labeling.md)
- R10. Reports handled inside Bytescape produce client-internal labels at launch; a published network labeler ships only after moderation is validated on tagged community posts.

**Platform**

- R11. The web application is the primary surface and carries the full feature set, including profile customization.
- R12. Mobile browsers receive a clean, legible responsive experience in which profile customization adapts to small screens without a separate app.

### Actors

- A1. Seller — posts a priced listing and pays the posting fee.
- A2. Community member — browses classifieds, messages others, uses the platform on web or mobile.
- A3. Moderator — triages reports into labels.

### Key Flows

- F1. Priced listing posting
  - **Trigger:** A1 submits a post with a price field.
  - **Actors:** A1
  - **Steps:** Fee computed from the declared price (R1); fee shown before commit (R4); post publishes only after payment (R2, R4); same flow applies whatever category the post carries (R1).
  - **Covers:** R1, R2, R3, R4
- F2. Promotion placement
  - **Trigger:** A1 pays to promote a listing.
  - **Actors:** A1, A2
  - **Steps:** Listing receives boosted placement inside classifieds browsing and search with a Promoted label (R5); it never enters the Home feed (R5).
  - **Covers:** R5
- F3. Report to label
  - **Trigger:** A2 reports content.
  - **Actors:** A2, A3
  - **Steps:** Report enters the queue; A3 applies a client-internal label (R10); presentation layer blurs, hides, or warns per label (R9); established labelers' labels render alongside (R9).
  - **Covers:** R9, R10

### Acceptance Examples

- AE1. **Covers R1.** **Given** a poster lists a $100 item, **when** the post is submitted, **then** the fee shown is $5 (5% beats the $1 floor) and the post publishes on payment.
- AE2. **Covers R1.** **Given** a landlord lists a $2,500/month rental, **when** the post is submitted, **then** the fee shown is $25 (the cap applies).
- AE3. **Covers R1, R3.** **Given** a member posts in Find People with no price field, **when** the post is submitted, **then** no fee is charged and the post publishes.
- AE4. **Covers R1, R3.** **Given** a seller posts a priced item under Local instead of Buy/Sell/Trade, **when** the post is submitted, **then** the same fee applies because the price field drives the fee, not the category.
- AE5. **Covers R5.** **Given** a seller has paid to promote a listing, **when** a member opens the Home feed, **then** no promoted listing appears; the Promoted label appears only in classifieds browsing.

### Scope Boundaries

**Deferred for later**

- Promoted placement pricing model and rollout (after the base fee works).
- Channels-and-roles group spaces build (after Roomy lexicon research).
- Published network labeler flip (after moderation validation).
- PWA or home-screen install.
- Fee percentage and cap tuning.

**Outside this product's identity**

- Platform-processed payments, escrow, or transaction outcome tracking.
- Native-app-first development.
- Paid injection into the Home feed or attention stream.

### Dependencies / Assumptions

- Charging posting fees requires a payment rail for the platform's own sales (the listing fees). That rail is used for Bytescape's sales only, never for users' transactions — the facilitator posture of R2 depends on this separation.
- Roomy lexicon research gates the record model of R8's spaces phase (interop-first vs control-first); it does not gate R6 or R7.

### Outstanding Questions

**Deferred to Planning**

- Rental and service fee semantics where "declared price" is a rate or range rather than a sale price.
- Promoted placement pricing model.
- Which payment rail collects posting fees.

<!-- ce-section: work-relationships -->

### How This Work Fits Together

This plan covers the four fork decisions only; features built under them are separately planned later work.

- Classifieds fee and listing build — *Depends on* this plan (R1–R5).
- Messaging phase 1 (chat client surfaces) — *Depends on* this plan (R6, R7); can proceed independently of spaces.
- Group spaces with channels and roles — *Depends on* this plan (R8) *and* Roomy lexicon research; *Shares* the portability axis with `compound-packs/bytescape/portability-first.md`.
- Labeler flip — *Depends on* moderation validation (sequence step 4 in `VISION.md`).
- Feed refocus (`openspec/changes/feed-refocus/`) — *Can proceed independently of* this plan.

## Appendix

### Sources / Research

- `VISION.md` — canon; corrected classifieds definition (title, location, price, communication options, custom fields, self-expiring) and category list.
- `wiki/product/mvp-scope.md` — density gates behind the monetization posture.
- Roomy (`roomy.space`) — gold-standard reference for group messaging; lexicon model research outstanding before the R8 phase.
- Bluesky chat (`chat.bsky.*`) — the R6/R7 substrate.
