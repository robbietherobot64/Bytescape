# Content Agent — Sprint: USER FEEDBACK — Content Classification (2026-03-15)

## 🚨 CRITICAL CONTEXT — THE USER IS UNHAPPY

The user gave 16 feedback items. After a full sprint, only 3 were completed. The user says "literally none of my feedback has been listened to." **This sprint is 100% user feedback. No GIF pickers. No emoji pickers. Only fixing what the user asked for.**

Your C1.4 (char count) and C2.1 (link cards) work was excellent and verified. But those were roadmap items — not user feedback. This sprint, everything you do must directly serve a UF item.

All work is on **`main`**. Verify with `git branch --show-current` before every commit.

---

## Priority 1 — UF.7: Content-Type Classification ⚡⚡⚡

**User:** *"Article reader is totally busted. It's mostly just YouTube videos, GIFs, and links. We want only blog posts, news articles, or largely text based long form posts. No videos, no gifs."*

**AND UF.6:** *"Gallery view sucks. Let's think about feed filters rather than views. For images, filter image posts. Same for videos, articles, short form text."*

The Frontend agent is building filter chips (UF.6) that depend on content being classified. Without your classification, those filters have nothing to filter on. **This is the critical dependency.**

### 1a. Add `BytescapeType` Variants

In `db.rs` (or wherever `BytescapeType` is defined), add `Link` and `Video`:

```rust
pub enum BytescapeType {
    Status,   // Short text, minimal media
    Gallery,  // Image-centric (≥2 images, text ≤300 chars)
    Article,  // Substantial text (>500 chars body)
    Link,     // URL embed with minimal original text (≤100 chars)
    Video,    // Video as primary content
}
```

Also update `src/types/index.ts` — add `'Link' | 'Video'` to the TypeScript `BytescapeType` union.

### 1b. Classification Heuristic

Create `classify_item(item: &BytescapeItem) -> BytescapeType`:

| Priority | Type | Signal |
|----------|------|--------|
| 1 | `Video` | Has video attachment(s) |
| 2 | `Gallery` | ≥2 images, text ≤300 chars |
| 3 | `Article` | >500 chars body (excluding URL + OG metadata) |
| 4 | `Link` | Has link_url, ≤100 chars original text |
| 5 | `Status` | Everything else (default) |

Check in priority order — most specific first.

### 1c. Wire Into Ingest

- When Bluesky timeline items are fetched, classify them using embed types (`app.bsky.embed.images`, `app.bsky.embed.video`, `app.bsky.embed.external`)
- When `publish_post_v2` creates a new item, auto-classify based on content + attachments
- When Facebook/Instagram archives are imported, classify each item

### 1d. Re-classification Command

```rust
#[tauri::command]
pub async fn reclassify_vault_items(state: State<'_, AppState>) -> Result<ReclassifyResult, String>
```

Re-scans ALL existing vault items and updates their `item_type`. Returns count of items re-classified per type. Register in `lib.rs`.

### 1e. Tests
- Each type boundary: short text→status, 2 images→gallery, long text→article, link+short text→link, video→video
- Edge cases: empty text, exactly at boundaries, mixed content (video + long text → video wins)
- Re-classification: verify items change type after heuristic update

---

## Priority 2 — UF.12 Support: Tracker Classification Data

**User:** *"Be more specific and clear about what trackers did find. Maybe classify and define them, so we aren't just left to the imagination on what they are tracking."*

The Frontend agent is fixing the AuditScreen to show tracker categories. But the Rust-side tracker data may only return name + event count. Check:

1. **Read `facebook.rs` tracker parsing** — what data does the audit produce? Just tracker names and counts, or categories too?
2. **If categories are missing**: Add a classification heuristic for tracker names:
   - `ad_tracking`: Trackers related to ads, advertising, ad preferences
   - `location_tracking`: Location, places, check-ins
   - `cross_site_tracking`: Off-Facebook activity, apps & websites, cookies
   - `data_collection`: Device info, session history, login data
   - `social_tracking`: Friends, contacts, interactions tracking
3. Return `TrackerInfo { name, event_count, category }` instead of just name + count
4. The Frontend will use `category` to group and label trackers in the UI

---

## Priority 3 — UF.6 Support: Feed Query by Content Type

**User wants filter chips on the feed.** The Frontend will add the UI, but the Backend/Content needs to support querying by type.

Check if `get_timeline` / `get_unified_feed` accepts a `content_type` filter parameter. If not:

1. Add an optional `item_type_filter: Option<String>` parameter to feed commands
2. When set, only return items matching that type (e.g., `item_type_filter: Some("Article")` → only articles)
3. The Frontend will call: `invoke('get_timeline', { limit: 50, itemTypeFilter: 'Article' })`

This is simple query-side filtering — it must be fast (no re-scanning).

---

## Anti-Patterns
- Do NOT start C2.2 GIF Picker or C2.3 Emoji Picker — those are roadmap items, not user feedback
- Do NOT modify `ProfileBuilderScreen`, `ProfileBrowserScreen`, or screen components — Track B owns those
- Do NOT add items to ROADMAP.md — PM scope only
- All work on `main` — no feature branches
- **Always run `git branch --show-current` before committing**
- Do NOT hallucinate crate imports — check Cargo.toml before using

## Definition of Done
- [ ] `BytescapeType::Link` and `BytescapeType::Video` variants added (Rust + TypeScript)
- [ ] `classify_item` heuristic works for all 5 types
- [ ] Bluesky ingest + publish auto-classify items
- [ ] `reclassify_vault_items` command registered and working
- [ ] Tracker data includes category classification
- [ ] Feed commands accept optional `item_type_filter`
- [ ] Tests cover each type boundary + edge cases
- [ ] `cargo check` passes (0 errors)
- [ ] `pnpm test` passes
- [ ] Committed to `main` with semantic messages
