import { useState } from 'react';
import { createPortal } from 'react-dom';
import { FeedEntry, TaggedPerson } from '../types';
import { SourceBadge } from './SourceBadge';
import { BlobImage } from './BlobImage';
import { VideoPlayer } from './VideoPlayer';
import { safeHostname, looksLikeGif } from '../utils/content';

interface FeedEntryCardProps {
    entry: FeedEntry;
    resolveMediaPath?: (p: string) => string;
    mediaDir?: string;
    onSyndicate?: (entry: FeedEntry) => void;
    isQuote?: boolean;
}

/** Safely parse JSON-serialized tagged people from the vault */
function parseTaggedPeople(json?: string): TaggedPerson[] {
    if (!json) return [];
    try {
        return JSON.parse(json) as TaggedPerson[];
    } catch {
        return [];
    }
}

/** Render tagged people as privacy-aware placeholders. */
function renderTaggedPeople(tags: TaggedPerson[]): string | null {
    if (tags.length === 0) return null;
    const names = tags.map(t =>
        t.linked_handle ? `@${t.linked_handle}` : '👤'
    );
    if (names.length === 1) return `with ${names[0]} `;
    if (names.length === 2) return `with ${names[0]} and ${names[1]} `;
    return `with ${names[0]} and ${names.length - 1} others`;
}

function formatTimestamp(ts: number): string {
    const date = new Date(ts * 1000);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

    if (diffDays === 0) {
        return date.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
    } else if (diffDays === 1) {
        return 'Yesterday';
    } else if (diffDays < 7) {
        return date.toLocaleDateString(undefined, { weekday: 'long' });
    } else {
        return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
    }
}

function sanitizeDisplayContent(raw: string): string {
    return raw.replace(/https?:\/\/[^\s]+/g, (match) => {
        if (match.length > 40) return match.substring(0, 37) + '...';
        return match;
    });
}

function syndicationLabel(status?: string) {
    switch (status) {
        case 'Syndicated':
            return { emoji: '✅', text: 'Syndicated to Bluesky', className: 'syndicated' };
        case 'PendingSyndication':
            return { emoji: '⏳', text: 'Syndication Pending', className: 'pending' };
        case 'LocalOnly':
            return { emoji: '🔒', text: 'Stored Locally Only', className: 'local' };
        default:
            return { emoji: '🗄️', text: 'Vault Item', className: 'default' };
    }
}

