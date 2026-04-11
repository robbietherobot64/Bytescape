# Profile Custom Pages Architecture (HTML/CSS Native)

## Purpose

Define a simple profile customization model comparable to Neocities/SpaceHey style pages:
- users write native HTML/CSS,
- users add their own pages to the left navigation,
- users may set any custom page as their profile home.

This is intentionally smaller and simpler than a full sandboxed app runtime.

## Product Goal

Let users treat their profile as a personal mini-site without learning a complex builder:
- create multiple custom pages,
- style pages with custom CSS,
- link pages in left nav,
- choose default landing page (including replacing current Home).

## Core Scope

1. HTML/CSS page editor
- Per-page `title`, `slug`, `html`, `css`
- Live preview in profile context
- Save drafts and publish

2. Left navigation customization
- Users can add custom page links into profile left nav
- Users can reorder nav items
- Users can hide/show built-in nav entries

3. Home page override
- Setting: `default_profile_page_slug`
- If set, opening a profile routes to that custom page instead of built-in Home
- Fallback to built-in Home if selected page is deleted/unpublished

4. Static assets
- Upload images/GIFs for profile pages
- Reference via managed asset URLs
- Basic size/type limits

## Deliberate Non-Goals (for this phase)

1. No arbitrary JavaScript execution
2. No plugin/capability system
3. No iframe/network permission model beyond existing app policies
4. No script-level app API bridge

This keeps delivery fast and behavior predictable.

## Rendering and Safety Model

1. Render user HTML/CSS in an isolated profile content container
2. Strip or reject `<script>` tags and inline event handlers
3. Restrict dangerous URL schemes in links/assets
4. Keep sanitization rules simple and explicit

## Data Model (Minimum)

1. ProfileCustomPage
- id
- owner_did
- slug
- title
- html_source
- css_source
- is_published
- created_at
- updated_at

2. ProfileNavConfig
- owner_did
- nav_items (ordered list of built-in + custom page refs)

3. ProfileSettings
- owner_did
- default_profile_page_slug (nullable)

## UX Flow

1. User opens Customize Profile
2. User creates page (e.g., `about-me`)
3. User writes HTML/CSS and previews
4. User publishes page
5. User adds page to left nav and reorders
6. User optionally sets this page as default home

## PM Track Split Guidance

### Track A (Backend)
- storage commands for custom pages/nav config/home override
- sanitization/validation on save/publish
- asset metadata and limits

### Track B (Frontend)
- custom page editor UI (HTML/CSS tabs + preview)
- left nav manager (add/reorder/hide)
- profile route behavior using home override

### Shared acceptance gates
- published custom pages render reliably
- left nav reflects user order and links
- home override works and falls back safely
- blocked script content is not executed

## Suggested Delivery Phases

### Phase P1: Custom pages base
- page CRUD
- HTML/CSS editor and preview
- publish/unpublish

### Phase P2: Nav and home routing
- left nav custom links + reorder
- built-in nav visibility toggles
- default profile page override

### Phase P3: Polish
- asset picker for GIFs/images
- starter templates (SpaceHey/Neocities vibes)
- import/export page source JSON

## Definition of Success

1. Users can build profile pages directly with HTML/CSS.
2. Users can add those pages to left nav with their own ordering.
3. Users can replace profile Home with a chosen custom page.
4. The feature is stable, understandable, and does not require a heavy dev-mode system.
