---
title: Bytescape
created: 2026-07-28
updated: 2026-08-09
status: archived-reference
---

# PRD: Bytescape

> **Archived reference (2026-10-10).** Vision and precedence now live in
> `VISION.md`; ship sequencing in `mvp-scope.md`. The §3.2 mass-market
> promise ("no Bluesky account required") is **deferred to a later phase** — Phase 1
> uses existing ATProto credentials only (see `DECISIONS.md`, 2026-10-10).

## 1. Vision

**Bytescape is your customizable home on the open web where you own your data, control your feed, and connect with people you know, allowing you to liberate your social media history from walled gardens and reconstruct your social graph in the Bluesky ecosystem.**

### The Problem

People feel trapped in walled gardens. The data they've accumulated — posts, photos, connections, memories — is held hostage by platforms that treat it as a captive asset. When someone leaves Facebook or Instagram, they leave behind years of their digital life. Meanwhile, open-web alternatives exist but don't offer a way to bring your history with you. Data lock-in is the force that keeps people trapped.

### The Opportunity

Since late 2024, searches for "Facebook alternatives" have exploded. People want out, but no single platform offers what they need: a place that preserves their history, reconnects them with scattered friends, and gives them real control over their feeds. The Bluesky ecosystem has the reach and the protocol infrastructure, but nobody in the ecosystem does archive import or social graph reconstruction.

### What Bytescape Does

Bytescape fills that gap. It's a web app where you:

- **Liberate your data** — import your Facebook, Instagram, and Threads archives from GDPR exports. Your history becomes living content, not a dead ZIP file.
- **Reconstruct your social graph** — find your friends on the open web. Match imported contacts against Bytescape and Bluesky users. One-click follow.
- **Own your presence** — customizable profiles with modernized MySpace levels of creative control. Drag and drop widget grid, resizing, themes, per-widget customization. Your profile is *yours*.
- **Control your feed** — Control what you see in your feed. Mutuals-first prioritized. No engagement algorithms. You choose your subscriptions, your follows, your feeds, based on interests selections or individually.
- **Connect and express** — A full-featured posting and interaction UI with the polish of modern social media but the indieweb aesthetic and ethos. Cross-post to Bluesky through Oauth account linking. React, reshare, tag, check in.
- **Discover the ecosystem** — Events, games, and a growing directory of AT Protocol apps. Bytescape aims to be a portal to the Bluesky ecosystem, not just a single-purpose tool.

### v2 Phase Model

v2 launches with both Bytescape-hosted and Bluesky-hosted users from the start (AD-01b). PDS hosting is active but invite-gated for initial testing (AD-13). Native signups (email/password) are invite-gated during early access and open to the general public once PDS hosting stability is confirmed (AD-13).

**Launch scope** covers the full vision: archive import (FB/IG/Threads), social graph reconstruction, customizable profiles (widgets, themes, custom pages, custom CSS), profile feed, wall posts, visitor counter, RSS generation and subscriptions, events, games, chat, marketplace, notifications, mute lists, cross-posting, reactions, and search.

**Deferred items** (true external or infrastructure gates only) are enumerated in Section 9.

## 2. The Three Pillars

### Pillar 1: Data Liberation

Import your history from walled gardens. Reconstruct your social graph. Your data lives in your AT Protocol repository — portable, owned by you. The wedge that gets people in the door.

### Pillar 2: Personalized Experience

Own your feed, customize your presence. Fine-grained control over every source — follows, feeds, hashtags. Default mode blends mutuals-first ordering with followed hashtags/feeds. Users can tune the feed from genuine connection at one end to fully anonymous, random discovery at the other. Modernized MySpace-level profile customization with an easier to use and highly flexible interface.

### Pillar 3: Fun and Useful

A good tool for a variety of purposes on the open web. Games, events, chat, ecosystem app integration. The site is laid out as a portal to the Bluesky ecosystem — navigation to events, groups, marketplace, games, and more. Useful and fun, not just a feed.

**Pillar priority at launch:** Data Liberation → Personalized Experience → Fun and Useful

## 3. Target Users

### 3.1 Primary Users

**Big Tech Refugee**

Someone leaving Facebook, Instagram, or Threads who doesn't want to lose years of their history. They may or may not have a Bluesky account yet. They want:
- To import their archive and see their history preserved
- To find where their friends went after leaving walled gardens
- A feed that shows what they subscribed to, not what an algorithm thinks will keep them scrolling
- A profile that feels like *theirs*, not a generic template

**Bluesky User Seeking More**

