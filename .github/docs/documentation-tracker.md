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
| 1 | Core Types & Models | 6 | 1,911 | ✅ Complete |
| 2 | Database Layer | 6 | 2,312 | ✅ Complete |
| 3 | Backend Core (Auth, Keys, App State) | 6 | 3,731 | ✅ Complete |
| 4 | Platform Connectors | 4 | 6,662 | ✅ Complete |
| 5 | P2P, Sync & Real-time | 8 | 3,393 | ✅ Complete |
| 6 | ActivityPub Federation | 9 | 2,866 | ✅ Complete |
| 7 | Tauri Commands (Backend API) | 21 | 10,220 | ✅ Complete |
| 8 | Frontend Foundation | 8 | 2,581 | ✅ Complete |
| 9 | Frontend Hooks | 7 | 2,349 | ✅ Complete |
| 10 | Frontend Utils | 13 | 1,432 | ✅ Complete |
| 11 | Frontend Screens | 16 | 7,604 | ✅ Complete |
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
| `src/types/index.ts` | 658 | ✅ | File header, 16 section dividers, 50+ interface docs, field comments |
| `src-tauri/src/models/mod.rs` | 659 | ✅ | Module doc, 10 section dividers, all struct/enum/fn docs |
| `src-tauri/src/models/groups.rs` | 133 | ✅ | Expanded module doc with related files |
| `src-tauri/src/connectors/mod.rs` | 77 | ✅ | Full module doc, enum variant docs, method docs |
| `src-tauri/src/error.rs` | 222 | ✅ | Expanded module doc, section explanations |
| `src-tauri/src/activitypub/types.rs` | 164 | ✅ | Full module doc, 7 struct docs |

**Subtotal**: 6 files · 1,913 lines

---

## Batch 2 — Database Layer

> **Why second**: The database is where all data lives. Understanding the storage layer explains how data flows in and out.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/db/mod.rs` | 437 | ✅ | File header explaining redb, table constants documented, DbManager docs, section dividers |
| `src-tauri/src/db/groups.rs` | 940 | ✅ | Module header with hierarchy diagram, composite key patterns, 25+ method docs |
| `src-tauri/src/db/vault.rs` | 615 | ✅ | Module header (dedup, cache, parity), section dividers, 20+ method docs |
| `src-tauri/src/db/chat.rs` | 193 | ✅ | Module header (Messages vs Vault Messages, multi-DID), section dividers |
| `src-tauri/src/db/peers.rs` | 82 | ✅ | Module header with P2P peer tracking explanation |
| `src-tauri/src/db/contacts.rs` | 45 | ✅ | Module header, all 3 methods documented |

**Subtotal**: 6 files · 2,312 lines

---

## Batch 3 — Backend Core (Auth, Keys, App State)

> **Why third**: These files set up the application — how it boots, how it authenticates users, how it stores secrets.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/lib.rs` | 1,028 | ✅ | Enhanced 37-line header, 7 section dividers, AppState field docs, run() init order |
| `src-tauri/src/auth.rs` | 74 | ✅ | 30-line module header (dual auth modes), Session field docs, method docs |
| `src-tauri/src/keychain_encrypted.rs` | 1,481 | ✅ | 52-line header (5 DID types, recovery escrow, security), 7 section dividers, AuthData/recovery type docs |
| `src-tauri/src/oauth.rs` | 1,031 | ✅ | 48-line header (full OAuth flow, DPoP, session persistence), 7 section dividers |
| `src-tauri/src/contacts.rs` | 113 | ✅ | Module header (P2P discovery protocol), section dividers, all method docs |
| `src-tauri/src/main.rs` | 6 | ✅ | Module header explaining binary/library separation |

**Subtotal**: 6 files · 3,733 lines

---

## Batch 4 — Platform Connectors

> **Why fourth**: These translate external platform data (Bluesky, Facebook, Mastodon, Instagram) into Bytescape's universal format.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/connectors/bluesky.rs` | 2,214 | ✅ | 43-line module header (auth modes, media pipeline, related files), 13 section dividers, 18+ type docs |
| `src-tauri/src/connectors/facebook.rs` | 2,766 | ✅ | 33-line module header (dual format, encoding quirk, related files), 8 section dividers, `parse_facebook_zip` doc |
| `src-tauri/src/connectors/mastodon.rs` | 1,420 | ✅ | Enhanced 35-line module header (How It Works, Bluesky-compat, rate limiting), 8 section dividers |
| `src-tauri/src/connectors/instagram.rs` | 570 | ✅ | 32-line module header (shared types, related files), 5 section dividers, 12+ function/type docs |

