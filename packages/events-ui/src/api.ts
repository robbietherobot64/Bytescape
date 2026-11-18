import type { FeedgenTopic } from "./sourceTypes";

// Events API — one module shared by both hosts (apps/web, apps/events).
// Relative `/api` paths: every origin proxies the backend (Caddy in prod,
// Vite in dev), so there is no base-URL configuration (design decision 8).
// Auth is a host-injected seam: the web host passes its localStorage token
// getter; the events host passes nothing and rides the family session cookie.

export interface RsvpCounts {
  going: number;
  interested: number;
  notgoing: number;
}

export interface EventAuthor {
  did: string;
  handle: string;
  display_name?: string | null;
  avatar?: string | null;
}

export interface EventLocation {
  name?: string | null;
  address?: string | null;
}

export interface EventUri {
  uri: string;
  name?: string | null;
}

export interface EventProvenance {
  /**
   * following | feeds | hashtags | interest. "hashtags" labels carry the
   * `#cats` form and only appear on literal hashtag matches; "interest" is a
   * bare word match (no `#`) — shared chip truth with the main feed.
   */
  kind: string;
  label: string;
}

export interface EventItem {
  id: string;
  cid: string;
  name: string;
  description?: string | null;
  starts_at?: string | null;
  ends_at?: string | null;
  mode?: string | null;
  status: string;
  location?: EventLocation | null;
  image?: string | null;
  uris: EventUri[];
  author: EventAuthor;
  is_upcoming: boolean;
  rsvp_status?: "going" | "maybe" | "not_going" | null;
  rsvp_counts: RsvpCounts;
  likes_count: number;
  replies_count: number;
  reaction_counts?: Record<string, number> | null;
  user_reaction?: string | null;
  /** Why the card is in this list; always at least one entry from the API. */
  provenance?: EventProvenance[];
}

export interface EventsPage {
  items: EventItem[];
  has_more: boolean;
}

export interface EventComment {
  id: string;
  content: string;
  created_at: string;
  author: { handle: string; display_name?: string | null; avatar?: string | null };
}

/** A followed account in the right-rail sources panel. */
export interface SourcePerson {
  did: string;
  handle: string;
  display_name?: string | null;
  avatar?: string | null;
}

/** A subscribed feed; `topics` are the event-topic rkeys it maps to. */
export interface SourceFeed {
  id: string;
  label: string;
  source_id: string;
  icon: string;
  topics: string[];
}

/** A subscribed hashtag. */
export interface SourceHashtag {
  id: string;
  label: string;
  source_id: string;
  icon: string;
  tag: string;
}

export interface EventSources {
  following: SourcePerson[];
  following_total: number;
  feeds: SourceFeed[];
  hashtags: SourceHashtag[];
}

export interface LinkPreviewData {
  title?: string | null;
  description?: string | null;
  image?: string | null;
  url?: string | null;
  site_name?: string | null;
}

export interface ViewerSession {
  user_id: string;
  username: string;
  dids: string[];
}

/** The events source tabs. */
export type SourceTab = "home" | "following" | "feeds" | "hashtags";
/** The three browsable source groups (Home is not a group). */
export type SourceGroup = "following" | "feeds" | "hashtags";

/** One scoped source inside a tab (a followed account, feed, or hashtag). */
export interface SourceScope {
  kind: SourceGroup;
  id: string;
  label: string;
}

export interface ListParams {
  status?: "all" | "upcoming" | "past";
  /** Source tab: Home is everything; the others filter by the viewer's
   * follows, feed subscriptions, or hashtag subscriptions. */
  source?: SourceTab;
  /** Scope inside the tab (ids from /api/events/sources). */
  followed_did?: string;
  feed_id?: string;
  hashtag_id?: string;
  limit?: number;
  q?: string;
  date_from?: string;
  date_to?: string;
  location?: string;
  cursor_starts_at?: string;
  cursor_uri?: string;
}

export interface CreateEventBody {
  name: string;
  description?: string;
  starts_at?: string;
  ends_at?: string;
  mode?: "inperson" | "virtual" | "hybrid";
  location?: string;
  picture_url?: string;
}

// Host seam: configure once at app boot (public API — DI boundary).
let getToken: () => string | null = () => null;

export function configureEventsApi(opts: { getToken?: () => string | null }): void {
  if (opts.getToken) getToken = opts.getToken;
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(init?.headers as Record<string, string> | undefined),
  };
  const token = getToken();
  if (token) headers["Authorization"] = `Bearer ${token}`;
  const res = await fetch(`/api${path}`, { ...init, headers });
  if (!res.ok) {
    let detail = "";
    try {
      const body = await res.json();
      detail = typeof body === "string" ? body : JSON.stringify(body);
    } catch {
      detail = `${res.status}`;
    }
    throw new Error(detail || `request failed (${res.status})`);
  }
  return res.json() as Promise<T>;
}