Someone already on Bluesky who wants a richer experience than the default client offers. They want:
- Custom profiles with creative control
- A feed with fine-grained source control and mutuals-first default
- Events, games, and ecosystem app discovery in one place
- A way to bring their Facebook/Instagram history into their Bluesky presence

### 3.2 Growth Strategy

**Early access — Both models, invite-gated:** The launch auth model is specified in Section 1. Bluesky users have zero auth friction — they already have a PDS account. The import + social graph wedge gives both user types a reason to try Bytescape. This grows the platform within the existing ecosystem while validating PDS hosting.

**Growth phase — Open PDS hosting:** Once invite-gated testing confirms PDS hosting stability, open native signups to the general public. Target the billions of people on Facebook/Instagram who haven't joined any AT Protocol app. The pitch: "Liberate your data, find your friends, start fresh on the open web — no Bluesky account required." This is the sleeper viral market.

### 3.3 Non-Users

- Users who only want a simple and clean microblogging experience (Bluesky's default client suffices)
- Users who want algorithmic content discovery as the default (Bytescape offers discovery as opt-in, not forced by default)
- Self-hosters looking for deployment software (Bytescape is open source, but self-hosting is not a product focus)
- Users who require privacy circles / audience control (deferred pending AT Protocol spec development)

## 4. User Journeys

### UJ-1: Marcus finally leaves Facebook

Marcus has been on Facebook for 15 years. He's watched the platform get worse year after year, but he can't leave — his photos, his memories, his friend connections are all there. He discovers Bytescape through a Bluesky post. He visits bytescape.social, signs in with his Bluesky account, and uploads his Facebook GDPR export. The import streams progress in real-time as 8,000 posts and 3,000 photos flow into his archive vault. He browses his history — photos from 2012, status updates from college, event RSVPs from weddings. Then he clicks "Find Friends" and Bytescape matches his Facebook contact list against existing Bluesky users. 14 of his friends are already here. He follows them all with one click. His feed immediately shows their posts — no algorithm, just his people. He shares a favorite photo from 2015 as a bytescape.social link on Bluesky, and it appears in his profile feed alongside his live posts. Within an hour, Marcus has left Facebook without losing anything.

### UJ-2: Aisha customizes her presence

Aisha has been on Bluesky for a year. She likes it, but her profile feels generic — the same layout as everyone else. She visits bytescape.social, signs in with Bluesky, and opens the profile editor. She picks the Terminal theme, arranges widgets on the 20-column grid — banner, avatar, about, posts, friends, a music player, a photo gallery. She customizes each widget's colors, fonts, and spacing. Her profile at bytescape.social/@aisha now looks like a retro terminal interface, uniquely hers. She imports her Instagram archive and her photos fill the gallery widget. Her Bluesky posts and her imported Instagram posts appear together in her profile feed, sorted chronologically, filterable by source. A friend visits her profile and sees a unified timeline of Aisha's entire digital life — not just Bluesky, not just Instagram, but everything.

### UJ-3: Dani discovers the ecosystem

Dani is on Bluesky but feels limited by the default client. She visits bytescape.social and explores the navigation. She finds Events — upcoming gatherings from Smoke Signal, browsable and RSVP-able directly in Bytescape. She finds Games — a directory of AT Protocol games she can log into and play with her Bluesky identity. She discovers WhiteWind blog posts from people she follows, rendered natively in her feed because Bytescape understands the `com.whtwnd.blog.entry` lexicon. She checks her DMs — chat is integrated via Bluesky's `chat.bsky.conversation.*` APIs. She joins a group chat space with channels and threads using `space.bytescape.*` rooms. Dani doesn't need five different apps and five different accounts. Bytescape is her portal to the whole ecosystem.

## 5. Features

### 5.1 Pillar 1: Data Liberation

#### FR-1: Facebook Archive Import

An authenticated user can upload a Facebook GDPR export (ZIP file from accountscenter.facebook.com). The system parses the archive server-side, extracts posts, photos, videos, events, and contact data, and stores them as custom lexicon records on the user's PDS.

**Consequences:**
- ZIP upload streams to disk with `DefaultBodyLimit::disable()` to accept multi-hundred-megabyte files
- Import progress streams to the client via SSE (Server-Sent Events)
- Posts are stored as `com.bytescape.archive.post` records with original timestamps
- Media (photos, videos) are stored as PDS blobs
- Import session tracks total posts, imported count, media count, and errors
- Duplicates detected by content hash and skipped

#### FR-2: Instagram Archive Import

An authenticated user can upload an Instagram GDPR export (ZIP file from the same export portal). Same processing model as Facebook.

**Consequences:**
- Instagram archive parser handles photos, videos, captions, timestamps, and contact data
- Stored as `com.bytescape.archive.post` records (same lexicon as Facebook)
- Media stored as PDS blobs

#### FR-3: Threads Archive Import

[ASSUMPTION] Threads exports are available from the same accountscenter.facebook.com portal. If the export format differs from Facebook/Instagram, a separate parser is needed. If the format is shared, the existing parser handles it.

**Consequences:**
- Stored as `com.bytescape.archive.post` records (same lexicon as Facebook/Instagram)
- Parser dispatch is conditional on the export format — open question Q1 (Section 10) gates whether a separate parser is needed
- Media stored as PDS blobs

#### FR-4: Social Graph Reconstruction

After importing an archive, the system extracts the user's contact/friend list and matches it against existing Bytescape and Bluesky users. Matched contacts are presented with one-click follow capability.

**Consequences:**
- Primary matching model: mutual archive import — if User A imports an archive containing User B, and User B imports an archive containing User A, suggest a follow
- Additional matching: handle/name matching against Bluesky DIDs and Bytescape profiles
- Match confidence is scored and displayed (high/medium/low)
- Users can follow all matched contacts with one click, or select individually
- Follows create `app.bsky.graph.follow` records on the user's PDS
- Unmatched contacts are stored for future matching as new users join
- Existing walled garden follower import tools (e.g., Sky Follower Bridge) should be analyzed for matching patterns and inspiration

#### FR-5: Archive Vault

Imported content is stored in a custom lexicon (`com.bytescape.archive.post`) that renders on Bytescape but is not delivered to followers' feeds. Users browse their imported history in a dedicated vault view.

**Consequences:**
- Archive records appear on the firehose but are ignored by Bluesky's AppView (standard AT Protocol behavior for custom lexicons)
- Bytescape's AppView indexes archive records for the user's vault view
- Vault view supports browsing by date, source platform, and content type
- Imported posts do NOT appear in followers' feeds (no network spam), but they do show up on the profile feed widget (see FR-11)
- Archived posts are viewable and shareable on Bytescape. They can be shared elsewhere as a bytescape.social link, or rendered natively on any platform that adopts the `com.bytescape.archive.post` lexicon

#### FR-6: Twitter/X Archive Import (Deferred)

[ASSUMPTION] Post-launch feature based on user demand. Existing tools (Cyd, Porto) handle Twitter/X import; Bytescape would offer unified multi-platform import as a differentiator.

#### FR-7: RSS Feed Generation from Imported Data

Imported content can be exposed as subscribable RSS feeds, allowing users to share their archived history with followers. Viability is gated by walled-garden platform restrictions.

#### FR-8: RSS Subscriptions for Social Graph Reconstruction

RSS subscriptions to external feeds supplement social graph reconstruction by monitoring walled garden contacts who post publicly. Viability is gated by platform-specific RSS availability.

### 5.2 Pillar 2: Personalized Experience

#### FR-9: Customizable Profiles

A user can customize their profile at bytescape.social/@{handle} with a drag-and-drop widget grid, theme selection, and per-widget customization.

**Consequences:**
- 20-column grid with drag-and-drop placement (widgets stay where placed, no auto-compaction)
- 16 widget types: ProfileHeader, Banner, AvatarName, About, Stats, Posts, Friends, Following, Wall, MusicPlayer, PhotoGallery, CustomLinks, Navigation, Status, Interests, VisitorCounter
- Four preset themes: Terminal, GeoCities 1999, XP Aero, Modern/Clean
- Per-widget customization: background color, text color, font, padding, shadow, border radius
- Widget layout, per-widget config, and themes stored in the application database (`users.widget_layout`, `users.widget_themes`, `users.theme`, `users.theme_app_wide`, `users.custom_css`) — NOT PDS records. Only profile identity (name, bio, avatar, banner) stays an `app.bsky.actor.profile` record (decision 2026-08-09)
- Custom nav pages (Blog, Gallery, Custom Link) with real page rendering — not placeholders
- Custom CSS editor for power users (sanitized/allowlisted subset; never applies to edit-mode chrome)
- Selected theme applies app-wide by default, with an opt-out "Apply theme across the app" toggle (profile-only mode)
- Mobile: grid collapses to single column below 900px viewport width
- Unauthenticated visitors can view public profiles

#### FR-10: Feed with Per-Source Control

An authenticated user can view a unified feed aggregating content from all sources, with per-source filtering and control.

**Consequences:**
- Sources: Following (Bluesky follows), Feeds (Bluesky custom feeds), Hashtags, Subscriptions
- Left panel with per-category search bars (context-appropriate)
- Mobile uses slide-out drawer (both desktop sidebar + mobile drawer)
- Default mode: mutuals-first (mutual follows, aka "Friends", prioritized, loaded before feeds or hashtags)
- Reverse-chronological combined feed with "All" as default
- Additive multi-select filtering
- Delete with confirmation popup
- Feed loads within 2 seconds on standard connections
- Refactored from the existing implementation: a liberal refactor with strategic ergonomic planning

#### FR-11: Profile Feed

A user's profile displays a unified feed mixing imported content and live Bluesky posts, sorted chronologically by default.

**Consequences:**
- Sortable by source (Facebook, Instagram, Bluesky, etc.) and content type (image, video, event, etc.)
- Imported and live posts treated equally in the profile feed
- The only difference: imported posts are NOT delivered to followers (`com.bytescape.archive.post`), while live posts ARE delivered (`app.bsky.feed.post`)
- Archive posts can be reshared on Bluesky as a bytescape.social link, or on Bytescape directly using `com.bytescape.archive.post`

#### FR-12: Media Screen

A dedicated media-focused view showing all media (imported and live), sortable by source and type.

**Consequences:**
- Grid gallery with lightbox view
- Sortable by source (Facebook, Instagram, Bluesky) and type (video, image)
- Both imported media and Bluesky post media appear together

#### FR-13: Post Composer

A full-featured post composition UI with modern social media functionality.

**Consequences:**
- Text content with rich formatting
- Media attachments: images, video
- [ASSUMPTION] Livestreams: scope TBD pending AT Protocol capabilities. Stream.place is the reference for livestreaming on AT Protocol. **Stories RESOLVED (2026-11-02):** shipped as `com.bytescape.story` records + `app.bsky.actor.status` text cards (see `wiki/architecture/stories-cross-protocol.md`)
- Tagging users (mentions)
- Location check-ins
- Mood/status prefixes
- Link attachments
- Cross-posting to Bluesky (creates `app.bsky.feed.post` on user's PDS)
- Empty posts rejected (no content, no media, no link)

#### FR-14: Cross-Posting to Bluesky

A user with a linked Bluesky account can cross-post content to Bluesky simultaneously with their Bytescape post.

**Consequences:**
- Cross-post creates `app.bsky.feed.post` record on user's PDS via OAuth
- Per-platform results returned (success/failure with error messages)
- Failure on Bluesky does not prevent Bytescape post from succeeding
- Cross-posted content includes media where supported

#### FR-15: Reactions and Engagement

A full interaction UI with Facebook/Instagram-level functionality.

**Consequences:**
- Emoji reactions (toggle on/off by clicking same emoji)
- Likes (creates `app.bsky.feed.like` record)
- Reposts/boosts (creates `app.bsky.feed.repost` record)
- Quote posts
- Tagging people in posts
- Location check-ins
- Comment threading
- Optimistic UI updates with rollback on failure

#### FR-16: Privacy Posting Modes (Deferred)

[ASSUMPTION] Deferred pending AT Protocol privacy spec development. Will implement once the spec is clear on post visibility levels, audience control, and how custom visibility interacts with the broader Bluesky network.

### 5.3 Pillar 3: Fun and Useful

#### FR-17: Events

Users can discover, create, and interact with events using existing AT Protocol event lexicons.

**Consequences:**
- Adopt the community standard `community.lexicon.calendar.event` and `community.lexicon.calendar.rsvp` lexicons (interoperable with atmo.rsvp and Smoke Signal)
- Bytescape AppView indexes event records from the firehose
- Event discovery: browse upcoming events, filter by date/location/search
- Event creation: name, description, time, location, image
- RSVP: going/maybe/not going
- Comments and emoji reactions on events

#### FR-18: Games Directory

A directory of AT Protocol games that users can discover and play with their Bluesky identity.

**Consequences:**
- Simple directory listing of AT Protocol games
- Users can log into games with their Bluesky identity (OAuth)
- No game hosting — Bytescape is a discovery portal, not a game platform

#### FR-19: Chat and DMs

Private messaging integrated with Bluesky's messaging ecosystem.

**Consequences:**
- 1:1 DMs via Bluesky's `chat.bsky.conversation.*` APIs for all users (including native Bytescape users). Bytescape acts as a chat client.
- Group chat via `space.bytescape.*` lexicons (see FR-20)
- [ASSUMPTION] E2EE for 1:1 DMs depends on Bluesky's chat spec direction. For group chat, TBD based on custom lexicon design.

#### FR-20: Groups (Group Chat)

Group chat with spaces, channels, and threads using custom lexicons inspired by Roomy's structure.

**Consequences:**
- Data model: Space → Room → Message (replicating Roomy's `space.roomy.*` structure in `space.bytescape.*` namespace)
- Lexicons: `space.bytescape.space`, `space.bytescape.member`, `space.bytescape.room`, `space.bytescape.message`
- Spaces: top-level containers with name, description, handle, member count
- Space membership: owner, admin, member roles
- Rooms: channels within spaces with name and description
- Room messages: content with thread support (reply_to for threading)
- Bytescape's AppView indexes all group chat records from the firehose
- Using our own namespace so we can evolve independently without depending on Roomy's implementation

#### FR-21: Marketplace

A marketplace for buying, selling, and trading between users.

**Consequences:**
- Listings stored as `com.bytescape.exchange.listing` records on user's PDS
- Listing fields: title, description, price (free-form), category, images, status
- Status tracking: active, sold, expired
- View count tracking
- Bytescape's AppView indexes marketplace records from the firehose
- Discovery: browse listings, filter by category/location/search

#### FR-22: Polls (Deferred)

[ASSUMPTION] Post-launch feature. Multi-option polls with configurable duration.

#### FR-23: Additional Ecosystem App Integrations (Deferred)

[ASSUMPTION] Ongoing integration with AT Protocol ecosystem apps as the platform matures. The site's navigation paradigm is a portal to the ecosystem — events, groups, marketplace, games, and more.

### 5.4 New Features for v2

*Features added in the v2 re-draw, grouped here for release-notes traceability; each belongs to a pillar above or to infrastructure (5.5).*

#### FR-24: Wall Posts

Users can post on each other's profile walls. Wall posts are addressed to a specific user's profile, not delivered to the main feed.

**Consequences:**
- Wall posts stored in the application database (app state, not PDS records — consistent with the widget-storage and visitor-comment decisions of 2026-08-09). A `com.bytescape.social.wallPost` PDS lexicon remains a possible interop path later, but v2.0 uses app-DB records
- A wall post is visible only on the recipient's profile, not in the main feed
- The recipient can promote a wall post to a regular `app.bsky.feed.post` if they want to share it in their main feed
- Login required to post on a wall (identity from the handle — no anonymous spam)
- The profile owner can delete any wall post on their profile; deletion never affects the visitor counter
- A new wall post notifies the owner via the `notifications` table (FR-25)
- The v1 guestbook implementation ("📖 Sign My Guestbook!") is the initial wall skin; the visitor_counter widget's comments (FR-9) provide the seed data

#### FR-25: Notifications

In-app notifications generated by the AppView when processing firehose events.

**Consequences:**
- Notification types: like, repost, follow, reply, mention, profile_comment (a wall post or visitor-counter comment on the user's profile)
- Notifications generated by AppView consumer when processing firehose events (and by app-DB events for profile_comment)
- Stored in the `notifications` table (not PDS records — application state)
- Read/unread status tracking
- Notification feed with filtering by type
- Real-time delivery via SSE or WebSocket

#### FR-26: Mute Lists

Users can mute other accounts to hide their content from feeds and notifications.

**Consequences:**
- Muted accounts stored in the `muted_accounts` table (application state, not PDS records)
- Muting hides the muted user's posts from the muter's feed
- Muting suppresses notifications from the muted user
- Mute is one-directional (User A mutes User B; User B doesn't know)
- Mute list management UI (add, remove, view muted accounts)

### 5.5 Infrastructure

#### FR-27: Authentication — Bluesky OAuth + Native Signups

Users sign in via Bluesky OAuth or sign up directly on Bytescape. The dual PDS model supports both from the start.

**Consequences:**
- **Bluesky-hosted users:** OAuth flow with DPoP and PKCE. Their DID is from bsky.social (e.g., `did:plc:abc123`). Their repo lives on bsky.social's PDS. We read via the firehose, write via XRPC calls using their OAuth token.
- **Bytescape-hosted users:** Native signup with email, password, and username. We create a `did:web` DID, host their PDS repo, manage their signing key. We can read and write to their repo directly.
- Access tokens short-lived; refresh tokens handle renewal
- PostgreSQL stores app state: import sessions, AppView index, preferences, social graph matches
- The AT Protocol layer uses a trait-based `AtProtoClient` with `LocalPdsClient` and `RemotePdsClient` implementations (AD-07). Application code receives `Arc<dyn AtProtoClient>` and doesn't know whether it's talking to a local or remote PDS.

#### FR-28: AppView

Bytescape runs a custom AppView that indexes records from the firehose for cross-user queries.

**Consequences:**
- Indexes `com.bytescape.archive.*` records (archive vault)
- Indexes `community.lexicon.calendar.*` records (events — interoperable with atmo.rsvp and Smoke Signal)
- Indexes `com.bytescape.exchange.*` records (marketplace)
- Indexes `space.bytescape.*` records (group chat)
- Powers cross-user social graph matching
- Powers cross-lexicon search (posts, events, profiles, marketplace, group chat)
- Consumes firehose from Bluesky relay network
- Widget layouts, wall posts, and visitor comments are app-DB state (decisions 2026-08-09) and are NOT indexed from the firehose

#### FR-29: Cross-PDS + Cross-Lexicon Search

Search across all AT Protocol content types, not just standard Bluesky posts.

**Consequences:**
- Search across PDSs for all AT Protocol compatible users
- Cross-lexicon: search posts, events, profiles, archive content, marketplace listings, group chat messages

#### FR-30: Native PDS-Hosted Signups

Users without a Bluesky account can sign up directly on bytescape.social. PDS hosting is active from the start but invite-gated for testing (AD-13).

**Consequences:**
- Bytescape hosts a PDS for native user accounts
- Users get a `did:web:{handle}.bytescape.social` identity (AD-03)
- Coexists with Bluesky OAuth (users can still sign in with existing Bluesky accounts)
- Invite codes gate PDS access during initial testing phase
- AppView access is separate: users can use Bytescape's AppView features via Bluesky OAuth without a Bytescape PDS account
- Scaling considerations: multiple domains (bytescape.us, bytescape.world) for load balancing when PDS hosting opens broadly
- Deployment: single binary + PostgreSQL + S3. Frontend served by the same binary.

## 6. Non-Goals (Explicit)

- **ActivityPub federation** — ActivityPub is removed entirely. No inbox, outbox, HTTP Signatures, or AP actor. Bytescape is AT Protocol only.
- **Mastodon integration** — No Mastodon account linking, no cross-posting to Mastodon. If users want Mastodon bridging, they use external tools like Bridgy Fed.
- **Algorithmic feed as default** — Discovery exists as opt-in. The default feed is chronological and subscription-based. No engagement-driven algorithms.
- **Crypto or token economy** — No cryptocurrency, token, or blockchain component.
- **Advertising platform** — No ad network, no promoted posts, no algorithmic boosting for pay.
- **Enterprise SSO** — No LDAP, SAML, or enterprise identity integration.
- **Self-hosting as a product** — Code is open source. Anyone can fork and self-host. But self-hosting tooling and documentation is not a product focus.
- **Native mobile app at launch** — Responsive web first. "Download our app for optimal experience" is an anti-pattern we reject. Mobile = responsive web app. Mobile app in app stores possibly a goal later, but not initial priority.
- **Local-first storage for MVP** — Local-first P2P is a purity ideal to strive toward, not an MVP requirement. Web app with server-side storage is sufficient for launch.
- **Privacy circles / audience control** — Deferred pending AT Protocol privacy spec development. Will not implement custom visibility controls until the spec is clear.

## 7. Success Metrics

**Privacy vs Success Metrics:** Bytescape should not track users at the level needed to measure metrics like "60% account linking rate" or "20% cross-platform posting rate" per individual. Success metrics are measured in aggregate, not per-user — for example, total registered users, monthly active users. Metrics are simple and publicly viewable.

### Primary

- **SM-1: Import completion** — ≥70% of users who start a data import complete it successfully. Validates FR-1, FR-2, FR-5.
- **SM-2: Social graph activation** — [ASSUMPTION] ≥40% of users who import an archive use the "Find Friends" feature and follow at least one matched contact. Validates FR-4.
- **SM-3: Return rate** — ≥30% of registered users return within 7 days of first login. Validates the overall value proposition.

### Secondary

- **SM-4: Feed latency** — Unified feed loads in ≤2 seconds on standard connections. Validates FR-10.
- **SM-5: Profile customization** — ≥40% of users customize their profile (change theme, add widgets, or edit layout) within 14 days. Validates FR-9.
- **SM-6: Cross-posting adoption** — [ASSUMPTION] ≥20% of posts are cross-posted to Bluesky from Bytescape. Validates FR-14.

### Counter-Metrics (Do Not Optimize)

- **SM-C1: Time on feed** — Do not optimize for session length. A healthy user gets what they need and leaves. An engaged user is not a retained-by-algorithms user.
- **SM-C2: Content volume** — Do not optimize for posts per user. Quality over quantity. Incentivizing posting incentivizes noise.
- **SM-C3: Granular user tracking** — Do not track users at the individual level needed to measure per-user metrics like "account linking rate" or "cross-platform posting rate." Success metrics are measured in aggregate.

## 8. Non-Functional Requirements

### Performance

- Feed loads within 2 seconds on standard connections (p95)
- Profile pages load within 2 seconds (p95)
- Import progress SSE stream updates within 1 second of server-side processing
- OAuth callback completes within 5 seconds
- Search returns within 3 seconds for standard queries

### Scalability

- Single binary architecture: PDS, AppView, and REST API in one Axum binary. Scales with PostgreSQL + standard web infrastructure.
- PDS hosting introduces scaling considerations as user count grows. Multiple domains (bytescape.us, bytescape.world) available for load balancing.
- PDS hosting scaling is readiness-dependent, not time-dependent. Do not open prematurely.

### Security

- OAuth with DPoP and PKCE for all Bluesky authentication
- Short-lived access tokens (15 min) with refresh token rotation
- Bot-resistant registration: invite codes for initial testing, plus CAPTCHA and email verification and/or 2FA
- Data export (GDPR compliance) required before public launch
- Password reset / account recovery required before public launch (for native PDS signups)
- Account deletion cascades all user data and invalidates all tokens

### Accessibility

**v2.0 floor (platform chrome guarantee — decision 2026-08-09):**
- aria-labels on all icon-only buttons (⚙ ✕ 📝 [+])
- Focus trap + focus return in modals (composer, identity, theme editor, comment popup); Escape closes modals and dropdowns
- Form labels on all inputs; aria-live for optimistic updates and import progress
- Visitor counter renders a text alternative (odometer digits are decorative; the count is readable)
- Color-not-only-signal: badges, filter chips, and status indicators carry a non-color cue (icon, text, shape)
- `prefers-reduced-motion` respected

**Post-launch:**
- Full keyboard path for the profile grid (arrow-key move, keyboard resize or steppers)
- WCAG AA contrast audit for chrome
- ≥44px touch targets
- 200% zoom without breakage
- Color-blind mode: platform-level setting remapping the palette to a color-blind-safe variant (chrome unconditional; user themes get a best-effort "render in safe palette" option)

### Compatibility

- AT Protocol compliant: custom lexicons follow namespace conventions, records are valid DAG-CBOR, firehose events are standard format
- Bluesky ecosystem compatible: OAuth client works with bsky.social and any standard PDS
- Web standards: responsive web app works in modern browsers (Chrome, Firefox, Safari, Edge)
- No native mobile app at launch — responsive web only

### Data Portability

- All portable user content is stored as AT Protocol records on the user's PDS — portable by design. Widget layouts, wall posts, and visitor comments are app-DB state (decisions 2026-08-09) and are not PDS-portable
- Custom lexicon records (`com.bytescape.archive.*`, `com.bytescape.exchange.*`, `space.bytescape.*`) are documented and adoptable by other apps. Widget layouts, wall posts, and visitor comments are app-DB state (decisions 2026-08-09), not lexicon records
- Community lexicon records (`community.lexicon.calendar.*`) are interoperable by design
- Archive data is exportable in standard formats
- No lock-in: users can leave Bytescape and take their data to any AT Protocol client

## 9. Out of Scope

### Deferred Features

| Item | Reason | Revisit |
|------|--------|---------|
| Twitter/X archive import | Existing tools handle this; focus on FB/IG/Threads wedge first | Post-launch, demand-driven |
| Privacy circles / audience control | AT Protocol privacy spec still WIP | Post-launch, spec-dependent |
| Polls | Not launch-critical | Post-launch |
| Native mobile app | Responsive web first; anti-pattern rejected | Post-launch, possible (PWA add-to-home-screen is the interim path) |
| Local-first storage | Purity ideal, not MVP requirement | Post-launch |
| did:plc support | did:web sufficient for launch; did:plc requires PLC directory integration (AD-03) | Post-1.0 |
| Opening PDS hosting to all users | Invite-gated testing must confirm stability first (AD-13) | Readiness-dependent, not time-dependent |
| Additional ecosystem app integrations | Ongoing as the platform matures (FR-23) | Continuous |

### Pre-Public-Launch Requirements

The following must exist before public launch but are not blocking for beta:
- Data export (GDPR compliance)
- Bot-resistant registration mechanism (invite codes + CAPTCHA + email/2FA)
- Password reset / account recovery (for native PDS signups)

## 10. Open Questions

1. **Threads export format** — Does Threads use the same GDPR export format as Facebook/Instagram (accountscenter.facebook.com), or does it require a separate parser? Needs verification.

2. **Bluesky messaging spec** — The Bluesky team is actively developing messaging. What is the current state of the chat spec? How does `chat.bsky.conversation.*` evolve? What about E2EE support?

3. ~~Livestreams and stories/shorts in post composer~~ — **Stories RESOLVED (2026-11-02):** feasible via custom lexicon; shipped as `com.bytescape.story` + official `app.bsky.actor.status` (see `wiki/architecture/stories-cross-protocol.md`). Livestreams remain open: Stream.place is the reference — study their approach.

### Resolved Questions

- **Custom lexicon namespace** (resolved): v2 uses `com.bytescape.*` for Bytescape-specific records (archives, marketplace, group chat) and `community.lexicon.*` for interoperable records (events, RSVPs). Standard AT Protocol records use `app.bsky.*`. Widgets, wall posts, and visitor comments are app-DB state, not lexicons (AD-01c, decisions 2026-08-09)
- **Profile customization storage** (resolved 2026-08-09): Widgets are NOT PDS records. The one-record-per-widget pattern (`com.bytescape.widget.*`) is dead. Widget layout, per-widget config, and themes live in the application database (`users.widget_layout`, `users.widget_themes`, `users.theme`, `users.custom_css`). Only profile identity (name, bio, avatar, banner) stays an `app.bsky.actor.profile` record. Same for wall posts and visitor comments — app-DB state, not lexicons.
- **Bot-resistant registration** (resolved): Invite codes for initial testing and early access, plus CAPTCHA and email verification and/or 2FA.
- **Social graph matching algorithm** (resolved): Primary model is mutual archive import — if User A imports an archive containing User B, and User B imports an archive containing User A, suggest a follow. Existing walled garden follower import tools (Sky Follower Bridge, etc.) should be analyzed for additional matching patterns.
- **PDS hosting phase** (resolved): PDS hosting is active from the start, invite-gated for testing. Not a Phase 2 feature. (AD-13)
- **Chat architecture** (resolved): 1:1 DMs via Bluesky's `chat.bsky.conversation.*` APIs. Group chat via `space.bytescape.*` lexicons (Space → Room → Message). No Matrix. (AD-01d, AD-10)
- **Lexicon strategy** (resolved): Use existing community/standard lexicons when they exist (`app.bsky.*`, `community.lexicon.calendar.*`), define `com.bytescape.*` only for data types with no existing standard. (AD-01c)

## 11. Assumptions Index

- **[A1]** Threads archive exports are available from the same portal as Facebook/Instagram. If the format differs, a separate parser is needed.
- **[A2]** Success metrics are measured in aggregate, not per-user, to respect user privacy preferences.
- **[A3]** The feed refactor is a liberal refactoring of the existing implementation with strategic ergonomic improvements, not a full rebuild.
- **[A4]** Stories delivered (2026-11-02) outside the post composer as a first-class surface (`com.bytescape.story` + `app.bsky.actor.status`). Livestreams remain desired but scope depends on AT Protocol capabilities; Stream.place is a reference.
- **[A5]** Chat/DMs follow Bluesky's `chat.bsky.conversation.*` APIs for 1:1 messaging. Group chat uses `space.bytescape.*` custom lexicons. E2EE implementation depends on Bluesky's chat spec direction.
- **[A6]** Data export (GDPR compliance) and bot-resistant registration (invite codes + CAPTCHA + email/2FA) must exist before public launch.
- **[A7]** The import pipeline writes to the user's PDS via `AtProtoClient::apply_writes` (batched commits), not directly to PostgreSQL. Import session metadata stays in app state tables. (AD-08)
- **[A8]** The AT Protocol layer uses a trait-based `AtProtoClient` with `LocalPdsClient` and `RemotePdsClient` implementations. Application code doesn't know whether it's talking to a local or remote PDS. (AD-07)
- **[A9]** Media blobs are uploaded via `AtProtoClient` to the PDS blob store. For remote PDS users, this is bsky.social's blob store. For local PDS users, this is Bytescape's S3 storage. (AD-12)
- **[A10]** Social graph matching uses mutual archive import as the primary model: if both users import archives containing each other, suggest a follow.
- **[A11]** PDS hosting scaling is readiness-dependent, not time-dependent. Do not open prematurely. (AD-13)
