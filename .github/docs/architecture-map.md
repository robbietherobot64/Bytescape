# Bytescape Architecture Map

> This document explains how Bytescape's codebase is structured and how data flows through the system. Read this first before diving into any individual file.

---

## What IS Bytescape?

Bytescape is a **local-first, peer-to-peer social media vault**. Think of it as:
- A **universal inbox** that pulls in your posts from Bluesky, Mastodon, Facebook, and Instagram into one place
- A **personal archive** where all your social media data lives on YOUR computer (not in the cloud)
- A **peer-to-peer network** where you can share directly with friends without going through any company's servers
- A **profile builder** where you can create a customizable personal page (like old-school MySpace)

---

## The Two Halves: Engine & Lens

```
┌─────────────────────────────────────────────────────────┐
│                    LENS (Frontend)                       │
│              src/ — React + TypeScript                   │
│                                                         │
│  What the user sees and interacts with.                 │
│  Screens, buttons, feeds, chat windows.                 │
│  Runs in a web view (like a browser tab).               │
│                                                         │
│  Cannot access the filesystem, network, or database     │
│  directly — must ask the Engine to do it.               │
└────────────────────┬────────────────────────────────────┘
                     │ Tauri IPC Bridge
                     │ (invoke() calls ↔ #[tauri::command])
                     │ 
                     │ The frontend calls functions in the backend
                     │ by name, like: invoke('get_timeline', { limit: 50 })
                     │ 
                     │ The backend can also push events to the frontend,
                     │ like: "new message arrived" or "import progress: 45%"
┌────────────────────┴────────────────────────────────────┐
│                   ENGINE (Backend)                       │
│          src-tauri/src/ — Rust                           │
│                                                         │
│  The powerhouse. Handles all the heavy lifting:         │
│  - Storing data in the local database (redb)            │
│  - Talking to Bluesky/Mastodon APIs                     │
│  - Parsing Facebook/Instagram archive files             │
│  - Peer-to-peer networking (Iroh)                       │
│  - Encryption and key management                        │
│  - ActivityPub federation                               │
│                                                         │
│  Runs as a native process (not in a browser).           │
│  Has full access to the filesystem and network.         │
└─────────────────────────────────────────────────────────┘
```

---

## Backend Architecture (src-tauri/src/)

### Layer Diagram

```
                    ┌───────────────────────────────────┐
                    │         commands/ (API Layer)       │
                    │                                     │
                    │  The "menu" of operations the       │
                    │  frontend can order from.            │
                    │  21 files, one per feature area:     │
                    │  auth, import, media, groups,        │
                    │  posting, feed_aggregation, etc.     │
                    └──────┬──────────┬──────────┬────────┘
                           │          │          │
              ┌────────────┘          │          └──────────────┐
              ▼                       ▼                         ▼
┌──────────────────┐   ┌──────────────────┐   ┌────────────────────────┐
│   connectors/     │   │   activitypub/    │   │   sync / protocol /    │
│                   │   │                   │   │   chat / collab         │
│  Talk to external │   │  Talk to the      │   │                        │
│  platforms:       │   │  fediverse:       │   │  Talk to other         │
│  - Bluesky API    │   │  - Inbox handling │   │  Bytescape peers:      │
│  - Facebook parse │   │  - Outbox publish │   │  - P2P sync (Iroh)     │
│  - Mastodon API   │   │  - Signatures     │   │  - Direct messages     │
│  - Instagram parse│   │  - WebFinger      │   │  - Live collaboration  │
│                   │   │  - Group actors   │   │  - File sharing        │
└───────┬──────────┘   └──────┬────────────┘   └──────────┬─────────────┘
        │                      │                           │
        └──────────┬───────────┘───────────────────────────┘
                   │
                   ▼
        ┌──────────────────┐         ┌───────────────────────┐
        │   models/         │         │  db/                   │
        │                   │◄───────►│                        │
        │  Data shapes:     │         │  Storage layer:        │
        │  BytescapeItem,   │         │  - Read/write items    │
        │  Contact, Message │         │  - Query by type       │
        │  Group, etc.      │         │  - Cache management    │
        └──────────────────┘         └───────────────────────┘
                   ▲
                   │
        ┌──────────────────┐
        │  auth / oauth /   │
        │  keychain_encrypted│
        │                   │
        │  Identity & Auth: │
        │  - Login flows    │
        │  - Token refresh  │
        │  - Key storage    │
        │  - DID management │
        └──────────────────┘
```

