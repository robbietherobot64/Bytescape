# Contributing to Bytescape

Thanks for helping build it. This file is the external contributor flow — the
contract between you and the maintainers. (The wiki's
[developer pages](wiki/developers/contributing.md) describe the maintainer's
internal change process; you don't need them to contribute.)

## Ground rules

- **Design decisions come from the owner, not from inference.** If your change
  contains a product or design judgment that the issue didn't settle, ask in the
  issue rather than choosing.
- **ROADMAP.md is the alignment doc.** Work serving the current roadmap gates
  gets attention first. Work that contradicts [VISION.md](VISION.md) is declined.
- **Governance is BDFL:** @korendian has the final say on design, scope, and
  merges.

## The flow

1. **Open an issue first.** Use the bug or feature template. Non-trivial work is
   shaped with a maintainer before you write code — a short alignment round up
   front beats a large wrong patch at review. (This replaces heavyweight spec
   machinery for outsiders: the issue is where scope gets agreed.)
2. **Fork and branch.** One branch per change, named for what it does.
3. **Build it, with tests**, per the conventions below.
4. **Open a PR against `main`** with the PR template filled in: what changed,
   why, and evidence it works.
5. **Review** is a conversation — expect questions and follow-up commits.

## Running the full test suite

```sh
# backend (Rust)
cd bytescape-server
cargo test

# web (TypeScript)
npm ci
npm test -w apps/web

# production builds (both must pass)
npm run build:web
npm run build:events
```

Database-backed backend tests read `DATABASE_URL` (the compose Postgres from
`docker compose up -d` works) and skip when it's unset — run them at least once
before claiming a data-path change is done. A PR is expected to leave the whole
suite green.

## Code conventions

- **Tests must catch consumer-visible bugs.** Assert behavior, boundaries,
  invariants, and state transitions — the things a user could observe breaking.
- **No tautological tests.** A test that echoes the implementation, checks only
  that a mock was called, or asserts "doesn't throw" is noise; it gets declined
  or deleted in review. If a behavior genuinely can't be tested, say why in the
  PR instead of faking a test.
- **Deletes are real.** A delete must actually remove the record from the
  store — not hide it in the UI, and not leave it reachable via the API.
- **Verify with exercised evidence.** Green builds are necessary, not
  sufficient. UI changes get demonstrated in a real browser against the changed
  surface; bug fixes get a before/after reproduction; feature paths get run front
  to back. Describe what you exercised in the PR — "tests pass" alone is not
  evidence that user-visible behavior works.
- **Markers are contracts.** Comment markers like `(spec: …)` cite the behavior
  contract a piece of code implements, and dated notes (`YYYY-MM-DD: …`) record
  why something odd is the way it is — keep them accurate or update them with
  the code. Data markers (`enc:v1:` prefixes, `marker` keys in metadata rows)
  guard idempotent migrations and repairs; treat those strings as immutable once
  shipped.

## What review looks like

Review checks that the change does what the issue shaped, stays inside its
scope, and comes with believable evidence. Common reasons a PR is declined:

- **Scope drift** — refactors, renames, or "fixes" the issue never asked for.
  Send them as separate PRs.
- **Unverifiable claims** — "should work", "probably fine", "tested locally"
  with no described evidence.
- Tests that assert nothing (see above), or changes that weaken existing tests.
- New dependencies or infrastructure with no justification in the issue.

## MANDATORY: AI-assisted contributions

AI-assisted contributions are welcome — this codebase is largely agent-written —
but they carry one hard rule:

**Every agentic contribution MUST be fully verified by either:**

**(a)** a human reviewer who exercises the change — actually runs it, drives the
changed surface, sees the behavior — **or**

**(b)** an AI model of equal-or-greater capability than GLM 5.3 running the
project verification protocol: the full test suite green **plus** exercised
evidence on the changed surface, as described above.

State in the PR which model produced the change and which human or model
verified it. **Unverified agent output is rejected on sight** — no exceptions, no
"I'll test it later". A PR that hides its agent involvement is treated as a
trust violation.

## License

By contributing, you agree your changes are licensed under AGPL-3.0-only, like
the rest of the project.