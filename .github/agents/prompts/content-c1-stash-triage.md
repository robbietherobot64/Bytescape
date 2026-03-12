# Content Agent Kickoff: C1 Rich Text Stash Triage

## Context

The C1 Rich Text Facets feature was partially implemented across two separate work sessions, resulting in overlapping artifacts:

1. **Committed work** (`636ce7c` on `security/frontend-hardening`): A complete `richtext.rs` module (+647 lines) with facet parsing, a wired-up `parse_richtext_facets` Tauri command, and posting.rs changes. This is already merged into the branch and will land on `main`.

2. **Stashed work** (`stash@{1}`, "WIP on track-c/c1-rich-text-facets"): An earlier iteration containing its own `posting.rs` changes (+104 lines), `ComposeBox.tsx` changes (+286 lines), `syndicate.rs` changes (+8 lines), `lib.rs` command registration (+3 lines), and agent doc updates. This was stashed from `track-c/c1-rich-text-facets` branch (which is still at `main` HEAD — `cfa582d`).

**The risk**: The stash may contain **unique frontend ComposeBox wiring** (facet highlighting, @mention resolution, character counting UI) that didn't make it into the committed version. But the Rust backend portions likely overlap/conflict with what's already committed.

## Your Task

### Step 1: Create a triage branch

After the PM merges security branches to `main`:

```bash
git checkout main
git pull  # ensure you have the merged security work
git checkout -b track-c/c1-stash-triage
```

### Step 2: Inspect the stash WITHOUT applying it

```bash
git stash show stash@{1} -p > /tmp/c1-stash-diff.txt
```

Read through `/tmp/c1-stash-diff.txt` and categorize each file:

| File | Likely Status | Action |
|------|--------------|--------|
| `.github/agents/*.agent.md` (3 files) | **Superseded** — agent docs have been updated multiple times since | Skip |
| `bytescape/src-tauri/Cargo.toml` | **Superseded** — already has the deps from committed work | Skip |
| `bytescape/src-tauri/src/commands/posting.rs` | **Partially superseded** — `636ce7c` committed posting.rs changes. Diff to find unique hunks | Cherry-pick unique hunks only |
| `bytescape/src-tauri/src/commands/syndicate.rs` | **May be unique** — check if facet-aware syndication was added | Evaluate |
| `bytescape/src-tauri/src/lib.rs` | **Partially superseded** — command registrations may differ | Evaluate |
| `bytescape/src/components/ComposeBox.tsx` | **Likely unique** — +286 lines of frontend wiring is the most valuable part | **Primary salvage target** |

### Step 3: Extract the ComposeBox diff specifically

```bash
git stash show stash@{1} -p -- bytescape/src/components/ComposeBox.tsx > /tmp/c1-composebox-diff.txt
```

Compare against the current ComposeBox to identify what's missing. The stash likely contains:
- **Facet highlighting** — colored spans for @mentions, #hashtags, URLs in the compose textarea
- **Character counter UI** — live grapheme count with Bluesky 300 / AP 500 limits
- **Mention autocomplete trigger** — `@` keypress detection and dropdown
- **Hashtag autocomplete trigger** — `#` keypress detection
- **URL detection** — link card preview trigger on URL paste

### Step 4: Apply only the unique parts

**Do NOT `git stash pop` directly** — it will conflict with the committed work on `main`. Instead, manually apply the unique ComposeBox hunks:

```bash
# Try applying just the ComposeBox file from the stash
git diff stash@{1}^..stash@{1} -- bytescape/src/components/ComposeBox.tsx | git apply --check
```

If that fails (base mismatch), manually read the diff and re-implement the unique frontend parts on top of the current ComposeBox.

### Step 5: Check syndicate.rs for unique facet wiring

The stash has +8 lines in `syndicate.rs`. Check if the committed version already handles facet emission during syndication. If not, add the missing facet pass-through.

### Step 6: Verify

1. `cd bytescape && npx tsc --noEmit`
2. `pnpm test` — all tests pass
3. `pnpm tauri dev` — verify ComposeBox shows character count, facet highlighting works

### Step 7: Commit

```bash
git add -A
git commit -m "feat(track-c): wire C1 rich text facets into ComposeBox frontend

- Character counter with Bluesky/AP limit indicators
- Facet highlighting for @mentions, #hashtags, URLs
- Foundation for C1.2 mention autocomplete and C1.3 hashtag autocomplete"
```

### Step 8: Merge to main and clean up

```bash
git checkout main
git merge track-c/c1-stash-triage
git branch -d track-c/c1-stash-triage
git branch -d track-c/c1-rich-text-facets  # empty branch, no longer needed
```

### Step 9: Drop the stash (only after verifying all unique content is committed)

```bash
git stash drop stash@{1}
```

## What NOT To Do

- Do NOT pop the stash directly onto `main` — it will conflict
- Do NOT rewrite `richtext.rs` — the committed version (+647 lines) is comprehensive and already handles byte-offset facet parsing, @mention resolution, #hashtag detection, and URL extraction
- Do NOT modify posting.rs facet handling unless the stash adds something the committed version doesn't have
- Do NOT touch lens components or FeedScreen — that's the Frontend Agent's B1 work

## After This Task

Once the stash is triaged and the unique ComposeBox wiring is committed, your next task is **C3.2 Long-Form Articles with Tiptap** (see `content-c32-tiptap-kickoff.md`).
