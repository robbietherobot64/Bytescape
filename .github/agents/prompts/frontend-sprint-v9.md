# Frontend Agent — Sprint v9 (2026-03-22) — WebView Tab Fix + Scroll Performance

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` — current user feedback.

### Organizational Context: Two-Agent Model

There are only **two agents**: you (Frontend/Track B — all of `src/`) and the Backend agent (Track A — `src-tauri/src/`).

### Performance Standards
- **Research before coding.** Read the existing code before changing it.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **Two-attempt cap:** If stuck after two genuine attempts, document what you tried in `progress.md` and move on.
- **Terminal discipline:** Kill background processes when done. Close every terminal you open.
- **Do NOT modify files in `src-tauri/`** — that is Backend scope.

---

## 🚨 ABSOLUTE RULES — VIOLATIONS WILL BREAK THE APP

### Rule 1: Do NOT create patch scripts
Do NOT create any `.cjs`, `.js`, or `.mjs` files in the project root or `bytescape/` root for the purpose of patching source files. If a file needs changing, change it directly.

### Rule 2: Do NOT add unnecessary dependencies
Do NOT add `react-window`, `react-virtualized`, or other virtualization libraries without PM approval. The scroll performance issues can be fixed with CSS and React optimization first.

---

## 🚨 Codebase State (as of commit `bb65218`)

### User Feedback (Critical)
1. **"Links and profiles now load in a new tab....but the tab breaks the whole UI."** — The webview tab has THREE problems:
   - Tab label stays "Loading page..." (never updates to the page title)
   - Webview renders in the **lower half** of the screen instead of filling the tab's content area
   - Webview **persists visibly** when switching to other tabs (feed, chat, etc.)
   
2. **"Images and overall UI somewhat jittery when scrolling."** — Multiple CSS/React causes identified.

---

## Sprint Theme: "Fix the WebView Tab + Smooth Scrolling"

---

## Phase 1 — WebView Tab Fixes (CRITICAL)

### P1 — Fix WebView Show/Hide on Tab Switch ⚡⚡⚡ CRITICAL

**File:** `src/screens/WebViewScreen.tsx` lines 207-222

When the tab becomes inactive, the component calls `browser_hide` — good. But when the tab becomes **active** again, it only calls `browser_set_bounds` — it does NOT call `browser_show`. The native webview may remain hidden.

**The Backend agent is adding `browser_show` and `browser_show_at` commands this sprint.** Use them:

```typescript
// ─── Show/hide native webview when tab activation changes ───
useEffect(() => {
    const label = labelRef.current;
    if (!createdRef.current) return;
    
    if (isActive) {
        // Show and position when tab becomes active
        const el = containerRef.current;
        if (el) {
            const rect = el.getBoundingClientRect();
            invoke('browser_show_at', {
                label,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            }).catch((err) => {
                // Fallback: try set_bounds + show separately
                console.warn('[WebView] browser_show_at failed, trying fallback:', err);
                invoke('browser_set_bounds', {
                    label, x: rect.x, y: rect.y,
                    width: rect.width, height: rect.height,
                }).catch(() => {});
                invoke('browser_show', { label }).catch(() => {});
            });
        }
    } else {
        // Hide when tab becomes inactive
        invoke('browser_hide', { label }).catch(() => {});
    }
}, [isActive]);
```

**If `browser_show_at` doesn't exist yet** (backend agent hasn't committed), use `browser_set_bounds` followed by a separate `invoke('browser_show', { label })` call. If `browser_show` doesn't exist either, document the dependency in `progress.md`.

### P2 — Fix Tab Label "Loading page..." ⚡⚡⚡ CRITICAL

**File:** `src/screens/WebViewScreen.tsx` lines 253-264

The title change listener finds the tab via:
```typescript
const tab = nav.tabs.find(t => t.screen === 'webview' && t.meta?.url === url);
```

This matches on `t.meta?.url === url` (the **initial** URL prop). After redirects, the actual page URL changes but the `url` prop doesn't update, so the tab is never found and the label never updates.

**Fix:** Match by tab ID instead:
```typescript
useEffect(() => {
    const label = labelRef.current;
    let unlisten: (() => void) | null = null;
    listen<string>(`browser-title-${label}`, (event) => {
        const newTitle = event.payload;
        if (!newTitle) return;
        const nav = useNavStore.getState();
        // Find tab by matching the webview label stored in meta, 
        // or fall back to matching by screen type + this component's URL
        const tab = nav.tabs.find(t => 
            t.screen === 'webview' && (
                t.meta?.label === label || 
                t.meta?.url === url
            )
        );
        if (tab && nav.updateTabLabel) {
            nav.updateTabLabel(tab.id, newTitle);
        }
    }).then(fn => { unlisten = fn; });
    return () => { unlisten?.(); };
}, [url]);
```

**Also:** When creating the webview tab (wherever `openTab` is called with `screen: 'webview'`), store the webview label in `meta.label` so we can match reliably:
- Search for where `openTab({ screen: 'webview', ... })` is called (likely `openInApp.ts` or `App.tsx`)
- Ensure `meta` includes `{ url, label: `browser-${id}` }` or at minimum the URL

**Alternatively** (simpler): Find the tab by its ID. The `WebViewScreen` component should receive the `tabId` as a prop from `App.tsx`, then use that directly:
```typescript
if (tab) nav.updateTabLabel(tab.id, newTitle);
```

### P3 — Fix WebView Positioning (Lower Half Bug) ⚡⚡⚡ CRITICAL

**File:** `src/screens/WebViewScreen.tsx`, CSS for `.webview-container`

The user reports the webview appears in the **lower half** of the screen. This happens because:

1. `getBoundingClientRect()` returns coordinates relative to the **viewport** (the Tauri main webview)
2. But `window.add_child()` may position the child webview relative to the **OS window** (including title bar, menu bar)
3. On Linux with a title bar (~30px), the webview is shifted down by that amount

**Investigate and fix:**
1. Log the `rect` values being sent to `browser_create` — `console.log('[WebView] rect:', rect.x, rect.y, rect.width, rect.height)`
2. If the webview appears too low, the y-coordinate needs to be adjusted. The container's `getBoundingClientRect().y` already accounts for all elements above it (tab bar, omnibar), so it should be correct IF `add_child` positions relative to the webview viewport
3. If `add_child` positions relative to the window frame, you need to subtract the frame offset

**Most likely fix:** Ensure the `.webview-container` CSS takes up the FULL height of the tab pane:
```css
.webview-container {
    width: 100%;
    height: 100%;
    flex: 1;
    min-height: 0;
}
```

Check that the parent `.tab-pane` also has proper height. If `tab-pane` doesn't have `height: 100%` or `flex: 1`, the container will have incorrect dimensions.

**Check `App.tsx` styles** for `.tab-pane--standard` and `.tab-pane--active`:
- Does the tab pane fill the content area?
- Is there a gap between the tab pane and the omnibar/sidebar that the webview is ignoring?
- Are there `padding` or `margin` values on the tab pane that the webview overlaps?

### P4 — Ensure WebView Hides on ALL Tab Switches ⚡⚡

The user says the webview "doesn't go away when you change tabs back to feed or other sections." This could mean:

1. The `isActive` prop isn't changing to `false` when switching away
2. The `browser_hide` call is failing silently
3. There are multiple webview instances and not all are being hidden

**Verify:**
1. Add a `console.log('[WebView] isActive changed:', isActive, 'label:', labelRef.current)` at the top of the show/hide effect
2. Check `App.tsx` line ~691 — the `style={isActive ? ... : { display: 'none' }}` hides the React container but does NOT affect the native webview overlay. The native webview is positioned absolutely over the window, so hiding its React container has no effect on it.
3. **This is the core architectural issue:** When `display: none` is set on the tab pane, the React container disappears but the native webview stays visible because it's a separate OS-level surface

The fix is already in P1 (calling `browser_hide` when `isActive` becomes false). Verify it actually fires.

---

## Phase 2 — Scroll Performance

### P5 — Remove `content-visibility: auto` ⚡⚡ CRITICAL

**File:** `src/App.css` line ~1005

```css
.feed-entry-card {
    content-visibility: auto;
    contain-intrinsic-size: auto 400px;
}
```

`content-visibility: auto` causes layout shifts when cards scroll into view — the browser estimates 400px for hidden cards, then recalculates when they become visible. Cards with different heights (images vs text-only vs quote posts) cause visible jumps.

**Fix:** Remove both properties:
```css
.feed-entry-card {
    /* removed: content-visibility: auto; */
    /* removed: contain-intrinsic-size: auto 400px; */
}
```

This may slightly increase initial render time for very long feeds, but eliminates the jitter. Feed performance was already optimized in sprint v5 (caching, pagination, 200 items max).

### P6 — Remove Rainbow Animation from Feed Cards ⚡⚡

**File:** `src/App.css` lines ~2956-2980

Every single feed card header runs a continuous `rainbow-slide` CSS animation. With 50-200 cards in the feed, that's 50-200 independently animating gradients during scroll. This causes compositor thrashing.

**Fix:** Remove the animation entirely, or make it only animate on hover:

```css
/* Option A: Remove entirely */
.feed-entry-header::after {
    content: '';
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    height: 1px;
    background: linear-gradient(90deg,
        #ff0000, #ff7700, #ffff00, #00ff00, #0000ff, #8b00ff, #ff0000
    );
    background-size: 200% 100%;
    opacity: 0.4;
    /* removed: animation: rainbow-slide 4s linear infinite; */
}