### The Universal Data Container: BytescapeItem

The single most important type in the entire codebase. Every post, photo, article, video — regardless of which platform it came from — gets converted into a `BytescapeItem`:

```
BytescapeItem {
    uri          → Unique identifier (e.g., "at://did:plc:.../app.bsky.feed.post/abc123")
    item_type    → Status | Gallery | Article | Link | Video
    source       → Bluesky | Mastodon | Facebook | Instagram | Local
    content      → The post text
    media        → Array of image/video URLs
    created_at   → When it was posted
    author       → Who wrote it
    ...many more fields
}
```

This "normalize everything" pattern is why Bytescape can show a Bluesky post next to a Facebook memory next to a Mastodon toot in the same feed.

---

## Frontend Architecture (src/)

### Layer Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                        App.tsx                               │
│         (App shell: tab bar, sidebar, screen routing)        │
└──────────────────────────┬──────────────────────────────────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
     ┌─────────────┐ ┌──────────┐ ┌──────────────┐
     │  screens/    │ │ screens/ │ │  screens/     │    ...16 screens total
     │  FeedScreen  │ │ ChatScreen│ │ VaultScreen  │
     └──────┬──────┘ └────┬─────┘ └──────┬───────┘
            │              │              │
            ▼              ▼              ▼
     ┌─────────────────────────────────────────────┐
     │              components/                     │
     │  Reusable UI building blocks:                │
     │  FeedEntryCard, ComposeBox, VideoPlayer,     │
     │  Sidebar, Omnibar, EmojiPicker, etc.         │
     │                                              │
     │  Also includes specialized subdirectories:   │
     │  - feed/ (feed-specific sub-components)      │
     │  - lenses/ (view modes: reader, gallery)     │
     │  - widgets/ (profile page building blocks)   │
     │  - Chat/ (chat-specific layout)              │
     └──────────────────┬──────────────────────────┘
                        │
            ┌───────────┼──────────────┐
            ▼           ▼              ▼
     ┌───────────┐ ┌──────────┐ ┌──────────────┐
     │  hooks/    │ │  store/  │ │  providers/  │
     │            │ │          │ │              │
     │  Reusable  │ │  Global  │ │  Data access │
     │  stateful  │ │  state   │ │  abstraction │
     │  logic:    │ │  (Zustand)│ │  layer       │
     │  useFeed,  │ │          │ │              │
     │  useBlob,  │ │  Tabs,   │ │  Translates  │
     │  useTheme  │ │  auth,   │ │  UI needs →  │
     │            │ │  cache   │ │  invoke()    │
     └───────────┘ └──────────┘ └──────┬───────┘
                                       │
                                       ▼
                              Tauri IPC → Backend
```

### How Data Flows (Feed Example)

```
1. User opens Feed tab
   └→ FeedScreen renders
      └→ calls useUnifiedFeed() hook

2. useUnifiedFeed() hook
   └→ calls dataProvider.getTimeline()
      └→ TauriDataProvider calls invoke('get_timeline')
         └→ [crosses IPC bridge to Rust]

3. Rust get_timeline command
   └→ calls bluesky connector to fetch from API
   └→ calls db to fetch local vault items  
   └→ merges results, sorts by time
   └→ returns as JSON to frontend

4. useUnifiedFeed() receives data
   └→ adapts to FeedEntry[] format
   └→ deduplicates (same post from multiple sources)
   └→ caches in Zustand store
   └→ returns to FeedScreen

