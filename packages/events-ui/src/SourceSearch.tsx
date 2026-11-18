import { useEffect, useRef, useState } from "react";
import type { FeedgenTopic, SourceCategory, SourceCategoryItem, SourceSearchResult } from "./sourceTypes";

interface Props {
  categories: SourceCategory[];
  activeCategory?: string; // 'following' | 'feeds' | 'hashtags' | undefined on Home
  onScope: (item: SourceCategoryItem, categoryId: string) => void;
  onGlobalSearch?: (query: string, mode?: string) => Promise<SourceSearchResult[]>;
  onGlobalAdd?: (source: SourceSearchResult) => Promise<void> | void;
  /** Refresh subscriptions + feed after a generated-feed subscribe succeeds. */
  onFeedgenSubscribed?: () => void | Promise<void>;
  /** Remove a subscription — the same handler the sources panel's chip-x uses. */
  onDelete: (categoryId: string, itemId: string) => void | Promise<void>;
  /** Load generated topic feeds for the "Generated feeds" group. */
  onLoadGenTopics?: () => Promise<FeedgenTopic[]>;
  /** Publish + subscribe to a generated topic feed. */
  onSubscribeGenTopic?: (topic: FeedgenTopic) => Promise<void>;
}

const GROUP_KIND: Record<string, string[]> = {
  following: ["following", "follows"],
  feeds: ["feed", "feeds"],
  hashtags: ["hashtag", "hashtags"],
};

/**
 * Universal source search — the same component the main feed renders above its
 * "Your sources" panel. The open tab's group leads and pre-filters; Home
 * searches everything. Hosts can supply generated-feed loading/subscription.
 */
