import { useState } from "react";
import type { SourceCategory, SourceCategoryItem } from "./sourceTypes";

interface Props {
  categories: SourceCategory[];
  onScope: (item: SourceCategoryItem, categoryId: string) => void;
  /** Remove a subscription — the same handler the search dropdown's chip-x uses. */
  onDelete: (categoryId: string, itemId: string) => void | Promise<void>;
  /** Refresh subscriptions + feed after a delete succeeds. */
  onFeedgenSubscribed?: () => void | Promise<void>;
  /** Open the interests editor (gear, top-right of the header line). */
  onInterests?: () => void;
}

const GROUP_KIND: Record<string, string[]> = {
  following: ["following", "follows"],
  feeds: ["feed", "feeds"],
  hashtags: ["hashtag", "hashtags"],
  groups: ["group", "groups", "community", "communities"],
};

const SECTIONS: { id: string; icon: string; label: string }[] = [
  { id: "following", icon: "👥", label: "Following" },
  { id: "feeds", icon: "📡", label: "Feeds" },
  { id: "hashtags", icon: "#", label: "Hashtags" },
  { id: "groups", icon: "🏘️", label: "Groups" },
];

/**
 * "Your sources" panel — the same component the main feed renders. Lists the
 * categories data the search dropdown uses: click a row to scope/open that
 * source, × to remove it (confirm-then-delete, same popup/classes as the
 * feed's source flows).
 */
export default function SourcesPanel({ categories, onScope, onDelete, onFeedgenSubscribed, onInterests }: Props) {
  // Delete-confirmation popup state, mirroring the search dropdown's flow.
  const [deleteTarget, setDeleteTarget] = useState<{ groupId: string; item: SourceCategoryItem } | null>(null);
  // Sections render collapsed by default; the header count keeps them informative.
  const [openSections, setOpenSections] = useState<Record<string, boolean>>({});

  const toggleSection = (id: string) => {
    setOpenSections((prev) => ({ ...prev, [id]: !prev[id] }));
  };

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    const { groupId, item } = deleteTarget;
    setDeleteTarget(null);
    await onDelete(groupId, item.id);
    await onFeedgenSubscribed?.();
  };

  return (
    <div className="sources-panel">
      <div className="sources-panel-header">
        <span>Your sources</span>
        {onInterests && (
          <button className="feed-gear-btn" onClick={onInterests} title="Edit Interests">⚙</button>
        )}
      </div>
      <div className="sources-panel-content">
        {SECTIONS.map((sec) => {
          const group = categories.find((c) =>
            GROUP_KIND[sec.id]?.some((k) => c.id.toLowerCase().includes(k)),
          );
          const items = group?.items ?? [];
          const open = !!openSections[sec.id];
          return (
            <div key={sec.id} className="sources-section">
              <div
                className="sources-section-title"
                onClick={() => toggleSection(sec.id)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleSection(sec.id); } }}
              >
                <span className={`sources-chevron${open ? " open" : ""}`}>v</span>
                {sec.icon} {sec.label} ({items.length})
              </div>
              {!open ? null : items.length === 0 ? (
                <div className="category-empty sources-empty">Nothing yet.</div>
              ) : (
                items.map((item) => (
                  <div
                    key={item.id}
                    className="sources-item"
                    onClick={() => group && onScope(item, group.id)}
                  >
                    {item.avatar
                      ? <img src={item.avatar} alt="" className="sources-avatar" />
                      : <span className="sources-dot" />}
                    <span className="sources-item-label">{item.label}</span>
                    <span
                      className="chip-x"
                      title={`Remove ${item.label}`}
                      onClick={(e) => {
                        e.stopPropagation(); // remove must never trigger onScope
                        if (group) setDeleteTarget({ groupId: group.id, item });
                      }}
                    >×</span>
                  </div>
                ))
              )}
            </div>
          );
        })}
      </div>
      {/* Delete confirmation popup — same markup/classes as the feed's flow */}
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
