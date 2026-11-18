# Learning Journal

One entry per study unit or tutor session. Keep it short — this is a log, not an essay.
Format: date, what covered, say-it-back result (pass/revisit), one sentence on what
finally "clicked", next step.

---

## 2026-09-25 — Framework setup
- Set up learning framework (`docs/learning/README.md`), tutor agent, explain-first protocol.
- Curriculum: the companion *Bytescape Onboarding* curriculum (7 phases), now mastery-gated units.
- Say-it-back: n/a (setup entry).
- Clicked: the gap was never the material — it was the absence of a loop binding study to real code and real tasks.
- Next: tutor session "repo tour: what runs where", then Phase 1 Unit 1.

## 2026-09-27 — Session 1: repo tour: what runs where
- Covered: system map (browser SPA ↔ Rust server ↔ PostgreSQL/MinIO ↔ live deployment), the two codebases, one request's journey (feed load).
- Say-it-back: PASS — named all three unaided: FeedLayout.tsx → get_feed in api/feed.rs → appview_posts.
- Clicked: folders narrow the search, files are where changes actually happen; the feed reads a local table that the firehose consumer fills — it never waits on Bluesky.
- Next: `/tutor trace a post from UI to DB` (guides: docs/learning/session-01-tracing-a-post.md), then Phase 1 Unit 1 of the curriculum.
