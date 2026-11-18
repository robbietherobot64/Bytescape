const API = window.location.hostname === 'localhost' ? 'http://localhost:8080' : '';

let refreshPromise: Promise<string | null> | null = null;

// ── Session-expired state (expired-token refetch-loop fix) ─────────────────
// A failed refresh is TERMINAL for this tab: exactly one refresh attempt per
// expiry, then every api()/apiRaw() call short-circuits with the expired
// error and ZERO network requests fire. The old failure path cleared the
// token and assigned window.location.href — a silent reload that re-ran the
// whole startup chain (the ~2.7s refetch loop, the forever "assembling
// stream" loader, and no error anywhere).
export const SESSION_EXPIRED_MSG = 'Session expired — sign in again';
let sessionExpired = false;
let refreshAttempted = false;
type ExpiredListener = () => void;
const expiredListeners = new Set<ExpiredListener>();

export function isSessionExpired(): boolean {
  return sessionExpired;
}

export function subscribeSessionExpired(fn: ExpiredListener): () => void {
  expiredListeners.add(fn);
  return () => {
    expiredListeners.delete(fn);
  };
}

function markSessionExpired() {
  if (sessionExpired) return;
  sessionExpired = true;
  clearToken();
  for (const fn of [...expiredListeners]) {
    try {
      fn();
    } catch {
      // Listener errors never block the state change.
    }
  }
}

export async function refreshAccessToken(): Promise<string | null> {
  // Exactly ONE refresh attempt per expiry: after any failed attempt the
  // latch stays set until a real login (setTokens) resets it.
  if (refreshAttempted) return null;
  if (refreshPromise) return refreshPromise;
  refreshPromise = (async () => {
    try {
      const refresh = localStorage.getItem('bytescape_refresh_token');
      const hadSession = Boolean(localStorage.getItem('bytescape_token') || refresh);
      if (!refresh) {
        // No way to recover a session that existed → the honest terminal
        // state; a first-time visitor (no tokens at all) is not "expired".
        if (hadSession) {
          refreshAttempted = true;
          markSessionExpired();
        }
        return null;
      }
      const res = await fetch(`${API}/api/auth/refresh`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ refresh_token: refresh }),
      });
      if (!res.ok) {
        refreshAttempted = true;
        markSessionExpired();
        return null;
      }
      const data = await res.json();
      setTokens(data.access_token, data.refresh_token);
      return data.access_token;
    } catch {
      // The attempt failed — terminal for this tab (no silent loop), the
      // user gets the interstitial instead of a spinner.
      refreshAttempted = true;
      if (localStorage.getItem('bytescape_token') || localStorage.getItem('bytescape_refresh_token')) {
        markSessionExpired();
      }
      return null;
    } finally {
      refreshPromise = null;
    }
  })();
  return refreshPromise;
}

export async function api<T>(path: string, opts?: RequestInit): Promise<T> {
  // Terminal session state: zero network requests, one honest error.
  if (sessionExpired) throw new Error(SESSION_EXPIRED_MSG);
  const h: Record<string, string> = { 'Content-Type': 'application/json' };
  const t = localStorage.getItem('bytescape_token');
  if (t) h['Authorization'] = `Bearer ${t}`;
  let res = await fetch(`${API}${path}`, { ...opts, headers: { ...h, ...(opts?.headers as Record<string, string> || {}) } });

  // Auto-refresh on 401
  if (res.status === 401) {
    const newToken = await refreshAccessToken();
    if (newToken) {
      h['Authorization'] = `Bearer ${newToken}`;
      res = await fetch(`${API}${path}`, { ...opts, headers: { ...h, ...(opts?.headers as Record<string, string> || {}) } });
    }
  }

  if (!res.ok) {
    let msg = await res.text();
    try { const j = JSON.parse(msg); msg = j.message || j.error || msg; } catch {}
    throw new Error(msg);
  }
  if (res.headers.get('content-type')?.includes('application/json')) {
    return res.json();
  }
  return res.text() as any;
}

export async function apiRaw(path: string, opts?: RequestInit): Promise<Response> {
  if (sessionExpired) throw new Error(SESSION_EXPIRED_MSG);
  const h: Record<string, string> = {};
  const t = localStorage.getItem('bytescape_token');
  if (t) h['Authorization'] = `Bearer ${t}`;
  let res = await fetch(`${API}${path}`, { ...opts, headers: { ...h, ...(opts?.headers as Record<string, string> || {}) } });

  if (res.status === 401) {
    const newToken = await refreshAccessToken();
    if (newToken) {
      h['Authorization'] = `Bearer ${newToken}`;
      res = await fetch(`${API}${path}`, { ...opts, headers: { ...h, ...(opts?.headers as Record<string, string> || {}) } });
    }
  }

  return res;
}

export function setTokens(access: string, refresh: string) {
  // A real login/refresh success restores the session: clear the terminal
  // state and its latch so future expiry behaves normally again.
  sessionExpired = false;
  refreshAttempted = false;
  localStorage.setItem('bytescape_token', access);
  localStorage.setItem('bytescape_refresh_token', refresh);
}
export function getToken(): string | null { return localStorage.getItem('bytescape_token'); }
export function clearToken() {
  localStorage.removeItem('bytescape_token');
  localStorage.removeItem('bytescape_refresh_token');
}

export function userFromToken(): { username: string; sub: string } | null {
  try {
    const t = getToken(); if (!t) return null;
    return JSON.parse(atob(t.split('.')[1]));
  } catch { return null; }
}

export function currentUserHandle(): string | null {
  const u = userFromToken();
  return u?.username || null;
}

// ── Types ──
export interface NewPostData {
  content: string;
  visibility: 'public' | 'followers' | 'friends';
  platforms: string[];
  media_urls: string[];
}
export interface AuthorInfo {
  handle: string;
  display_name?: string;
  avatar?: string;
}

/** AP group (community) a post belongs to — the group post card header. */
export interface GroupRef {
  actor_id: string;
  name: string;
  title?: string;
  instance: string;
  kind: string;
  nsfw: boolean;
}

export interface MediaAttachment {
  url: string;
  thumbnail_url?: string;
  mime_type?: string;
  width?: number;
  height?: number;
  alt_text?: string;
}

export interface EmbedInfo {
  kind: string;
  url: string;
  title?: string;
  description?: string;
  thumbnail?: string;
}

export interface FeedPost {
  id: string;
  source: string;
  author: AuthorInfo;
  content: string;
  content_type: string;
  media: MediaAttachment[];
  embeds: EmbedInfo[];
  likes_count: number;
  shares_count: number;
  replies_count: number;
  created_at: string;
  url?: string;
  thread_id?: string;
  author_url?: string;
  reaction_counts?: Record<string, number>;
  user_reaction?: string;
  cid?: string;
  reposted_by?: string;
  boosted_by?: string;
  quoted_post_uri?: string;
  group_name?: string;
  group?: GroupRef;
  upvotes?: number;
  downvotes?: number;
  my_vote?: string | null;
  display_name?: string;
  action_text?: string;
  is_reshare?: boolean;
  original_post?: FeedPost;
  is_own?: boolean;
  is_following?: boolean;
  follow_pending?: boolean;
  cross_posts?: Array<{ platform: string; external_url: string; ok: boolean }>;
  matched_sources?: Array<{ label: string; kind: string; rank: number }>;
  labels?: string[];
  nsfw?: boolean;
  author_external_id?: string;
}

export interface FeedResponse {
  items: FeedPost[];
  cursor?: string;
  has_more: boolean;
  /** Platform mix for the request's feed scope (bucket key → count). */
  platform_counts?: Record<string, number>;
}

export interface Subscription {
  id: string;
  source: string;
  source_id: string;
  label: string;
  icon: string;
  category: string;
  enabled: boolean;
  pinned?: boolean;
}

export interface RssSubscription {
  id: string;
  source: string;
  source_id: string;
  label: string;
  icon: string;
  category: 'rss';
  enabled: boolean;
  url: string;
  lastFetchedAt?: string;
  fetchError?: string;
}

export interface ImportSession {
  id: string;
  source: string;
  status: string;
  totalPosts: number;
  importedPosts: number;
  totalMedia: number;
  importedMedia: number;
  errorMessage?: string;
  startedAt?: string;
  completedAt?: string;
}

export interface LinkedAccount {
  id: string;
  provider: string;
  external_username: string;
  created_at: string;
  following: number;
  followers: number;
  posts: number;
  /** Mastodon only: home-instance follows mirrored locally at link time. */
  imported_follows: number;
}

export interface SocialGraphMatch {
  user_id: string;
  display_name: string;
  handle: string;
  avatar: string | null;
  match_type: string;
  confidence: string;
  source: string;
}

export interface SocialGraphRssCandidate {
  contact_name: string;
  contact_handle: string | null;
  source: string;
  feed_url: string;
}

