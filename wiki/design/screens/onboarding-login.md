# Screen Spec: Onboarding / Login (v2)

> **Status note (2026-10-10):** acceptance criterion 1 (dual-path landing with native
> invite-gated signup) is **superseded** by `DECISIONS.md` — phase 1 is existing ATProto
> credentials only; native signups are deferred to the mass-market phase.

**Status:** Draft — complete
**Date:** 2026-08-12
**Route:** `/login` (unauthenticated landing); `/oauth/callback` (Bluesky OAuth return); `/signup` (native signup, invite-gated)
**Satisfies:** FR-27 (Authentication — Bluesky OAuth + Native Signups), FR-30 (Native PDS-Hosted Signups); contributes to SM-3 (return rate — first-run lands on a usable profile/feed, not a dead end), §8 Security (DPoP + PKCE, invite codes, CAPTCHA/email verification).

**Sources:**
- v1 implementation: `screens/LoginFlow.tsx` (118), `screens/BlueskyLanding.tsx` (76), `components/OnboardingTour.tsx` (158), `components/CoachMark.tsx` (230), `components/TerminalBoot.tsx` (61), `components/TerminalMenu.tsx` (68), `components/InterestStep.tsx` (139), `components/ImportStep.tsx` (471)
- v1 spec: `_ref/artifacts/_bmad-output/evolution/specs/bluesky-first-onboarding-spec.md`
- v2 constraints: `wiki/product/prd.md` (FR-27, FR-30, §3.2 growth, §9 pre-launch), `wiki/architecture/decisions.md` (AD-01b dual-PDS, AD-03 did:web, AD-13 invite-gated hosting)

> **v1→v2 shift:** v1 deleted every auth method but Bluesky OAuth. v2 re-adds a **native signup** path because PDS hosting is now active (invite-gated, AD-13) — but it does **not** resurrect the v1 17-phase state machine. Both paths land on the same app shell; the guided tour is the shared first-run experience.

---

## 1. Overview

The onboarding screen has one job: get a first-time or returning user from *no session* to *their own profile/feed with a completed first-run tour* as frictionlessly as possible. It is the only screen that branches on the **dual-PDS model** (AD-01b):

| Path | Who | Identity | Result |
|---|---|---|---|
| **Bluesky OAuth** | Existing Bluesky user | `did:plc` on bsky.social | Signed in, profile pre-populated, tour |
| **Native signup** | No Bluesky account; invite holder | `did:web:{handle}.bytescape.social` (AD-03) | Account created, tour |

Both user classes see the **identical** post-login experience — the split surfaces only at the entry point and (for native) the signup form. AppView access does **not** require a Bytescape PDS account: a Bluesky OAuth user gets every AppView feature without creating a native identity.

### 1.1 v1 salvage — what carries, what doesn't

| Asset | Verdict | Rationale |
|---|---|---|
| `BlueskyLanding.tsx` (76) | **KEEP (concept)** | Single-button landing aesthetic is correct. Becomes one of **two** entry cards (OAuth + native), not the only path. |
| `LoginFlow.tsx` (118) | **REWORK** | `landing → oauth-callback → boot → done` state machine is sound, but must gain a `signup` branch and drop the legacy Google OAuth popup effect (lines 92-103). |
| `TerminalBoot.tsx` (61) | **REFINE** | Boot animation is brand-defining. Two fixes: `did:key:z6Mk…` line must become `did:web` (v2 has no did:key identity); it must not block returning users (skip when token already valid). |
| `TerminalMenu.tsx` (68) | **DROP** | v1 was a shell menu for a terminal-mode app; no v2 role. Mine for styling tokens. |
| `OnboardingTour.tsx` (158) + `CoachMark.tsx` (230) | **KEEP (concept), REFINE** | 3-step coach-mark tour (interests → import → customize) is the correct first-run shape. Keep the robustness contract (wait/retry 3s, never trap, localStorage step persistence, pause/resume on nav) from the v1 spec verbatim. |
| `InterestStep.tsx` (139) | **KEEP** | Interest picker reused as tour step 1 target action. |
| `ImportStep.tsx` (471) | **KEEP (as modal)** | Reused as tour step 2 target action. See `import.md`. |

