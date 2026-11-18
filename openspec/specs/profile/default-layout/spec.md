# profile/default-layout Specification

## Purpose
TBD - created by archiving change profile-import-rework. Update Purpose after archive.

## Requirements

### Requirement: R1 — Simplified default profile layout

The default profile layout SHALL contain only the profile header widget and the
posts/feed widget. Other widget types SHALL remain available in the widget picker
(add-widget in edit mode) but SHALL NOT be present in `DEFAULT_LAYOUT`.

#### Scenario: New profile shows only header and posts

- **Given** a user with a fresh profile (no saved `widget_layout`)
- **When** the profile screen renders
- **Then** only the profile header widget and posts widget are displayed
- **And** the add-widget picker still offers other widget types in edit mode

### Requirement: R2 — Profile content fills available width

The profile content area SHALL use the full width available after the left sidebar,
not a fixed 600px column. The CSS grid SHALL be `260px minmax(0,1fr)` (left nav +
profile content), removing the right sidebar as a separate grid column.

#### Scenario: Profile content is not crushed

- **Given** a viewport wider than 1200px
- **When** the profile screen renders
- **Then** the profile content area occupies the full remaining width (not capped
  at 600px)
- **And** the header and posts widgets render at natural scale (not compressed)

### Requirement: R3 — Left sidebar unchanged

The left sidebar navigation SHALL remain as-is: static, highlighting the active tab
(including Profile when on the profile screen).

#### Scenario: Left sidebar highlights Profile

- **Given** the user navigates to the Profile tab
- **When** the left sidebar renders
- **Then** the Profile item is highlighted
- **And** other nav items render unchanged

### Requirement: R4 — Chat launcher on left sidebar

The Chat/Messages launcher SHALL be positioned in the left sidebar, immediately
above the Logout button, so it remains accessible while viewing the profile.

#### Scenario: Chat launcher is above Logout

- **Given** the app is rendered with the left sidebar visible
- **When** the sidebar renders
- **Then** the Chat/Messages launcher appears above the Logout button
- **And** clicking it opens the chat dock (behavior unchanged)