export interface SocialGraphUnmatched {
  contact_name: string;
  contact_handle: string | null;
  source: string;
}

export interface SocialGraphStats {
  total_contacts: number;
  matched_count: number;
  handle_matched: number;
  name_matched: number;
}

export interface SocialGraphResponse {
  stats: SocialGraphStats;
  matches: SocialGraphMatch[];
  rss_candidates: SocialGraphRssCandidate[];
  unmatched_with_handles: SocialGraphUnmatched[];
}

export interface MutualFriend {
  handle: string;
  display_name: string;
  avatar?: string;
}

export interface AccountStats {
  following: number;
  followers: number;
  posts: number;
}

export interface Widget {
  id: string;
  widget_type: string;
  x: number;
  y: number;
  w: number;
  h: number;
  data: Record<string, unknown>;
  theme_id: string;
}

export interface WidgetTheme {
  id: string;
  name: string;
  background?: string;
  border_color?: string;
  border_radius?: string;
  text_color?: string;
  font_family?: string;
  padding?: string;
  shadow?: string;
}

export interface WidgetData {
  [key: string]: unknown;
}

// ── Settings (preferences) — settings-account-ia ──
/** Three-state NSFW preference (clean cutover from the `show_nsfw` boolean). */
export type NsfwPref = 'exclude' | 'blur' | 'show';

/** App-chrome theme roster (app-wide, independent from profile themes).
 *  Terminal merged into Default 2026-11-16 — the stored 'terminal' value is
 *  legacy-mapped to 'default' on read. */
export type AppThemeId =
  | 'default'
  | '1999'
  | 'xp'
  | 'retrofuturistic'
  | 'light'
  | 'dark';

/** Feed default-sort + source-mix preference (`users.feed_prefs`). */
export type FeedSort = 'new' | 'popular';
export type SourceMix = 'all' | 'bluesky' | 'mastodon';

/** `GET /api/settings` blob. */
export interface SettingsState {
  theme: AppThemeId;
  nsfw_pref: NsfwPref;
  interests: string[];
  sort: FeedSort;
  source_mix: SourceMix;
  manually_approves_followers: boolean;
}

// ── Notifications ──

/** Notification type vocabulary (FR-25 + follow_request). */
export type NotificationType =
  | 'like'
  | 'repost'
  | 'follow'
  | 'follow_request'
  | 'reply'
  | 'mention'
  | 'profile_comment';

export type NotificationNetwork = 'activitypub' | 'atproto' | 'local';

/** One notification row (backend `notifications` table). */
export interface NotificationItem {
  id: string;
  notification_type: NotificationType | string;
  title: string;
  body?: string | null;
  data?: Record<string, unknown> | null;
  read: boolean;
  created_at: string;
  dedupe_key?: string | null;
  actor_id?: string | null;
  network: NotificationNetwork | string;
}

export interface NotificationListResponse {
  notifications: NotificationItem[];
}

/** A pending follow request (locked accounts). */
export interface FollowRequest {
  id: string;
  fan_out: string;
  user_id: string;
  inbox_url?: string | null;
  follow_activity_id?: string | null;
  network: NotificationNetwork | string;
  decision?: string | null;
  created_at: string;
}

/** `PATCH /api/settings/{section}` response: the updated settings blob. */
export interface SettingsUpdateResponse {
  ok: boolean;
  settings: Partial<SettingsState>;
}

export interface ProfileData {
  id: string;
  username: string;
  display_name: string;
  bio: string;
  avatar_url: string | null;
  header_url: string | null;
  theme: string;
  custom_css: string;
  created_at: string;
  posts_count: number;
  followers_count: number;
  following_count: number;
  is_own_profile: boolean;
  is_following: boolean;
  /** The viewer's follow toward this profile is a pending (requested) follow. */
  follow_pending: boolean;
  location: string | null;
  website: string | null;
  pronouns: string | null;
  relationship_status: string | null;
  birthday: string | null;
  occupation: string | null;
  interests: string[];
  onboarded: boolean;
  /** Server now serves `nsfw_pref` (three-state); the boolean is gone (clean cutover). */
  nsfw_pref: NsfwPref;
  custom_profile_fields?: Array<{ field_key: string; label?: string; content?: string; visible: boolean }>;
  widget_layout: Widget[];
  widget_themes: WidgetTheme[];
}

export interface UpdateProfileRequest {
  display_name?: string;
  bio?: string;
  theme?: string;
  custom_css?: string;
  avatar_url?: string;
  header_url?: string;
  location?: string | null;
  website?: string | null;
  pronouns?: string | null;
  relationship_status?: string | null;
  birthday?: string | null;
  occupation?: string | null;
  interests?: string[];
  nsfw_pref?: NsfwPref;
  custom_profile_fields?: { field_key: string; label?: string; content?: string; visible: boolean }[];
  widget_layout?: Widget[];
  widget_themes?: WidgetTheme[];
}

export interface WallPost {
  id: string;
  author: AuthorInfo;
  content: string;
  created_at: string;
}

export interface FollowingView {
  username: string;
  handle: string;
  display_name?: string;
  avatar?: string;
  is_local: boolean;
  /** Which network the relationship lives on (merged graph provenance). */
  network: string;
  /** Which of the profile owner's identities produced this row. */
  identity_used: string;
  /** The viewer's follow state for this row. */
  following: boolean;
  follow_pending: boolean;
  /** Remove-follower affordance (own profile + network permits). */
  can_remove: boolean;
}

/** The merged graph response (identity-graph Phase 6 visibility disclosures). */
export interface MergedGraphResponse {
  rows: FollowingView[];
  /** The profile's follow lists are hidden (private graph, not the owner). */
  hidden: boolean;
  /** The home instance hides its collections (hide_collections). */
  home_graph_hidden: boolean;
  /** The managing home instance for a Mastodon-linked account. */
  home_instance: string | null;
}

/** Visibility settings: one surface over the three networks (Phase 6). */
export interface VisibilitySettings {
  graph_visibility: string;
  home: {
    linked: boolean;
    instance: string | null;
    hide_collections: boolean | null;
    settings_url: string | null;
    can_proxy: boolean;
  };
  bluesky: { linked: boolean; caveat: string };
  caveat: string;
}

export interface FriendView {
  username: string;
  handle: string;
  display_name?: string;
  avatar?: string;
}

