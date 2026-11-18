# wiki/publishing Specification

## Purpose

Publishes the project's `wiki/` folder as a live site at `/wiki` with
auto-generated navigation, working cross-links, and a scheduled refresh from
the GitLab `main` branch.

## Requirements

### Requirement: Wiki is served at /wiki
The published wiki SHALL be served at `https://bytescape.social/wiki` and
`https://test.bytescape.social/wiki` from a dedicated document root, routed
ahead of the SPA fallback. The wiki route SHALL intercept only `/wiki` and
`/wiki/*`; no other route (app root, `/api`, `/oauth`, `/assets`) may change.

#### Scenario: Wiki home on production
- **WHEN** a visitor opens `https://bytescape.social/wiki`
- **THEN** the wiki home page loads (wiki title and sidebar), not the app shell

#### Scenario: Wiki home on test host
- **WHEN** a visitor opens `https://test.bytescape.social/wiki/`
- **THEN** the wiki home page loads

#### Scenario: Other routes unchanged
- **WHEN** a request targets the app root, `/api/*`, `/oauth/*`, or `/assets/*`
- **THEN** routing behaves exactly as before this change

#### Scenario: Unknown wiki path falls back to wiki entry
- **WHEN** a visitor opens `/wiki/<unknown-path>`
- **THEN** the wiki entry page loads, not the app shell

### Requirement: Navigation lists every published page
The build SHALL generate the navigation manifest from the publish tree, and the
wiki page SHALL render sidebar entries from that manifest, grouped by the
page's directory, so every published page is reachable from the sidebar.

#### Scenario: Sidebar covers all published pages
- **WHEN** a visitor loads the wiki
- **THEN** the sidebar lists every published page with its title, grouped by
  section derived from the page's directory

#### Scenario: New page appears without code changes
- **WHEN** a new markdown page is added under `wiki/` and republished
- **THEN** it appears in the sidebar with no edit to `index.html`

#### Scenario: Manifest failure is visible
- **WHEN** the navigation manifest cannot be loaded
- **THEN** the sidebar shows a visible error state rather than an empty or
  stale navigation

### Requirement: Pages are addressable by shareable URL
The wiki SHALL reflect the current page in the URL fragment so that copying the
address bar reopens the same page, and browser back/forward SHALL follow wiki
navigation.

#### Scenario: Direct open of a deep link
- **WHEN** a visitor opens `/wiki/#/getting-started/overview`
- **THEN** that page renders, not the home page

#### Scenario: Shared URL opens the same page
- **WHEN** a visitor shares the URL of a non-home page and it is opened
- **THEN** the same page renders for the recipient

#### Scenario: History navigation
- **WHEN** the browser back or forward buttons are used
- **THEN** the wiki navigates to the previous or next page in history

### Requirement: Content links navigate within the wiki
Clicking a relative markdown link in page content SHALL resolve the target
against the current page's location and render it in the wiki; references
that point above the wiki root SHALL clamp to the publish root. Absolute
`http(s)` links SHALL navigate normally.

#### Scenario: Link from home to a nested page
- **WHEN** a visitor clicks `getting-started/overview.md` from the home page
- **THEN** that page renders inside the wiki

#### Scenario: Relative link from a nested page
- **WHEN** a visitor clicks `../architecture/README.md` from a nested page
- **THEN** the target renders, resolved against the current page's directory

#### Scenario: Link above wiki root clamps to publish root
- **WHEN** a page links to `../VISION.md`
- **THEN** the published `VISION.md` content renders in the wiki

#### Scenario: External link leaves the wiki
- **WHEN** content contains an absolute `https://` link
- **THEN** the browser navigates to that URL normally

### Requirement: Referenced root documents are published
The four repository-root documents referenced by the wiki — `VISION.md`,
`STATUS.md`, `DECISIONS.md`, and `docs/README.md` — SHALL be published within
the wiki tree so those references resolve for visitors.

#### Scenario: Vision link resolves
- **WHEN** a visitor follows the `VISION.md` reference from the wiki home
- **THEN** the published VISION content renders

#### Scenario: Project guide link resolves
- **WHEN** a visitor follows `docs/README.md` from Getting Started
- **THEN** the published project guide renders

### Requirement: Only intended content is published
The publish set SHALL be the files under `wiki/` except editor/scratch files
(dotfiles, editor swap files), plus the four referenced root documents.
`wiki/product/code-audit.md` SHALL NOT be published or listed in navigation.

#### Scenario: Audit page is not served
- **WHEN** the published site is requested at `/wiki/product/code-audit.md`
- **THEN** the audit content is not served; the request falls back to the wiki
  entry page

#### Scenario: No scratch files on the published site
- **WHEN** the published tree is inspected on the server
- **THEN** no dotfile or editor swap file is present

#### Scenario: Audit page absent from navigation
- **WHEN** navigation is rendered
- **THEN** no entry for `code-audit` appears

### Requirement: Wiki refreshes on a schedule from GitLab
The published wiki SHALL be rebuilt from the `main` branch of the hosted
GitLab repository and republished automatically at least once daily, and SHALL
also republish on pushes to `main` that change the wiki or the publish
tooling. A failed build or transfer SHALL leave the previously published wiki
live and unchanged.

#### Scenario: Scheduled daily refresh
- **WHEN** the daily scheduled pipeline runs
- **THEN** the published site reflects the current `main`-branch wiki content

#### Scenario: Push-triggered refresh
- **WHEN** a push to `main` changes `wiki/**` or the publish tooling
- **THEN** a republish runs without waiting for the schedule

#### Scenario: Failure preserves the live wiki
- **WHEN** a build or transfer fails partway through
- **THEN** the previously published release remains live in full; no partial
  or half-written content is ever served

### Requirement: Published updates are visible without cache purge
Wiki responses SHALL carry cache directives that require revalidation, so a
completed publish is reflected on a visitor's next request without manual
cache invalidation.

#### Scenario: New content visible after publish
- **WHEN** a publish completes and a visitor reloads a wiki page
- **THEN** the response reflects the newly published content rather than a
  stale cached copy
