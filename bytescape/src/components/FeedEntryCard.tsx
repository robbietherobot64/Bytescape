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

/* ─── SVG Icons for Engagement Bar ─── */

function ReplyIcon() {
    return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z" />
        </svg>
    );
}

function RepostIcon() {
    return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="17 1 21 5 17 9" />
            <path d="M3 11V9a4 4 0 0 1 4-4h14" />
            <polyline points="7 23 3 19 7 15" />
            <path d="M21 13v2a4 4 0 0 1-4 4H3" />
        </svg>
    );
}

function HeartIcon({ filled }: { filled?: boolean }) {
    if (filled) {
        return (
            <svg viewBox="0 0 24 24" fill="currentColor" stroke="none">
                <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
            </svg>
        );
    }
    return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
        </svg>
    );
}

function ShareIcon() {
    return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
            <path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8" />
            <polyline points="16 6 12 2 8 6" />
            <line x1="12" y1="2" x2="12" y2="15" />
        </svg>
    );
}

function ExpandIcon() {
    return (
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" width="16" height="16">
            <polyline points="15 3 21 3 21 9" />
            <polyline points="9 21 3 21 3 15" />
            <line x1="21" y1="3" x2="14" y2="10" />
            <line x1="3" y1="21" x2="10" y2="14" />
        </svg>
    );
}

/* ─── Helpers ─── */

function parseTaggedPeople(json?: string): TaggedPerson[] {
    if (!json) return [];
    try {
        return JSON.parse(json) as TaggedPerson[];
    } catch {
        return [];
    }
}

function renderTaggedPeople(tags: TaggedPerson[]): string | null {
    if (tags.length === 0) return null;
    const names = tags.map(t => t.linked_handle ? `@${t.linked_handle}` : '👤');
    if (names.length === 1) return `with ${names[0]}`;
    if (names.length === 2) return `with ${names[0]} and ${names[1]}`;
    return `with ${names[0]} and ${names.length - 1} others`;
}

