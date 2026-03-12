# Content Agent Kickoff: C3.2 Long-Form Articles with Tiptap

## Context

This is a **load-bearing architectural choice**. The Article editor MUST use **Tiptap** because its first-class Yjs bindings (`@tiptap/extension-collaboration`, `@tiptap/extension-collaboration-cursor`) are the foundation for C6 Collaborative Editing. Using a plain textarea or a different editor would require a costly swap later.

C3.2 has no external dependencies — it can proceed in parallel with all other work.

## Prerequisites

- C1 stash triage is complete (see `content-c1-stash-triage.md`)
- Security branches merged to `main`

## Your Task

### Step 1: Install Tiptap packages

```bash
cd bytescape
pnpm add @tiptap/react @tiptap/starter-kit @tiptap/extension-placeholder @tiptap/extension-collaboration @tiptap/extension-collaboration-cursor @tiptap/pm
```

Note: `@tiptap/extension-collaboration` and `@tiptap/extension-collaboration-cursor` are installed now but NOT wired up until C6. They just need to be in `package.json` so C6 doesn't require a dep change.

### Step 2: Create the Article Editor component

Create `bytescape/src/components/ArticleEditor.tsx`:

**Requirements:**
- Tiptap editor instance with `StarterKit` (headings, bold, italic, lists, blockquotes, code blocks, horizontal rules)
- Markdown import: accept pasted Markdown and convert to Tiptap JSON via `@tiptap/starter-kit` parsing
- Markdown export: convert Tiptap JSON back to Markdown string for storage/syndication
- Placeholder text: "Write your article..."
- Toolbar: Bold, Italic, H1/H2/H3, Bullet List, Ordered List, Blockquote, Code Block, Horizontal Rule
- Title field: separate `<input>` above the editor for article title
- Reading time estimate: word count ÷ 200, displayed below toolbar
- Character/word count display
- GeoCities aesthetic: use CSS custom properties (`--bg-primary`, `--text-primary`, `--border-color`, etc.) and `Win98Box` container
- `onChange` callback that emits `{ title: string, content: string, contentJson: object, wordCount: number }`

**Component signature:**
```typescript
interface ArticleEditorProps {
    initialTitle?: string;
    initialContent?: string;  // Markdown string
    onChange: (data: { title: string; content: string; contentJson: object; wordCount: number }) => void;
    readOnly?: boolean;
}
```

### Step 3: Integrate into ComposeBox

Add an "Article mode" toggle to ComposeBox:
- When toggled ON, the textarea is replaced by `<ArticleEditor>`
- The ComposeBox `item_type` switches from `'Status'` to `'Article'`
- The article content (Markdown) goes into the `text` field of `onPublish`
- The article JSON goes into a new `contentJson` field (for future Yjs backing in C6)
- When toggled OFF, revert to the standard textarea

### Step 4: Create the Article Reader component

Create `bytescape/src/components/ArticleReader.tsx`:

**Requirements:**
- Renders an Article `BytescapeItem` in clean, readable typography
- Title rendered as `<h1>` with large font
- Content rendered from Markdown → sanitized HTML (use the existing `sanitize.ts` utility)
- Reading time estimate
- Scroll progress bar at top of view
- Source badge showing where the article came from
- Created/updated timestamps
- GeoCities aesthetic with `Win98Box` container

This component will be used by the Reader Lens (B1.2) and standalone article views.

### Step 5: Backend — Article storage