// ── Feed API ──
export const feedApi = {
  aggregated: () => api<FeedResponse>('/api/feed'),
  bluesky: () => api<FeedResponse>('/api/feed/bluesky'),
  hashtag: (tag: string) => api<FeedResponse>(`/api/feed/hashtag?tag=${encodeURIComponent(tag)}`),
  search: (q: string) => api<FeedResponse>(`/api/feed/search?q=${encodeURIComponent(q)}`),
  discover: () => api<FeedResponse>('/api/discover'),
  interests: (ids: string[]) => api<FeedResponse>(`/api/feed/interests?ids=${ids.map(encodeURIComponent).join(',')}`),
  combined: (interestIds: string[], cursor?: string, phase?: 'seed' | 'full', since?: string, sort?: string, author?: string, sourceCategories?: string, types?: string, sourceMix?: string, platform?: string, windowPreset?: string, windowFrom?: string, windowTo?: string) => {
    const params = new URLSearchParams();
    if (interestIds.length > 0) params.set('ids', interestIds.join(','));
    if (cursor) params.set('cursor', cursor);
    if (phase) params.set('phase', phase);
    if (since) params.set('since', since);
    if (sort) params.set('sort', sort);
    if (author) params.set('author', author);
    if (sourceCategories) params.set('source_categories', sourceCategories);
    if (types) params.set('types', types);
    // Saved source-mix preference (Settings → Feed & sources). Omitted when
    // 'all' so the request stays unscoped and the backend default applies.
    if (sourceMix && sourceMix !== 'all') params.set('source_mix', sourceMix);
    // Platforms filter bucket. Omitted for 'all' — the backend treats an
    // absent param as the unfiltered feed.
    if (platform && platform !== 'all') params.set('platform', platform);
    // Time-period filter. Omitted for the neutral 'all'; a custom range sends
    // its endpoints (either may be empty = open-ended).
    if (windowPreset && windowPreset !== 'all') {
      params.set('window', windowPreset);
      if (windowPreset === 'custom') {
        if (windowFrom) params.set('from', windowFrom);
        if (windowTo) params.set('to', windowTo);
      }
    }
    // Cache-busting: unique timestamp prevents browser from serving stale response
    params.set('_t', Date.now().toString());
    return api<FeedResponse>(`/api/feed/combined?${params.toString()}`, { cache: 'no-store' });
  },
  sources: (sources: string, cursor?: string) =>
    api<FeedResponse>(
      `/api/feed/sources?sources=${encodeURIComponent(sources)}${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`
    ),
  searchPeople: (q: string) => {
    // Bluesky actors only (AD-00: no fediverse API calls)
    return fetch(`https://api.bsky.app/xrpc/app.bsky.actor.searchActors?q=${encodeURIComponent(q)}&limit=10`)
      .then(r => r.json()).catch(() => ({ actors: [] }))
      .then((bsky: any) => (bsky.actors || []).map((a: any) => ({
        label: a.handle,
        source_id: a.did,
        description: a.description,
        avatar: a.avatar,
      })));
  },
  searchHashtags: (q: string) => {
    // Local AppView check only (AD-00: no fediverse API calls)
    return fetch(`${API}/api/feed/sources?sources=hashtag:${encodeURIComponent(q)}&limit=1`)
      .then(r => r.json()).catch(() => ({ items: [] }))
      .then((appview: any) => {
        const appviewCount = (appview.items || []).length;
        if (appviewCount > 0) {
          return [{
            label: `#${q}`,
            source_id: q,
            description: `${appviewCount}+ posts (Bluesky)`,
            platform: 'bluesky',
          }];
        }
        return [];
      });
  },
  searchFeeds: (q: string) => {
    // Search Bluesky feed generators by keyword + local AppView posts
    return Promise.all([
      fetch(`https://api.bsky.app/xrpc/app.bsky.unspecced.getPopularFeedGenerators?limit=25`)
        .then(r => r.json()).catch(() => ({ feeds: [] })),
      fetch(`${API}/api/feed/search?q=${encodeURIComponent(q)}&limit=5`)
        .then(r => r.json()).catch(() => ({ items: [] })),
    ]).then(([popular, appview]) => {
      // Filter popular feeds by keyword match on displayName or description
      const lowerQ = q.toLowerCase();
      const feedResults = (popular.feeds || [])
        .filter((f: any) => {
          const name = (f.displayName || '').toLowerCase();
          const desc = (f.description || '').toLowerCase();
          return name.includes(lowerQ) || desc.includes(lowerQ);
        })
        .map((f: any) => ({
          label: f.displayName,
          source_id: f.uri,
          description: f.description,
          avatar: f.avatar,
        }));
      // Add unique hashtag search terms from AppView posts
      const hashtagResult = {
        label: `#${q}`,
        source_id: q,
        description: `${(appview.items || []).length}+ posts on Bluesky`,
        platform: 'bluesky',
      };
      return [hashtagResult, ...feedResults];
    });
  },
  reply: (postId: string, source: string, content: string, parentReplyId?: string) =>
    api<any>('/api/feed/comments', {
      method: 'POST',
      body: JSON.stringify({ post_id: postId, source, content, parent_reply_id: parentReplyId }),
    }),
  searchGroups: (q: string) => {
    // Local group registry (ap_groups) — Lemmy/PieFed communities ingested
    // over AP. Returns the SourceSearchResult shape so the shared search
    // dropdown renders them like any other source.
    return api<GroupView[]>(`/api/groups?q=${encodeURIComponent(q)}&limit=6`)
      .then((groups) =>
        groups.map((g) => ({
          label: g.title || g.name,
          source_id: `groups:${g.actor_id}`,
          description: `${g.name} · ${g.instance} · ${g.post_count} posts`,
          source_type: 'community',
          instance: g.instance,
          uri: g.actor_id,
        })),
      )
      .catch(() => []);
  },
  // Group-variant feed: one parameterized query over group-linked posts,
  // scoped by the Groups view's tab (home/following/feeds/hashtags/subscriptions).
  groupScope: (scope: string, limit = 50) =>
    api<FeedResponse>(`/api/feed/groups?scope=${encodeURIComponent(scope)}&limit=${limit}`),
};

// ── Groups (Lemmy/PieFed communities over ActivityPub) ──
export interface GroupView {
  actor_id: string;
  name: string;
  title?: string;
  description?: string;
  instance: string;
  kind: string;
  nsfw: boolean;
  post_count: number;
  following: boolean;
}

export interface GroupPostCounts {
  upvotes: number;
  downvotes: number;
  comments: number;
  /** upvotes ÷ total, 2 decimals; null when no votes (renders "—"). */
  ratio: number | null;
  /** The caller's own standing federated vote: 'up' | 'down' | null. */
  my_vote: string | null;
}

export interface GroupSubscriptionView {
  actor_id: string;
  name: string;
  title?: string;
  instance: string;
  kind: string;
  label: string;
}

export interface FollowResp {
  following: boolean;
  /** True when a federated AP Follow was queued; false = local-only follow. */
  federated: boolean;
  actor_id: string;
}

export interface VoteResp {
  upvotes: number;
  downvotes: number;
  ratio: number | null;
  my_vote: string | null;
  federated: boolean;
}

export const groupsApi = {
  list: (params: { q?: string; instance?: string; kind?: string; limit?: number } = {}) => {
    const q = new URLSearchParams();
    if (params.q) q.set('q', params.q);
    if (params.instance) q.set('instance', params.instance);
    if (params.kind) q.set('kind', params.kind);
    if (params.limit) q.set('limit', String(params.limit));
    const qs = q.toString();
    return api<GroupView[]>(`/api/groups${qs ? `?${qs}` : ''}`);
  },
  get: (actorId: string) =>
    api<GroupView>(`/api/groups/detail?actor_id=${encodeURIComponent(actorId)}`),
  posts: (actorId: string, limit = 30) =>
    api<FeedResponse>(`/api/groups/posts?actor_id=${encodeURIComponent(actorId)}&limit=${limit}`),
  follow: (actorId: string, label?: string) =>
    api<FollowResp>('/api/groups/follow', {
      method: 'POST',
      body: JSON.stringify({ actor_id: actorId, label }),
    }),
  unfollow: (actorId: string) =>
    api<FollowResp>(`/api/groups/follow?actor_id=${encodeURIComponent(actorId)}`, { method: 'DELETE' }),
  subscriptions: () => api<GroupSubscriptionView[]>('/api/groups/subscriptions'),
  vote: (postUri: string, vote: 'up' | 'down' | 'clear') =>
    api<VoteResp>('/api/groups/vote', {
      method: 'POST',
      body: JSON.stringify({ post_uri: postUri, vote }),
    }),
};

// ── OAuth API ──
export const oauthApi = {
  blueskyAuthorize: () => api<{ url: string }>('/api/auth/bluesky/authorize'),
  blueskyCheck: () => api<{ linked: boolean; accounts: LinkedAccount[] }>('/api/oauth/bluesky/check'),
  blueskySignup: (inviteCode?: string) =>
    api<{ url: string }>(
      `/api/auth/oauth-signup/bluesky${inviteCode ? `?invite_code=${encodeURIComponent(inviteCode)}` : ''}`,
      { method: 'POST' },
    ),
  completeSignup: (tempToken: string, username: string, publicKey: string, inviteCode?: string) =>
    api<{ access_token: string; refresh_token: string }>('/api/auth/oauth-complete', {
      method: 'POST',
      body: JSON.stringify({ temp_token: tempToken, username, public_key: publicKey, invite_code: inviteCode }),
    }),
  // Mastodon / fediverse OAuth — instance-agnostic: the user supplies their
  // instance host (or a full @user@instance handle). Returns the authorize URL.
  mastodonStart: (instance: string, inviteCode?: string, usernameHint?: string) =>
    api<{ url: string; state: string }>('/api/auth/mastodon/start', {
      method: 'POST',
      body: JSON.stringify({ instance, invite_code: inviteCode ?? '', username_hint: usernameHint ?? '' }),
    }),
};

// ── Email auth (the migration-crowd primary path) ──
export interface EmailSignupRequest {
  username: string;
  email: string;
  password: string;
  public_key: string;
  invite_code?: string;
}

export const authApi = {
  // Log in with an existing email + password account.
  login: (email: string, password: string) =>
    api<{ access_token: string; refresh_token: string; user_id: string; username: string }>(
      '/api/auth/login',
      { method: 'POST', body: JSON.stringify({ email, password }) },
    ),
  // Create a new email + password account. `publicKeyHex` comes from
  // generateSigningKey(password). May return a pending state (email code).
  signup: (req: EmailSignupRequest) =>
    api<{ status: string; access_token?: string; refresh_token?: string; username?: string; code?: string }>(
      '/api/auth/signup',
      { method: 'POST', body: JSON.stringify(req) },
    ),
};

// ── Error Display Helper ──
const ERROR_MESSAGES: Record<string, string> = {
  ACCOUNT_DELETED: 'This account has been deleted.',
  ACCOUNT_PENDING: 'This account hasn\'t been verified yet.',
  TOO_MANY_ATTEMPTS: 'Too many attempts. Please try again later.',
  ACCOUNT_ALREADY_EXISTS: 'An account with this email already exists. Please log in.',
  BLUESKY_TOKEN_EXPIRED: 'Your Bluesky session has expired. Please try again.',
  INVALID_CREDENTIALS: 'Invalid email or password.',
};

