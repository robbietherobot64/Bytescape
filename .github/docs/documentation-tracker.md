# Bytescape Codebase Documentation Tracker

> **Goal**: Every source file should be commented so a layperson can open it and fully understand what it does, why it exists, and how it connects to the rest of the system.
>
> **Total scope**: 164 source files · ~58,000 lines · Rust + TypeScript/React
>
> **Approach**: Work bottom-up through the architecture — foundation first, then layers that depend on it.

---

## Progress Summary

| Batch | Layer | Files | Lines | Status |
|-------|-------|-------|-------|--------|
| 1 | Core Types & Models | 6 | 1,911 | ⬜ Not started |
| 2 | Database Layer | 6 | 2,312 | ⬜ Not started |
| 3 | Backend Core (Auth, Keys, App State) | 6 | 3,731 | ⬜ Not started |
| 4 | Platform Connectors | 4 | 6,662 | ⬜ Not started |
| 5 | P2P, Sync & Real-time | 8 | 3,393 | ⬜ Not started |
| 6 | ActivityPub Federation | 9 | 2,866 | ⬜ Not started |
| 7 | Tauri Commands (Backend API) | 20 | 9,824 | ⬜ Not started |
| 8 | Frontend Foundation | 8 | 2,581 | ⬜ Not started |
| 9 | Frontend Hooks | 7 | 2,349 | ⬜ Not started |
| 10 | Frontend Utils | 12 | 1,429 | ⬜ Not started |
| 11 | Frontend Screens | 16 | 7,526 | ⬜ Not started |
| 12 | Frontend Components (Large) | 13 | 5,694 | ⬜ Not started |
| 13 | Frontend Components (Medium) | 13 | 2,164 | ⬜ Not started |
| 14 | Feed, Lenses, Widgets & Small Components | 33 | 2,698 | ⬜ Not started |
| **Total** | | **161** | **~55,140** | |

*Note: 3 trivial files (vite-env.d.ts, setupTests.ts, main.rs) excluded from batching.*

---

## Batch 1 — Core Types & Models (Foundation)

> **Why first**: Every other file in the codebase imports from these. Understanding the data shapes is the key to understanding everything else.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/types/index.ts` | 658 | ⬜ | TypeScript interfaces mirroring Rust structs |
| `src-tauri/src/models/mod.rs` | 659 | ⬜ | Core Rust data models (BytescapeItem, etc.) |
| `src-tauri/src/models/groups.rs` | 133 | ⬜ | Group/community data models |
| `src-tauri/src/connectors/mod.rs` | 77 | ⬜ | Source enum, connector trait definitions |
| `src-tauri/src/error.rs` | 222 | ⬜ | Error types used across the backend |
| `src-tauri/src/activitypub/types.rs` | 164 | ⬜ | ActivityPub protocol type definitions |

**Subtotal**: 6 files · 1,913 lines

---

## Batch 2 — Database Layer

> **Why second**: The database is where all data lives. Understanding the storage layer explains how data flows in and out.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/db/mod.rs` | 437 | ⬜ | Database manager, table definitions, core operations |
| `src-tauri/src/db/groups.rs` | 940 | ⬜ | Group CRUD, membership, invites |
| `src-tauri/src/db/vault.rs` | 615 | ⬜ | Personal vault storage operations |
| `src-tauri/src/db/chat.rs` | 193 | ⬜ | Chat message storage |
| `src-tauri/src/db/peers.rs` | 82 | ⬜ | Peer identity tracking |
| `src-tauri/src/db/contacts.rs` | 45 | ⬜ | Contact/friend storage |

**Subtotal**: 6 files · 2,312 lines

---

## Batch 3 — Backend Core (Auth, Keys, App State)

> **Why third**: These files set up the application — how it boots, how it authenticates users, how it stores secrets.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/lib.rs` | 1,028 | ⬜ | App entry point, Tauri command registration, AppState |
| `src-tauri/src/auth.rs` | 74 | ⬜ | Bluesky session management |
| `src-tauri/src/keychain_encrypted.rs` | 1,481 | ⬜ | Secure credential storage with encryption |
| `src-tauri/src/oauth.rs` | 1,031 | ⬜ | OAuth PKCE+DPoP implementation |
| `src-tauri/src/contacts.rs` | 113 | ⬜ | Contact management logic |
| `src-tauri/src/main.rs` | 6 | ⬜ | Rust binary entry point (trivial) |

**Subtotal**: 6 files · 3,733 lines

---

## Batch 4 — Platform Connectors

> **Why fourth**: These translate external platform data (Bluesky, Facebook, Mastodon, Instagram) into Bytescape's universal format.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/connectors/bluesky.rs` | 2,093 | ⬜ | AT Protocol / Bluesky API integration |
| `src-tauri/src/connectors/facebook.rs` | 2,676 | ⬜ | Facebook archive HTML parser |
| `src-tauri/src/connectors/mastodon.rs` | 1,386 | ⬜ | Mastodon / ActivityPub client |
| `src-tauri/src/connectors/instagram.rs` | 507 | ⬜ | Instagram archive JSON parser |