The `BytescapeType::Article` variant already exists. Verify that the `BytescapeItem` `content` field handles long-form text (it should — it's a `String`). If there's a length limit anywhere in the posting pipeline, raise it for Article types.

Check `commands/posting.rs` → `publish_post_v2` to ensure:
- It accepts `item_type: "Article"` properly
- Long text content is stored without truncation
- The content is stored as Markdown (the Tiptap JSON is a frontend concern, stored in `metadata` or a new field)

### Step 6: Add Tiptap CSS

Add to `src/App.css`:

```css
/* Article Editor */
.article-editor-container {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius);
    background: var(--bg-secondary);
}

.article-title-input {
    width: 100%;
    padding: 12px 16px;
    font-size: 24px;
    font-weight: bold;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border-color);
    color: var(--text-primary);
    outline: none;
}

.article-toolbar {
    display: flex;
    gap: 4px;
    padding: 8px;
    border-bottom: 1px solid var(--border-color);
    flex-wrap: wrap;
}

.article-toolbar button {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 4px 8px;
    border-radius: 3px;
    cursor: pointer;
    font-size: 13px;
}

.article-toolbar button:hover {
    background: var(--bg-primary);
    color: var(--text-primary);
}

.article-toolbar button.is-active {
    background: var(--accent-purple);
    color: white;
    border-color: var(--accent-purple);
}

.ProseMirror {
    padding: 16px;
    min-height: 300px;
    outline: none;
    color: var(--text-primary);
    font-size: 15px;
    line-height: 1.7;
}

.ProseMirror h1 { font-size: 28px; margin: 24px 0 12px; }
.ProseMirror h2 { font-size: 22px; margin: 20px 0 10px; }
.ProseMirror h3 { font-size: 18px; margin: 16px 0 8px; }

.ProseMirror blockquote {
    border-left: 3px solid var(--accent-purple);
    padding-left: 16px;
    margin-left: 0;
    color: var(--text-secondary);
}

.ProseMirror code {
    background: var(--bg-tertiary);
    padding: 2px 6px;
    border-radius: 3px;
    font-family: monospace;
}

.ProseMirror pre {
    background: var(--bg-tertiary);
    padding: 12px;
    border-radius: var(--border-radius);
    overflow-x: auto;
}

.ProseMirror p.is-editor-empty:first-child::before {
    content: attr(data-placeholder);
    color: var(--text-muted);
    pointer-events: none;
    float: left;
    height: 0;
}

/* Article Reader */
.article-reader {
    max-width: 720px;
    margin: 0 auto;
    padding: 24px;
}

.article-reader h1 {
    font-size: 32px;
    margin-bottom: 8px;
    color: var(--text-primary);
}

.article-reader-meta {
    display: flex;
    gap: 16px;
    color: var(--text-muted);
    font-size: 13px;
    margin-bottom: 24px;
}

.article-scroll-progress {
    position: sticky;
    top: 0;
    height: 3px;
    background: var(--accent-purple);
    transition: width 0.1s linear;
    z-index: 10;
}
```

### Step 7: Tests

Create `bytescape/src/components/ArticleEditor.test.tsx`:
- Renders without crashing
- Title input updates
- onChange fires with expected shape
- Word count calculation is correct
- Reading time estimate is reasonable (e.g., 200 words → ~1 min)

### Step 8: Verify

1. `npx tsc --noEmit` — no type errors
2. `pnpm test` — all tests pass
3. `pnpm tauri dev` — verify:
   - Article mode toggle appears in ComposeBox
   - Tiptap editor renders with toolbar
   - Can type rich text, add headings, lists, etc.
   - Publishing stores article with full content
   - Article items render in Reader Lens (if B1 is merged)

### Step 9: Commit

```bash
git add -A
git commit -m "feat(track-c): C3.2 long-form article editor with Tiptap

- ArticleEditor component with Tiptap StarterKit
- Markdown import/export, title field, reading time estimate
- Article mode toggle in ComposeBox
- ArticleReader component for clean article display
- Tiptap collaboration extensions installed (wired in C6)
- CSS styling matching GeoCities theme"
```

## Architecture Notes for C6

When C6 (Collaborative Editing) begins, the `ArticleEditor` will be extended:
1. `@tiptap/extension-collaboration` will connect to a Yjs `Y.Doc`
2. `@tiptap/extension-collaboration-cursor` will show remote cursors
3. The `Y.Doc` will sync via Tauri commands to `yrs` (Rust Yjs port) in `collab.rs`
4. `iroh-gossip` will broadcast Yjs updates to connected peers

**Do NOT** add any Yjs wiring now. Just ensure the editor architecture doesn't prevent it — specifically:
- The Tiptap `Editor` instance should be accessible via `useEditor()` hook (not buried in a ref)
- The `onChange` callback should work alongside future Yjs providers
- Don't store editor state in React state (let Tiptap own it)

## Files You Will Create
- `bytescape/src/components/ArticleEditor.tsx`
- `bytescape/src/components/ArticleReader.tsx`
- `bytescape/src/components/ArticleEditor.test.tsx`

## Files You Will Modify
- `bytescape/src/components/ComposeBox.tsx` (article mode toggle)
- `bytescape/src/App.css` (Tiptap + article styles)
- `bytescape/package.json` / `pnpm-lock.yaml` (Tiptap deps)
- `ROADMAP.md` (mark C3.2 complete)

## Files You Must NOT Touch
- `bytescape/src-tauri/src/richtext.rs` (already done, Track C/security work)
- `bytescape/src/components/lenses/` (Track B)
- `bytescape/src-tauri/src/connectors/` (Track A)