**Subtotal**: 4 files · 6,970 lines (+308 lines of documentation)

---

## Batch 5 — P2P, Sync & Real-time

> **Why fifth**: These handle peer-to-peer networking, data synchronization, and real-time features. Complex but central to Bytescape's local-first architecture.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/sync.rs` | 668 | ✅ | 32-line module header (4 subsystems), 12 section dividers, 4 type docs, enhanced function docs |
| `src-tauri/src/protocol.rs` | 702 | ✅ | Module header expanded 4→40 lines (CRIT-4 security docs), 8 section dividers |
| `src-tauri/src/chat.rs` | 419 | ✅ | 30-line module header (4-step flow), 9 section dividers, 4 function docs |
| `src-tauri/src/collab.rs` | 305 | ✅ | Related Files added to header, 5 section dividers |
| `src-tauri/src/blob_hash.rs` | 265 | ✅ | Related Files section added to existing header, 2 section dividers |
| `src-tauri/src/richtext.rs` | 650 | ✅ | Header expanded 6→29 lines (pipeline, UTF-8 byte offsets caveat), 1 section divider |
| `src-tauri/src/media_server.rs` | 77 | ✅ | Full 31-line module header (cache strategy, CRIT-5 security, related files) |
| `src-tauri/src/traffic.rs` | 107 | ✅ | 25-line module header (scaffolding status noted), 5 section dividers, 7 doc comments |

**Subtotal**: 8 files · 3,193 lines

---

## Batch 6 — ActivityPub Federation

> **Why sixth**: ActivityPub is the federation protocol for connecting to Mastodon, Lemmy, and other fediverse servers.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/activitypub/mod.rs` | 14 | ✅ | Expanded module header with 6-step HOW IT WORKS, RELATED FILES |
| `src-tauri/src/activitypub/inbox.rs` | 849 | ✅ | 34-line module header, 6 section dividers, enhanced function docs |
| `src-tauri/src/activitypub/outbox.rs` | 146 | ✅ | 23-line module header, enhanced function docs |
| `src-tauri/src/activitypub/actor.rs` | 170 | ✅ | 39-line header (security note), 5 section dividers, all method docs |
| `src-tauri/src/activitypub/activities.rs` | 211 | ✅ | Expanded header, 4 section dividers, 12 function docs |
| `src-tauri/src/activitypub/signatures.rs` | 404 | ✅ | Expanded header (SHA-256 note), 7 section dividers, 11 function docs |
| `src-tauri/src/activitypub/webfinger.rs` | 117 | ✅ | 22-line header, struct docs |
| `src-tauri/src/activitypub/group_actor.rs` | 425 | ✅ | Expanded header (HOW IT WORKS, RELATED FILES), 5 section dividers |
| `src-tauri/src/activitypub/lemmy.rs` | 531 | ✅ | Expanded header, 4 section dividers (═══ style) |

**Subtotal**: 9 files · 2,867 lines

---

## Batch 7 — Tauri Commands (Backend API Surface)

> **Why seventh**: These are the "verbs" of the backend — every function the frontend can call. Documenting these creates a complete API reference.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src-tauri/src/commands/mod.rs` | 44 | ✅ | Expanded header with 20-module submodule map, 4 section dividers by domain |
| `src-tauri/src/commands/auth.rs` | 541 | ✅ | 40-line header (3-tier auth, HOW IT WORKS), 7 section dividers, 15 function docs |
| `src-tauri/src/commands/import.rs` | 2,910 | ✅ | 49-line header (God file + tech debt), 10 section dividers, 19 command docs, 12 helper docs |
| `src-tauri/src/commands/media.rs` | 1,402 | ✅ | 36-line header (5-step HOW IT WORKS), 8 section dividers, 10 IPC-annotated command docs |
| `src-tauri/src/commands/groups.rs` | 1,190 | ✅ | Expanded header, 8 section dividers, all public functions documented |
| `src-tauri/src/commands/mastodon.rs` | 1,089 | ✅ | Expanded header (OAuth, DM paths), 8 section dividers, 35 function docs, C2S/S2S annotations |
| `src-tauri/src/commands/peers.rs` | 686 | ✅ | 41-line header (tech debt), 5 section dividers, all commands with IPC annotations |
| `src-tauri/src/commands/vault.rs` | 660 | ✅ | 39-line header (tech debt), 8 section dividers, all commands with IPC annotations |
| `src-tauri/src/commands/posting.rs` | 520 | ✅ | 39-line header (multi-dest routing, tech debt), 3 section dividers, all function docs |
| `src-tauri/src/commands/browser.rs` | 505 | ✅ | RELATED FILES added, 4 section dividers, GTK cross-system annotations |
| `src-tauri/src/commands/feed_aggregation.rs` | 493 | ✅ | 36-line header (3-source merge, tech debt), 3 section dividers, struct + command docs |
| `src-tauri/src/commands/profile.rs` | 251 | ✅ | Expanded header (5-step HOW IT WORKS), 5 section dividers, 10 function docs |
| `src-tauri/src/commands/interactions.rs` | 335 | ✅ | Full header (Bluesky-only noted), ═══ dividers (converted from ───), 6 function docs |
| `src-tauri/src/commands/syndicate.rs` | 258 | ✅ | Full header (Bluesky-only, 4-image limit), 2 section dividers, all function docs |
| `src-tauri/src/commands/node.rs` | 255 | ✅ | Expanded header (HOW IT WORKS), 3 section dividers, IPC annotations |
| `src-tauri/src/commands/export.rs` | 184 | ✅ | Full header (memory concern noted), struct + function docs |
| `src-tauri/src/commands/unfurl.rs` | 167 | ✅ | Full header (SSRF security note), LinkCard struct doc, SSRF inline warning |
| `src-tauri/src/commands/relay.rs` | 153 | ✅ | RELATED FILES added, ═══ section dividers, stub function docs |
| `src-tauri/src/commands/ap.rs` | 151 | ✅ | 34-line header (tech debt: double-deser), 5 function docs with IPC annotations |
| `src-tauri/src/commands/collab.rs` | 97 | ✅ | Expanded header (HOW IT WORKS, RELATED FILES), 6 function docs preserved |
| `src-tauri/src/commands/classify.rs` | 75 | ✅ | 27-line header (tech debt: O(n) scan), struct + function docs |

