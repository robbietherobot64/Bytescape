# Backend Agent — Sprint v9 (2026-03-22) — WebView Architecture Fix + Browser Commands

## ⚠️ OPERATIONAL DISCIPLINE — READ CAREFULLY

Read `.github/instructions/agent-guardrails.instructions.md` before doing anything.
Read `.github/agents/prompts/user-feedback` — current user feedback.

### Organizational Context: Two-Agent Model

There are only **two agents**: you (Backend/Track A — `src-tauri/src/`, `Cargo.toml`) and the Frontend agent (Track B — all of `src/`).

### Performance Standards
- **Research before coding.** Read existing code before changing it.
- **One feature per commit.** Each commit addresses one priority item.
- **Verify before committing:** `cargo check` + `npx tsc --noEmit` + `pnpm test -- --run` — all must pass.
- **Two-attempt cap:** If stuck after two genuine attempts, document what you tried in `progress.md` and move on.
- **Terminal discipline:** Kill background processes when done. Close every terminal you open.
- **Do NOT modify files in `src/`** — that is Frontend scope.

---

## 🚨 Codebase State (as of commit `bb65218`)

### Sprint v8 Results (your previous work — ALL COMPLETE ✅)
- Identity overhaul: LinkedAccount, proper DIDs, fake mastodon:instance:id removed
- Mastodon auth returns master_did, creates LinkedAccount
- list_follows/list_followers merge Mastodon contacts
- unlink_account command
- AP actor DID derivation fixed

### What's Broken Now (user feedback)
1. **WebView tab breaks entire UI** — "the tab itself is still a 'loading page...' tab, but the lower half of the screen is where the webview opens. That lower half is also now permanently there, and doesn't go away when you change tabs back to feed or other sections."

This is primarily a **backend architecture problem**: the native webview (created via `window.add_child()`) is a window-level overlay that sits on top of the React DOM. The current `browser_hide` command only moves it off-screen to `(-10000, -10000)` — it doesn't actually hide it. And there's no `browser_show` command to properly restore it.

---

## Sprint Theme: "Robust WebView Lifecycle"

The in-app browser is the #1 user complaint. The native webview must be a well-behaved citizen in the tab system — invisible when inactive, properly positioned when active, and cleaned up on close.

---

### P1 — Add `browser_show` Command ⚡⚡⚡ CRITICAL

**File:** `src-tauri/src/commands/browser.rs`

There is no `browser_show` command. When the user switches back to a webview tab, the frontend can only call `browser_set_bounds` to move it back on-screen. But the webview may have been hidden (GTK internal state), so just repositioning it isn't enough.

**Add:**
```rust
#[tauri::command]
pub async fn browser_show(app: AppHandle, label: String) -> Result<(), String> {
    let webview = app.get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;
    webview.show().map_err(|e| format!("show: {}", e))?;
    webview.set_focus().map_err(|e| format!("set_focus: {}", e))?;
    Ok(())
}
```

Register it in `lib.rs` alongside the other browser commands.

### P2 — Fix `browser_hide` to Actually Hide ⚡⚡⚡ CRITICAL

**File:** `src-tauri/src/commands/browser.rs` line 211

Current implementation:
```rust
pub async fn browser_hide(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.set_position(LogicalPosition::new(-10000.0_f64, -10000.0_f64));
    }
    Ok(())
}
```

This is a hack that moves the webview off-screen. It doesn't actually hide it. On some platforms (especially WebKitGTK on Linux), the webview may still consume resources and paint.

**Fix:** Use the proper `hide()` API:
```rust
pub async fn browser_hide(app: AppHandle, label: String) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        let _ = webview.hide();
    }
    Ok(())
}
```

If `webview.hide()` doesn't exist in the Tauri v2 API, use **both** approaches:
1. Move off-screen as fallback
2. Set size to (0, 0) to prevent painting

Test that `hide()` → `show()` properly restores the webview. If it doesn't, document the limitation.