/** Single toast contract for interaction failures (one user-facing literal). */
export function interactionFailedToast(): void {
  window.dispatchEvent(new CustomEvent('bytescape:toast', {
    detail: { type: 'error', message: 'Interaction failed — the action was not saved' },
  }));
}

export function apiErrorDisplay(error: unknown): string {
  if (typeof error === 'string') {
    const known = ERROR_MESSAGES[error];
    if (known) return known;
    return error;
  }
  if (error && typeof error === 'object' && 'message' in error) {
    return (error as any).message || 'An error occurred';
  }
  return 'An error occurred';
}

// ── Account API ──
export const accountApi = {
  list: () => api<LinkedAccount[]>('/api/accounts'),
  unlink: (id: string) => apiRaw(`/api/accounts/${id}`, { method: 'DELETE' }),
  stats: () => api<AccountStats>('/api/accounts/stats'),
  refreshFollows: () => api<{ count: number }>('/api/accounts/refresh-follows', { method: 'POST' }),
  deleteAccount: (immediate: boolean) =>
    api<{ status: string; deleted_at?: string }>('/api/account/delete', {
      method: 'POST',
      body: JSON.stringify({ immediate }),
    }),
  restoreAccount: () =>
    api<{ status: string }>('/api/account/restore', { method: 'POST' }),
  exportData: async () => {
    const token = getToken();
    const resp = await fetch(`${API}/api/account/export`, {
      headers: { Authorization: `Bearer ${token}` },
    });
    if (!resp.ok) throw new Error('Export failed');
    const blob = await resp.blob();
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = resp.headers.get('content-disposition')?.match(/filename="(.+)"/)?.[1] || 'bytescape-export.zip';
    document.body.appendChild(a);
    a.click();
    a.remove();
    window.URL.revokeObjectURL(url);
  },
};

// ── Settings API (section-scoped preference saves) ──

/** App-chrome theme roster in selector order (app-themes spec).
 *  Terminal merged into Default 2026-11-16 — 'terminal' is legacy-mapped. */
export const APP_THEMES: Array<{ id: AppThemeId; label: string; swatch: string[] }> = [
  { id: 'default', label: 'Default', swatch: ['#050505', '#33ff66', '#0d0d0d'] },
  { id: '1999', label: '1999', swatch: ['#3a6ea5', '#c0c0c0', '#000080'] },
  { id: 'xp', label: 'XP', swatch: ['#f0f0f0', '#245edc', '#e4eff9'] },
  { id: 'retrofuturistic', label: 'Retrofuturistic', swatch: ['#0d0f12', '#ffb000', '#16191d'] },
  { id: 'light', label: 'Light', swatch: ['#f5f5f7', '#3478f6', '#ffffff'] },
  { id: 'dark', label: 'Dark', swatch: ['#1c1c1e', '#0a84ff', '#2c2c2e'] },
];

/** Per-theme shipped effect defaults (what the preset applies when the user
 *  has NOT overridden it). Retrofuturistic + Default ship scanlines + glow ON
 *  (their designs); XP/Light/Dark ship card elevation ON; the rest OFF.
 *  Used by the Customize panel so the controls display the EFFECTIVE state,
 *  not a hardcoded "Off". */
export const PRESET_FX: Record<AppThemeId, { scanlines: boolean; glow: boolean; shadow: boolean }> = {
  default: { scanlines: true, glow: true, shadow: false },
  '1999': { scanlines: false, glow: false, shadow: false },
  xp: { scanlines: false, glow: false, shadow: true },
  retrofuturistic: { scanlines: true, glow: true, shadow: false },
  light: { scanlines: false, glow: false, shadow: true },
  dark: { scanlines: false, glow: false, shadow: true },
};

/** Per-theme metric defaults: card border width (px) and corner radius (px)
 *  as shipped. The panel's sliders show these until the user overrides. */
export const PRESET_METRICS: Record<AppThemeId, { lineWidth: number; radius: number }> = {
  default: { lineWidth: 2, radius: 0 },
  '1999': { lineWidth: 2, radius: 0 },
  xp: { lineWidth: 1, radius: 7 },
  retrofuturistic: { lineWidth: 2, radius: 0 },
  light: { lineWidth: 1, radius: 16 },
  dark: { lineWidth: 1, radius: 16 },
};

/** Font families offered by the font control. `comp` is the per-font size
 *  normalization factor so rendered body text matches default metrics. */
export interface FontOption {
  value: string;
  label: string;
  stack: string;
  comp: number;
}

export const FONT_OPTIONS: FontOption[] = [
  { value: 'system', label: 'System UI', stack: "system-ui, -apple-system, 'Segoe UI', sans-serif", comp: 1 },
  { value: 'vt323', label: 'VT323 (terminal)', stack: "'VT323', 'Courier New', monospace", comp: 0.69 },
  { value: 'press-start', label: 'Press Start 2P (pixel)', stack: "'Press Start 2P', monospace", comp: 0.6 },
  { value: 'share-tech', label: 'Share Tech Mono', stack: "'Share Tech Mono', 'Courier New', monospace", comp: 1 },
  { value: 'tahoma', label: 'Tahoma', stack: "'Tahoma', 'Segoe UI', sans-serif", comp: 1 },
  { value: 'verdana', label: 'Verdana', stack: "'Verdana', 'Segoe UI', sans-serif", comp: 1 },
  { value: 'inter', label: 'Inter', stack: "Inter, 'Segoe UI', sans-serif", comp: 1 },
  { value: 'consolas', label: 'Consolas', stack: "'Consolas', 'Courier New', monospace", comp: 1 },
];