**Subtotal**: 4 files · 6,662 lines

---

## Batch 5 — P2P, Sync & Real-time

> **Why fifth**: These handle peer-to-peer networking, data synchronization, and real-time features. Complex but central to Bytescape's local-first architecture.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/sync.rs` | 668 | ⬜ | Iroh Docs/Blobs/Gossip sync engine |
| `src-tauri/src/protocol.rs` | 702 | ⬜ | P2P protocol message handling |
| `src-tauri/src/chat.rs` | 419 | ⬜ | P2P chat implementation |
| `src-tauri/src/collab.rs` | 305 | ⬜ | Collaborative editing (CRDT) |
| `src-tauri/src/blob_hash.rs` | 265 | ⬜ | Content-addressed blob hashing |
| `src-tauri/src/richtext.rs` | 650 | ⬜ | Rich text facet parsing (AT Protocol) |
| `src-tauri/src/media_server.rs` | 77 | ⬜ | Local media serving for WebView |
| `src-tauri/src/traffic.rs` | 107 | ⬜ | Network traffic monitoring |

**Subtotal**: 8 files · 3,193 lines

---

## Batch 6 — ActivityPub Federation

> **Why sixth**: ActivityPub is the federation protocol for connecting to Mastodon, Lemmy, and other fediverse servers.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/activitypub/mod.rs` | 14 | ⬜ | Module declarations |
| `src-tauri/src/activitypub/inbox.rs` | 849 | ⬜ | Incoming federated message handling |
| `src-tauri/src/activitypub/outbox.rs` | 146 | ⬜ | Outgoing federated message publishing |
| `src-tauri/src/activitypub/actor.rs` | 170 | ⬜ | ActivityPub actor representation |
| `src-tauri/src/activitypub/activities.rs` | 211 | ⬜ | Activity type builders (Create, Like, etc.) |
| `src-tauri/src/activitypub/signatures.rs` | 404 | ⬜ | HTTP signatures for federation auth |
| `src-tauri/src/activitypub/webfinger.rs` | 117 | ⬜ | WebFinger discovery protocol |
| `src-tauri/src/activitypub/group_actor.rs` | 425 | ⬜ | Group federation actor |
| `src-tauri/src/activitypub/lemmy.rs` | 531 | ⬜ | Lemmy-specific federation compat |

**Subtotal**: 9 files · 2,867 lines

---

## Batch 7 — Tauri Commands (Backend API Surface)

> **Why seventh**: These are the "verbs" of the backend — every function the frontend can call. Documenting these creates a complete API reference.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/commands/mod.rs` | 44 | ⬜ | Command module declarations |
| `src-tauri/src/commands/auth.rs` | 541 | ⬜ | Login, logout, session refresh commands |
| `src-tauri/src/commands/import.rs` | 2,910 | ⬜ | Archive import pipeline (largest file!) |
| `src-tauri/src/commands/media.rs` | 1,671 | ⬜ | Media blob operations |
| `src-tauri/src/commands/groups.rs` | 1,189 | ⬜ | Group management commands |
| `src-tauri/src/commands/mastodon.rs` | 1,088 | ⬜ | Mastodon-specific API commands |
| `src-tauri/src/commands/peers.rs` | 685 | ⬜ | P2P peer management commands |
| `src-tauri/src/commands/vault.rs` | 660 | ⬜ | Personal vault file operations |
| `src-tauri/src/commands/posting.rs` | 519 | ⬜ | Content creation & cross-posting |
| `src-tauri/src/commands/browser.rs` | 504 | ⬜ | In-app browser/WebView commands |
| `src-tauri/src/commands/feed_aggregation.rs` | 493 | ⬜ | Feed fetching & merging pipeline |
| `src-tauri/src/commands/profile.rs` | 250 | ⬜ | User profile management |
| `src-tauri/src/commands/interactions.rs` | 334 | ⬜ | Likes, reposts, bookmarks |
| `src-tauri/src/commands/syndicate.rs` | 257 | ⬜ | Cross-platform syndication |
| `src-tauri/src/commands/node.rs` | 254 | ⬜ | Iroh node management |
| `src-tauri/src/commands/export.rs` | 183 | ⬜ | Data export commands |
| `src-tauri/src/commands/unfurl.rs` | 166 | ⬜ | URL preview/unfurl commands |
| `src-tauri/src/commands/relay.rs` | 152 | ⬜ | Relay server commands |
| `src-tauri/src/commands/ap.rs` | 150 | ⬜ | ActivityPub-related commands |
| `src-tauri/src/commands/collab.rs` | 96 | ⬜ | Collaborative document commands |
| `src-tauri/src/commands/classify.rs` | 74 | ⬜ | Content classification commands |