/* Option B: Only animate on hover (less impact) */
.feed-entry-header::after {
    /* ... same as above ... */
    animation: none;
}
.feed-entry-card:hover .feed-entry-header::after {
    animation: rainbow-slide 4s linear infinite;
}
```

Option A is preferred for maximum performance.

### P7 — Stabilize Image Loading ⚡

Images without explicit dimensions cause layout shifts when they load. Check `FeedEntryCard.tsx` for `<img>` tags:

1. Find all `<img>` elements in feed cards
2. Ensure each has either:
   - `width` and `height` attributes (from the data, if available)
   - `aspect-ratio` CSS set on the container (e.g., `aspect-ratio: 16/9` for a standard media container)
   - `min-height` on the image wrapper to reserve space before load
3. Add `loading="lazy"` to all feed card images (enables native lazy loading)
4. Consider adding `decoding="async"` to prevent image decode from blocking the main thread during scroll

### P8 — Remove Feed Card Hover Border Shift ⚡

**File:** `src/App.css` line ~1009

```css
.feed-entry-card:hover {
    border-color: var(--link-blue);
    box-shadow: 2px 2px 0 #444;
}
```

The `border-color` change from `#999` to `var(--link-blue)` doesn't change size, but the `box-shadow` growing from `1px 1px` to `2px 2px` adds 1px of visual shift. Combined with browser repaints during scroll, this contributes to the "jittery" feel.