function formatTimestamp(ts: number): string {
    const date = new Date(ts * 1000);
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffSec = Math.floor(diffMs / 1000);
    const diffMin = Math.floor(diffSec / 60);
    const diffHrs = Math.floor(diffMin / 60);
    const diffDays = Math.floor(diffHrs / 24);

    if (diffSec < 60) return 'just now';
    if (diffMin < 60) return `${diffMin}m`;
    if (diffHrs < 24) return `${diffHrs}h`;
    if (diffDays === 1) return 'Yesterday';
    if (diffDays < 7) return date.toLocaleDateString(undefined, { weekday: 'short' });
    return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

function formatCount(n?: number): string {
    if (!n || n === 0) return '';
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
    return n.toString();
}

function sanitizeDisplayContent(raw: string): string {
    return raw.replace(/https?:\/\/[^\s]+/g, (match) => {
        if (match.length > 50) return match.substring(0, 47) + '...';
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
            return { emoji: '🔒', text: 'Stored Locally', className: 'local' };
        default:
            return null;
    }
}

/* ─── Component ─── */

export function FeedEntryCard({ entry, resolveMediaPath, mediaDir, onSyndicate, isQuote = false }: FeedEntryCardProps) {
    const [isGalleryOpen, setIsGalleryOpen] = useState(false);
    const [galleryIndex, setGalleryIndex] = useState(0);
    const taggedPeople = parseTaggedPeople(entry.tagged_people);
    const tagLine = renderTaggedPeople(taggedPeople);

    const mediaSources = entry.media_urls.map(src => resolveMediaPath ? resolveMediaPath(src) : src);
    const thumbnailSources = entry.media_thumbnails.map(src => resolveMediaPath ? resolveMediaPath(src) : src);

    const authorName = entry.author_handle || 'You';
    const displayContent = sanitizeDisplayContent(entry.content || '');
    const isGif = looksLikeGif(entry.link_url);
    const externalHost = entry.link_url ? safeHostname(entry.link_url) : '';
    const hasMedia = entry.media_urls.length > 0;

    const currentSyndication = syndicationLabel(entry.syndication_status);
    const canSyndicate = entry.syndication_status === 'LocalOnly' && (entry.source !== 'bluesky' && entry.source !== 'rss');

    const openGallery = (index: number) => {
        setGalleryIndex(index);
        setIsGalleryOpen(true);
    };

    const handleSyndicate = (e: React.MouseEvent) => {
        e.stopPropagation();
        if (onSyndicate) onSyndicate(entry);
    };

    return (
        <div className={`feed-entry-card ${isQuote ? 'is-quote' : ''}`}>
            {/* Repost Banner */}
            {entry.reposted_by_handle && !isQuote && (
                <div className="repost-banner">
                    <RepostIcon /> {entry.reposted_by_handle} reposted
                </div>
            )}

            {/* Header */}
            <div className="feed-entry-header">
                <div className="avatar">
                    {entry.author_avatar ? (
                        <img src={entry.author_avatar} alt="" />
                    ) : (
                        authorName.charAt(0).toUpperCase()
                    )}
                </div>
                <div className="author-info">
                    <div className="author-name">{authorName}</div>
                    <div className="post-time">{formatTimestamp(entry.timestamp)}</div>
                </div>
                {!isQuote && (
                    <div className="header-actions">
                        {canSyndicate && (
                            <button
                                className="syndicate-button"
                                onClick={handleSyndicate}
                                title="Syndicate to Bluesky"
                                style={{
                                    background: 'var(--accent-purple)',
                                    color: 'white',
                                    border: 'none',
                                    borderRadius: 6,
                                    padding: '3px 10px',
                                    fontSize: '0.72rem',
                                    fontWeight: 600,
                                    cursor: 'pointer',
                                    display: 'flex',
                                    alignItems: 'center',
                                    gap: 4
                                }}
                            >
                                🌐 Syndicate
                            </button>
                        )}
                        {currentSyndication && !canSyndicate && (
                            <span title={currentSyndication.text} style={{ fontSize: '0.85rem' }}>
                                {currentSyndication.emoji}
                            </span>
                        )}
                        <SourceBadge source={entry.source} />
                    </div>
                )}
            </div>

            {/* Syndicated link */}
            {entry.syndicated_uri && (
                <div className="syndicated-link">
                    <a href={entry.syndicated_uri} target="_blank" rel="noopener noreferrer">
                        🔗 View on Bluesky
                    </a>
                </div>
            )}

            {/* Content Body */}
            <div className="feed-entry-body">
                {displayContent && (
                    <div className="post-text">
                        {displayContent}
                        {tagLine && <span className="post-context-chip"> — {tagLine}</span>}
                    </div>
                )}

                {/* Location chip */}
                {entry.location && (
                    <div className="post-context-chip" style={{ marginTop: 4 }}>
                        📍 {entry.location}
                    </div>
                )}

                {/* Media Grid */}
                {hasMedia && (
                    <div className={`post-media-grid items-${Math.min(entry.media_urls.length, 4)}`}>
                        {entry.media_urls.slice(0, 4).map((_, i) => {
                            const kind = (entry.media_kinds && entry.media_kinds[i]) || 'image';
                            const thumb = thumbnailSources[i] || mediaSources[i];

                            return (
                                <div key={i} className="media-item">
                                    {kind === 'video' ? (
                                        /* Inline video player — plays directly in the feed */
                                        <div onClick={(e) => e.stopPropagation()}>
                                            <VideoPlayer
                                                src={mediaSources[i]}
                                                poster={thumb}
                                                className="inline-video-container"
                                                style={{ width: '100%', height: '100%' }}
                                            />
                                            <button
                                                className="media-expand-btn"
                                                onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                                                title="Open fullscreen"
                                            >
                                                <ExpandIcon />
                                            </button>
                                        </div>
                                    ) : (
                                        /* Image */
                                        <div onClick={(e) => { e.stopPropagation(); openGallery(i); }}>
                                            <BlobImage
                                                src={mediaSources[i]}
                                                alt=""
                                                style={{ width: '100%', height: '100%', objectFit: 'cover', display: 'block' }}
                                            />
                                            <button
                                                className="media-expand-btn"
                                                onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                                                title="Open fullscreen"
                                            >
                                                <ExpandIcon />
                                            </button>
                                        </div>
                                    )}
                                </div>
                            );
                        })}
                    </div>
                )}

                {/* Link Preview */}
                {entry.link_url && !isGif && !hasMedia && (
                    <a href={entry.link_url} target="_blank" rel="noopener noreferrer" className="link-preview-card">
                        {entry.link_thumbnail && (
                            <div className="link-thumb">
                                <img src={entry.link_thumbnail} alt="" />
                            </div>
                        )}
                        <div className="link-meta">
                            {externalHost && <div className="link-host">{externalHost}</div>}
                            <div className="link-title">{entry.link_title || entry.link_url}</div>
                            {entry.link_description && <div className="link-desc">{entry.link_description}</div>}
                        </div>
                    </a>
                )}

                {/* Quoted Post */}
                {entry.quoted_post && (
                    <FeedEntryCard entry={entry.quoted_post} isQuote={true} resolveMediaPath={resolveMediaPath} mediaDir={mediaDir} />
                )}
            </div>

            {/* Engagement Action Bar */}
            {!isQuote && (
                <div className="engagement-bar">
                    <button className="engagement-btn btn-reply" title="Replies">
                        <ReplyIcon />
                        <span className="count">{formatCount(entry.reply_count)}</span>
                    </button>
                    <button className="engagement-btn btn-repost" title="Reposts">
                        <RepostIcon />
                        <span className="count">{formatCount(entry.repost_count)}</span>
                    </button>
                    <button className={`engagement-btn btn-like ${entry.like_count ? '' : ''}`} title="Likes">
                        <HeartIcon filled={false} />
                        <span className="count">{formatCount(entry.like_count)}</span>
                    </button>
                    <button className="engagement-btn btn-share" title="Share" style={{ marginLeft: 'auto' }}>
                        <ShareIcon />
                    </button>
                </div>
            )}

            {/* Fullscreen Gallery Portal — UNCHANGED from original */}
            {isGalleryOpen && createPortal(
                <div className="gallery-overlay" onClick={() => setIsGalleryOpen(false)}>
                    <div className="gallery-content" onClick={e => e.stopPropagation()}>
                        {(entry.media_kinds && entry.media_kinds[galleryIndex]) === 'video' ? (
                            <VideoPlayer
                                src={mediaSources[galleryIndex]}
                                poster={thumbnailSources[galleryIndex] || mediaSources[galleryIndex]}
                                autoPlay={true}
                            />
                        ) : (
                            <img
                                src={mediaSources[galleryIndex]}
                                alt=""
                                style={{ maxWidth: '100%', maxHeight: '90vh', objectFit: 'contain' }}
                            />
                        )}
                        {entry.media_urls.length > 1 && (
                            <div className="gallery-dots">
                                {entry.media_urls.map((_, i) => (
                                    <button
                                        key={i}
                                        className={`gallery-dot ${i === galleryIndex ? 'active' : ''}`}
                                        onClick={() => setGalleryIndex(i)}
                                    />
                                ))}
                            </div>
                        )}
                        <button className="gallery-close-btn" onClick={() => setIsGalleryOpen(false)}>✕</button>
                    </div>
                </div>,
                document.body
            )}
        </div>
    );
}
