# import/signup-integration Specification

## Purpose
TBD - created by archiving change profile-import-rework. Update Purpose after archive.

## Requirements

### Requirement: R1 — Signup routes to Import screen

After OAuth signup completes, the app SHALL route the user to the Import screen
(immediately), not to the Profile tab. The user can complete the import or skip
to the app.

#### Scenario: New user lands on Import screen

- **Given** a new user completes the Bluesky OAuth signup flow
- **When** the app mounts after `oauth_complete`
- **Then** the active tab is set to Import (not Profile)
- **And** the Import screen is displayed immediately

### Requirement: R2 — OnboardingTour import step removed

The `OnboardingTour`'s import step (which opened `ImportStep` as a modal) SHALL be
removed, replaced by the real Import screen. The `interests` and `customize` tour
steps MAY be kept as lightweight first-visit hints or dropped (decision D2).

#### Scenario: No import modal during tour

- **Given** a new user with `onboarding_complete='false'`
- **When** the Profile screen renders
- **Then** the `OnboardingTour` does NOT open an import modal
- **And** the Import screen is reachable via the left nav

### Requirement: R3 — Skip and complete behavior

The Import screen SHALL support "skip" (proceed to Feed without importing) and
"complete" (after successful import, proceed to Feed or Profile). Both paths
set `onboarding_complete='true'` so the signup routing does not repeat.

#### Scenario: Skip import

- **Given** a new user on the Import screen
- **When** the user clicks "Skip"
- **Then** the app navigates to the Feed tab
- **And** `onboarding_complete` is set to `'true'`
