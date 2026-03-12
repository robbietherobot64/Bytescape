# Frontend Agent Kickoff: B1 Lens Integration Finalization

## Context

The B1 Lenses feature (Gallery, Reader, FeedItemRenderer) was committed on `track-b/b1-lenses` branch (`1147c54`) and has already been merged into `security/frontend-hardening`. However, the **FeedScreen lens toggle integration** was stashed before it could be committed. This stash contains the UI wiring that lets users switch between List, Gallery, and Reader views on the feed.

## Your Task

### Step 1: Ensure you're on the right branch

After the PM merges the security branches to `main`, you should be working on `main` (or a new branch off `main`). The lens components (`GalleryLens.tsx`, `ReaderLens.tsx`, `FeedItemRenderer.tsx`, `Lightbox.tsx`) are already committed and merged. What's missing is the FeedScreen integration.

### Step 2: Pop and apply stash@{2}

```bash
git stash list  # Verify stash@{2} is "WIP on track-b/b1-lenses"
```

The stash contains exactly 2 files:
- `ROADMAP.md` — a small roadmap checkbox (may conflict with current roadmap, discard this hunk if so)
- `bytescape/src/screens/FeedScreen.tsx` — **this is the important one** (+54 lines)

The FeedScreen changes add:
1. Imports for `GalleryLens` and `ReaderLens` components
2. A `FeedLens` type export (`'list' | 'gallery' | 'reader'`)
3. An `activeLens` state variable
4. A lens toggle button bar (☰ list / ▦ gallery / 📖 reader) in the feed header
5. Conditional rendering: gallery lens → `<GalleryLens>`, reader lens → `<ReaderLens>`, list → existing `<FeedEntryCard>` map

### Step 3: Apply the FeedScreen changes

Pop the stash onto a temporary branch:
```bash
git stash pop stash@{2}
```

If there are conflicts in `ROADMAP.md`, resolve by keeping the current version (the roadmap has been significantly updated since the stash was created). The `FeedScreen.tsx` changes should apply cleanly since the stash base and current FeedScreen share the same structure.

### Step 4: Add CSS for the lens toggle

The lens toggle buttons need styling. Add to `src/App.css`:

```css
.feed-lens-toggle {
    display: flex;
    gap: 4px;
    margin-left: auto;
}

.lens-btn {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 4px 8px;
    border-radius: var(--border-radius);
    cursor: pointer;
    font-size: 14px;
    transition: all 0.15s ease;
}

.lens-btn:hover {
    background: var(--bg-secondary);
    color: var(--text-primary);
}

.lens-btn.active {
    background: var(--accent-purple);
    color: white;
    border-color: var(--accent-purple);
}
```

### Step 5: Verify

1. `cd bytescape && npx tsc --noEmit` — check types
2. `pnpm test` — all tests pass
3. `pnpm tauri dev` — verify lens toggle works visually (switches between list/gallery/reader views)

### Step 6: Commit

```bash
git add -A
git commit -m "feat(track-b): wire B1 lens toggle into FeedScreen

- Add list/gallery/reader toggle buttons to feed header
- GalleryLens renders masonry grid for media-heavy items
- ReaderLens renders article-focused clean typography view
- Default view remains list (FeedEntryCard)"
```

### Step 7: Update Roadmap

Mark B1.1, B1.2, B1.3 as complete in `ROADMAP.md`:
```
- [x] **B1.1 Lens: Gallery**
- [x] **B1.2 Lens: Reader**
- [x] **B1.3 Item-Type Rendering**
```

## Files You Will Touch
- `bytescape/src/screens/FeedScreen.tsx` (from stash)
- `bytescape/src/App.css` (lens toggle styles)
- `ROADMAP.md` (checkboxes)

## Files You Must NOT Touch
- `bytescape/src/components/ComposeBox.tsx` (Track C)
- `bytescape/src-tauri/src/` (no Rust changes needed)
- Any lens component files (already committed and correct)
