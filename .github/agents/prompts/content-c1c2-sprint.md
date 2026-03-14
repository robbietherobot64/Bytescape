# Content Agent — Sprint: USER FEEDBACK v2 (2026-03-15)

## 🚨 READ FIRST — User Reverted Agent Work

The user manually reverted the content classification system you built:
- ❌ `commands/classify.rs` module removed from `commands/mod.rs` (file still exists but is dead code)
- ❌ `reclassify_vault_items` removed from `lib.rs` command registration
- ❌ `Link`/`Video` removed from TypeScript `BytescapeType` — only 3 types in TS: `Status | Gallery | Article`
- ❌ `ReclassifyResult` removed from TypeScript types
- ❌ `category` removed from TypeScript `TrackerEntry`
- ❌ Content-type filter chips removed from FeedScreen — user restored lens toggles (List/Gallery/Reader)
- ❌ `item_type_filter` removed from `get_timeline_unified` backend command

### What this means for you:
- The Rust `BytescapeType` enum STILL has `Link` and `Video` — that's fine in Rust
- The `classify_item` and `classify_feed_entry` functions in `models/mod.rs` still exist — they should be used on ingest
- But do NOT expose them as standalone commands or add filter parameters to feed queries
- The frontend uses **lenses** (GalleryLens, ReaderLens) with client-side filtering, NOT server-side type filtering
- Do NOT add `category` back to the TypeScript `TrackerEntry` type
- Do NOT add `Link`/`Video` to TypeScript `BytescapeType`
- Do NOT create GIF pickers, emoji pickers, or any roadmap items

All work on **`main`**. Run `git branch --show-current` before every commit.

Read the user's latest feedback at `.github/agents/prompts/user-feedback` before starting.

---

## Priority 1 — Appearance Customization ⚡⚡

**User:** *"Let's make appearance more customizable. You should be able to click on any one of those selected colors in a given theme and change it. You should also be able to import images or gifs for animated and engaging backgrounds and themes."*

### Fix:
1. In the Appearance/Theme settings section, make each theme color swatch clickable
2. On click: show a color picker (native HTML `<input type="color">` is fine) that lets the user override that specific CSS variable
3. Save custom color overrides to localStorage or the vault
4. Add an option to import a background image/gif URL
5. Apply the custom background as `background-image` on the app root
6. Persist customization across sessions

---

## Priority 2 — Appearance: Default Background Options

**User:** *"If we could have some defaults provided, that would be cool."*

### Fix:
- Provide 4-6 default background presets: abstract patterns, gradient meshes, or subtle textures
- These can be CSS gradients (no external images needed) or embedded SVG patterns
- Add a "Background" row in theme settings with thumbnail previews

---

## Priority 3 — ComposeBox Enhancement Support

Review the ComposeBox for any outstanding issues. The user hasn't complained about it recently, so this is lower priority. Check:
1. Link preview unfurling still works
2. Rich text formatting works
3. Multi-platform posting targets work
4. No regressions from the user's manual edits

---

## Anti-Patterns
- Do NOT create GIF pickers, emoji pickers, or any C2.2/C2.3 roadmap items
- Do NOT add `Link`/`Video` to TypeScript BytescapeType
- Do NOT add `category` to TypeScript TrackerEntry
- Do NOT re-create `commands/classify.rs` module registration
- Do NOT add `item_type_filter` to feed commands
- Do NOT modify Rust connectors — Backend scope
- Do NOT modify FeedScreen, ProfileScreen, or AuditScreen — Frontend scope
- **Always run `git branch --show-current` before committing**

## Definition of Done
- [ ] Theme colors are individually customizable via color picker
- [ ] Custom colors persist across sessions
- [ ] Background image/gif support works
- [ ] Default background presets available
- [ ] ComposeBox still works correctly (no regressions)
- [ ] `pnpm tauri dev` compiles
- [ ] `pnpm test` passes
- [ ] Committed to `main`