### 1.2 Design tokens (kept from v1)

Terminal aesthetic: VT323 (display) / Press Start 2P (accents) / Share Tech Mono (body), dark `#1a1a2e` background, green-phosphor accent. `>` prompt prefixes on buttons/labels. This is the brand surface; it does not change in v2.

---

## 2. Wireframe

### 2.1 Landing (`/login`) — the dual-path entry

```
┌──────────────────────────────────────────────────┐
│                                                  │
│                  BYTESCAPE                       │
│            your home on the open web             │
│                                                  │
│   ┌────────────────────────────────────────┐     │
│   │  🦋  Sign in with Bluesky              │     │
│   └────────────────────────────────────────┘     │
│   ┌────────────────────────────────────────┐     │
│   │  📦  Create a Bytescape account        │     │
│   │      (invite required)                 │     │
│   └────────────────────────────────────────┘     │
│                                                  │
│   no Bluesky account? get one at bsky.social     │
│   already have an invite? create a home here     │
│                                                  │
└──────────────────────────────────────────────────┘
```

**Behavior:**
- Two primary actions, vertically stacked, no method picker, no login/signup toggle.
- **"Sign in with Bluesky"** → `oauthApi.blueskyAuthorize()` → redirect to bsky.social.
- **"Create a Bytescape account"** → reveal inline native signup form (2.2), or navigate to `/signup`.
- Returning user with a valid token in storage: never sees this screen — routes straight to the app (token validity checked once at bootstrap).

### 2.2 Native signup (`/signup`) — invite-gated

```
┌──────────────────────────────────────────────────┐
│  > create your bytescape home                    │
│                                                  │
│  invite code       [  ▁▁▁▁▁▁▁▁▁▁  ]  [verify]   │
│  email             [  user@…           ]          │
│  handle            [  alice            ] .bytescape.social
│  password          [  ••••••••         ]          │
│  password again    [  ••••••••         ]          │
│  [I'm not a robot] ▢  CAPTCHA                    │
│                                                  │
│  [  > create account  ]                          │
│  ─────────────────────────────                    │
│  ← back to sign in                               │
└──────────────────────────────────────────────────┘
```

**Behavior:**
- Invite code is **required** and validated (server-side) before the form submits (AD-13: hosting invite-gated during testing). AppView access via Bluesky OAuth needs no invite.
- Handle availability check (debounced) against `pds_accounts.handle` UNIQUE.
- Email verification: after create, show "check your inbox" state; account is `pending` until verified (FR-27/§9).
- Password: Argon2id server-side; client enforces a minimum length + strength hint only.
- On success → native user lands on their (near-empty) profile + the guided tour.

### 2.3 OAuth callback → boot → profile

```
landing → (Bluesky redirect) → /oauth/callback → TerminalBoot → /@{handle} + tour
                                  (or native) → TerminalBoot → /@{handle} + tour
```

TerminalBoot lines (v2-corrected): `> Booting bytescape kernel…`, `> Connecting to AT Protocol network…`, `> Resolving DID identity… [OK]`, `> Loading feed subscriptions… [OK]`, `> Profile loaded. Welcome to bytescape.` — **no `did:key`**, **no AP/P2P references**.

---

## 3. Component breakdown