### P3 — Add `browser_show_at` Combined Command ⚡⚡

To minimize IPC round-trips, add a single command that shows the webview AND positions it in one call:

```rust
#[tauri::command]
pub async fn browser_show_at(
    app: AppHandle,
    label: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let webview = app.get_webview(&label)
        .ok_or_else(|| format!("Webview '{}' not found", label))?;
    webview.set_position(LogicalPosition::new(x, y))
        .map_err(|e| format!("set_position: {}", e))?;
    webview.set_size(LogicalSize::new(width, height))
        .map_err(|e| format!("set_size: {}", e))?;
    webview.show().map_err(|e| format!("show: {}", e))?;
    webview.set_focus().map_err(|e| format!("set_focus: {}", e))?;
    Ok(())
}
```

Register in `lib.rs`.

### P4 — Investigate WebView Positioning Bug ⚡⚡

The user says the webview appears in the **lower half** of the screen. This suggests the `y` coordinate passed to `browser_create` is wrong — it's placing the webview below the tab content area instead of aligned with it.

**Investigate:**
1. Add `eprintln!` logging to `browser_create` and `browser_set_bounds` that prints the received x, y, width, height values
2. Check if `window.add_child()` uses coordinates relative to the window or the screen
3. The Tauri v2 `add_child` docs may clarify — the position should be relative to the parent window's content area
4. If the frontend is passing `getBoundingClientRect()` values and the window has a title bar, the y-offset may need adjustment

**The frontend sends `rect.x, rect.y, rect.width, rect.height` from `getBoundingClientRect()`**. On Linux with a window title bar, `getBoundingClientRect().y` is relative to the web content viewport (i.e., the Tauri webview), but `add_child` may position relative to the OS window (including title bar). If there's a mismatch, the webview will be shifted down by the title bar height (~30px). This could explain "lower half" positioning.

**Fix approach:** If `add_child` is window-relative, subtract the title bar offset. Or better: use `window.inner_position()` to get the content area offset and adjust.

### P5 — Clean Up Diagnostic Logging ⚡

`browser_create` has 20+ `eprintln!` statements that were useful during debugging but pollute the terminal. Convert them to structured logging:
- Use `log::debug!` for normal flow
- Use `log::warn!` for non-fatal errors
- Use `log::error!` for actual failures
- Keep the existing message format but behind the log level

### P6 — Delete Patch Scripts ⚡

Remove all leftover patch scripts from the workspace:
- `patch_browser.cjs` (project root)
- `patch_chat.cjs` (project root)
- `patch_feedscreen.cjs` (project root)
- `patch_feedscreen_hook.cjs` (project root)
- `patch_useunifiedfeed.cjs` (project root)
- `patch_panels.cjs` (project root)
- `bytescape/run_patch.cjs` (bytescape dir)

These are workaround scripts that agents should never create. Delete them.

---

## Anti-Patterns (MUST AVOID)
- ❌ Do NOT modify files in `src/` — Frontend scope
- ❌ Do NOT create patch scripts (`.cjs` files in project root)
- ❌ Do NOT leave zombie terminals open
- ❌ Do NOT add `sleep` loops — use commands directly
- ❌ Do NOT use `eprintln!` for production logging — use `log::debug!`/`log::warn!`/`log::error!`

## Definition of Done
- [ ] `browser_show` command exists and is registered in `lib.rs`
- [ ] `browser_hide` uses real `hide()` (not just position hack)
- [ ] `browser_show_at` combined command exists and is registered
- [ ] WebView positioning investigated — documented finding in progress.md
- [ ] Diagnostic logging cleaned up (debug/warn/error levels)
- [ ] All 7 patch scripts deleted
- [ ] `cargo check` — 0 errors
- [ ] `npx tsc --noEmit` — 0 errors
- [ ] `pnpm test -- --run` — all pass
- [ ] All terminals closed
