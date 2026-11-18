# Database

**Schema source of truth: `bytescape-server/migrations/`** — when this page
and a migration disagree, the migration wins. Access modules live in
`bytescape-server/src/db/` (one file per table family). The server applies
migrations at startup.

**Live schema: 57 tables** (measured 2026-11-08; the count is here so drift is
visible — re-run `pg_tables` when in doubt).

> The migration runner's known skip-list on fresh databases is tracked
> internally; real migration tooling is a roadmap item (see `STATUS.md`).

## Table map

| Area | Tables |
|---|---|
| Users & identity | `users`, `linked_accounts`, `posting_identities`, `refresh_tokens`, `email_verifications`, `invite_codes` |
| Content | `posts`, `media`, `reactions`, `cached_posts`, `wall_posts` |
| Social graph | `follows`, `followers`, `subscriptions` |
| Import & migration | `import_sessions`, `imported_contacts`, `social_graph_matches`, `archive_identities`, `cross_posts` |
| Events | `calendar_events`, `calendar_rsvps`, `event_comments` |
| Forums & exchange | `forum_topics`, `forum_replies`, `exchange_listings`, `exchange_messages` |
| Chat | `chat_messages`, `chat_profiles`, `matrix_events`, `matrix_room_members`, `matrix_sync_tokens` |
| Notifications & moderation | `notifications`, `muted_accounts` |
| Feeds & sources | `feed_combos`, `feed_snapshots`, `rss_feed_items`, `published_feeds` |
| PDS internals | `pds_accounts`, `pds_blobs`, `pds_repo_roots`, `pds_ipld_blocks`, `pds_indexed_records`, `pds_firehose_seq`, `pds_consumer_offset` |
| OAuth | `oauth_sessions`, `oauth_auth_codes`, `oauth_par_requests`, `oauth_dpop_nonces` |
| AppView index | `appview_posts`, `appview_did_handle`, `appview_cursor`, `appview_post_tags`, `appview_status`, `appview_feed_settings`, `appview_topic_terms`, `appview_stories`, `appview_story_seen` |

## Storage model

- **Native posts, follows, likes** are AT Protocol records written to the
  user's PDS first (AD-01); the database rows are the queryable index. Our own
  writes insert into `pds_indexed_records` directly (read-after-write fast
  path), then the firehose consumer reconciles.
- **Events and RSVPs** are `community.lexicon.calendar.*` records on user
  PDSes; `calendar_events` / `calendar_rsvps` are their derived index, fed by
  a relay arm, a local-PDS arm, and write-through after our own writes.
  Comments and emoji reactions on events (`event_comments`) are app rows keyed
  by the record URI (they cascade on delete).
- **Imported archives and community extras** (forums, exchange, wall)
  currently live as app-database rows — migrating them to custom
  `com.bytescape.*` records is the documented architecture direction, not
  today's behavior.
- **Presentation state** (widget layout, themes, profile customization) is
  app-database by design (`users.widget_layout`, `users.widget_themes`,
  `users.theme`).
- **ActivityPub content** (rolling out) maps into the same `appview_*` tables
  the feed already reads — `https://` URIs distinguish AP rows from AT ones.
