# Bytescape Code Documentation Standards

> **Audience**: A layperson (non-developer) should be able to open any source file and understand what it does, why it exists, and how it fits into Bytescape.

---

## 1. Every File Gets a Header Block

### TypeScript / TSX
```typescript
/**
 * FeedScreen.tsx — Main Social Feed View
 * 
 * This is the primary screen users see after logging in. It displays a scrollable
 * feed of posts from all connected platforms (Bluesky, Mastodon, imported archives)
 * merged into a single timeline.
 * 
 * HOW IT WORKS:
 * 1. When the screen loads, it calls the `useUnifiedFeed` hook to fetch posts
 *    from all connected sources (Bluesky API, local vault, Mastodon API)
 * 2. Posts are deduplicated, sorted by time, and rendered in a virtualized list
 *    (only posts visible on screen are actually rendered — saves memory)
 * 3. The user can filter by content type (text, photos, videos, articles)
 *    using filter chips at the top
 * 4. Clicking a post opens it in the right panel for detailed view
 * 
 * RELATED FILES:
 * - useUnifiedFeed.ts — The hook that fetches and merges feed data
 * - FeedEntryCard.tsx — The component that renders each individual post
 * - VirtualizedFeedList.tsx — The virtual scrolling container
 * - DataProvider.ts — The abstract interface for fetching data from backends
 */
```

### Rust
```rust
//! # commands/import.rs — Archive Import Pipeline
//!
//! This module handles importing social media data archives (Facebook, Instagram)
//! into Bytescape's local database. When a user downloads their data from Facebook
//! or Instagram and drops the ZIP file into Bytescape, this code:
//!
//! 1. Extracts the archive to a temporary directory
//! 2. Detects whether it's a Facebook or Instagram export
//! 3. Parses the platform-specific format (HTML for Facebook, JSON for Instagram)
//! 4. Converts each post, photo, and message into Bytescape's universal format
//!    (`BytescapeItem`)
//! 5. Stores everything in the local redb database
//!
//! ## Related Files
//! - `connectors/facebook.rs` — Facebook HTML parsing logic
//! - `connectors/instagram.rs` — Instagram JSON parsing logic  
//! - `models/mod.rs` — The `BytescapeItem` type that all data is normalized into
//! - `db/mod.rs` — Database storage layer
```

---

## 2. Every Public Function/Component Gets a Doc Comment

### What to include:
- **What** it does (one sentence)
- **Why** it exists (what problem does it solve?)
- **Parameters** — what each one means in plain English
- **Returns** — what the caller gets back
- **Side effects** — does it modify state, make network calls, write to disk?

### TypeScript example:
```typescript
/**
 * Fetches the user's Bluesky timeline and converts it to Bytescape's universal format.
 * 
 * This is called every time the feed screen loads or the user pulls to refresh.
 * It talks to Bluesky's API servers, downloads the latest posts, and translates
 * them from Bluesky's "AT Protocol" format into our `FeedEntry` format that the
 * UI can render regardless of which platform the post came from.
 * 
 * @param cursor - A pagination token from Bluesky (like a bookmark saying 
 *                 "give me posts after this point"). Pass null for the first page.
 * @param limit - How many posts to fetch (default 50, max 100)
 * @returns An object with the posts array and a new cursor for the next page
 * @throws If the user's session has expired (they need to re-login)
 */
async function fetchTimeline(cursor: string | null, limit: number): Promise<TimelineResponse>
```

### Rust example:
```rust
/// Saves a BytescapeItem to the local database.
///
/// This is the main "write" operation for the vault. Every post — whether it came
/// from Bluesky, was imported from a Facebook archive, or was composed locally —
/// ends up here. The item is serialized to JSON and stored in redb with its URI
/// as the key.
///
/// # Arguments
/// * `item` - The normalized item to store (any type: Status, Gallery, Article, etc.)
///
/// # Side Effects
/// * Writes to the `bytescape_items` table in redb
/// * Invalidates the in-memory header cache (forces a re-scan on next feed load)
///
/// # Errors
/// Returns an error string if the database write fails (e.g., disk full)
pub fn save_item(&self, item: &BytescapeItem) -> Result<(), String>
```

