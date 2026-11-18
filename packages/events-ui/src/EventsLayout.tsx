import { useCallback, useEffect, useMemo, useState } from "react";
import EventsColumn from "./EventsColumn";
import EventsRightPanel from "./EventsRightPanel";
import EventDetail from "./EventDetail";
import CreateEventModal from "./CreateEventModal";
import SourcesPanel from "./SourcesPanel";
import SourceSearch from "./SourceSearch";
import { eventsApi } from "./api";
import type {
  EventSources,
  ListParams,
  SourceGroup,
  SourceScope,
  SourceTab,
} from "./api";
import type { FeedgenTopic, SourceCategory, SourceCategoryItem } from "./sourceTypes";

export interface EventsViewer {
  username: string;
  dids: string[];
}

interface EventsLayoutProps {
  /** Signed-in identity (hosts fetch it — web from its token, events from
   * /api/auth/session); null = anonymous visitor. */
  viewer: EventsViewer | null;
  /** Anonymous participation → hosts route to sign-in (SSO hop / login). */
  onRequireSignIn: () => void;
  /** Selection is host-controlled when these props are provided (the events
   * SPA routes /event/:uri); otherwise the layout keeps it internally. */
  selectedUri?: string | null;
  onSelectedUriChange?: (uri: string | null) => void;
}

const SOURCE_TABS: { id: SourceTab; icon: string; label: string }[] = [
  { id: "home", icon: "🏠", label: "Home" },
  { id: "following", icon: "👥", label: "Following" },
  { id: "feeds", icon: "📡", label: "Feeds" },
  { id: "hashtags", icon: "#️⃣", label: "Hashtags" },
];

const STATUS_TABS = [
  ["upcoming", "Upcoming"],
  ["past", "Past"],
  ["all", "All"],
] as const;

const groupForCategory = (categoryId: string): SourceGroup => {
  const id = categoryId.toLowerCase();
  if (id.includes("follow")) return "following";
  if (id.includes("feed")) return "feeds";
  return "hashtags";
};

/** The shared Events surface: the main feed's tab strip + action bar + sources
 * rail, with events in the reading column. Consumed by apps/events
 * (standalone) and the main app's Events tab — one component tree, both hosts
 * (spec: events/browse). */