export const eventsApi = {
  list(params: ListParams): Promise<EventsPage> {
    const qs = new URLSearchParams();
    if (params.status) qs.set("status", params.status);
    if (params.limit) qs.set("limit", String(params.limit));
    if (params.q) qs.set("q", params.q);
    if (params.date_from) qs.set("date_from", params.date_from);
    if (params.date_to) qs.set("date_to", params.date_to);
    if (params.location) qs.set("location", params.location);
    if (params.source) qs.set("source", params.source);
    if (params.followed_did) qs.set("followed_did", params.followed_did);
    if (params.feed_id) qs.set("feed_id", params.feed_id);
    if (params.hashtag_id) qs.set("hashtag_id", params.hashtag_id);
    if (params.cursor_starts_at) qs.set("cursor_starts_at", params.cursor_starts_at);
    if (params.cursor_uri) qs.set("cursor_uri", params.cursor_uri);
    return req<EventsPage>(`/events?${qs.toString()}`);
  },

  detail(uri: string): Promise<EventItem> {
    return req<EventItem>(`/events/detail?${new URLSearchParams({ uri })}`);
  },

  sources(): Promise<EventSources> {
    return req<EventSources>(`/events/sources`);
  },

  /** Remove a source subscription — the same endpoint the feed's panel uses. */
  deleteSubscription(id: string): Promise<unknown> {
    return req(`/subscriptions/${encodeURIComponent(id)}`, { method: "DELETE" });
  },

  /** Generated topic feeds for the shared source search. */
  async feedgenTopics(): Promise<FeedgenTopic[]> {
    const res = await req<{ topics: FeedgenTopic[] }>(`/feedgen/topics`);
    return res.topics;
  },

  publishFeedgenTopic(rkey: string): Promise<{ at_uri: string }> {
    return req<{ at_uri: string }>(`/feedgen/publish`, {
      method: "POST",
      body: JSON.stringify({ rkey }),
    });
  },

  createSubscription(body: {
    source: string;
    source_id: string;
    label: string;
    icon?: string;
    category?: string;
  }): Promise<unknown> {
    return req(`/subscriptions`, { method: "POST", body: JSON.stringify(body) });
  },

  /** OpenGraph metadata for a link card. Auth rides the host seam (Bearer on
   * web, the family session cookie on the standalone — the server middleware
   * injects it as Bearer). */
  og(url: string): Promise<LinkPreviewData> {
    return req<LinkPreviewData>(`/og?${new URLSearchParams({ url })}`);
  },

  myRsvps(limit = 50): Promise<EventsPage> {
    return req<EventsPage>(`/events/my-rsvps?${new URLSearchParams({ limit: String(limit) })}`);
  },

  create(body: CreateEventBody): Promise<EventItem> {
    return req<EventItem>(`/events`, { method: "POST", body: JSON.stringify(body) });
  },

  rsvp(uri: string, status: "going" | "maybe" | "not_going"): Promise<{ uri: string; status: string; rsvp_counts: RsvpCounts }> {
    return req(`/events/rsvp`, { method: "POST", body: JSON.stringify({ uri, status }) });
  },

  removeRsvp(uri: string): Promise<{ deleted: boolean }> {
    return req(`/events/rsvp?${new URLSearchParams({ uri })}`, { method: "DELETE" });
  },

  removeEvent(uri: string): Promise<{ deleted: boolean }> {
    return req(`/events?${new URLSearchParams({ uri })}`, { method: "DELETE" });
  },

  addReaction(uri: string, emoji: string): Promise<{ reaction_counts?: Record<string, number> | null; user_reaction?: string | null }> {
    return req(`/events/reactions`, { method: "POST", body: JSON.stringify({ uri, emoji }) });
  },

  removeReaction(uri: string): Promise<{ reaction_counts?: Record<string, number> | null; user_reaction?: string | null }> {
    return req(`/events/reactions?${new URLSearchParams({ uri })}`, { method: "DELETE" });
  },

  comments(uri: string, limit = 50, offset = 0): Promise<EventComment[]> {
    return req(`/events/comments?${new URLSearchParams({ uri, limit: String(limit), offset: String(offset) })}`);
  },

  addComment(uri: string, content: string): Promise<EventComment> {
    return req(`/events/comments`, { method: "POST", body: JSON.stringify({ uri, content }) });
  },

  deleteComment(id: string): Promise<{ deleted: boolean }> {
    return req(`/events/comments?${new URLSearchParams({ id })}`, { method: "DELETE" });
  },

  async uploadImage(file: File): Promise<string> {
    const form = new FormData();
    form.append("file", file);
    const headers: Record<string, string> = {};
    const token = getToken();
    if (token) headers["Authorization"] = `Bearer ${token}`;
    const res = await fetch(`/api/events/upload-image`, { method: "POST", headers, body: form });
    if (!res.ok) throw new Error(`upload failed (${res.status})`);
    const body = (await res.json()) as { url: string };
    return body.url;
  },

  session(): Promise<ViewerSession> {
    return req<ViewerSession>(`/auth/session`);
  },
};
