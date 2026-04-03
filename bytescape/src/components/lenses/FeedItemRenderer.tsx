import { FeedEntry } from '../../types';
import { FeedEntryCard } from '../FeedEntryCard';

/**
 * PURPOSE:
 * Applies type-specific visual framing around a base `FeedEntryCard`.
 *
 * HOW IT WORKS:
 * - Reads `entry.item_type` and computes a class + optional badge icon.
 * - Adds article link treatment for `Article` entries.
 * - Delegates primary feed body rendering to `FeedEntryCard`.
 *
 * RELATED FILES:
 * - src/components/feed/VirtualizedFeedList.tsx
 * - src/components/FeedEntryCard.tsx
 * - src/types/index.ts (`FeedEntry`)
 * - src-tauri/src/connectors/* (normalization into shared feed shape)
 *
 * CROSS-SYSTEM NOTES:
 * The Rust engine and connector pipeline determine the canonical `item_type`.
 * This renderer is intentionally a thin composition layer and does not infer
 * or rewrite semantic type classifications.
 */

interface FeedItemRendererProps {
    entry: FeedEntry;
    resolveMediaPath?: (path: string) => string;
    mediaDir?: string;
}

// ============================================================================
// Visual affordance mapping
// ============================================================================

const TYPE_ICONS: Record<string, string> = {
    Gallery: '🖼️',
    Article: '📰',
};

/**
 * Exported item renderer wrapper for lens/type presentation.
 *
 * HOW IT WORKS:
 * - Computes per-type style class and optional icon badge.
 * - Injects lightweight article link chrome where relevant.
 * - Composes `FeedEntryCard` for the core item body.
 *
 * Exported symbol docs:
 * - `FeedItemRenderer` is safe to reuse anywhere a normalized `FeedEntry` needs
 *   type-aware framing while preserving base card behavior.
 */
export function FeedItemRenderer({ entry, resolveMediaPath, mediaDir }: FeedItemRendererProps) {
    const itemType = entry.item_type || 'Status';
    const typeClass = `feed-item-type-${itemType.toLowerCase()}`;
    const icon = TYPE_ICONS[itemType];

    return (
        <div className={`feed-item-renderer ${typeClass}`}>
            {/* ==================== Type badge ==================== */}
            {icon && (
                <div className="feed-item-type-badge">
                    <span className="feed-item-type-icon">{icon}</span>
                    <span className="feed-item-type-label">{itemType}</span>
                </div>
            )}

            {/* ==================== Article link treatment ==================== */}
            {itemType === 'Article' && entry.link_url && (
                <div className="feed-item-article-title">
                    <a href={entry.link_url} target="_blank" rel="noopener noreferrer">
                        {entry.link_url}
                    </a>
                </div>
            )}

            {/* ==================== Base feed card ==================== */}
            <FeedEntryCard
                entry={entry}
                resolveMediaPath={resolveMediaPath}
                mediaDir={mediaDir}
            />
        </div>
    );
}
