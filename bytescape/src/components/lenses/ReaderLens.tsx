import { useState, useEffect, useRef, useMemo } from 'react';
import { SourceBadge } from '../SourceBadge';
import { BlobImage } from '../BlobImage';
import { FeedEntry } from '../../types';

interface ReaderLensProps {
    entries: FeedEntry[];
    mediaDir?: string;
    resolveMediaPath?: (p: string) => string;
}

/** Estimate reading time in minutes from word count */
function estimateReadingTime(text: string): number {
    const words = text.trim().split(/\s+/).length;
    return Math.max(1, Math.ceil(words / 200));
}

/** Format a unix timestamp to a readable date */
function formatDate(ts: number): string {
    const d = new Date(ts * 1000);
    return d.toLocaleDateString(undefined, {
        year: 'numeric', month: 'long', day: 'numeric',
    });
}

/**
 * ReaderArticle — renders a single Article-type entry in clean
 * typography with reading time and scroll progress.
 */
function ReaderArticle({ entry, mediaDir, resolveMediaPath }: {
    entry: FeedEntry;
    mediaDir?: string;
    resolveMediaPath?: (p: string) => string;
}) {
    const [expanded, setExpanded] = useState(false);
    const [scrollProgress, setScrollProgress] = useState(0);
    const articleRef = useRef<HTMLDivElement>(null);

    const readingTime = useMemo(() => estimateReadingTime(entry.content || ''), [entry.content]);
    const isLong = (entry.content || '').length > 600;

    useEffect(() => {
        if (!expanded || !articleRef.current) return;

        const container = articleRef.current;
        const handleScroll = () => {
            const { scrollTop, scrollHeight, clientHeight } = container;
            const maxScroll = scrollHeight - clientHeight;
            if (maxScroll > 0) {
                setScrollProgress(Math.min((scrollTop / maxScroll) * 100, 100));
            }
        };

        container.addEventListener('scroll', handleScroll);
        return () => container.removeEventListener('scroll', handleScroll);
    }, [expanded]);

    const resolve = (src: string) => resolveMediaPath ? resolveMediaPath(src) : src;

    const heroImage = entry.media_urls.length > 0 ? resolve(entry.media_urls[0]) : entry.link_thumbnail;

    return (
        <article className={`reader-article ${expanded ? 'expanded' : ''}`}>
            {/* Scroll progress bar (visible when expanded) */}
            {expanded && (
                <div className="reader-progress-track">
                    <div
                        className="reader-progress-fill"
                        style={{ width: `${scrollProgress}%` } as React.CSSProperties}
                    />
                </div>
            )}

            {/* Hero image */}
            {heroImage && (
                <div className="reader-hero">
                    <BlobImage
                        src={heroImage}
                        alt=""
                        mediaDir={mediaDir}
                        className="reader-hero-img"
                    />
                </div>
            )}

            {/* Article header */}
            <div className="reader-header">
                <h2 className="reader-title">
                    {entry.title || entry.link_title || 'Untitled Article'}
                </h2>
                <div className="reader-meta">
                    <SourceBadge source={entry.source} />
                    <span className="reader-date">{formatDate(entry.timestamp)}</span>
                    <span className="reader-reading-time">📖 {readingTime} min read</span>
                </div>
                {entry.author_handle && (
                    <div className="reader-author">
                        {entry.author_avatar && (
                            <img src={entry.author_avatar} alt="" className="reader-author-avatar" />
                        )}
                        <span className="reader-author-name">{entry.author_handle}</span>
                    </div>
                )}
            </div>

            {/* Article body */}
            <div
                className={`reader-body ${!expanded && isLong ? 'truncated' : ''}`}
                ref={expanded ? articleRef : undefined}
            >
                <div className="reader-content">
                    {entry.content}
                </div>

                {/* Link preview as article source */}
                {entry.link_url && (
                    <a
                        href={entry.link_url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="reader-source-link"
                    >
                        🔗 {entry.link_title || entry.link_url}
                        {entry.link_description && (
                            <span className="reader-link-desc">{entry.link_description}</span>
                        )}
                    </a>
                )}
            </div>

            {/* Read more / collapse toggle */}
            {isLong && (
                <button
                    className="reader-expand-btn"
                    onClick={() => setExpanded(!expanded)}
                >
                    {expanded ? '▲ Collapse' : '▼ Read more'}
                </button>
            )}
        </article>
    );
}

/**
 * ReaderLens — article reading view for Article-type BytescapeItems.
 *
 * Clean typography, reading-time estimate, scroll progress indicator.
 * Filters to only Article-type items or items with significant text content.
 */
export function ReaderLens({ entries, mediaDir, resolveMediaPath }: ReaderLensProps) {
    const [sourceFilter, setSourceFilter] = useState<string>('all');

    const articleEntries = useMemo(() => {
        return entries.filter(e => {
            // Show Article-type items, or items with long text content (300+ chars)
            const isArticle = e.item_type === 'Article' ||
                (e.content && e.content.length >= 300) ||
                e.link_url;
            if (!isArticle) return false;
            if (sourceFilter !== 'all' && e.source !== sourceFilter) return false;
            return true;
        });
    }, [entries, sourceFilter]);

    const sources = useMemo(() => {
        const set = new Set<string>();
        entries.forEach(e => {
            const isArticle = e.item_type === 'Article' ||
                (e.content && e.content.length >= 300) ||
                e.link_url;
            if (isArticle && e.source) set.add(e.source);
        });
        return Array.from(set).sort();
    }, [entries]);

    return (
        <div className="reader-lens">
            {/* Toolbar */}
            <div className="reader-lens-toolbar">
                <div className="reader-lens-stats">
                    <span className="reader-lens-count">📰 {articleEntries.length} articles</span>
                </div>
                <div className="reader-lens-filters">
                    <select
                        className="reader-lens-source-select"
                        value={sourceFilter}
                        onChange={e => setSourceFilter(e.target.value)}
                    >
                        <option value="all">All Sources</option>
                        {sources.map(s => (
                            <option key={s} value={s}>{s.charAt(0).toUpperCase() + s.slice(1)}</option>
                        ))}
                    </select>
                </div>
            </div>

            {/* Empty state */}
            {articleEntries.length === 0 && (
                <div className="reader-lens-empty">
                    <div className="empty-icon">📰</div>
                    <div className="empty-title">No articles</div>
                    <div className="empty-hint">
                        {sourceFilter !== 'all'
                            ? `No articles found from ${sourceFilter}. Try "All Sources".`
                            : 'Import or save articles to read them in this view.'}
                    </div>
                </div>
            )}

            {/* Article list */}
            <div className="reader-article-list">
                {articleEntries.map(entry => (
                    <ReaderArticle
                        key={entry.id}
                        entry={entry}
                        mediaDir={mediaDir}
                        resolveMediaPath={resolveMediaPath}
                    />
                ))}
            </div>
        </div>
    );
}
