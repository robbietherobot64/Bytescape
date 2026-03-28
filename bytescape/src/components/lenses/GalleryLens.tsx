import { useState, useMemo, useCallback } from 'react';
import { BlobImage } from '../BlobImage';
import { SourceBadge } from '../SourceBadge';
import { Lightbox } from './Lightbox';
import { FeedEntry } from '../../types';

interface GalleryLensProps {
    entries: FeedEntry[];
    mediaDir?: string;
    resolveMediaPath?: (p: string) => string;
}

/** Extract unique source values from a list of entries */
function extractSources(entries: FeedEntry[]): string[] {
    const set = new Set<string>();
    for (const e of entries) {
        if (e.source) set.add(e.source);
    }
    return Array.from(set).sort();
}

/** Count media items for a given entry */
function mediaCount(entry: FeedEntry): number {
    return entry.media_urls.length;
}

/**
 * GalleryLens — masonry grid view for Gallery-type BytescapeItems.
 *
 * Shows only entries that have media. Renders them in a responsive
 * CSS-columns masonry layout with source filtering and lightbox expand.
 */
export function GalleryLens({ entries, mediaDir, resolveMediaPath }: GalleryLensProps) {
    const [sourceFilter, setSourceFilter] = useState<string>('all');
    const [lightboxOpen, setLightboxOpen] = useState(false);
    const [lightboxIndex, setLightboxIndex] = useState(0);
    const [lightboxMediaIndex, setLightboxMediaIndex] = useState(0);

    // Filter to entries with media, optionally by source
    const galleryEntries = useMemo(() => {
        return entries.filter(e => {
            if (e.media_urls.length === 0) return false;
            if (sourceFilter !== 'all' && e.source !== sourceFilter) return false;
            return true;
        });
    }, [entries, sourceFilter]);

    const sources = useMemo(() => extractSources(entries.filter(e => e.media_urls.length > 0)), [entries]);

    const totalMedia = useMemo(() => galleryEntries.reduce((sum, e) => sum + mediaCount(e), 0), [galleryEntries]);

    const openLightbox = useCallback((entryIndex: number, mediaIdx: number = 0) => {
        setLightboxIndex(entryIndex);
        setLightboxMediaIndex(mediaIdx);
        setLightboxOpen(true);
    }, []);

    const resolve = useCallback((src: string) => {
        return resolveMediaPath ? resolveMediaPath(src) : src;
    }, [resolveMediaPath]);

    const formatDate = (ts: number) => {
        const d = new Date(ts * 1000);
        return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
    };

    return (
        <div className="gallery-lens">
            {/* Toolbar */}
            <div className="gallery-lens-toolbar">
                <div className="gallery-lens-stats">
                    <span className="gallery-lens-count">📸 {totalMedia} media</span>
                    <span className="gallery-lens-items">{galleryEntries.length} posts</span>
                </div>
                <div className="gallery-lens-filters">
                    <select
                        className="gallery-lens-source-select"
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
            {galleryEntries.length === 0 && (
                <div className="gallery-lens-empty">
                    <div className="empty-icon">🖼️</div>
                    <div className="empty-title">No gallery items</div>
                    <div className="empty-hint">
                        {sourceFilter !== 'all'
                            ? `No media found from ${sourceFilter}. Try "All Sources".`
                            : 'Import photos from your archives or post media to see them here.'}
                    </div>
                </div>
            )}

            {/* Masonry Grid */}
            <div className="gallery-masonry">
                {galleryEntries.map((entry, entryIdx) => (
                    entry.media_urls.map((url, mediaIdx) => {
                        const kind = (entry.media_kinds && entry.media_kinds[mediaIdx]) || 'image';
                        const resolved = resolve(url);
                        const thumbResolved = entry.media_thumbnails[mediaIdx]
                            ? resolve(entry.media_thumbnails[mediaIdx])
                            : undefined;

                        return (
                            <div
                                key={`${entry.id}-${mediaIdx}`}
                                className="gallery-masonry-item"
                                onClick={() => openLightbox(entryIdx, mediaIdx)}
                            >
                                {kind === 'video' ? (
                                    <div className="gallery-masonry-video">
                                        <BlobImage
                                            src={thumbResolved || resolved}
                                            alt=""
                                            mediaDir={mediaDir}
                                            className="gallery-masonry-img"
                                            displayVariant
                                        />
                                        <div className="gallery-masonry-video-badge">▶</div>
                                    </div>
                                ) : (
                                    <BlobImage
                                        src={resolved}
                                        alt={entry.content?.slice(0, 60) || ''}
                                        mediaDir={mediaDir}
                                        className="gallery-masonry-img"
                                        displayVariant
                                    />
                                )}

                                {/* Hover overlay */}
                                <div className="gallery-masonry-overlay">
                                    <SourceBadge source={entry.source} />
                                    <span className="gallery-masonry-date">{formatDate(entry.timestamp)}</span>
                                    {entry.content && (
                                        <span className="gallery-masonry-caption">
                                            {entry.content.length > 60
                                                ? entry.content.slice(0, 60) + '…'
                                                : entry.content}
                                        </span>
                                    )}
                                </div>
                            </div>
                        );
                    })
                ))}
            </div>

            {/* Lightbox */}
            {lightboxOpen && (
                <Lightbox
                    items={galleryEntries}
                    initialIndex={lightboxIndex}
                    initialMediaIndex={lightboxMediaIndex}
                    onClose={() => setLightboxOpen(false)}
                    mediaDir={mediaDir}
                />
            )}
        </div>
    );
}