**Subtotal**: 20 files · 10,220 lines

---

## Batch 8 — Frontend Foundation

> **Why eighth**: The app shell, global state, and data provider layer. Every screen and component depends on these.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/main.tsx` | 12 | ⬜ | React DOM entry point |
| `src/App.tsx` | 770 | ⬜ | App shell, routing, tab management |
| `src/store/index.ts` | 476 | ⬜ | Zustand global state store |
| `src/providers/DataProvider.ts` | 591 | ⬜ | Abstract data provider interface |
| `src/providers/TauriDataProvider.ts` | 642 | ⬜ | Tauri-specific data provider implementation |
| `src/providers/DataProviderContext.tsx` | 50 | ⬜ | React context for data provider injection |
| `src/providers/index.ts` | 19 | ⬜ | Provider re-exports |
| `src/contexts/TabActiveContext.tsx` | 21 | ⬜ | Tab visibility context for resource management |

**Subtotal**: 8 files · 2,581 lines

---

## Batch 9 — Frontend Hooks

> **Why ninth**: Custom React hooks encapsulate reusable stateful logic (feed fetching, location, blob URLs). Core to understanding data flow.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/hooks/useUnifiedFeed.ts` | 641 | ⬜ | Multi-source feed aggregation hook |
| `src/hooks/useGroupsData.ts` | 450 | ⬜ | Group membership & data management |
| `src/hooks/useLocation.ts` | 445 | ⬜ | Geolocation with privacy controls |
| `src/hooks/useBlobUrl.ts` | 294 | ⬜ | Content-addressed media URL resolution |
| `src/hooks/useCollabDoc.ts` | 216 | ⬜ | CRDT collaborative document hook |
| `src/hooks/useTheme.ts` | 159 | ⬜ | Theme loading and application |
| `src/hooks/useCharacterCount.ts` | 144 | ⬜ | Post character counting with facets |

**Subtotal**: 7 files · 2,349 lines

---

## Batch 10 — Frontend Utils

> **Why tenth**: Utility/helper modules — pure functions used across the frontend.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/utils/richtext.ts` | 315 | ⬜ | Rich text rendering helpers |
| `src/utils/markdown.ts` | 272 | ⬜ | Markdown parsing/rendering |
| `src/utils/adapters.ts` | 216 | ⬜ | Data format adapters |
| `src/utils/openInApp.ts` | 151 | ⬜ | URL routing to in-app screens |
| `src/utils/feedAnalytics.ts` | 90 | ⬜ | Feed usage analytics |
| `src/utils/VideoPlaybackManager.ts` | 85 | ⬜ | Video element pooling/lifecycle |
| `src/utils/prefetchBlobUrls.ts` | 85 | ⬜ | Preloading media blob URLs |
| `src/utils/media.ts` | 55 | ⬜ | Media type detection helpers |
| `src/utils/content.ts` | 50 | ⬜ | Content processing helpers |
| `src/utils/sanitize.ts` | 48 | ⬜ | HTML sanitization |
| `src/utils/mediaServer.ts` | 33 | ⬜ | Media server URL helpers |
| `src/utils/testUtils.tsx` | 23 | ⬜ | Test setup utilities |
| `src/utils/date.ts` | 9 | ⬜ | Date formatting |

**Subtotal**: 13 files · 1,432 lines

---

## Batch 11 — Frontend Screens

> **Why eleventh**: Top-level views the user interacts with directly.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/screens/ChatScreen.tsx` | 778 | ⬜ | Direct messaging interface |
| `src/screens/VaultScreen.tsx` | 710 | ⬜ | Personal file vault |
| `src/screens/LoginScreen.tsx` | 707 | ⬜ | Authentication flow |
| `src/screens/FriendsScreen.tsx` | 697 | ⬜ | Friends list & management |
| `src/screens/ProfileBuilderScreen.tsx` | 656 | ⬜ | Drag-and-drop profile page builder |
| `src/screens/ProfileBrowserScreen.tsx` | 647 | ⬜ | Browse other profiles |
| `src/screens/SettingsScreen.tsx` | 609 | ⬜ | App settings & preferences |
| `src/screens/FeedScreen.tsx` | 529 | ⬜ | Main social feed view |
| `src/screens/PeerProfileScreen.tsx` | 460 | ⬜ | Viewing a peer's profile |
| `src/screens/GroupScreen.tsx` | 432 | ⬜ | Group/community view |
| `src/screens/ProfileScreen.tsx` | 298 | ⬜ | User's own profile |
| `src/screens/WebViewScreen.tsx` | 281 | ⬜ | In-app web browser |
| `src/screens/AuditScreen.tsx` | 246 | ⬜ | Data audit/review |
| `src/screens/ForumScreen.tsx` | 232 | ⬜ | Discussion forum view |
| `src/screens/ProfileViewScreen.tsx` | 187 | ⬜ | Profile view (read-only) |
| `src/screens/ImportScreen.tsx` | 135 | ⬜ | Archive import wizard |