export function FeedEntryCard({ entry, resolveMediaPath, mediaDir, onSyndicate, isQuote = false }: FeedEntryCardProps) {
    const [isGalleryOpen, setIsGalleryOpen] = useState(false);
    const [galleryIndex, setGalleryIndex] = useState(0);
    const taggedPeople = parseTaggedPeople(entry.tagged_people);
    const tagLine = renderTaggedPeople(taggedPeople);

    const handleSyndicate = (e: React.MouseEvent) => {
        e.stopPropagation();
        if (onSyndicate) {
            onSyndicate(entry);
        } else {
            console.warn("FeedEntryCard: No onSyndicate callback provided.");
        }
    };

    const mediaSources = entry.media_urls.map(src => resolveMediaPath ? resolveMediaPath(src) : src);
    const thumbnailSources = entry.media_thumbnails.map(src => resolveMediaPath ? resolveMediaPath(src) : src);

    const authorName = entry.author_handle || 'You';
    const displayContent = sanitizeDisplayContent(entry.content || '');
    const isGif = looksLikeGif(entry.link_url);
    const externalHost = entry.link_url ? safeHostname(entry.link_url) : '';

    const currentSyndication = syndicationLabel(entry.syndication_status);
    const canSyndicate = entry.syndication_status === 'LocalOnly' && (entry.source !== 'bluesky' && entry.source !== 'rss');

    const openGallery = (index: number) => {
        setGalleryIndex(index);
        setIsGalleryOpen(true);
    };

    const hasMedia = entry.media_urls.length > 0;

    return (
        <div className={`feed-entry-card ${isQuote ? 'is-quote' : ''}`} onClick={() => !isQuote && hasMedia && openGallery(0)}>
            {/* Header */}
            <div className="feed-entry-header" style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 16px' }}>
                <div style={{ width: 36, height: 36, background: 'var(--brand-primary)', borderRadius: '50%', color: 'white', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '1rem', fontWeight: 600 }}>
                    {authorName.charAt(0).toUpperCase()}
                </div>
                <div style={{ minWidth: 0 }}>
                    <div style={{ fontWeight: 600, fontSize: '0.94rem', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{authorName}</div>
                    <div style={{ fontSize: '0.78rem', color: 'var(--text-muted)' }}>{formatTimestamp(entry.timestamp)}</div>
                </div>
                {!isQuote && (
                    <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 8 }}>
                        {canSyndicate ? (
                            <button
                                className="syndicate-button"
                                onClick={handleSyndicate}
                                title="Syndicate to Bluesky"
                                style={{
                                    background: 'var(--brand-primary)',
                                    color: 'white',
                                    border: 'none',
                                    borderRadius: 6,
                                    padding: '2px 8px',
                                    fontSize: '0.75rem',
                                    fontWeight: 600,
                                    cursor: 'pointer',
                                    display: 'flex',
                                    alignItems: 'center',
                                    gap: 4
                                }}
                            >
                                🌐 Syndicate
                            </button>
                        ) : entry.syndication_status && (
                            <span className={`syndication-badge ${currentSyndication.className}`} title={currentSyndication.text}>
                                {currentSyndication.emoji}
                            </span>
                        )}
                        <SourceBadge source={entry.source} />
                    </div>
                )}
            </div>

            {entry.syndicated_uri && (
                <div style={{ fontSize: '0.75rem', marginTop: -8, marginBottom: 8, padding: '0 16px' }}>
                    <a href={entry.syndicated_uri} target="_blank" rel="noopener noreferrer" style={{ color: 'var(--brand-primary)', textDecoration: 'none' }}>
                        🔗 View on Bluesky
                    </a>
                </div>
            )}

            {/* Content Area */}
            <div className="feed-entry-body" style={{ padding: '0 16px 16px 16px' }}>
                <div className="post-text" style={{ marginBottom: 12, lineHeight: 1.5, wordBreak: 'break-word' }}>
                    {displayContent}
                    {tagLine && <span style={{ opacity: 0.6, fontSize: '0.9em', marginLeft: 4 }}>{tagLine}</span>}
                </div>

                {/* Media Section */}
                {hasMedia && (
                    <div className={`post-media-grid items-${entry.media_urls.length}`} style={{ marginBottom: 12 }}>
                        {entry.media_urls.map((_, i) => {
                            const kind = (entry.media_kinds && entry.media_kinds[i]) || 'image';
                            const thumb = thumbnailSources[i] || mediaSources[i];
                            return (
                                <div key={i} className="media-item" onClick={(e) => { e.stopPropagation(); openGallery(i); }}>
                                    {kind === 'video' ? (
                                        <div style={{ position: 'relative' }}>
                                            <BlobImage src={thumb} alt="" style={{ borderRadius: 8, width: '100%', height: '100%', objectFit: 'cover' }} />
                                            <div style={{ position: 'absolute', top: '50%', left: '50%', transform: 'translate(-50%, -50%)', background: 'rgba(0,0,0,0.5)', borderRadius: '50%', width: 40, height: 40, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                                                ▶️
                                            </div>
                                        </div>
                                    ) : (
                                        <BlobImage src={mediaSources[i]} alt="" style={{ borderRadius: 8, width: '100%', height: '100%', objectFit: 'cover' }} />
                                    )}
                                </div>
                            );
                        })}
                    </div>
                )}

                {/* Link Preview Section */}
                {entry.link_url && !isGif && !hasMedia && (
                    <a href={entry.link_url} target="_blank" rel="noopener noreferrer" className="link-preview-card" style={{ display: 'block', textDecoration: 'none', border: '1px solid var(--border-color)', borderRadius: 10, overflow: 'hidden', background: 'var(--bg-secondary)' }}>
                        {entry.link_thumbnail && (
                            <div style={{ height: 160, overflow: 'hidden' }}>
                                <img src={entry.link_thumbnail} alt="" style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
                            </div>
                        )}
                        <div style={{ padding: 12 }}>
                            {externalHost && <div style={{ fontSize: '0.75rem', color: 'var(--text-muted)', marginBottom: 4, textTransform: 'uppercase', letterSpacing: '0.05em' }}>{externalHost}</div>}
                            <div style={{ fontWeight: 600, fontSize: '0.95rem', color: 'var(--text-primary)', marginBottom: 4 }}>{entry.link_title || entry.link_url}</div>
                            {entry.link_description && <div style={{ fontSize: '0.85rem', color: 'var(--text-secondary)', display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}>{entry.link_description}</div>}
                        </div>
                    </a>
                )}

                {/* Quoted Post Support */}
                {entry.quoted_post && (
                    <div style={{ marginTop: 12 }}>
                        <FeedEntryCard entry={entry.quoted_post} isQuote={true} resolveMediaPath={resolveMediaPath} mediaDir={mediaDir} />
                    </div>
                )}
            </div>

            {/* Gallery Portal */}
            {isGalleryOpen && createPortal(
                <div className="gallery-overlay" onClick={() => setIsGalleryOpen(false)} style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.95)', zIndex: 1000, display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 20 }}>
                    <div className="gallery-content" onClick={e => e.stopPropagation()} style={{ position: 'relative', maxWidth: '90vw', maxHeight: '90vh' }}>
                        {(entry.media_kinds && entry.media_kinds[galleryIndex]) === 'video' ? (
                            <VideoPlayer
                                src={mediaSources[galleryIndex]}
                                poster={thumbnailSources[galleryIndex] || mediaSources[galleryIndex]}
                            />
                        ) : (
                            <img src={mediaSources[galleryIndex]} alt="" style={{ maxWidth: '100%', maxHeight: '90vh', objectFit: 'contain' }} />
                        )}
                        {entry.media_urls.length > 1 && (
                            <div style={{ position: 'absolute', bottom: -40, left: '50%', transform: 'translateX(-50%)', display: 'flex', gap: 10 }}>
                                {entry.media_urls.map((_, i) => (
                                    <button key={i} onClick={() => setGalleryIndex(i)} style={{ width: 10, height: 10, borderRadius: '50%', background: i === galleryIndex ? 'white' : 'white(255,255,255,0.3)', border: 'none', cursor: 'pointer' }} />
                                ))}
                            </div>
                        )}
                        <button onClick={() => setIsGalleryOpen(false)} style={{ position: 'absolute', top: -50, right: -50, background: 'none', border: 'none', color: 'white', fontSize: 32, cursor: 'pointer' }}>✕</button>
                    </div>
                </div>,
                document.body
            )}
        </div>
    );
}