/** Match a CSS font-family stack to a FONT_OPTIONS key by first family name. */
function matchFontOptionKey(stack: string): string {
  const first = (stack.split(',')[0] || '').replace(/["']/g, '').trim().toLowerCase();
  const hit = FONT_OPTIONS.find((f) => f.stack.split(',')[0].replace(/["']/g, '').trim().toLowerCase() === first);
  return hit ? hit.value : 'system';
}

/**
 * The active preset's effective visual values, read from the LIVE computed
 * CSS custom properties on the `.app` element (which carries the
 * `app-theme-<id>` class). Colors are normalized to #rrggbb for
 * <input type=color>; fontFamily is a FONT_OPTIONS key.
 */
export interface PresetEffectiveValues {
  textColor: string;
  textSecondary: string;
  textMuted: string;
  menuColor: string;
  menuHoverColor: string;
  onAccentColor: string;
  accent: string;
  accentDim: string;
  dangerColor: string;
  bgColor: string;
  panelBg: string;
  elevatedBg: string;
  surfaceBg: string;
  borderColor: string;
  borderDim: string;
  fontFamily: string;
}

export function readPresetEffectiveValues(): PresetEffectiveValues {
  const app = document.querySelector('.app') as HTMLElement | null;
  const el = app ?? document.documentElement;
  const cs = getComputedStyle(el);
  const val = (name: string) => cs.getPropertyValue(name).trim();
  const toHex = (c: string) => {
    if (!c) return '#000000';
    if (c.startsWith('#')) return c.slice(0, 7);
    const m = c.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)/);
    if (!m) return '#000000';
    return '#' + [Number(m[1]), Number(m[2]), Number(m[3])].map((n) => n.toString(16).padStart(2, '0')).join('');
  };
  return {
    textColor: toHex(val('--text-primary') || val('--text')),
    textSecondary: toHex(val('--text-secondary')),
    textMuted: toHex(val('--text-muted')),
    menuColor: toHex(val('--nav-color')),
    menuHoverColor: toHex(val('--nav-color-hover')),
    onAccentColor: toHex(val('--nav-on-accent')),
    accent: toHex(val('--accent')),
    accentDim: toHex(val('--accent-dim')),
    dangerColor: toHex(val('--danger')),
    bgColor: toHex(val('--bg-primary') || val('--bg')),
    panelBg: toHex(val('--bg-secondary')),
    elevatedBg: toHex(val('--bg-tertiary') || val('--surface-secondary')),
    surfaceBg: toHex(val('--surface') || val('--card-surface')),
    borderColor: toHex(val('--border-color')),
    borderDim: toHex(val('--border-dim')),
    fontFamily: matchFontOptionKey(val('--font-ui') || val('--font-mono') || cs.fontFamily),
  };
}

/** Legacy theme id → current roster id (Terminal merged into Default). */
const LEGACY_THEME_MAP: Record<string, AppThemeId> = { terminal: 'default' };

/** Roster membership + legacy mapping; unknown values fall back to Default. */
export function normalizeAppTheme(value: unknown): AppThemeId {
  const v = typeof value === 'string' ? LEGACY_THEME_MAP[value] ?? value : value;
  return (APP_THEMES.some((t) => t.id === v) ? v : 'default') as AppThemeId;
}

/**
 * Apply an app-chrome theme to the app root via `app-theme-<id>` (the
 * app-themes spec's root-level class layer). Call with the stored value on
 * boot and after every Appearance save.
 */
export function applyAppTheme(theme: unknown): AppThemeId {
  const id = normalizeAppTheme(theme);
  const root = document.querySelector('.app') as HTMLElement | null;
  const el = root ?? document.body;
  for (const t of APP_THEMES) el.classList.remove(`app-theme-${t.id}`);
  // Legacy 'terminal' class kept for any stale markup; harmless.
  el.classList.remove('app-theme-terminal');
  el.classList.add(`app-theme-${id}`);
  return id;
}

// ── Theme customization (user overrides layered on a preset) ──
// Persisted in localStorage (no backend field exists for freeform visual
// overrides; the settings PATCH API has no JSON blob for this). Tradeoff:
// overrides do NOT sync across devices — documented in the panel UI.

export interface ThemeOverrides {
  fontScale?: number; // 0.8–1.5
  fontFamily?: string; // FONT_OPTIONS key
  textColor?: string; // CSS color (body text)
  textSecondary?: string;
  textMuted?: string;
  menuColor?: string; // sidebar/menu/nav text (the "4th color")
  menuHoverColor?: string; // nav hover text
  onAccentColor?: string; // text sitting on accent backgrounds
  accent?: string;
  accentDim?: string;
  dangerColor?: string;
  bgColor?: string; // page background
  panelBg?: string;
  elevatedBg?: string;
  surfaceBg?: string; // card surface
  borderColor?: string;
  borderDim?: string;
  scanlines?: boolean;
  glow?: boolean;
  shadow?: boolean; // card elevation
  lineWidth?: number; // px, 0–4
  radius?: number; // px, 0–24
  bgImage?: string; // data-URL or url(...)
}

// ── Theme token registry ──
// EVERY property a theme preset touches is a registry entry, and the
// Customize panel is GENERATED from this list — no theme-controlled property
// exists without a control. Each entry maps a ThemeOverrides key to:
//   vars     — CSS custom properties written INLINE on html + .app (the
//              mechanism that made accent work: inline beats the preset class)
//   cssProps — direct CSS properties written INLINE (for what presets set as
//              element properties, e.g. color/background-color/font-family)
//   gateClass / onClass / offClass — class flags rendered in the React
//              className (for radius/line gates and the FX toggles)

export type ThemeTokenKind = 'color' | 'font' | 'slider' | 'toggle' | 'file';
export type ThemeTokenGroup = 'colors' | 'type' | 'metrics' | 'effects' | 'image';

export interface ThemeTokenSpec {
  key: keyof ThemeOverrides;
  label: string;
  kind: ThemeTokenKind;
  group: ThemeTokenGroup;
  vars?: string[];
  cssProps?: string[];
  gateClass?: string;
  onClass?: string;
  offClass?: string;
  min?: number;
  max?: number;
  step?: number;
  unit?: string;
  scale?: number; // slider display multiplier (fontScale 1 → 100)
}

// Preset defaults are resolved PER ACTIVE THEME (every preset ships its own
// palette) via readPresetEffectiveValues (colors/font), PRESET_METRICS
// (sliders) and PRESET_FX (toggles) — not stored here.
export const THEME_TOKEN_REGISTRY: ThemeTokenSpec[] = [
  // ── Colors: every palette token a preset sets ──
  { key: 'textColor', label: 'Text color', kind: 'color', group: 'colors', vars: ['--text-primary', '--text'], cssProps: ['color'] },
  { key: 'textSecondary', label: 'Secondary text', kind: 'color', group: 'colors', vars: ['--text-secondary'] },
  { key: 'textMuted', label: 'Muted text', kind: 'color', group: 'colors', vars: ['--text-muted'] },
  { key: 'menuColor', label: 'Menu / sidebar text', kind: 'color', group: 'colors', vars: ['--nav-color'] },
  { key: 'menuHoverColor', label: 'Menu hover text', kind: 'color', group: 'colors', vars: ['--nav-color-hover'] },
  { key: 'onAccentColor', label: 'Text on accent', kind: 'color', group: 'colors', vars: ['--nav-on-accent'] },
  { key: 'accent', label: 'Accent', kind: 'color', group: 'colors', vars: ['--accent', '--nav-color-selected'] },
  { key: 'accentDim', label: 'Accent (dim)', kind: 'color', group: 'colors', vars: ['--accent-dim'] },
  { key: 'dangerColor', label: 'Danger color', kind: 'color', group: 'colors', vars: ['--danger'] },
  { key: 'bgColor', label: 'Page background', kind: 'color', group: 'colors', vars: ['--bg-primary', '--bg'], cssProps: ['background-color'] },
  { key: 'panelBg', label: 'Panel background', kind: 'color', group: 'colors', vars: ['--bg-secondary'] },
  { key: 'elevatedBg', label: 'Elevated background', kind: 'color', group: 'colors', vars: ['--bg-tertiary', '--surface-secondary'] },
  { key: 'surfaceBg', label: 'Card surface', kind: 'color', group: 'colors', vars: ['--surface', '--card-surface'] },
  { key: 'borderColor', label: 'Border color', kind: 'color', group: 'colors', vars: ['--border-color', '--card-border-color'] },
  { key: 'borderDim', label: 'Border (dim)', kind: 'color', group: 'colors', vars: ['--border-dim'] },
  // ── Type ──
  { key: 'fontFamily', label: 'Font family', kind: 'font', group: 'type', vars: ['--font-ui', '--font-mono', '--font-retro'], cssProps: ['font-family'] },
  // ── Metrics ──
  { key: 'fontScale', label: 'Font size', kind: 'slider', group: 'metrics', vars: ['--font-scale'], min: 80, max: 150, step: 5, unit: '%', scale: 100 },
  { key: 'lineWidth', label: 'Line thickness', kind: 'slider', group: 'metrics', vars: ['--line-width'], gateClass: 'app-user-line', min: 0, max: 4, step: 1, unit: 'px' },
  { key: 'radius', label: 'Corner curve', kind: 'slider', group: 'metrics', vars: ['--user-radius-value'], gateClass: 'app-user-radius', min: 0, max: 24, step: 1, unit: 'px' },
  // ── Effects ──
  { key: 'scanlines', label: 'Scanlines', kind: 'toggle', group: 'effects', onClass: 'app-fx-scanlines', offClass: 'app-no-scanlines' },
  { key: 'glow', label: 'Text glow', kind: 'toggle', group: 'effects', onClass: 'app-fx-glow', offClass: 'app-no-glow' },
  { key: 'shadow', label: 'Card elevation', kind: 'toggle', group: 'effects', onClass: 'app-fx-shadow', offClass: 'app-no-shadow' },
  // ── Background image ──
  { key: 'bgImage', label: 'Background image', kind: 'file', group: 'image', vars: ['--user-bg-image'] },
];

const OVERRIDES_KEY = 'bytescape_theme_overrides';

/** Validate a parsed blob into a clean ThemeOverrides (drop wrong-typed keys,
 *  clamp sliders, reject unknown font keys). Registry-driven: every known
 *  token is validated by its kind. */
function sanitizeOverrides(raw: unknown): ThemeOverrides {
  const o = (raw && typeof raw === 'object' ? raw : {}) as Record<string, unknown>;
  const out: ThemeOverrides = {};
  for (const spec of THEME_TOKEN_REGISTRY) {
    const v = o[spec.key as string];
    if (v === undefined || v === null) continue;
    if (spec.kind === 'color' && typeof v === 'string') {
      (out as Record<string, unknown>)[spec.key as string] = v;
    } else if (spec.kind === 'file' && typeof v === 'string') {
      out.bgImage = v;
    } else if (spec.kind === 'font' && typeof v === 'string' && FONT_OPTIONS.some((f) => f.value === v)) {
      out.fontFamily = v;
    } else if (spec.kind === 'slider' && typeof v === 'number' && Number.isFinite(v)) {
      (out as Record<string, unknown>)[spec.key as string] = Math.min(spec.max ?? v, Math.max(spec.min ?? v, v));
    } else if (spec.kind === 'toggle' && typeof v === 'boolean') {
      (out as Record<string, unknown>)[spec.key as string] = v;
    }
  }
  return out;
}

/**
 * Load user overrides, tolerant of legacy/malformed state. A real user's
 * browser carries keys from earlier builds (malformed JSON, wrong types) —
 * never let that break the panel or silently swallow a working override.
 */
export function loadThemeOverrides(): ThemeOverrides {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(OVERRIDES_KEY);
    if (raw == null) {
      // One-time migration from legacy key names used by earlier builds.
      for (const legacy of ['bytescape_theme_overrides_v1', 'bytescape_theme_custom']) {
        const l = localStorage.getItem(legacy);
        if (l != null) { raw = l; localStorage.setItem(OVERRIDES_KEY, l); localStorage.removeItem(legacy); break; }
      }
    }
  } catch {
    return {};
  }
  if (raw == null) return {};
  try {
    return sanitizeOverrides(JSON.parse(raw));
  } catch {
    // Malformed blob: recover by clearing it so the panel is usable.
    try { localStorage.removeItem(OVERRIDES_KEY); } catch { /* ignore */ }
    return {};
  }
}

export function saveThemeOverrides(o: ThemeOverrides): void {
  localStorage.setItem(OVERRIDES_KEY, JSON.stringify(o));
}

export function clearThemeOverrides(): void {
  localStorage.removeItem(OVERRIDES_KEY);
}

/**
 * The React-rendered class list for user overrides. The `.app` div's
 * className is React-controlled (imperative classList.add is wiped on
 * re-render), so these flags must be rendered as part of className. Returns
 * a space-separated string (empty when no overrides). Registry-driven: gate
 * classes for sliders, on/off classes for FX toggles.
 */
export function userOverrideClassNames(o: ThemeOverrides): string {
  const classes: string[] = [];
  for (const spec of THEME_TOKEN_REGISTRY) {
    const v = (o as Record<string, unknown>)[spec.key as string];
    if (spec.gateClass && v !== undefined && v !== null) classes.push(spec.gateClass);
    if (spec.kind === 'toggle') {
      if (v === true && spec.onClass) classes.push(spec.onClass);
      if (v === false && spec.offClass) classes.push(spec.offClass);
    }
  }
  return classes.join(' ');
}

/**
 * Apply user overrides as inline CSS variables / properties — the SAME
 * mechanism that made accent work: the consumed tokens are written INLINE on
 * `html` + `.app`, and inline declarations beat the preset class rules on the
 * same element (`.app-theme-*`), so every override wins over the preset.
 *
 * The gate/FX classes are ALSO rendered as React classes on `.app` (see
 * userOverrideClassNames) because `.app`'s className is React-controlled and
 * imperative classList edits get wiped on re-render. Here we set them
 * imperatively for immediate effect and dispatch
 * `bytescape:themeOverridesChanged` so App re-renders with the classes.
 */
export function applyThemeOverrides(o: ThemeOverrides): void {
  const html = document.documentElement;
  const app = (document.querySelector('.app') as HTMLElement | null) ?? html;
  const targets = [html, app];
  const set = (k: string, v: string | null) => {
    for (const t of targets) {
      if (v === null || v === undefined) t.style.removeProperty(k);
      else t.style.setProperty(k, v);
    }
  };

  for (const spec of THEME_TOKEN_REGISTRY) {
    const v = (o as Record<string, unknown>)[spec.key as string];
    if (spec.kind === 'toggle') continue; // class-only, handled below
    if (spec.kind === 'file') {
      for (const vr of spec.vars ?? []) set(vr, typeof v === 'string' && v ? `url(${v})` : null);
      continue;
    }
    if (spec.kind === 'font') {
      const opt = FONT_OPTIONS.find((f) => f.value === v);
      for (const vr of spec.vars ?? []) set(vr, opt ? opt.stack : null);
      for (const cp of spec.cssProps ?? []) set(cp, opt ? opt.stack : null);
      set('--font-comp', opt ? String(opt.comp) : null);
      continue;
    }
    // color / slider. Slider vars: fontScale is a UNITLESS multiplier (consumed
    // by calc() — the '%' unit is display-only); other sliders carry their unit.
    const formatted =
      spec.kind === 'slider' && typeof v === 'number'
        ? spec.scale
          ? String(v)
          : `${v}${spec.unit ?? ''}`
        : typeof v === 'string'
          ? v
          : null;
    for (const vr of spec.vars ?? []) set(vr, formatted);
    for (const cp of spec.cssProps ?? []) set(cp, formatted);
  }

  // Class-flagged overrides (gates + FX toggles) — matches
  // userOverrideClassNames, the React-rendered class list being authoritative.
  const classFlags: Array<[string, boolean]> = [];
  for (const spec of THEME_TOKEN_REGISTRY) {
    const v = (o as Record<string, unknown>)[spec.key as string];
    if (spec.gateClass) classFlags.push([spec.gateClass, v !== undefined && v !== null]);
    if (spec.kind === 'toggle') {
      if (spec.onClass) classFlags.push([spec.onClass, v === true]);
      if (spec.offClass) classFlags.push([spec.offClass, v === false]);
    }
  }
  for (const [cls, on] of classFlags) {
    for (const t of targets) t.classList.toggle(cls, on);
  }
  // Tell App to re-render `.app` with these classes in its React className
  // (imperative classList is wiped on the next re-render otherwise).
  window.dispatchEvent(new CustomEvent('bytescape:themeOverridesChanged'));
}

export const settingsApi = {
  get: () => api<SettingsState>('/api/settings'),
  /** Visibility (Phase 6): graph_visibility + home hide_collections + caveats. */
  visibility: {
    get: () => api<VisibilitySettings>('/api/settings/visibility'),
    patch: (body: { graph_visibility?: string; hide_collections?: boolean }) =>
      api<{ ok: boolean; graph_visibility: string; home_hide_collections: boolean | null }>(
        '/api/settings/visibility',
        { method: 'PATCH', body: JSON.stringify(body) },
      ),
  },
  updateAppearance: (theme: AppThemeId) =>
    api<SettingsUpdateResponse>('/api/settings/appearance', {
      method: 'PATCH',
      body: JSON.stringify({ theme }),
    }),
  updateModeration: (nsfwPref: NsfwPref) =>
    api<SettingsUpdateResponse>('/api/settings/moderation', {
      method: 'PATCH',
      body: JSON.stringify({ nsfw_pref: nsfwPref }),
    }),
  updateInterests: (interests: string[]) =>
    api<SettingsUpdateResponse>('/api/settings/interests', {
      method: 'PATCH',
      body: JSON.stringify({ interests }),
    }),
  updateFeed: (feed: { sort?: FeedSort; source_mix?: SourceMix }) =>
    api<SettingsUpdateResponse>('/api/settings/feed', {
      method: 'PATCH',
      body: JSON.stringify(feed),
    }),
  updatePrivacy: (manuallyApprovesFollowers: boolean) =>
    api<SettingsUpdateResponse>('/api/settings/privacy', {
      method: 'PATCH',
      body: JSON.stringify({ manually_approves_followers: manuallyApprovesFollowers }),
    }),
};

// ── Notifications API ──
export const notificationsApi = {
  list: (params: { type?: string; cursor?: string; limit?: number } = {}) => {
    const q = new URLSearchParams();
    if (params.type) q.set('type', params.type);
    if (params.cursor) q.set('cursor', params.cursor);
    if (params.limit) q.set('limit', String(params.limit));
    const qs = q.toString();
    return api<NotificationListResponse>(`/api/notifications${qs ? `?${qs}` : ''}`);
  },
  unreadCount: () => api<{ count: number }>('/api/notifications/unread-count'),
  markRead: (id: string) =>
    api<{ status: string }>(`/api/notifications/${id}/read`, { method: 'POST' }),
  markAllRead: () =>
    api<{ status: string }>('/api/notifications/read-all', { method: 'POST' }),
  followRequests: () =>
    api<{ requests: FollowRequest[] }>('/api/notifications/follow-requests'),
  approveRequest: (id: string) =>
    api<{ status: string }>(`/api/notifications/follow-requests/${id}/approve`, { method: 'POST' }),
  denyRequest: (id: string) =>
    api<{ status: string }>(`/api/notifications/follow-requests/${id}/deny`, { method: 'POST' }),
};

// ── Friends API ──
export const friendsApi = {
  mutual: () => api<MutualFriend[]>('/api/friends'),
};

// ── Subscription API ──
export const subscriptionApi = {
  list: () => api<Subscription[]>('/api/subscriptions'),
  create: (sub: { source: string; source_id: string; label: string; icon?: string; category?: string }) =>
    api<Subscription>('/api/subscriptions', { method: 'POST', body: JSON.stringify(sub) }),
  bootstrap: (interest_ids: string[]) =>
    api<Subscription[]>('/api/subscriptions/bootstrap', { method: 'POST', body: JSON.stringify({ interest_ids }) }),
  toggle: (id: string, enabled: boolean) =>
    api<Subscription>(`/api/subscriptions/${id}`, { method: 'PATCH', body: JSON.stringify({ enabled }) }),
  // Phase 2: pinned source tabs persist server-side (PATCH /api/subscriptions/:id).
  pin: (id: string, pinned: boolean) =>
    api<Subscription>(`/api/subscriptions/${id}`, { method: 'PATCH', body: JSON.stringify({ pinned }) }),
  delete: (id: string) => apiRaw(`/api/subscriptions/${id}`, { method: 'DELETE' }),
};

// ── Feed Generator API (generated topic feeds + combos) ──
export interface FeedgenTopic {
  rkey: string;
  name: string;
  description: string;
  tags: string[];
}

export interface FeedgenPublishResult {
  at_uri: string;
  rkey: string;
}

export interface FeedgenComboResult {
  at_uri: string;
  rkey: string;
  name: string;
}

export const feedgenApi = {
  topics: () => api<{ topics: FeedgenTopic[] }>('/api/feedgen/topics'),
  publish: (rkey: string) =>
    api<FeedgenPublishResult>('/api/feedgen/publish', {
      method: 'POST',
      body: JSON.stringify({ rkey }),
    }),
  createCombo: (name: string, topics: string[]) =>
    api<FeedgenComboResult>('/api/feedgen/combos', {
      method: 'POST',
      body: JSON.stringify({ name, topics }),
    }),
};

// ── RSS API ──
export const rssApi = {
  list: () => api<RssSubscription[]>('/api/rss/subscriptions'),
  create: (url: string, label?: string) =>
    api<RssSubscription>('/api/rss/subscriptions', {
      method: 'POST',
      body: JSON.stringify({ url, label }),
    }),
  delete: (id: string) => apiRaw(`/api/rss/subscriptions/${id}`, { method: 'DELETE' }),
  feed: () => api<FeedResponse>('/api/rss/feed'),
};

// ── Import API ──
export const importApi = {
  googleUpload: async (file: File) => {
    const fd = new FormData();
    fd.append('file', file);
    const res = await apiRaw('/api/import/google', { method: 'POST', body: fd });
    if (!res.ok) {
      const msg = await res.text();
      throw new Error(msg);
    }
    return res.json() as Promise<{ imported: number; session_id: string; errors: string[] }>;
  },
  facebookUpload: async (file: File) => {
    const fd = new FormData();
    fd.append('file', file);
    const res = await apiRaw('/api/import/facebook', { method: 'POST', body: fd });
    if (!res.ok) {
      const msg = await res.text();
      throw new Error(msg);
    }
    return res.json() as Promise<{ imported: number; session_id: string; errors: string[] }>;
  },
  instagramUpload: async (file: File) => {
    const fd = new FormData();
    fd.append('file', file);
    const res = await apiRaw('/api/import/instagram', { method: 'POST', body: fd });
    if (!res.ok) {
      const msg = await res.text();
      throw new Error(msg);
    }
    return res.json() as Promise<{ imported: number; session_id: string; errors: string[] }>;
  },
  upload: async (file: File) => {
    const fd = new FormData();
    fd.append('file', file);
    // Auto-detect source by trying unified endpoint first
    const res = await apiRaw('/api/import', { method: 'POST', body: fd });
    if (!res.ok) {
      const msg = await res.text();
      throw new Error(msg);
    }
    return res.json() as Promise<{ imported: number; session_id: string; errors: string[]; source?: string }>;
  },
  uploadStream: async (file: File, onEvent: (event: any) => void): Promise<{ imported: number; errors: string[]; session_id?: string; source?: string }> => {
    const fd = new FormData();
    fd.append('file', file);
    const t = localStorage.getItem('bytescape_token');
    const res = await fetch(`${API}/api/import/stream`, {
      method: 'POST',
      headers: t ? { 'Authorization': `Bearer ${t}` } : {},
      body: fd,
    });
    if (!res.ok || !res.body) {
      const msg = await res.text();
      throw new Error(msg);
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let imported = 0;
    let errors: string[] = [];
    let session_id: string | undefined;
    let source: string | undefined;
    let complete = false;

    while (!complete) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      const lines = buffer.split('\n');
      buffer = lines.pop() || '';

      for (const line of lines) {
        if (line.startsWith('data: ')) {
          try {
            const event = JSON.parse(line.slice(6));
            if (event.type === 'complete') {
              imported = event.imported || 0;
              errors = event.errors || [];
              session_id = event.session_id;
              source = event.source;
              complete = true;
            }
            onEvent(event);
          } catch {}
        }
      }
    }
    return { imported, errors, session_id, source };
  },
  socialGraph: async (sessionId: string) => {
    return api<SocialGraphResponse>(`/api/import/${sessionId}/social-graph`);
  },
};

// ── Vault API ──
export interface MediaItem {
  id: string;
  storage_url: string;
  thumbnail_url: string | null;
  mime_type: string;
  width: number | null;
  height: number | null;
  alt_text: string | null;
  file_size: number | null;
  created_at: string;
}

export interface VaultResponse {
  items: MediaItem[];
  has_more: boolean;
}

export const vaultApi = {
  list: (limit?: number, offset?: number) =>
    api<VaultResponse>(`/api/vault?limit=${limit || 20}&offset=${offset || 0}`),
  detail: (id: string) => api<MediaItem>(`/api/vault/media/${id}`),
};

export const profileApi = {
  me: () => api<ProfileData>('/api/profile/me'),
  get: (username: string) => api<ProfileData>(`/api/profile/${encodeURIComponent(username)}`),
  update: (req: UpdateProfileRequest) =>
    api<ProfileData>('/api/profile/me', { method: 'PUT', body: JSON.stringify(req) }),
  feed: (username: string, limit = 20, offset = 0, filters?: { source?: string; dateFrom?: string; dateTo?: string }) => {
    let url = `/api/profile/${encodeURIComponent(username)}/feed?limit=${limit}&offset=${offset}`;
    if (filters?.source) url += `&source=${encodeURIComponent(filters.source)}`;
    if (filters?.dateFrom) url += `&date_from=${encodeURIComponent(filters.dateFrom)}`;
    if (filters?.dateTo) url += `&date_to=${encodeURIComponent(filters.dateTo)}`;
    return api<FeedResponse>(url);
  },
  follow: (username: string) =>
    api<{ status: string; pending?: boolean }>(`/api/profile/${encodeURIComponent(username)}/follow`, { method: 'POST' }),
  unfollow: (username: string) =>
    api<{ status: string }>(`/api/profile/${encodeURIComponent(username)}/unfollow`, { method: 'POST' }),
  relationships: (usernames: string[]) =>
    api<Record<string, boolean>>(`/api/profile/relationships?usernames=${usernames.map(encodeURIComponent).join(',')}`),
  wall: (username: string, limit = 50, offset = 0) =>
    api<{ items: WallPost[]; has_more: boolean }>(`/api/profile/${encodeURIComponent(username)}/wall?limit=${limit}&offset=${offset}`),
  postWall: (username: string, content: string) =>
    api<WallPost>(`/api/profile/${encodeURIComponent(username)}/wall`, { method: 'POST', body: JSON.stringify({ content }) }),
  following: (username: string) =>
    api<MergedGraphResponse>(`/api/profile/${encodeURIComponent(username)}/following`),
  followers: (username: string) =>
    api<MergedGraphResponse>(`/api/profile/${encodeURIComponent(username)}/followers`),
  removeFollower: (username: string, actorId: string) =>
    api<{ removed: boolean; network: string; undo_queued?: boolean }>(
      `/api/profile/${encodeURIComponent(username)}/followers/remove`,
      { method: 'POST', body: JSON.stringify({ actor_id: actorId }) },
    ),
  friends: (username: string) =>
    api<FriendView[]>(`/api/profile/${encodeURIComponent(username)}/friends`),
  uploadAvatar: async (file: File) => {
    const fd = new FormData();
    fd.append('file', file);
    const res = await apiRaw('/api/profile/me/avatar', { method: 'POST', body: fd });
    if (!res.ok) {
      const msg = await res.text();
      throw new Error(msg);
    }
    return res.json() as Promise<{ avatar_url: string }>;
  },
  uploadHeader: async (file: File) => {
    const fd = new FormData();
    fd.append('file', file);
    const res = await apiRaw('/api/profile/me/header', { method: 'POST', body: fd });
    if (!res.ok) {
      const msg = await res.text();
      throw new Error(msg);
    }
    return res.json() as Promise<{ header_url: string }>;
  },
};

// ── Follow API (by user ID / handle) ──
export const followApi = {
  follow: (handle: string) => {
    const username = handle.startsWith('@') ? handle.slice(1) : handle;
    return api<{ status: string }>(`/api/profile/${encodeURIComponent(username)}/follow`, { method: 'POST' });
  },
  unfollow: (handle: string) => {
    const username = handle.startsWith('@') ? handle.slice(1) : handle;
    return api<{ status: string }>(`/api/profile/${encodeURIComponent(username)}/unfollow`, { method: 'POST' });
  },
};

// ── Mute API ──
export interface MutedAccount {
  id: string;
  muted_did: string | null;
  muted_handle: string | null;
  muted_type: string;
  muted_at: string;
}

export const muteApi = {
  mute: (did: string | null, handle: string | null, type: 'mute' | 'block' = 'mute') =>
    api<{ ok: boolean; muted_did: string | null; muted_handle: string | null; type: string }>('/api/mute', {
      method: 'POST',
      body: JSON.stringify({ did, handle, type }),
    }),
  unmute: (did: string) =>
    api<{ ok: boolean; unmuted_did: string }>('/api/unmute', {
      method: 'POST',
      body: JSON.stringify({ did }),
    }),
  list: () => api<{ accounts: MutedAccount[] }>('/api/muted'),
};

// ── Forums API ──
export interface ForumTopicData {
  id: string;
  title: string;
  content: string;
  content_html?: string;
  tags: string[];
  community?: string;
  pinned: boolean;
  locked: boolean;
  reply_count: number;
  view_count: number;
  last_reply_at?: string;
  created_at: string;
  author: { handle: string; display_name: string; avatar?: string };
}

export interface ForumTopicListResponse {
  items: ForumTopicData[];
  has_more: boolean;
}

export interface ForumReplyData {
  id: string;
  topic_id: string;
  content: string;
  content_html?: string;
  parent_reply_id?: string;
  created_at: string;
  author: { handle: string; display_name: string; avatar?: string };
}

// ── Exchange API ──
export interface ExchangeListingData {
  id: string;
  user_id: string;
  title: string;
  description: string;
  listing_type: string;
  category: string;
  price_cents: number | null;
  currency: string | null;
  location: string | null;
  tags: string[];
  image_urls: string[];
  status: string;
  view_count: number;
  created_at: string;
  updated_at: string;
  author?: { handle: string; display_name: string; avatar?: string };
}

export interface ExchangeMessageData {
  id: string;
  listing_id: string;
  sender_id: string;
  recipient_id: string;
  content: string;
  read: boolean;
  created_at: string;
  sender?: { handle: string; display_name: string; avatar?: string };
  recipient?: { handle: string; display_name: string; avatar?: string };
}

export interface ExchangeListResponse {
  items: ExchangeListingData[];
  has_more: boolean;
}

export const exchangeApi = {
  listListings: (params: string) =>
    api<ExchangeListResponse>(`/api/exchange/listings?${params}`),
  createListing: (data: {
    title: string; description: string; listing_type?: string;
    category?: string; price_cents?: number | null; currency?: string;
    location?: string; tags?: string[]; image_urls?: string[];
  }) =>
    api<ExchangeListingData>('/api/exchange/listings', { method: 'POST', body: JSON.stringify(data) }),
  getListing: (id: string) =>
    api<ExchangeListingData>(`/api/exchange/listings/${encodeURIComponent(id)}`),
  updateStatus: (id: string, status: string) =>
    api<ExchangeListingData>(`/api/exchange/listings/${encodeURIComponent(id)}`, {
      method: 'PUT', body: JSON.stringify({ status }),
    }),
  deleteListing: (id: string) =>
    apiRaw(`/api/exchange/listings/${encodeURIComponent(id)}`, { method: 'DELETE' }),
  sendMessage: (listingId: string, content: string) =>
    api<ExchangeMessageData>(`/api/exchange/listings/${encodeURIComponent(listingId)}/messages`, {
      method: 'POST', body: JSON.stringify({ content }),
    }),
  listMessages: (listingId: string, params: string) =>
    api<ExchangeMessageData[]>(`/api/exchange/listings/${encodeURIComponent(listingId)}/messages?${params}`),
  listInbox: (params: string) =>
    api<ExchangeMessageData[]>(`/api/exchange/messages/inbox?${params}`),
};

export const forumsApi = {
  listTopics: (params: string) =>
    api<ForumTopicListResponse>(`/api/forums/topics?${params}`),
  createTopic: (data: { title: string; content: string; community?: string; tags?: string[] }) =>
    api<ForumTopicData>('/api/forums/topics', { method: 'POST', body: JSON.stringify(data) }),
  getTopic: (id: string) =>
    api<ForumTopicData>(`/api/forums/topics/${encodeURIComponent(id)}`),
  addReply: (topicId: string, data: { content: string; parent_reply_id?: string }) =>
    api<ForumReplyData>(`/api/forums/topics/${encodeURIComponent(topicId)}/replies`, { method: 'POST', body: JSON.stringify(data) }),
  listReplies: (topicId: string, params: string) =>
    api<ForumReplyData[]>(`/api/forums/topics/${encodeURIComponent(topicId)}/replies?${params}`),
};

// ── Stories ──
export interface StoryMediaItem {
  type: string; // "image" | "video"
  cid: string;
  mime_type: string;
  alt: string;
  duration: number | null;
}
export interface StoryItem {
  kind: 'story' | 'status';
  uri: string;
  author_did: string;
  created_at: string;
  expires_at: string | null;
  active_from: string | null;
  pinned: boolean;
  text: string | null;
  media: StoryMediaItem[] | null;
  music_url: string | null;
  /** Bluesky "go live" status — render the honest live label + stream link. */
  is_live: boolean;
  style: { bg?: { color?: string; image?: string } } | null;
  can_react: boolean;
  can_reply: boolean;
  like_count: number;
  repost_count: number;
  reply_count: number;
  state?: string; // mine only: active | scheduled | never | archived
}
export interface StoryGroupAuthor {
  did: string;
  handle: string;
  display_name: string | null;
  avatar: string | null;
}
export interface StoryGroup {
  author: StoryGroupAuthor;
  seen: boolean;
  items: StoryItem[];
}

export function storyMediaUrl(authorDid: string, m: StoryMediaItem): string {
  // One uniform path: our getBlob route serves native authors from the local
  // blob store and relays linked authors' blobs from their own PDS. (Bluesky's
  // CDN only serves feed-pipeline media — story blobs have no HLS rendition.)
  return `${API}/xrpc/com.atproto.sync.getBlob?did=${encodeURIComponent(authorDid)}&cid=${encodeURIComponent(m.cid)}`;
}

export function isStoryQuoteUri(uri?: string | null): boolean {
  return !!uri && uri.includes('/com.bytescape.story/');
}

export interface CreateStoryData {
  media_urls: string[];
  alts?: string[];
  durations?: (number | null)[];
  text?: string | null;
  expires_at?: string | null;
  active_from?: string | null;
  pinned?: boolean;
}

export interface SetStatusData {
  text: string;
  duration_minutes: number;
  embed_uri?: string | null;
  style?: { bg?: { color?: string; image?: string } } | null;
}

export interface StoryComment {
  id: string;
  author_handle: string;
  author_avatar: string | null;
  content: string;
  created_at: string;
  parent_id: string | null;
  likes_count: number;
}

export const storiesApi = {
  rail: () => api<{ groups: StoryGroup[] }>('/api/stories/rail'),
  explorer: (params: { sort?: string; person?: string; kind?: string; limit?: number; offset?: number }) => {
    const q = new URLSearchParams();
    if (params.sort) q.set('sort', params.sort);
    if (params.person) q.set('person', params.person);
    if (params.kind) q.set('kind', params.kind);
    if (params.limit) q.set('limit', String(params.limit));
    if (params.offset) q.set('offset', String(params.offset));
    return api<{ groups: StoryGroup[]; total: number }>(`/api/stories/explorer?${q.toString()}`);
  },
  mine: () => api<{ items: StoryItem[] }>('/api/stories/mine'),
  seen: (uri: string) =>
    api<{ ok: boolean }>('/api/stories/seen', { method: 'POST', body: JSON.stringify({ uri }) }),
  item: (uri: string) => api<StoryItem>(`/api/stories/item?uri=${encodeURIComponent(uri)}`),
  create: (data: CreateStoryData) =>
    api<{ uri: string }>('/api/stories', { method: 'POST', body: JSON.stringify(data) }),
  setStatus: (data: SetStatusData) =>
    api<{ ok: boolean }>('/api/stories/status', { method: 'POST', body: JSON.stringify(data) }),
  remove: (uri: string) =>
    api<{ ok: boolean }>('/api/stories', { method: 'DELETE', body: JSON.stringify({ uri }) }),
  archiveMediaUrl: (uri: string, cid: string) =>
    `${API}/api/stories/archive/media?uri=${encodeURIComponent(uri)}&cid=${encodeURIComponent(cid)}`,
  archiveDelete: (uri: string) =>
    api<{ ok: boolean }>('/api/stories/archive', { method: 'DELETE', body: JSON.stringify({ uri }) }),
  react: (uri: string, reaction: string) =>
    api<{ reaction_counts?: Record<string, number>; user_reaction?: string | null }>('/api/feed/reaction', {
      method: 'POST',
      body: JSON.stringify({ post_id: uri, source: 'story', reaction }),
    }),
  comments: (uri: string) =>
    api<StoryComment[]>(`/api/feed/comments?post_id=${encodeURIComponent(uri)}&source=story`),
  comment: (uri: string, content: string) =>
    api<{ ok: boolean; uri: string }>('/api/feed/comments', {
      method: 'POST',
      body: JSON.stringify({ post_id: uri, source: 'story', content }),
    }),
  reshare: (uri: string) =>
    api<{ ok: boolean; post_id: string }>('/api/posts/repost', {
      method: 'POST',
      body: JSON.stringify({ post_id: uri }),
    }),
};
