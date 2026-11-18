# Roadmap to public release

**Source of truth:** OpenSpec change tasks. This page is the owner-facing overview; it does not
replace `STATUS.md` (current phase), `DECISIONS.md` (dated decisions), or the change folders.
**Last updated:** 2026-11-17 (planning complete, implementation not started).

## Where we are

| Change | Tasks | State | Folder |
|---|---|---|---|
| `harden-for-public-alpha` | 0/46 | planned | `openspec/changes/harden-for-public-alpha/` |
| `add-classifieds` | 0/38 | planned | `openspec/changes/add-classifieds/` |
| `add-location-maps` | 0/17 | planned | `openspec/changes/add-location-maps/` |

Append `openspec list` for live counts.

## Opening condition

The alpha opens when hardening gates 1–7 are verified. No calendar date. Gates, in order:

| Gate | What “done” means | Blocking |
|---|---|---|
| 1 Containment | SSRF guarded, body/upload limits, CORS allowlist, secrets/invite hygiene, graceful shutdown | yes |
| 2 Abuse floor | rate limits, firehose poison policy, reports + moderation queue/actions | yes |
| 3 Legal | ToS/privacy/guidelines/DMCA/age/cookies/security contact, paid-posting terms + tax posture | yes |
| 4 Operations | backups + restore drill, uptime/error alerts, support/status, incident runbook, payment reconciliation | yes |
| 5 Quality | blocking CI with Postgres, migration replay, frontend tests, E2E smoke, bug bash, perf check | yes |
| 6 Contributor packaging | AGPL-3.0 LICENSE, README, CONTRIBUTING, CODEOWNERS, SECURITY, templates, source link | before public repo |
| 7 Rollout | failure drills, first invite wave (10–25), fee-mode decision, two-week review | yes |

## Build order

1. **Sprint H1 — containment** (`harden-for-public-alpha` 1.1–1.7). No user-facing change except
   safer uploads and bounded requests.
2. **Sprint C1 — classifieds foundation** (`add-classifieds` 1.x, 2b.1). Lexicon, migration, dual
   write path, validation, category/fee config.
3. **Sprint H2 — abuse floor** (2.1–2.8) and **C2 — read/API + payments** in parallel where the
   interfaces allow.
4. **Sprint C3 — federation** (3.x, 4.x, 5.x), then **C4 — UI** (6.x, 7.x).
5. **Sprint L1 — location** (`add-location-maps`) after Classifieds and Events records both carry
   places; map UI last.
6. **Sprint H3 — legal/ops/quality** starts alongside H1 because DMCA/tax/counsel have external lead
   time; **H4 — rollout** closes the gates.

Classifieds does not block the opening unless it is promised in the first wave; hardening does.

## Alpha envelope

- Invite-only; recommended ceiling **250 registered / 30 concurrent**.
- Invites stay on after the wave until a real report is handled end to end.
- Fee mode starts at provider **test** until the tax/refund sign-off is recorded.
- Do not plan the opening around contributors; parallel lanes only after CI blocks.

## Keeping this page honest

- Check off OpenSpec tasks as they are verified; update the counts here with them.
- One dated line per decision in `DECISIONS.md`.
- `STATUS.md` gets the current phase and next action, not the whole roadmap.
- Optional kanban: Plane workspace `bytescape` exists, but the project listing endpoint returned 404
  on 2026-11-17 — create a project (e.g. “Bytescape Public Alpha”, identifier `BSALPHA`) and the
  OpenSpec tasks can be mirrored as work items.