export default function EventsLayout({
  viewer,
  onRequireSignIn,
  selectedUri: controlledUri,
  onSelectedUriChange,
}: EventsLayoutProps) {
  const [internalUri, setInternalUri] = useState<string | null>(null);
  const selectedUri = controlledUri !== undefined ? controlledUri : internalUri;
  const select = (uri: string | null) => {
    if (onSelectedUriChange) onSelectedUriChange(uri);
    else setInternalUri(uri);
  };

  const [status, setStatus] = useState<"all" | "upcoming" | "past">("upcoming");
  const [q, setQ] = useState("");
  const [appliedQ, setAppliedQ] = useState("");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [location, setLocation] = useState("");
  const [source, setSource] = useState<SourceTab>("home");
  const [scope, setScope] = useState<SourceScope | null>(null);
  const [sourceTabs, setSourceTabs] = useState<Array<{ key: string; label: string }>>([]);
  const [sources, setSources] = useState<EventSources | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [showCreate, setShowCreate] = useState(false);

  // Search commits after a pause — no request per keystroke.
  useEffect(() => {
    const timer = setTimeout(() => setAppliedQ(q.trim()), 300);
    return () => clearTimeout(timer);
  }, [q]);

  const loadSources = useCallback(() => {
    if (!viewer) {
      setSources(null);
      return;
    }
    eventsApi
      .sources()
      .then(setSources)
      .catch(() => setSources(null));
  }, [viewer]);

  // The right-rail browsing panel's source lists (follows, feeds, hashtags).
  useEffect(() => {
    loadSources();
  }, [loadSources]);

  const categories: SourceCategory[] = useMemo(() => {
    if (!sources) return [];
    return [
      {
        id: "following",
        label: "Following",
        icon: "👤",
        items: sources.following.map((p) => ({
          id: p.did,
          label: p.display_name || p.handle,
          avatar: p.avatar,
          source_id: `follows:${p.did}`,
          source_type: "follows",
        })),
      },
      {
        id: "feeds",
        label: "Feeds",
        icon: "📡",
        items: sources.feeds.map((f) => ({
          id: f.id,
          label: f.label,
          source_id: f.source_id,
          source_type: "feed",
          icon: f.icon,
        })),
      },
      {
        id: "hashtags",
        label: "Hashtags",
        icon: "#",
        items: sources.hashtags.map((h) => ({
          id: h.id,
          label: h.label,
          source_id: h.source_id,
          source_type: "hashtag",
          icon: h.icon,
        })),
      },
    ];
  }, [sources]);

  const filters: ListParams = useMemo(
    () => ({
      q: appliedQ || undefined,
      date_from: dateFrom ? new Date(dateFrom).toISOString() : undefined,
      date_to: dateTo
        ? new Date(new Date(dateTo).getTime() + 86_400_000).toISOString()
        : undefined,
      location: location.trim() || undefined,
      source,
      followed_did: scope?.kind === "following" ? scope.id : undefined,
      feed_id: scope?.kind === "feeds" ? scope.id : undefined,
      hashtag_id: scope?.kind === "hashtags" ? scope.id : undefined,
    }),
    [appliedQ, dateFrom, dateTo, location, source, scope],
  );

  const advancedCount = [dateFrom, dateTo, location.trim()].filter(Boolean).length;
  const scopeKey = scope ? `${scope.kind}:${scope.id}` : null;

  const pickTab = (tab: SourceTab) => {
    if (tab !== "home" && !viewer) {
      onRequireSignIn();
      return;
    }
    setSource(tab);
    setScope(null);
    select(null);
  };

  const openSource = (item: SourceCategoryItem, categoryId: string) => {
    if (!viewer) {
      onRequireSignIn();
      return;
    }
    const kind = groupForCategory(categoryId);
    const key = `${kind}:${item.id}`;
    setSourceTabs((prev) => (prev.some((t) => t.key === key) ? prev : [...prev, { key, label: item.label }]));
    setSource(kind);
    setScope({ kind, id: item.id, label: item.label });
    select(null);
  };

  const openSavedSource = (tab: { key: string; label: string }) => {
    const splitAt = tab.key.indexOf(":");
    const kind = tab.key.slice(0, splitAt) as SourceGroup;
    const id = tab.key.slice(splitAt + 1);
    setSource(kind);
    setScope({ kind, id, label: tab.label });
    select(null);
  };

  const closeSource = (key: string) => {
    setSourceTabs((prev) => prev.filter((t) => t.key !== key));
    if (scope?.kind && `${scope.kind}:${scope.id}` === key) {
      setScope(null);
      setSource("home");
      select(null);
    }
  };

  const handleDeleteSource = async (categoryId: string, itemId: string) => {
    const kind = groupForCategory(categoryId);
    // Follow rows are network follows (DIDs), not subscription rows; only
    // feeds and hashtags have a /subscriptions/:id to remove.
    if (kind !== "following") {
      await eventsApi.deleteSubscription(itemId);
    }
    setSourceTabs((prev) => prev.filter((t) => t.key !== `${kind}:${itemId}`));
    if (scope?.kind === kind && scope.id === itemId) {
      setScope(null);
      setSource("home");
    }
    loadSources();
    setRefreshKey((k) => k + 1);
  };

  const refreshSources = async () => {
    loadSources();
    setRefreshKey((k) => k + 1);
  };

  const subscribeGenTopic = async (topic: FeedgenTopic) => {
    const { at_uri } = await eventsApi.publishFeedgenTopic(topic.rkey);
    await eventsApi.createSubscription({
      source: "bytescape_feed",
      source_id: at_uri,
      label: topic.name,
      icon: "✨",
      category: "feeds",
    });
  };

  return (
    <div className="ev-page">
      <main className="ev-center">
        {/* Same tab strip as the main feed: Home / Following / Feeds /
            Hashtags, plus a tab per opened source. */}
        <div className="feed-tabstrip">
          {SOURCE_TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={`btn feed-tab${!scope && source === tab.id ? " active" : ""}`}
              onClick={() => pickTab(tab.id)}
            >
              {tab.icon} {tab.label}
            </button>
          ))}
          {sourceTabs.map((tab) => (
            <span key={tab.key} className="feed-tab-group">
              <button
                type="button"
                className={`btn feed-tab feed-tab-ephemeral${scopeKey === tab.key ? " active" : ""}`}
                onClick={() => openSavedSource(tab)}
              >
                {tab.label}
              </button>
              <button
                type="button"
                className="feed-tab-close"
                title="Close"
                onClick={() => closeSource(tab.key)}
              >
                ×
              </button>
            </span>
          ))}
        </div>

        {/* Same action bar shape as the feed: a sort select, search, a
            secondary action, and the accent create button pinned right. */}
        <div
          className="feed-actionbar"
          style={{ display: "flex", alignItems: "center", gap: 10, padding: "2px 0 8px" }}
        >
          <select
            className="feed-sort-select"
            value={status}
            onChange={(e) => {
              setStatus(e.target.value as "all" | "upcoming" | "past");
              select(null);
            }}
            title="Event time range"
          >
            {STATUS_TABS.map(([value, label]) => (
              <option key={value} value={value}>{label}</option>
            ))}
          </select>
          <input
            className="input-field"
            type="search"
            placeholder="Search events…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            style={{ flex: 1, minWidth: 140, padding: "6px 10px" }}
          />
          <button
            type="button"
            className={filtersOpen ? "btn active" : "btn"}
            onClick={() => setFiltersOpen((v) => !v)}
            title="Date and location filters"
          >
            Filters{advancedCount > 0 ? ` (${advancedCount})` : ""} {filtersOpen ? "▴" : "▾"}
          </button>
          <button
            type="button"
            className="btn feed-compose-btn"
            style={{
              marginLeft: "auto",
              background: "var(--accent)",
              color: "#000",
              fontWeight: 700,
              borderRadius: 999,
              padding: "8px 16px",
              border: "none",
              boxShadow: "0 2px 12px rgba(0,0,0,0.4)",
              cursor: "pointer",
            }}
            onClick={() => {
              if (!viewer) {
                onRequireSignIn();
                return;
              }
              setShowCreate(true);
            }}
          >
            + New event
          </button>
        </div>

        {filtersOpen && (
          <div className="ev-filters">
            <label>
              <span>From</span>
              <input type="date" value={dateFrom} onChange={(e) => setDateFrom(e.target.value)} />
            </label>
            <label>
              <span>To</span>
              <input type="date" value={dateTo} onChange={(e) => setDateTo(e.target.value)} />
            </label>
            <label className="ev-filters-grow">
              <span>Location</span>
              <input
                type="text"
                placeholder="city, venue…"
                value={location}
                onChange={(e) => setLocation(e.target.value)}
              />
            </label>
            <button
              type="button"
              className="btn"
              onClick={() => {
                setDateFrom("");
                setDateTo("");
                setLocation("");
              }}
            >
              Clear
            </button>
          </div>
        )}

        <div className="ev-column-scroll">
          {selectedUri ? (
            <EventDetail
              uri={selectedUri}
              viewer={viewer}
              onRequireSignIn={onRequireSignIn}
              onBack={() => select(null)}
              onMutated={() => setRefreshKey((k) => k + 1)}
            />
          ) : (
            <EventsColumn
              status={status}
              filters={filters}
              selectedUri={selectedUri}
              onSelect={select}
              refreshKey={refreshKey}
              signedIn={!!viewer}
              onRequireSignIn={onRequireSignIn}
            />
          )}
        </div>
      </main>

      {/* Right column: the main feed's search + "Your sources" panel, then
          the event-specific Your events section. */}
      <aside className="sources-column ev-right">
        <div className="sources-search">
          <SourceSearch
            categories={categories}
            activeCategory={source === "home" ? undefined : source}
            onScope={openSource}
            onFeedgenSubscribed={refreshSources}
            onDelete={handleDeleteSource}
            onLoadGenTopics={() => eventsApi.feedgenTopics()}
            onSubscribeGenTopic={subscribeGenTopic}
          />
        </div>
        <SourcesPanel
          categories={categories}
          onScope={openSource}
          onDelete={handleDeleteSource}
          onFeedgenSubscribed={refreshSources}
        />
        <EventsRightPanel
          signedIn={!!viewer}
          selectedUri={selectedUri}
          onSelect={select}
          refreshKey={refreshKey}
        />
      </aside>

      {showCreate && (
        <CreateEventModal
          onClose={() => setShowCreate(false)}
          onCreated={(uri) => {
            select(uri);
            setRefreshKey((k) => k + 1);
          }}
        />
      )}
    </div>
  );
}
