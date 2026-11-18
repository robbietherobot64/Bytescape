---
title: Bytescape MVP Scope + Phase-2 Shelf
created: 2026-10-04
status: active — supersedes PRD §1 "Launch scope" for v1 sequencing
---

# MVP Scope + Phase-2 Shelf

**Precedence:** `VISION.md` (vision) > this document (ship gates) >
`wiki/product/prd.md` (archived reference). Decisions: `DECISIONS.md`.

This document is the scope police. `VISION.md` is the vision (see precedence above);
the PRD remains the archived phase-2 feature reference. **Nothing ships before its
shelf gate below.**
When a feature is requested, check this page first.

## The pitch

**Your corner of the open web: rescue your archive, find your friends, build a
page that's actually yours.**

Media-first: the UI is made of images and videos, with or without accompanying
text. Fun is the growth engine — a profile worth showing is what recruits the
next user (no marketing budget required).

## The demo — front to back (defines the MVP)

1. **Sign up** — Bluesky OAuth (native email signup stays invite-gated).
2. **Import** — upload the FB/IG GDPR archive; streamed, painless, progress visible.
3. **Find friends** — imported contacts matched against the network; one-click
   follow. *(Demo seeds: two preloaded Instagram contact lists.)*
4. **Profile populates** — imported posts and media become the page.
5. **Customize** — drag/resize tiles; simple widgets: links, music player
   (Bandcamp/SoundCloud embeds), uploaded/linked GIFs; preset themes.
6. **Feed** — interest picker → automatic hashtag follows → personalized
   media-first feed that works **even when nobody you know has joined**.

If the demo runs front to back on a fresh account in under 10 minutes, the MVP is
done. If a step needs an admin tool, it isn't.

## MVP surface (the only build/finish list)

- Auth: Bluesky OAuth
- Import: Facebook + Instagram archive parsing (existing work: finish, don't expand)
- Friend matching (mutual-archive + handle matching; existing work)
- Profile: tile grid (drag/resize), widget set from step 5, preset themes,
  imported gallery/timeline
- Feed: interest picker, hashtag follows, media-first rendering, per-source filter
- Open question (answer before implementation): **is a minimal media-first post
  composer in the MVP?** Lean: yes — an app you can't post to can't retain.

Everything else is hidden behind "coming soon" tiles at most. Zero build cost.

## Phase-2 shelf — unlock gates (not dates)

Unlock depends on the density each feature needs, not on enthusiasm.

| Feature | Gate | Verdict / notes |
|---|---|---|
| **Circles / groups** | **T1** — gated on Atproto Spaces 1.0 | Spaces is the right primitive ("forums from dozens to millions" is a stated design target) but is **alpha** as of 2026-08-20: breaking changes, no security review, no production data. Launch expected late 2026. Optional cheap spike (weekend): prototype against the `bulletin` sample + `@atproto` alpha SDK to size the future build. Alternates if Spaces slips: Bluesky group chats (`chat.bsky.*`, GA June 2026, ≤50 people) or Bluesky's announced "communities". |
| **Events** | **T1** — index before create | Browse/RSVP to existing `community.lexicon.calendar.event` records from the network (Smoke Signal / atmo.rsvp interop) — aggregation solves cold-start. Native creation later. |
| **Marketplace** | **T2+** — needs scale + safety work | Trust-economy feature: fraud, scams, payments, disputes. The one feature that can get users hurt. v0, if ever, is a listings board scoped to circles (no payments, no strangers). Likely needs funding/partnership before this is real. |
| **Chat / DMs** | T1–T2 | Bluesky chat client (`chat.bsky.*`) is comparatively cheap but still a surface — behind the shelf until the demo is delightful. |
| **Stories, RSS, custom CSS, games directory** | T2+ | Shiny. Follow retention, never lead it. |

## Definition of done

- Demo runs front to back, fresh account, <10 min, no admin tools.
- Performance bar: feed interactive in <2s; import progress always visible.
- 10 people who are not you complete the demo unprompted.

## Success ladder (what "not a waste" means)

- **L0 — mission:** data liberation + friend reconnection actually works. This is
  success independent of user counts (stated goal, 2026-10-04).
- **L1 — ~100** non-friend users, weekly actives among them.
- **L2 — ~1,000+:** the Pixelfed-shaped trajectory (relatable hero product,
  compounding visibility via shareable profile pages and SEO). Contributor
  inbound is the signal to open the shelf; investor conversations only if wanted.

Friendica-scale (a few thousand loyal users) is the floor worth having, not a
failure. But the design targets the L2 path: profile pages as shareable,
SEO-visible artifacts; import as the acquisition pitch; fun as the engine.
