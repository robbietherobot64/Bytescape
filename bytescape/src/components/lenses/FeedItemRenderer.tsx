import { FeedEntry } from '../../types';
import { FeedEntryCard } from '../FeedEntryCard';

interface FeedItemRendererProps {
    entry: FeedEntry;
    resolveMediaPath?: (path: string) => string;
    mediaDir?: string;
}

const TYPE_ICONS: Record<string, string> = {
    Gallery: '🖼️',
    Article: '📰',
};

/**
 * Wraps FeedEntryCard with type-specific styling based on BytescapeType.
 * Status items render plain; Gallery and Article get a type badge and accent class.
 */
export function FeedItemRenderer({ entry, resolveMediaPath, mediaDir }: FeedItemRendererProps) {
    const itemType = entry.item_type || 'Status';
    const typeClass = `feed-item-type-${itemType.toLowerCase()}`;
    const icon = TYPE_ICONS[itemType];

    return (
        <div className={`feed-item-renderer ${typeClass}`}>
            {icon && (
                <div className="feed-item-type-badge">
                    <span className="feed-item-type-icon">{icon}</span>
                    <span className="feed-item-type-label">{itemType}</span>
                </div>
            )}
            {itemType === 'Article' && entry.link_url && (
                <div className="feed-item-article-title">
                    <a href={entry.link_url} target="_blank" rel="noopener noreferrer">
                        {entry.link_url}
                    </a>
                </div>
            )}
            <FeedEntryCard
                entry={entry}
                resolveMediaPath={resolveMediaPath}
                mediaDir={mediaDir}
            />
        </div>
    );
}
