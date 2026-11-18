# import/screen Specification

## Purpose
TBD - created by archiving change profile-import-rework. Update Purpose after archive.

## Requirements

### Requirement: R1 — Dedicated Import screen on left nav

A dedicated Import screen SHALL exist as a tab on the left navigation bar,
rendering the import flow (source selection, upload, progress, results) as a
full-screen tab content — not a modal.

#### Scenario: Import screen is reachable from left nav

- **Given** the user is logged in
- **When** the left sidebar renders
- **Then** an "Import" nav item is visible (not gated on onboarding state)
- **And** clicking it opens the Import screen

### Requirement: R2 — All import sources are equal

The Import screen SHALL treat all supported import sources (Facebook, Instagram,
Threads, Mastodon) as equal first-class options. No source SHALL be assumed to be
the user's origin. The user explicitly selects their import source via a source
picker at the start of the flow.

[SCOPE] The initially supported sources are the Meta archive platforms
(Facebook, Instagram, Threads) and Mastodon. Other platforms may be added later.

[TEST-ORDER] Import parsing is tested in priority order: Instagram, then
Facebook, then Threads, then Mastodon.

#### Scenario: Source picker is the first step

- **Given** the Import screen is open
- **When** the flow begins
- **Then** a source picker displays Facebook, Instagram, Threads, and Mastodon
  as equal options
- **And** no source is pre-selected or implied as default

#### Scenario: Per-source hand-holding guide

- **Given** the user selects an import source
- **When** the guide step renders
- **Then** step-by-step instructions are shown for obtaining the archive file
  from that specific platform
- **And** the instructions cover where to find the export, what settings to use,
  and what file to expect

### Requirement: R3 — Dynamic upload feedback

The upload process SHALL dynamically show the user: (a) files detected in the
archive as they are scanned, (b) how those files are being parsed (current parse
phase/activity), and (c) contacts and posts being saved or linked to the user's
account in real time.

#### Scenario: Archive scan shows detected files

- **Given** the user has uploaded an archive and processing has begun
- **When** the archive scan phase runs
- **Then** the screen displays the files detected in the archive (counts and
  types: JSON, HTML, media)

#### Scenario: Parse progress is visible

- **Given** the archive is being parsed
- **When** parsing proceeds through phases (posts, media, contacts)
- **Then** the current parse activity is displayed and updates dynamically

#### Scenario: Saved/linked records are shown

- **Given** posts and contacts are being ingested
- **When** records are saved or linked to the user's account
- **Then** the running counts of posts imported and contacts matched/linked
  are shown live on screen

### Requirement: R4 — Mastodon platform guide

The platform guides SHALL include a Mastodon entry alongside Facebook, Instagram,
and Threads, with steps for requesting the Mastodon GDPR archive.

#### Scenario: Mastodon guide is available

- **Given** the Import screen is open
- **When** the platform guide picker renders
- **Then** a "Mastodon" option is present with export steps
- **And** selecting it shows the Mastodon GDPR export instructions

### Requirement: R5 — Always-visible entry point

The Import nav item SHALL be visible to all logged-in users at all times (not
gated on `onboarding_complete` or any other flag), so returning users can import
additional archives.

#### Scenario: Returning user can access Import

- **Given** a user who has already completed onboarding
- **When** the left sidebar renders
- **Then** the Import nav item is visible and clickable