**Subtotal**: 16 files · 7,604 lines

---

## Batch 12 — Frontend Components (Large, >250 lines)

> **Why twelfth**: Substantial reusable UI components with complex logic.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/components/EmojiPicker.tsx` | 750 | ⬜ | Emoji selection UI |
| `src/components/ProfileBioSections.tsx` | 700 | ⬜ | Profile bio section components |
| `src/components/ThemeCustomizer.tsx` | 657 | ⬜ | Theme editor with live preview |
| `src/components/ComposeBox.tsx` | 633 | ⬜ | Post composition editor (Tiptap) |
| `src/components/FeedEntryCard.tsx` | 574 | ⬜ | Individual feed post card |
| `src/components/Omnibar.tsx` | 483 | ⬜ | Universal search/command bar |
| `src/components/RawHtmlEditor.tsx` | 439 | ⬜ | HTML editor for profiles |
| `src/components/FriendsPanel.tsx` | 392 | ⬜ | Friends sidebar panel |
| `src/components/FeedRightPanel.tsx` | 329 | ⬜ | Feed detail/context panel |
| `src/components/ProfileRenderer.tsx` | 297 | ⬜ | Profile HTML renderer |
| `src/components/ArticleEditor.tsx` | 280 | ⬜ | Long-form article editor |
| `src/components/VideoPlayer.tsx` | 277 | ⬜ | Click-to-play video player |
| `src/components/Chat/ChatLayout.tsx` | 261 | ⬜ | Chat window layout |

**Subtotal**: 13 files · 6,072 lines

---

## Batch 13 — Frontend Components (Medium, 100–250 lines)

> **Why thirteenth**: Modals, specialized input components, and mid-size UI pieces.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/components/CreateGroupModal.tsx` | 213 | ⬜ | Group creation form |
| `src/components/AudienceDrawer.tsx` | 210 | ⬜ | Post audience selection |
| `src/components/RecoverySetupModal.tsx` | 209 | ⬜ | Account recovery setup |
| `src/components/QuoteRepostModal.tsx` | 209 | ⬜ | Quote-repost composition |
| `src/components/VaultFileModal.tsx` | 206 | ⬜ | Vault file detail modal |
| `src/components/ReplyCompose.tsx` | 176 | ⬜ | Reply composition |
| `src/components/AudiencePills.tsx` | 151 | ⬜ | Audience selection pills UI |
| `src/components/LinkPreviewCard.tsx` | 144 | ⬜ | Link unfurl/preview card |
| `src/components/AutocompleteDropdown.tsx` | 133 | ⬜ | Mention/hashtag autocomplete |
| `src/components/IdentitySetupModal.tsx` | 131 | ⬜ | DID identity setup |
| `src/components/GroupPickerModal.tsx` | 130 | ⬜ | Group selection modal |
| `src/components/ThemePicker.tsx` | 119 | ⬜ | Theme selection grid |
| `src/components/Sidebar.tsx` | 115 | ⬜ | Main navigation sidebar |

