# Content Agent — Sprint: UF.7 Classification + ComposeBox Polish (2026-03-15)

## 🚨 PM AUDIT FINDINGS — READ THIS FIRST

### ✅ Verified Completions — Excellent Work
- **C1.4 Character Count** ✅ — `useCharacterCount.ts` (121 lines), 28 tests, Bluesky grapheme segmentation correct. *(Commit: `23c7bbe`)*
- **C2.1 Link Card Embeds** ✅ — `unfurl.rs` (166 lines), `LinkPreviewCard.tsx` (144 lines), integrated into ComposeBox. *(Commits: `9123e0b`, `93fde71`)*
- **P3 Content Classification Research** ✅ — Thorough, well-reasoned analysis. PM is acting on your recommendations this sprint.

### ✅ Process — Clean
Your work was solid. No major process issues. The cross-branch commit of C1.4 appearing on track-b (`cf205df`) was likely a merge artifact, not intentional.

### Current State
All branches merged to **`main`** at `0f9f0a6`. All work happens on `main` going forward. No more feature branches.

---

## Priority 1 — UF.7: Content-Type Classification (Cross-Track Implementation)

Your P3 research proposed a clean classification scheme. Now implement it. This is the most impactful content quality improvement we can make.

### 1a. Add `BytescapeType` Variants (Rust)

In `db.rs` (or wherever `BytescapeType` is defined), add `Link` and `Video` variants:

```rust
pub enum BytescapeType {
    Status,
    Gallery,
    Article,
    Link,    // NEW — URL embed with minimal original text
    Video,   // NEW — video as primary content
}
```

Also update `src/types/index.ts` to add `'Link' | 'Video'` to the `BytescapeType` union.

### 1b. Classification Heuristic (Rust)

Create a `classify_item(item: &BytescapeItem) -> BytescapeType` function that applies your proposed heuristic table:

| Type | Signal | Threshold |
|------|--------|-----------|
| `video` | Has video attachment(s) | Any video embed/attachment |
| `gallery` | Image-centric | ≥2 images, text ≤300 chars |
| `article` | Substantial text | >500 chars body (excluding URL + OG metadata) |
| `link` | URL embed + minimal text | Has link_url, ≤100 chars original text |
| `status` | Everything else | Default fallback |

**Check order matters**: video → gallery → article → link → status (most specific first).

### 1c. Wire Into Connectors

- **Bluesky connector** (`connectors/bluesky.rs`): When fetching timeline items, apply classification using embed type (`app.bsky.embed.images`, `app.bsky.embed.video`, `app.bsky.embed.external`). This is Track A territory — **coordinate with the Backend agent** or propose a clean interface. If the Backend agent is busy with UF.16, you may add the classification call at the point where `BytescapeItem` is constructed, but document what you touched.
- **`publish_post_v2`**: When publishing, auto-classify based on content + attachments.

### 1d. Re-classification Command

Create a `reclassify_vault_items` Tauri command that re-scans ALL existing vault items and updates their `item_type`. This handles items imported before classification was improved. Register in `lib.rs`.

### 1e. Tests
- Classification heuristic: test each type boundary (short text→status, 2 images→gallery, long text→article, link+short text→link, video→video)
- Edge cases: empty text, text exactly at boundaries, mixed content (video + long text)

### PM Decision on Your Open Question
> "Should `Link` be a new `BytescapeType` variant, or a metadata flag?"

**Decision: New variant.** Simpler, cleaner, better for filtering. If a linked URL has substantial body content (>500 chars of original text, not OG description), classify it as `article`, not `link`. The heuristic handles the ambiguity.

---

## Priority 2 — C2.2: GIF Picker

### Backend (Rust)

Create `src-tauri/src/commands/gif.rs`:

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct GifResult {
    pub id: String,
    pub preview_url: String,    // Low-res preview
    pub full_url: String,       // Full-res GIF/MP4
    pub width: u32,
    pub height: u32,
    pub title: Option<String>,
}

#[tauri::command]
pub async fn search_gifs(query: String, limit: Option<u32>) -> Result<Vec<GifResult>, String>

#[tauri::command]
pub async fn get_trending_gifs(limit: Option<u32>) -> Result<Vec<GifResult>, String>
```

Use the **Tenor API v2** (free tier, 50 req/min). The API key can be stored in app config (hardcode a dev key for now — PM will provide a production key later).

Register both commands in `lib.rs`.

### Frontend (React)

1. **GIF button**: Add a GIF icon button to the ComposeBox toolbar (next to media attach)
2. **Picker modal**: Clicking opens a searchable grid modal:
   - Search bar at top
   - Trending GIFs shown by default (via `get_trending_gifs`)
   - Results update on search (debounced 300ms, via `search_gifs`)
   - Grid layout, 2-3 columns, lazy-loaded previews
3. **Selection**: Click a GIF → inserts it as a media attachment on the post
4. **For Bluesky**: Upload the GIF as a blob and attach as `app.bsky.embed.images` (Bluesky renders GIFs as static images, but we send the full URL)
5. **For local vault**: Store as media attachment with `image/gif` MIME type

### Tests
- GIF search returns results
- Picker renders grid
- Selection adds attachment to compose state
- Handles API errors gracefully

---

## Priority 3 — C2.3: Emoji Picker

### Frontend (React)

Use an existing emoji picker library (e.g., `emoji-mart` or `@emoji-mart/react`) to avoid reimplementing the Unicode emoji database.

1. **Emoji button**: Add to ComposeBox toolbar (smiley face icon)
2. **Picker popover**: Clicking opens the emoji picker:
   - Full Unicode emoji grid with categories
   - Search by name/keyword
   - Skin tone selector
   - Recently-used section (persist in localStorage)
3. **Selection**: Click an emoji → insert Unicode character at cursor position in compose text
4. **Integration**: Must work alongside the existing `@mention` and `#hashtag` autocomplete without conflicts

### Tests
- Emoji insertion at cursor position
- Search functionality
- Skin tone selection persists
- No interference with mention/hashtag autocomplete

---

## Anti-Patterns
- Do NOT modify `ProfileBuilderScreen`, `ProfileBrowserScreen`, or lens components — Track B owns those
- Do NOT modify Rust connector files (`connectors/*.rs`) beyond adding classification calls — Track A owns those. Coordinate with Backend agent.
- Do NOT add items to ROADMAP.md — PM scope only
- Do NOT start unassigned items after completing these priorities — report back via progress.md
- All work on `main` — no feature branches
- **Always run `git branch --show-current` before committing**
- Do NOT hallucinate npm packages — check `package.json` before importing

---

## Definition of Done (This Sprint)
- [ ] `BytescapeType::Link` and `BytescapeType::Video` variants added (Rust + TypeScript)
- [ ] `classify_item` heuristic implemented with all 5 type paths
- [ ] `reclassify_vault_items` command registered and working
- [ ] GIF picker: search + trending + selection + attachment
- [ ] Emoji picker: full grid + search + insertion
- [ ] Classification tests cover each type boundary + edge cases
- [ ] `cargo check` passes (0 errors)
- [ ] `pnpm test` passes
- [ ] Committed to `main` with semantic messages
