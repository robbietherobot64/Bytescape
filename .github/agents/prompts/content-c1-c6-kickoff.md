# Content Agent Kickoff: C1 Facet Frontend + C6.1 CRDT Storage

## Priority 1 — C1.1b + C1.2b + C1.3b: Rich Text Facet Frontend

The backend work for facets is **done** — `richtext.rs` has `detect_facets()`, `parse_facets_for_bluesky()`, and `parse_facets_local()` with 23 unit tests. The `search_mentions` and `search_hashtags_autocomplete` Tauri commands are registered. Your job is the **frontend rendering and autocomplete UI**.

### C1.1b — Facet Rendering in Feed

**Goal:** @mentions, #hashtags, and hyperlinks in post content should render as clickable, styled inline elements — not plain text.

**Files to modify:**
- `src/components/FeedEntryCard.tsx` — main feed item card
- `src/components/ArticleReader.tsx` — article content reader *(PM note: ensure this uses the same facet pipeline)*
- `src/types/index.ts` — add facet TypeScript types

**Implementation:**
1. Add facet types to `types/index.ts`:
```typescript
export interface RichTextFacet {
  index: { byteStart: number; byteEnd: number };
  features: FacetFeature[];
}

export type FacetFeature =
  | { $type: 'app.bsky.richtext.facet#mention'; did: string }
  | { $type: 'app.bsky.richtext.facet#tag'; tag: string }
  | { $type: 'app.bsky.richtext.facet#link'; uri: string };
```

2. Create a `RichText` component (or utility function) that takes `text: string` and `facets: RichTextFacet[]` and returns JSX with:
   - @mentions → `<a>` styled as mentions (accent color, clickable → navigate to profile)
   - #hashtags → `<a>` styled as tags (clickable → filter feed by tag)
   - URLs → `<a href>` styled as links (clickable → open in browser)

3. Handle the byte-offset-to-character-index conversion. Bluesky facets use **UTF-8 byte offsets**, not JS string indices. Use `TextEncoder`/`TextDecoder` for conversion:
```typescript
function byteToCharIndex(text: string, byteIndex: number): number {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(text);
  const slice = bytes.slice(0, byteIndex);
  return new TextDecoder().decode(slice).length;
}
```

4. Wire `RichText` into `FeedEntryCard` wherever post content is rendered. Also wire into `ArticleReader` so articles with facets render them interactively.

**Testing:** Write tests for the byte-offset conversion and facet rendering. Emoji and multi-byte characters are the edge cases.

### C1.2b — @Mention Autocomplete

**Goal:** Typing `@` in ComposeBox triggers a dropdown with contact suggestions.

**Files to modify:**
- `src/components/ComposeBox.tsx` — your primary component

**Backend command (already exists):**
```typescript
const suggestions = await invoke<MentionSuggestion[]>('search_mentions', { query: 'ali' });
// Returns: [{ handle: 'alice.bsky.social', did: 'did:plc:...', avatar_url: '...' }]
```

**Implementation:**
1. Detect `@` followed by typing in the compose textarea
2. After 2+ characters, call `search_mentions` with the text after `@`
3. Show a positioned dropdown below/above the cursor with matching contacts
4. On selection, replace `@partial` with `@handle` in the compose text
5. Store the selected mention's DID for facet generation on publish
6. Debounce the search (200ms)

### C1.3b — #Hashtag Autocomplete

**Goal:** Typing `#` triggers a dropdown with hashtag suggestions.

**Backend command (already exists):**
```typescript
const tags = await invoke<string[]>('search_hashtags_autocomplete', { query: 'byte' });
// Returns: ['bytescape', 'bytecode', ...]
```

**Implementation:** Same pattern as C1.2b but triggered by `#`. Simpler — no DID resolution needed, just insert the tag text.

---

## Priority 2 — C6.1: CRDT Storage Layer

**Prerequisite:** C3.2 is done ✅ (Tiptap editor shipped as commit `8610577`).

This is the **Rust backend foundation** for collaborative editing. You'll work in both Rust and TypeScript.

### Backend: `collab.rs`

**New file:** `bytescape/src-tauri/src/collab.rs`

**Dependencies to add to `Cargo.toml`:**
```toml
yrs = "0.21"  # Rust port of Yjs — check crates.io for latest
```

**Implementation:**
1. New redb table:
```rust
const COLLAB_DOCS: TableDefinition<&str, &[u8]> = TableDefinition::new("collab_docs");
// Key: doc_id (UUID string), Value: binary Yjs state
```

2. Core functions:
```rust
pub fn create_collab_doc(doc_id: &str) -> Result<Vec<u8>, String>
pub fn apply_collab_update(doc_id: &str, update: &[u8]) -> Result<Vec<u8>, String>
pub fn get_collab_state(doc_id: &str) -> Result<Vec<u8>, String>
pub fn snapshot_collab_to_item(doc_id: &str, item_id: &str) -> Result<(), String>
```

3. Tauri commands wrapping the above, registered in `lib.rs`.

4. Add `collab_doc_id: Option<String>` field to `BytescapeItem` in `models/mod.rs`. **This is the only schema change** — everything else is additive.

### Frontend: Wire Tiptap Collaboration

**Files to modify:**
- `src/components/ArticleEditor.tsx`
- Add `yjs` and `y-prosemirror` npm packages (or use `@tiptap/extension-collaboration` which is already installed)

**Implementation (local-only first):**
1. When ArticleEditor opens in "collaborative" mode, create a Yjs `Doc` on the frontend
2. Wire it to Tiptap via `@tiptap/extension-collaboration` (already installed as dep)
3. On every Yjs update, send the update to the backend via `apply_collab_update`
4. On editor mount, load state from backend via `get_collab_state`
5. Verify round-trip: edit → Yjs update → Tauri command → `yrs` merge → redb persist → reload editor → content preserved

**DO NOT wire P2P sync yet** — that's C6.3. This is single-user CRDT-backed editing only.

---

## Files You Will Create
- `bytescape/src-tauri/src/collab.rs` (C6.1)
- `src/components/RichText.tsx` or `src/utils/richtext.ts` (C1.1b)
- Test files for facet rendering

## Files You Will Modify
- `src/components/FeedEntryCard.tsx` (C1.1b — facet rendering)
- `src/components/ArticleReader.tsx` (C1.1b — facet rendering)
- `src/components/ComposeBox.tsx` (C1.2b, C1.3b — autocomplete)
- `src/types/index.ts` (facet types, collab_doc_id)
- `src/components/ArticleEditor.tsx` (C6.1 — Yjs wiring)
- `bytescape/src-tauri/src/lib.rs` (C6.1 — register collab commands)
- `bytescape/src-tauri/src/models/mod.rs` (C6.1 — collab_doc_id field)
- `bytescape/src-tauri/Cargo.toml` (C6.1 — yrs dep)
- `ROADMAP.md` (mark completed items)

## Files You Must NOT Touch
- `src/screens/LoginScreen.tsx` (Backend agent — OAuth UI)
- `src/hooks/useTheme.ts` (Frontend agent — Theme Engine)
- `src/components/ThemePicker.tsx` (Frontend agent)
- `bytescape/src-tauri/src/oauth.rs` (Backend agent)