**Fix:** Use `outline` instead of changing `box-shadow`, or use a fixed `box-shadow` size:
```css
.feed-entry-card:hover {
    border-color: var(--link-blue);
    /* Keep shadow same size to prevent shift */
    box-shadow: 1px 1px 0 #666;
}
```

Or remove the hover effect entirely on feed cards (the user has complained about image hover jitter across multiple sprints).

---

## Phase 3 — Cleanup

### P9 — Verify `USE_SYSTEM_BROWSER` State ⚡

**File:** `src/utils/openInApp.ts` line 21

Currently set to `false` (in-app browser active). This is correct IF the in-app browser works properly after the P1-P4 fixes. 

**However:** If after P1-P4, the webview still has issues (can't position correctly, can't hide reliably), set it back to `true` as a safety measure and document the findings.

**Decision tree:**
- If P1-P4 fixes make webview tabs work properly → keep `USE_SYSTEM_BROWSER = false`
- If webview still broken after P1-P4 → set `USE_SYSTEM_BROWSER = true` and add a comment explaining why

---

## Coordination with Backend Agent

The Backend agent is adding/fixing these commands this sprint:

| Command | What it does | Your action if missing |
|---------|-------------|----------------------|
| `browser_show(label)` | Shows a hidden webview + focuses it | Use `browser_set_bounds` as fallback |
| `browser_show_at(label, x, y, w, h)` | Combined position + show + focus | Use `browser_set_bounds` + `browser_show` separately |
| `browser_hide(label)` | Properly hides webview (not just off-screen) | Current off-screen hack still works as fallback |

If the backend commands don't exist yet when you start, implement P1 with the fallback approach and document the dependency.

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT create patch scripts (`.cjs` files)
- ❌ Do NOT modify files in `src-tauri/` — Backend scope
- ❌ Do NOT add heavy dependencies (react-window, etc.) without PM approval
- ❌ Do NOT leave zombie terminals open
- ❌ Do NOT set `USE_SYSTEM_BROWSER = true` unless P1-P4 fail (document reasoning)

## Definition of Done
### Phase 1 (WebView — must complete)
- [ ] WebView hidden when switching to non-webview tabs (verified with console logs)
- [ ] WebView shown in correct position when switching back to webview tab
- [ ] Tab label updates from "Loading page..." to actual page title
- [ ] WebView fills the tab content area (not shifted to lower half)
### Phase 2 (Scroll — must complete)
- [ ] `content-visibility: auto` removed from feed cards
- [ ] Rainbow animation removed or hover-only on feed card headers
- [ ] Image containers have stable dimensions (no layout shift on load)
- [ ] Feed card hover effect doesn't cause layout shift
### Phase 3 (Cleanup)
- [ ] `USE_SYSTEM_BROWSER` state documented with reasoning
### Build verification
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
- [ ] Committed to `main` with semantic messages
