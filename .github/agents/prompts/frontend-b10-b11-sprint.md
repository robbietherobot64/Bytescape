# Frontend Agent — Sprint: UF Completion & Layout Polish (2026-03-15)

## 🚨 PM AUDIT FINDINGS — READ THIS FIRST

### ⚠️ Process Violations Identified

1. **You merged ALL branches into `main` without PM authorization.** The PM manages cross-branch merges. You merged both `track-b/ui-redesign` and `track-c/c1-facet-frontend` into `main` after the user noticed a rollback. While the fix was necessary in the moment, you should have flagged it in progress.md and waited for PM direction rather than self-initiating a 3-branch merge.

2. **You fabricated UF item descriptions in your progress report.** Your "Deferred Items" section lists: "UF.5 (Compose Box visibility), UF.6 (Compose Box targeting), UF.9 (Video player), UF.10 (RSS feeds), UF.11 (Hashtag feeds), UF.14 (Settings)". **Every single description is wrong.** The actual UF items are:
   - UF.5 = Following/Subscribed Tab Dedup
   - UF.6 = Feed Content-Type Filters (Replaces Lenses)
   - UF.9 = Profile Page Rework
   - UF.10 = Social Module Redesign
   - UF.11 = Settings Consolidation
   - UF.14 = Theme Customization Enhancement
   
   You either didn't read the ROADMAP or hallucinated from a prior context. **Always verify item descriptions against ROADMAP.md before citing them.**

3. **UF.1 is INCOMPLETE.** You claimed UF.1 (Layout & Spacing Overhaul) in your commit but `tauri.conf.json` still has no `"maximized": true` — the window does NOT launch maximized. Content max-width is still 960px, not truly full-width. Fix this.

### ✅ Verified Completions
- **UF.2** ✅ — Aspect ratio changed to 4/3, verified in code
- **UF.4** ✅ — Chat click handler wired in FriendsPanel with DID field
- **UF.8** ✅ — GrapesJS `panels: { defaults: [] }` removed, editor renders correctly
- **UF.12** Partial — Header text changed to "Import Archive", some centering done

---

## Current State

You are now on **`main`** at `0f9f0a6`. All three tracks are merged. Feature branches `track-b/ui-redesign` and `track-c/c1-facet-frontend` have been fully merged into main.

**Going forward, all work happens on `main`.** No more feature branches unless the PM explicitly creates them.

---

## Priority 1 — Fix UF.1: Window Maximized + True Full-Width ⚡

Your UF.1 work is incomplete. Fix it:

1. **`tauri.conf.json`**: Add `"maximized": true` to the window config. The window must launch maximized.
2. **`App.css`**: Remove or increase `--content-max-width: 960px`. The feed content area should use the full viewport width minus sidebar widths, not a fixed 960px cap. Consider `max-width: 100%` or removing the constraint entirely, letting the grid/flex layout handle proportional sizing.
3. **Responsive test**: After changes, resize the window to various sizes and verify sidebars and content scale proportionally. No dead space at any viewport width.

---

## Priority 2 — Complete UF.12: Import Screen Remaining Items ⚡

Your UF.12 work was partial. Complete it:

1. ✅ "Shred Surveillance & Save Memories" → "Import Archive" — Done
2. Verify: "Exorcism Complete" → "Import Complete" — Is this actually changed?
3. **Tracker classification**: Currently just shows a count. Add category labels: "Ad tracking (N), Location tracking (N), Cross-site tracking (N), Data collection (N)". The Backend's `facebook.rs` already categorizes tracker types — surface these categories in the UI.
4. **Centering**: Verify the import wizard is truly centered, not cramped to upper-left.

---

## Priority 3 — UF.5: Following/Subscribed Tab Cleanup

**User feedback:** *"My contacts section on the following view is bad. It's just literally a worse version of the friends tab."*

1. Remove the "My Contacts" sub-section from the Following tab — it's redundant with the Social tab
2. Merge "Subscribed" tab content into the "All" tab with a filter chip for subscription sources
3. If Following tab becomes too sparse, consolidate it: show followed accounts' posts without the redundant contacts section

---

## Deferred to Next Sprint (Do NOT Start)

- **UF.6** Feed content-type filters (replaces lenses) — MAJOR rework, needs PM design spec
- **UF.9** Profile page rework (wall vs posts tabs, narrow cards, verified badges)
- **UF.10** Social module redesign — MAJOR rework, needs PM design spec
- **UF.11** Settings consolidation (merge Account + Connected Accounts, add privacy/search settings)
- **UF.14** Theme customization (per-color picker, animated backgrounds)
- **UF.15** Export manager & account deletion
- **B10.4** Reload/Refresh
- **Navigation Unification** (useNavStore → useLocationStore migration)

---

## Branch & Commit Strategy
- Work on `main` directly — all branches are merged
- Commit after each priority with descriptive messages
- Update `progress.md` at the end of your session

## Anti-Patterns
- Do NOT modify `ComposeBox.tsx` — Track C owns that
- Do NOT modify Rust connector files (`connectors/*.rs`) — Track A owns those
- Do NOT modify `ROADMAP.md` — PM scope only
- Do NOT start unassigned items after completing these priorities — report back via progress.md
- Do NOT fabricate item descriptions — ALWAYS verify against ROADMAP.md before citing UF/B/C items
- Do NOT merge branches without PM authorization
