## What changed

<!-- What does this PR do, in plain language? -->

## Why

<!-- Link the issue this was shaped against: Fixes #… -->

## Evidence (required)

<!-- "Tests pass" is necessary but not sufficient — describe what you exercised. -->

- [ ] Full suite green: `cargo test` (in `bytescape-server/`) and
      `npm test -w apps/web`, plus `npm run build:web` and `npm run build:events`
- [ ] Changed surface exercised — describe what you ran and what you saw:
  <!-- UI: real browser, changed screen/controls. Bugs: before/after reproduction. Features: the path, front to back. -->

## AI-assisted contributions (mandatory to state)

<!-- See CONTRIBUTING.md — unverified agent output is rejected on sight. -->

- Produced by (model or human):
- Verified by (human who exercised it, or model of equal-or-greater capability
  than GLM 5.3 running the verification protocol):
- [ ] This contribution has been verified per CONTRIBUTING.md

## Checklist

- [ ] Tests assert consumer-visible behavior (no tautologies or wiring echoes)
- [ ] Scope matches the issue (no drive-by refactors or renames)
- [ ] Deletes, if any, actually remove records — not just UI state