| Component | New/Kept | Responsibility |
|---|---|---|
| `LandingScreen` | Rework of `BlueskyLanding` | Dual-path entry; OAuth trigger; route to `/signup` |
| `NativeSignupForm` | **NEW** | Invite code + email + handle + password + CAPTCHA + verification state |
| `OAuthCallbackHandler` | Rework of `LoginFlow` effect | Exchange code, store tokens, route to boot |
| `TerminalBoot` | Refine | Brand boot animation; v2-correct lines; skip for returning users |
| `OnboardingTour` | Refine | 3-step coach-mark state machine + localStorage persistence |
| `CoachMark` | Keep | Tooltip + highlight overlay + wait/retry robustness |
| `InterestStep` | Keep | Tour step 1 target modal |
| `ImportStep` | Keep (modal) | Tour step 2 target modal |
| `SessionBootstrap` | **NEW** | Validates token at app start; decides `login` vs `app` phase; refreshes expired token (replaces `getInitialPhase` in v1 `App.tsx`) |

---

## 4. States

| State | Trigger | UI |
|---|---|---|
| **Idle (landing)** | Unauthenticated visit | Dual-path buttons |
| **OAuth in-flight** | Click "Sign in with Bluesky" | Button → `> CONNECTING...`, disabled |
| **Signup in-flight** | Submit signup | Submit button disabled + spinner |
| **Signup field error** | Bad invite / taken handle / weak password / CAPTCHA fail | Inline field error (non-color cue: ⚠ + text) |
| **Email pending** | Account created, not verified | "Check your inbox" screen + resend link |
| **OAuth error** | `?oauth_error=` or token exchange failure | Error banner `> {message}` + retry |
| **Booting** | Post-auth | TerminalBoot lines type out |
| **First-run tour** | New user (`onboarding_complete` != true) | Coach marks over profile |
| **Returning** | Valid token at bootstrap | Straight to feed, no boot, no tour |

**Error handling:** OAuth/token errors are user-readable (`apiErrorDisplay`), never raw stack traces. Signup errors are field-scoped. The callback handler must not trap the user — an exchange failure returns to `/login` with a retryable error.

---

## 5. Interactions

1. **Sign in with Bluesky** → set `sessionStorage` OAuth intent, `GET /oauth/par` + redirect. Callback must complete ≤5s (§8).
2. **Create account** → reveal/route to signup form. Invite code verified first (async), then fields enable.
3. **Handle input** → debounced (300ms) availability check; live `alice.bytescape.social` preview.
4. **Submit signup** → CAPTCHA gate → POST → `pending` state → email verification flow.
5. **Tour** → step 1 interests → step 2 import → step 3 customize; "Skip tutorial" always available and always navigates to feed; "Go to feed" on final step. Each step persists to localStorage so refresh never restarts the tour.

**Robustness (carried from v1 spec verbatim):**
- Missing tour target → wait/retry 3s, then skip to next step (never trap).
- Navigate away during tour → pause, resume on return.
- Any step handler error → fall through to next step.
- `Skip tutorial` is always the escape hatch.

---

## 6. Responsive behavior

| Breakpoint | Layout |
|---|---|
| Desktop (>900px) | Centered card (~420px); coach marks point at sidebar/header elements |
| Mobile (≤900px) | Single column, full-width card; coach marks point downward at mobile-accessible targets (interests button, not a hidden sidebar widget) |

---

## 7. Acceptance criteria

- [ ] Landing shows exactly two paths: "Sign in with Bluesky" and "Create a Bytescape account" — no email/password on the landing, no Google/Mastodon/fediverse options.
- [ ] Bluesky OAuth uses DPoP + PKCE; callback completes ≤5s; token refresh works.
- [ ] Native signup requires a valid invite code; handle is checked for uniqueness; email verification gates activation.
- [ ] Returning users with a valid token skip landing, boot, and tour — straight to feed.
- [ ] TerminalBoot contains no `did:key`, AP, or P2P references (v2-corrected copy).
- [ ] Tour is 3 steps (interests → import → customize), skippable at every step, shows once (localStorage), survives refresh, never traps.
- [ ] Both user classes land on their own profile with the tour; the post-login experience is identical.
- [ ] Native signup creates a `did:web:{handle}.bytescape.social` identity (verified via `pds_accounts`).
- [ ] No dead auth endpoints (`/api/auth/signup` email-only, Google, Mastodon) are referenced by the client.