**Subtotal**: 21 files · ~10,220 lines

---

## Batch 8 — Frontend Foundation

> **Why eighth**: The app shell, global state, and data provider layer. Every screen and component depends on these.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/main.tsx` | 12 | ✅ | File header with boot sequence, DataProviderRoot annotation |
| `src/App.tsx` | 770 | ✅ | Full header (5-step HOW IT WORKS), 11 section dividers, 12 function docs, tech debt notes |
| `src/store/index.ts` | 476 | ✅ | 38-line header, 8 ═══ section dividers, all stores/interfaces documented, tech debt notes |
| `src/providers/DataProvider.ts` | 591 | ✅ | Expanded header, 16 section dividers in interface, DataProvider abstraction doc, tech debt notes |
| `src/providers/TauriDataProvider.ts` | 642 | ✅ | Expanded header (IPC flow), class doc with snake_case mapping, 16 section dividers |
| `src/providers/DataProviderContext.tsx` | 50 | ✅ | RELATED FILES section added to existing header |
| `src/providers/index.ts` | 19 | ✅ | RELATED FILES section added to existing header |
| `src/contexts/TabActiveContext.tsx` | 21 | ✅ | File header with HOW IT WORKS, RELATED FILES (VideoPlayer, App.tsx) |

**Subtotal**: 8 files · 2,581 lines

---

## Batch 9 — Frontend Hooks

> **Why ninth**: Custom React hooks encapsulate reusable stateful logic (feed fetching, location, blob URLs). Core to understanding data flow.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/hooks/useUnifiedFeed.ts` | 641 | ✅ | Expanded module header, ═══ sections, exported hook/type docs, provider boundary notes |
| `src/hooks/useGroupsData.ts` | 450 | ✅ | Full header, ═══ sections, hook/function docs, mock-vs-backend caveat notes |
| `src/hooks/useLocation.ts` | 445 | ✅ | Header with URL scheme flow, route/type docs, NavStore/ChatStore sync notes |
| `src/hooks/useBlobUrl.ts` | 294 | ✅ | Header + section docs, exported cache/hook docs, media IPC and cache caveat notes |
| `src/hooks/useCollabDoc.ts` | 216 | ✅ | Header + lifecycle docs, Yjs/Tauri sync annotations, queue durability caveat |
| `src/hooks/useTheme.ts` | 159 | ✅ | Header + persistence flow docs, localStorage side-effect annotations, singleton caveat |
| `src/hooks/useCharacterCount.ts` | 144 | ✅ | Header, API/type docs, grapheme-counting rationale and Segmenter caveat |

**Subtotal**: 7 files · 2,349 lines

---

## Batch 10 — Frontend Utils