5. FeedScreen renders
   └→ VirtualizedFeedList creates virtual scroll container
      └→ only renders FeedEntryCard components for visible posts
         └→ each card shows: author avatar, text, media, engagement bar
```

---

## How the 14 Documentation Batches Map to Architecture

```
DOCUMENTATION ORDER (bottom-up through the dependency tree):

Batch 1:  Types & Models ──────── THE FOUNDATION (what shapes does data take?)
Batch 2:  Database Layer ──────── HOW DATA IS STORED
Batch 3:  Auth & Keys ─────────── HOW USERS PROVE IDENTITY  
Batch 4:  Connectors ──────────── HOW EXTERNAL DATA COMES IN
Batch 5:  P2P & Sync ──────────── HOW PEERS COMMUNICATE
Batch 6:  ActivityPub ─────────── HOW FEDERATION WORKS
Batch 7:  Commands (API) ──────── THE BRIDGE (backend → frontend)
          ─────────── BACKEND COMPLETE, START FRONTEND ───────────
Batch 8:  Frontend Foundation ─── APP SHELL, STATE, PROVIDERS
Batch 9:  Hooks ────────────────── REUSABLE LOGIC
Batch 10: Utils ────────────────── HELPER FUNCTIONS
Batch 11: Screens ──────────────── TOP-LEVEL VIEWS
Batch 12: Components (Large) ──── COMPLEX UI PIECES
Batch 13: Components (Medium) ─── MODALS & MID-SIZE UI
Batch 14: Feed/Lenses/Widgets ─── LEAF-LEVEL UI
```

Each batch only depends on batches above it, so documenting bottom-up means you always have context when you reach the next layer.

---

## Key Concepts Glossary

| Term | What it means |
|------|---------------|
| **BytescapeItem** | The universal container for any post/photo/article from any platform |
| **Vault** | The user's local database — their personal archive of all social media data |
| **Lens** | A viewing mode for content (feed lens, reader lens, gallery lens) |
| **Connector** | A module that talks to an external platform (Bluesky, Mastodon, etc.) |
| **DID** | "Decentralized Identifier" — a unique identity string like `did:plc:abc123` |
| **AT Protocol** | The protocol Bluesky uses. Bytescape speaks it natively. |
| **ActivityPub** | The protocol Mastodon/Lemmy use. Bytescape can federate with them. |
| **Iroh** | The P2P networking library. Handles direct peer connections without servers. |
| **redb** | The embedded database. Like SQLite but Rust-native. All data stored locally. |
| **Tauri** | The framework that lets the Rust backend + React frontend work together as a desktop app |
| **IPC** | "Inter-Process Communication" — how the frontend talks to the backend |
| **invoke()** | The frontend function to call a backend command |
| **CRDT** | "Conflict-free Replicated Data Type" — allows two peers to edit the same doc without conflicts |
| **Facet** | Bluesky's term for rich text annotations (this word is a mention, this is a link, etc.) |
| **Syndication** | Publishing the same post to multiple platforms at once |
| **WebFinger** | A protocol for discovering user profiles across federated servers |
| **Blob** | A binary chunk of data (usually an image or video file) |

---

## File Naming Conventions

| Pattern | Meaning | Example |
|---------|---------|---------|
| `*Screen.tsx` | A full-page view | `FeedScreen.tsx`, `ChatScreen.tsx` |
| `use*.ts` | A React hook (reusable stateful logic) | `useUnifiedFeed.ts`, `useTheme.ts` |
| `*Modal.tsx` | A popup dialog | `CreateGroupModal.tsx` |
| `*Widget.tsx` | A draggable profile page block | `MusicPlayerWidget.tsx` |
| `*Lens.tsx` | A content viewing mode | `ReaderLens.tsx`, `GalleryLens.tsx` |
| `commands/*.rs` | Backend API functions | `commands/auth.rs` |
| `connectors/*.rs` | Platform integrations | `connectors/bluesky.rs` |
| `db/*.rs` | Database operations | `db/vault.rs` |
| `activitypub/*.rs` | Federation protocol | `activitypub/inbox.rs` |