**Subtotal**: 13 files · 2,146 lines

---

## Batch 14 — Feed Subcomponents, Lenses, Widgets & Small Components

> **Why last**: These are the smallest, most leaf-level UI pieces.

### Feed Subcomponents
| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/components/feed/VirtualizedFeedList.tsx` | 139 | ⬜ | Virtual scrolling feed list |
| `src/components/feed/EngagementBar.tsx` | 114 | ⬜ | Like/repost/reply action bar |
| `src/components/feed/helpers.ts` | 68 | ⬜ | Feed utility functions |
| `src/components/feed/MediaGrid.tsx` | 67 | ⬜ | Multi-image grid layout |
| `src/components/feed/ThreadReplies.tsx` | 52 | ⬜ | Thread reply tree |
| `src/components/feed/icons.tsx` | 49 | ⬜ | Feed SVG icons |
| `src/components/feed/GalleryOverlay.tsx` | 47 | ⬜ | Image gallery overlay |
| `src/components/feed/FeedItemRenderer.tsx` | 46 | ⬜ | Feed item dispatch renderer |
| `src/components/feed/index.ts` | 6 | ⬜ | Feed barrel exports |

### Lenses
| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/components/lenses/ReaderLens.tsx` | 224 | ⬜ | Article/reader view mode |
| `src/components/lenses/GalleryLens.tsx` | 169 | ⬜ | Gallery/photo view mode |
| `src/components/lenses/Lightbox.tsx` | 159 | ⬜ | Full-screen image viewer |
| `src/components/lenses/FeedItemRenderer.tsx` | 46 | ⬜ | Lens-specific feed item renderer |

### Widgets (Profile Page Builder)
| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/components/widgets/MusicPlayerWidget.tsx` | 299 | ⬜ | Embeddable music player widget |
| `src/components/widgets/GuestbookWidget.tsx` | 195 | ⬜ | Visitor guestbook widget |
| `src/components/widgets/AwayMessageWidget.tsx` | 153 | ⬜ | Away/status message widget |
| `src/components/widgets/FriendsListWidget.tsx` | 104 | ⬜ | Friends list profile widget |
| `src/components/widgets/MediaGridWidget.tsx` | 102 | ⬜ | Media gallery profile widget |
| `src/components/widgets/PostFeedWidget.tsx` | 79 | ⬜ | Recent posts profile widget |
| `src/components/widgets/VisitorCounterWidget.tsx` | 55 | ⬜ | MySpace-style visitor counter |
| `src/components/widgets/index.ts` | 31 | ⬜ | Widget registry & barrel exports |

### Small Components (<100 lines)
| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/components/ContextPanel.tsx` | 103 | ⬜ | Side context panel |
| `src/components/TopBar.tsx` | 99 | ⬜ | Window title bar |
| `src/components/Win98Box.tsx` | 96 | ⬜ | Windows 98 themed container |
| `src/components/BlobImage.tsx` | 94 | ⬜ | Content-addressed image display |
| `src/components/ArticleReader.tsx` | 94 | ⬜ | Article reading view |
| `src/components/ReactionPicker.tsx` | 93 | ⬜ | Reaction emoji picker |
| `src/components/AppLayout.tsx` | 91 | ⬜ | Root layout wrapper |
| `src/components/RichText.tsx` | 78 | ⬜ | Rich text renderer |
| `src/components/RelayStatusBadge.tsx` | 62 | ⬜ | P2P relay status indicator |
| `src/components/TabBar.tsx` | 40 | ⬜ | Browser-style tab bar |
| `src/components/StatusBar.tsx` | 25 | ⬜ | Bottom status bar |
| `src/components/SourceBadge.tsx` | 21 | ⬜ | Platform source indicator |

**Subtotal**: 33 files · 2,698 lines

---

## Completion Log

*Track completed documentation sessions here:*

| Date | Session | Batch(es) | Files Done | Agent |
|------|---------|-----------|------------|-------|
| — | — | — | — | — |

---

## Notes

- **CSS files excluded**: `App.css` (5,193 lines) and other CSS files are not included in this tracker. CSS commenting is a separate, lower-priority task.
- **Test files excluded**: `*.test.*` files are not tracked here; they should be self-documenting via test descriptions.
- **Estimated effort**: ~15–20 sessions to complete all 14 batches, depending on complexity and comment density needed.
- **Commit strategy**: Commit after completing each batch (or half-batch for large batches like Batch 7).