---

## 3. Complex Logic Gets Inline "Why" Comments

Don't comment *what* the code does (that's visible from the code). Comment *why*.

### Bad ❌
```typescript
// Check if the post is a reply
if (post.reply_parent_uri) {
```

### Good ✅
```typescript
// Replies to the post author's own posts should be nested under the parent
// rather than shown as standalone posts, since Bluesky treats self-replies
// as a "thread" concept — the user posted a sequence of connected thoughts.
if (post.reply_parent_uri) {
```

---

## 4. Data Types Get Field-Level Explanations

Every struct/interface field that isn't self-evident gets a comment:

```typescript
export interface FeedEntry {
    /** Unique ID for this entry (content hash or platform-assigned CID) */
    id: string;
    
    /** The post text content, may contain rich text facets (mentions, links) */
    content: string;
    
    /** 
     * The original URL on the source platform (e.g., AT Protocol URI for Bluesky).
     * Used for deduplication — if we see the same post from multiple sources,
     * this URL tells us it's the same post.
     */
    original_url?: string;
    
    /** 
     * Which platform this post came from. Used to show the source badge
     * (Bluesky butterfly, Mastodon elephant, etc.) and to know which API 
     * to call for interactions (like, repost, etc.)
     */
    source: 'bluesky' | 'mastodon' | 'vault' | 'facebook' | 'instagram';
}
```

---

## 5. Section Dividers for Long Files

Files over 200 lines should use section dividers to create a "table of contents" feel:

### TypeScript
```typescript
// ═══════════════════════════════════════════════════════════════════
// SECTION 1: STATE & REFS
// Everything the component needs to track between renders
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// SECTION 2: DATA FETCHING
// How the component loads its data from the backend
// ═══════════════════════════════════════════════════════════════════
```

### Rust
```rust
// ═══════════════════════════════════════════════════════════════════
// SECTION 1: TABLE DEFINITIONS
// These constants define the database tables — each one is like a
// spreadsheet with a specific schema for a type of data
// ═══════════════════════════════════════════════════════════════════
```

---

## 6. Architectural Connection Comments

When a function calls another system (backend → frontend, or vice versa), explain the connection:

```typescript
// This calls the Rust backend via Tauri's IPC bridge.
// The 'get_timeline' string must match exactly with the #[tauri::command]
// function name registered in src-tauri/src/lib.rs
const posts = await invoke<BlueskyPost[]>('get_timeline', { limit: 50 });
```

```rust
// This function is called from the React frontend via Tauri IPC.
// The frontend invokes it as: invoke('get_timeline', { limit: 50 })
// See: src/hooks/useUnifiedFeed.ts
#[tauri::command]
async fn get_timeline(state: State<'_, AppState>, limit: i32) -> Result<Vec<BlueskyPost>, String> {
```

---

## 7. What NOT to Do

- ❌ Don't add comments that just repeat the code: `// increment counter` above `counter += 1`
- ❌ Don't add TODO comments — implement it or file an issue
- ❌ Don't reference line numbers in comments (they change as code evolves)
- ❌ Don't write novel-length comments — aim for clarity, not exhaustiveness
- ❌ Don't break existing functionality — comments are additive only
- ❌ Don't remove existing useful comments

---

## 8. Quality Checklist (Per File)

Before marking a file as ✅ documented:

- [ ] File has a header block explaining its purpose in plain English
- [ ] The "HOW IT WORKS" section gives a 3-5 step overview of the file's logic
- [ ] The "RELATED FILES" section links to connected modules
- [ ] Every exported function/component/type has a doc comment
- [ ] Complex algorithms have inline "why" comments
- [ ] Long files (200+ lines) have section dividers
- [ ] Cross-system calls (Tauri invoke, P2P messages) are annotated
- [ ] No existing code was modified (comments only)
- [ ] File still compiles / type-checks after comments are added