export default function SourceSearch({
  categories,
  activeCategory,
  onScope,
  onGlobalSearch,
  onGlobalAdd,
  onFeedgenSubscribed,
  onDelete,
  onLoadGenTopics,
  onSubscribeGenTopic,
}: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<string>(activeCategory || "all");
  const [addResults, setAddResults] = useState<SourceSearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [genTopics, setGenTopics] = useState<FeedgenTopic[] | null>(null);
  const [subscribingRkey, setSubscribingRkey] = useState<string | null>(null);
  // Delete-confirmation popup, mirroring the panel's chip-x flow.
  const [deleteTarget, setDeleteTarget] = useState<{ groupId: string; item: SourceCategoryItem } | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) setScope(activeCategory || "all");
  }, [open, activeCategory]);

  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDoc);
    return () => document.removeEventListener("mousedown", onDoc);
  }, [open]);

  // Generated feeds: fetch topic generators whenever the dropdown opens.
  useEffect(() => {
    if (!open || !onLoadGenTopics) return;
    let cancelled = false;
    onLoadGenTopics()
      .then((topics) => { if (!cancelled) setGenTopics(topics); })
      .catch(() => { if (!cancelled) setGenTopics((prev) => prev ?? []); });
    return () => { cancelled = true; };
  }, [open, onLoadGenTopics]);

  useEffect(() => {
    if (!open || !onGlobalSearch || query.trim().length < 2) {
      setAddResults([]);
      return;
    }
    let cancelled = false;
    setSearching(true);
    const t = setTimeout(async () => {
      try {
        const res = await onGlobalSearch(query.trim());
        if (!cancelled) setAddResults(res.slice(0, 6));
      } catch {
        if (!cancelled) setAddResults([]);
      } finally {
        if (!cancelled) setSearching(false);
      }
    }, 350);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, open, onGlobalSearch]);

  const matches = (item: SourceCategoryItem) =>
    !query.trim() ||
    item.label.toLowerCase().includes(query.trim().toLowerCase());

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    const { groupId, item } = deleteTarget;
    setDeleteTarget(null);
    await onDelete(groupId, item.id);
    await onFeedgenSubscribed?.();
  };

  const genQuery = query.trim().toLowerCase();
  const matchedTopics = (genTopics ?? []).filter((t) =>
    !genQuery ||
    t.name.toLowerCase().includes(genQuery) ||
    t.description.toLowerCase().includes(genQuery) ||
    t.tags.some((tag) => tag.toLowerCase().includes(genQuery)),
  );
  const showGenGroup =
    !!onSubscribeGenTopic &&
    (scope === "all" || scope === "feeds") &&
    genTopics !== null &&
    matchedTopics.length > 0;

  // Generated topics already subscribed: topic rkey vs the feeds group's
  // source_id tail ("feed:at://<did>/<rkey>" → "<rkey>").
  const feedsGroup = categories.find((c) => GROUP_KIND["feeds"]?.some((k) => c.id.toLowerCase().includes(k)));
  const subscribedFeedByRkey: Record<string, SourceCategoryItem> = {};
  feedsGroup?.items.forEach((it) => {
    const tail = it.source_id.split("/").pop();
    if (tail) subscribedFeedByRkey[tail] = it;
  });

  const visibleGroups = categories
    .filter((c) => {
      if (scope === "all") return true;
      return GROUP_KIND[scope]?.some((k) => c.id.toLowerCase().includes(k));
    })
    .map((c) => ({ ...c, items: c.items.filter(matches) }))
    .filter((c) => c.items.length > 0 || !query.trim());

  // Smart ordering: the open tab's group first; on Home the natural order holds.
  if (scope === "all" && activeCategory) {
    visibleGroups.sort((a, b) => {
      const aKey = GROUP_KIND[activeCategory]?.some((k) => a.id.toLowerCase().includes(k)) ? 0 : 1;
      const bKey = GROUP_KIND[activeCategory]?.some((k) => b.id.toLowerCase().includes(k)) ? 0 : 1;
      return aKey - bKey;
    });
  }

  return (
    <div className="feed-search-dropdown" ref={ref}>
      <input
        className="input-field feed-search-bar"
        placeholder="🔍 Search…"
        value={query}
        onChange={(e) => { setQuery(e.target.value); setOpen(true); }}
        onFocus={() => setOpen(true)}
      />
      {open && (
        <div className="feed-search-panel">
          <div className="feed-search-scopes">
            {[
              { id: "all", label: "All" },
              { id: "following", label: "👥" },
              { id: "feeds", label: "📡" },
              { id: "hashtags", label: "#️⃣" },
            ].map((s) => (
              <button
                key={s.id}
                className={`feed-search-scope${scope === s.id ? " active" : ""}`}
                onClick={() => setScope(s.id)}
              >
                {s.label}
              </button>
            ))}
          </div>
          <div className="feed-search-results">
            {visibleGroups.map((group) => (
              <div key={group.id} className="feed-search-group">
                <div className="feed-search-group-title">
                  {group.icon} {group.label}
                </div>
                {group.items.slice(0, 8).map((item) => (
                  <button
                    key={item.id}
                    className="feed-search-item"
                    onClick={() => {
                      onScope(item, group.id);
                      setOpen(false);
                      setQuery("");
                    }}
                  >
                    {item.avatar ? <img src={item.avatar} alt="" className="feed-search-avatar" /> : null}
                    <span className="feed-search-item-label">{item.label}</span>
                    <span
                      className="chip-x"
                      title={`Remove ${item.label}`}
                      onClick={(e) => {
                        e.stopPropagation(); // remove must never trigger onScope
                        setDeleteTarget({ groupId: group.id, item });
                      }}
                    >×</span>
                  </button>
                ))}
                {group.items.length > 8 && (
                  <div className="feed-search-more">+{group.items.length - 8} more…</div>
                )}
              </div>
            ))}
            {showGenGroup && (
              <div className="feed-search-group">
                <div className="feed-search-group-title">✨ Generated feeds</div>
                {matchedTopics.slice(0, 8).map((t) => {
                  const subscribedFeed = subscribedFeedByRkey[t.rkey];
                  return (
                    <div key={t.rkey} className="feed-search-item feed-search-add-row">
                      <span className="feed-search-item-label" title={t.description}>{t.name}</span>
                      <button
                        className={`btn feed-search-add-btn${subscribedFeed ? " feed-search-subscribed" : ""}`}
                        disabled={subscribingRkey === t.rkey}
                        onClick={async () => {
                          if (subscribedFeed && feedsGroup) {
                            // Already subscribed → same confirm/delete path as chip-x.
                            setDeleteTarget({ groupId: feedsGroup.id, item: subscribedFeed });
                            return;
                          }
                          setSubscribingRkey(t.rkey);
                          try {
                            await onSubscribeGenTopic?.(t);
                            window.dispatchEvent(new CustomEvent("bytescape:toast", { detail: { message: `Subscribed to ${t.name}!`, type: "success" } }));
                            await onFeedgenSubscribed?.();
                            setOpen(false);
                            setQuery("");
                          } catch (e) {
                            window.dispatchEvent(new CustomEvent("bytescape:toast", {
                              detail: { message: e instanceof Error ? e.message : "Failed to subscribe", type: "error" },
                            }));
                          } finally {
                            setSubscribingRkey(null);
                          }
                        }}
                      >
                        {subscribingRkey === t.rkey ? "…" : subscribedFeed ? "✓ Subscribed" : "＋ Subscribe"}
                      </button>
                    </div>
                  );
                })}
                {matchedTopics.length > 8 && (
                  <div className="feed-search-more">+{matchedTopics.length - 8} more…</div>
                )}
              </div>
            )}
            {onGlobalSearch && onGlobalAdd && query.trim().length >= 2 && (
              <div className="feed-search-group">
                <div className="feed-search-group-title">
                  {searching ? "Searching Bluesky…" : "Add new"}
                </div>
                {addResults.map((r, i) => (
                  <div key={r.source_id || r.id || i} className="feed-search-item feed-search-add-row">
                    <span className="feed-search-item-label">{r.label}</span>
                    <button
                      className="btn feed-search-add-btn"
                      onClick={async () => {
                        await onGlobalAdd?.(r);
                        setOpen(false);
                        setQuery("");
                      }}
                    >
                      ＋ Add
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
      {/* Delete confirmation popup — same markup/classes as the panel */}
      {deleteTarget && (
        <div className="delete-confirm-overlay" onClick={() => setDeleteTarget(null)}>
          <div className="delete-confirm-popup" onClick={(e) => e.stopPropagation()}>
            <h3 className="delete-confirm-title">Delete this source?</h3>
            <p className="delete-confirm-name">"{deleteTarget.item.label}"</p>
            <p className="delete-confirm-warning">This will remove it from your feed. You can always add it again later.</p>
            <div className="delete-confirm-buttons">
              <button
                className="btn delete-confirm-cancel"
                onClick={() => setDeleteTarget(null)}
                autoFocus
              >Cancel</button>
              <button
                className="btn delete-confirm-delete"
                onClick={confirmDelete}
              >🔴 DELETE</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