> **Why tenth**: Utility/helper modules — pure functions used across the frontend.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/utils/richtext.ts` | 315 | ✅ | Expanded header, section dividers, facet/autocomplete API docs, caveat notes |
| `src/utils/markdown.ts` | 272 | ✅ | Expanded header, parser/serializer section docs, exported helper docs |
| `src/utils/adapters.ts` | 216 | ✅ | Header + adaptation flow docs, dedup and source-inference caveat annotations |
| `src/utils/openInApp.ts` | 151 | ✅ | Header + routing flow docs, store/webview boundary notes, runtime-flag caveat |
| `src/utils/feedAnalytics.ts` | 90 | ✅ | Header + scoring model docs, exported metric helper docs |
| `src/utils/VideoPlaybackManager.ts` | 85 | ✅ | Header + singleton lifecycle docs, React ownership caveat notes |
| `src/utils/prefetchBlobUrls.ts` | 85 | ✅ | Header + batch-resolution docs, IPC boundary notes and fallback behavior |
| `src/utils/media.ts` | 55 | ✅ | Header + resolver docs, duplication caveat annotations |
| `src/utils/content.ts` | 50 | ✅ | Header + content-kind/sanitization helper docs |
| `src/utils/sanitize.ts` | 48 | ✅ | Header + policy/limits docs, fixed allowlist caveat note |
| `src/utils/mediaServer.ts` | 33 | ✅ | Header + media-port cache docs, cache invalidation caveat note |
| `src/utils/testUtils.tsx` | 23 | ✅ | Header + test render-wrapper docs |
| `src/utils/date.ts` | 9 | ✅ | Header + timestamp formatting contract doc |

**Subtotal**: 13 files · 1,432 lines

---

## Batch 11 — Frontend Screens

> **Why eleventh**: Top-level views the user interacts with directly.

| File | Lines | Status | Notes |
|------|-------|--------|-------|
| `src/screens/ChatScreen.tsx` | 778 | ✅ | Expanded header, section dividers, chat flow docs, transport/state caveat notes |
| `src/screens/VaultScreen.tsx` | 710 | ✅ | Full header, section dividers, vault lifecycle docs, cache/memory caveats |
| `src/screens/LoginScreen.tsx` | 707 | ✅ | Full header, auth-flow sections, OAuth cross-system docs |
| `src/screens/FriendsScreen.tsx` | 697 | ✅ | Full header, section dividers, peer status/load docs, sequential-check caveat |
| `src/screens/ProfileBuilderScreen.tsx` | 656 | ✅ | Expanded header, builder lifecycle docs, block/persistence cross-system notes |
| `src/screens/ProfileBrowserScreen.tsx` | 647 | ✅ | Expanded header, navigation/history/docs, provider integration annotations |
| `src/screens/SettingsScreen.tsx` | 609 | ✅ | Expanded header, settings domains documented, OAuth/listener boundary notes |
| `src/screens/FeedScreen.tsx` | 529 | ✅ | Full header, feed orchestration sections, event/provider coupling annotations |
| `src/screens/PeerProfileScreen.tsx` | 460 | ✅ | Header + section docs, peer fetch/pagination notes |
| `src/screens/GroupScreen.tsx` | 432 | ✅ | Header + tab/command docs, moderation/member flow annotations |
| `src/screens/ProfileScreen.tsx` | 298 | ✅ | Header + mode-render docs (custom profile vs wall) |
| `src/screens/WebViewScreen.tsx` | 281 | ✅ | Header + lifecycle docs, browser command boundary annotations |
| `src/screens/AuditScreen.tsx` | 246 | ✅ | Header + audit pipeline docs, log/memory visualization notes |
| `src/screens/ForumScreen.tsx` | 232 | ✅ | Header + topic tree/group browse docs, recursion notes |
| `src/screens/ProfileViewScreen.tsx` | 187 | ✅ | Header + profile fetch/render docs, sanitization boundary notes |
| `src/screens/ImportScreen.tsx` | 135 | ✅ | Header + drag/drop/import flow docs |

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
| 2026-03-28 | S6.13 | Batch 1 | 6 files (1,913 lines) | Systems |
| — | — | — | — | — |

---

## Notes

- **CSS files excluded**: `App.css` (5,193 lines) and other CSS files are not included in this tracker. CSS commenting is a separate, lower-priority task.
- **Test files excluded**: `*.test.*` files are not tracked here; they should be self-documenting via test descriptions.
- **Estimated effort**: ~15–20 sessions to complete all 14 batches, depending on complexity and comment density needed.
- **Commit strategy**: Commit after completing each batch (or half-batch for large batches like Batch 7).
