# Fediverse Standards Readiness Brief

Date: 2026-04-15  
Owner: PM / Systems  
Scope type: architecture + execution planning (no sprint prompt generation)

---

## 1) Objective

Convert the Mastodon Sovereign Tech Agency deliverables into a concrete, phase-gated Bytescape program that PM can run without rewriting current architecture.

Primary outcomes:
- interop readiness for moderation exchange and account portability standards,
- stronger federation trust and transport reliability,
- clear sequencing across backend, frontend, and infrastructure tracks,
- measurable acceptance gates for go/no-go decisions.

---

## 2) Current Baseline (What Exists Already)

### Core strengths
- ActivityPub primitives exist (actor, inbox, outbox, signatures, WebFinger).
- Mastodon/AP connector and command surfaces already ship C2S and S2S paths.
- Strong local/P2P media substrate exists via Iroh blobs and media server.
- Unified feed merge pipeline and source filtering already exist.
- Security utilities and URL validation helpers are already in place.

### Known gaps relevant to standards readiness
- No FIRES/FASP protocol adapter layer exists yet.
- RSS tab path is still placeholder at UI orchestration level.
- AP cryptographic posture is not yet future-ready (digest and signature agility gaps remain).
- Relay provisioning and hosted interoperability paths are still early-stage stubs.

---

## 3) System Map

## 3.1 Actors
- Local client (desktop/mobile Tauri): authoring, vault, and sync authority.
- Hosted platform (`bytescape.social`): AP instance, PDS, relay, identity resolver.
- External fediverse instances: Mastodon/Lemmy and FIRES/FASP-capable peers.
- PM governance layer: release gates, conformance checks, rollout controls.

## 3.2 Components in Scope
- Federation core: AP inbox/outbox/signature/webfinger modules.
- Connectors: Mastodon + future standards adapters.
- Moderation exchange service: FIRES-compatible ingest/export and policy mapping.
- Portability exchange service: FASP capability negotiation, announce/backfill state machine.
- Transport/media service: remote media references, lazy fetch, cache policy.
- Trust/crypto service: digest verification, signature profile upgrades, key lifecycle.

## 3.3 Boundary Definition
- Keep existing engine/lens split intact.
- Add standards functionality as adapter modules behind existing command boundaries.
- Avoid frontend rewrites; expose standards state through narrowly scoped commands/events.

---

## 4) Target Data Flows

## Flow A: Moderation Exchange (FIRES-style)
1. Remote server sends moderation payload or advisory event.
2. Adapter validates signature, digest, and source trust policy.
3. Payload is normalized into local moderation schema with provenance metadata.
4. Policy engine produces local action recommendation (never blind auto-ban by default).
5. UI surfaces recommendation, source, confidence, and operator action controls.

## Flow B: Portability/Discovery Exchange (FASP-style)
1. Capability discovery establishes what peer supports (announce, backfill, subscriptions).
2. Local server issues signed announce request.
3. Remote server acknowledges and provides staged transfer callbacks/events.
4. Backfill pipeline imports historical records into normalized BytescapeItem paths.
5. State machine records checkpoints for retry, audit, and resumability.

## Flow C: Federation Trust Upgrade
1. Inbound AP request enters verification pipeline.
2. Digest and signature profile checks run before payload acceptance.
3. Trust metadata and verification result are persisted for forensics.
4. Failed verification routes to quarantine path and operator diagnostics.

## Flow D: Remote Media Reliability
1. Federated item stores remote media as reference-first metadata.
2. Lazy fetch occurs on render or prefetch trigger.
3. Content-addressed cache persists validated payload.
4. Repeat requests serve from local cache with predictable UX.

---

## 5) Program Structure (PM-Executable)

## Phase S0: Baseline and Contract Definition (1 week)
Deliverables:
- freeze standards scope and conformance targets,
- define canonical internal schemas for moderation and portability events,
- establish integration test fixtures and sample payload corpus.

Exit gate:
- written interface contracts approved,
- fixture pack committed,
- no unresolved ownership ambiguity across tracks.

## Phase S1: Trust and Verification Foundation (1-2 weeks)
Deliverables:
- digest verification enforcement in federation ingest path,
- signature validation hardening and timeout/SSRF controls,
- verification telemetry for pass/fail/quarantine.

Exit gate:
- all fixture signatures/digests validated,
- malicious fixture corpus rejected correctly,
- no regression in existing AP interoperability smoke tests.

## Phase S2: Moderation Exchange Adapter (2 weeks)
Deliverables:
- FIRES-style adapter (ingest + export pathways),
- local mapping from external moderation classes to internal policy levels,
- operator-visible moderation provenance UI fields.

Exit gate:
- bidirectional exchange validated against fixture peers,
- moderation decisions remain policy-controlled locally,
- rollback path proven under malformed payload replay.

## Phase S3: Portability and Discovery Adapter (2-3 weeks)
Deliverables:
- FASP-like capability negotiation,
- announce + callback + backfill orchestration state machine,
- resumable imports with idempotent checkpoints.

Exit gate:
- successful dry-run transfer on test dataset,
- deterministic retry on network interruption,
- full audit trail from request to final import checkpoint.

## Phase S4: Media and Feed Reliability Closure (1-2 weeks)
Deliverables:
- remote media reference-first ingestion + lazy fetch,
- RSS ingestion completion in unified feed path,
- UX states for partial availability and backfill progress.

Exit gate:
- no broken-media regressions on federated sample set,
- RSS source produces non-empty feed items end-to-end,
- performance SLOs met for warm and cold cache scenarios.

## Phase S5: Conformance, Pilot, and Rollout (1 week)
Deliverables:
- standards conformance checklist and scorecard,
- pilot rollout with feature flags,
- operator runbook and incident playbook updates.

Exit gate:
- pilot success criteria achieved,
- rollback and kill-switch tested,
- PM sign-off for wider rollout.

---

## 6) Workstream Ownership

- Track A (Backend): protocol adapters, trust pipeline, ingest/export state machines, media/reference handlers.
- Track B (Frontend): moderation provenance views, transfer status surfaces, feed/RSS completion UX.
- Track I (Infrastructure): hosted peer harness, test fixtures, observability, rollout controls.
- PM/Systems: gate reviews, acceptance criteria enforcement, dependency/risk management.

---

## 7) Primary Risks and Mitigations

- Spec churn risk (draft standards evolve):
  - Mitigation: isolate protocol adapters behind versioned translation boundaries.
- Security regression risk in federation ingest:
  - Mitigation: enforce verification-first ingest and quarantine defaults.
- Cross-track sequencing drift:
  - Mitigation: gate progression on explicit conformance artifacts, not calendar dates.
- Performance regression from richer verification:
  - Mitigation: bounded async workers, cache hot paths, and telemetry-driven tuning.

---

## 8) Success Metrics

- Trust correctness:
  - 100% digest/signature validation on standards fixture corpus.
  - 0 accepted malformed signed payloads in security replay suite.
- Portability reliability:
  - successful end-to-end transfer in pilot with deterministic resume.
  - no duplicate imports after interrupted transfer replay.
- Moderation interoperability:
  - external moderation events normalized with provenance retained.
  - operator action latency under agreed threshold on pilot loads.
- User-visible quality:
  - broken-media rate reduced on federated samples.
  - RSS tab no longer returns placeholder-empty datasets.

---

## 9) PM Operating Notes

- This brief is the planning source for standards readiness and roadmap sequencing.
- Do not generate new sprint prompt files as part of this planning step.
- Execute through existing active sprint cadence and track ownership.