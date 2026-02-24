import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { FeedEntry, TaggedPerson, PostLocation, SyndicationStatus } from '../types';
import { invoke } from '@tauri-apps/api/core';
import { SourceBadge } from './SourceBadge';
import { BlobImage } from './BlobImage';
import { VideoPlayer } from './VideoPlayer';
import { inferContentKind, safeHostname, looksLikeGif } from '../utils/content';

interface FeedEntryCardProps {
    entry: FeedEntry;
    resolveMediaPath?: (p: string) => string;
    mediaDir?: string;
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

/** Safely parse JSON-serialized location data */
function parseLocation(json?: string): PostLocation | null {
    if (!json) return null;
    try {
        return JSON.parse(json) as PostLocation;
    } catch {
        return null;
    }
}

/** Render tagged people as privacy-aware placeholders. */
function renderTaggedPeople(tags: TaggedPerson[]): string | null {
    if (tags.length === 0) return null;
    const names = tags.map(t =>
        t.linked_handle ? `@${t.linked_handle}` : '👤'
    );
    if (names.length === 1) return `with ${names[0]}`;
    if (names.length === 2) return `with ${names[0]} and ${names[1]}`;
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
    } else if (date.getFullYear() === now.getFullYear()) {
        return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
    } else {
        return date.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
    }
}

function sanitizeDisplayContent(content: string): string {
    if (!content) return content;
    return content
        .replace(/\bTimeline photos\b/gi, ' ')
        .replace(/\s{2,}/g, ' ')
        .trim();
}

function typeIcon(type?: string): string {
    switch (type) {
        case 'Gallery': return '🖼️';
        case 'Article': return '📄';
        case 'Status':
        default: return '💬';
    }
}

function syndicationLabel(status?: string): { text: string; emoji: string; className: string } {
    switch (status) {
        case 'PendingSyndication':
            return { text: 'Queued', emoji: '⏳', className: 'syndication-pending' };
        case 'Syndicated':
            return { text: 'Syndicated', emoji: '🌐', className: 'syndication-live' };
        case 'LocalOnly':
        default:
            return { text: 'Local Only', emoji: '🛡️', className: 'syndication-local' };
    }
}

export function FeedEntryCard({ entry, resolveMediaPath, mediaDir, isQuote = false }: FeedEntryCardProps) {
    const hasMedia = entry.media_urls.length > 0;
    const [isGalleryOpen, setIsGalleryOpen] = useState(false);
    const [galleryIndex, setGalleryIndex] = useState(0);
    const taggedPeople = parseTaggedPeople(entry.tagged_people);
    const location = parseLocation(entry.location);
    const tagLine = renderTaggedPeople(taggedPeople);

    // --- Syndication state ---
    const [isSyndicating, setIsSyndicating] = useState(false);
    const [syndicateError, setSyndicateError] = useState<string | null>(null);
    const [localSyndicationStatus, setLocalSyndicationStatus] = useState<string | undefined>(entry.syndication_status);
    const [localSyndicatedUri, setLocalSyndicatedUri] = useState<string | undefined | null>(entry.syndicated_uri);

    const handleSyndicate = async (e: React.MouseEvent) => {
        e.stopPropagation();
        if (isSyndicating) return;

        setIsSyndicating(true);
        setSyndicateError(null);
        try {
            console.log("FeedEntryCard: Syndicating item", entry.id);
            const uri = await invoke<string>('syndicate_vault_item', { itemId: entry.id });
            setLocalSyndicationStatus('Syndicated');
            setLocalSyndicatedUri(uri);
        } catch (err) {
            console.error("Syndication failed:", err);
            setLocalSyndicationStatus('LocalOnly');
            setSyndicateError(typeof err === 'string' ? err : 'Publication failed');
        } finally {
            setIsSyndicating(false);
        }
    };

    const mediaSources = entry.media_urls.map(src => resolveMediaPath ? resolveMediaPath(src) : src);
    const thumbnailSources = entry.media_thumbnails.map(src => resolveMediaPath ? resolveMediaPath(src) : src);

    const authorName = entry.author_handle || 'You';
    const displayContent = sanitizeDisplayContent(entry.content || '');
    const isGif = looksLikeGif(entry.link_url);
    const isPdfLink = !!(entry.link_url && entry.link_url.toLowerCase().includes('.pdf'));
    const externalHost = entry.link_url ? safeHostname(entry.link_url) : '';

    const currentSyndication = syndicationLabel(localSyndicationStatus);
    const canSyndicate = localSyndicationStatus === 'LocalOnly' && (entry.source !== 'bluesky' && entry.source !== 'rss');

    const openGallery = (index: number) => {
        setGalleryIndex(index);
        setIsGalleryOpen(true);
    };

    const closeGallery = () => setIsGalleryOpen(false);

    const showPrev = () => {
        setGalleryIndex(prev => (prev - 1 + mediaSources.length) % mediaSources.length);
    };

    const showNext = () => {
        setGalleryIndex(prev => (prev + 1) % mediaSources.length);
    };

    useEffect(() => {
        if (!isGalleryOpen) return;
        const onKey = (e: KeyboardEvent) => {
            if (e.key === 'Escape') closeGallery();
            if (e.key === 'ArrowLeft') showPrev();
            if (e.key === 'ArrowRight') showNext();
        };
        window.addEventListener('keydown', onKey);
        return () => window.removeEventListener('keydown', onKey);
    }, [isGalleryOpen, mediaSources.length]);

    return (
        <div className={`post-card bytescape-item-card ${isQuote ? 'quote-card' : ''}`}>
            {/* Repost Header */}
            {entry.reposted_by_handle && !isQuote && (
                <div className="repost-header" style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: 8, display: 'flex', alignItems: 'center', gap: 6 }}>
                    <span>🔁</span>
                    <span>{entry.reposted_by_handle} reposted</span>
                </div>
            )}

            {/* Header row: avatar + author + timestamp + badges */}
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <div className="post-avatar">
                    {entry.author_avatar ? (
                        <img src={entry.author_avatar} alt="" />
                    ) : (
                        <div className="avatar-placeholder">
                            {authorName[0].toUpperCase()}
                        </div>
                    )}
                </div>
                <div className="post-meta">
                    <span className="post-author-name">
                        {authorName}
                    </span>
                    {tagLine && (
                        <span className="post-tagged-people" style={{ opacity: 0.7, fontSize: '0.85em' }}>
                            {' '}{tagLine}
                        </span>
                    )}
                    <span className="post-time">
                        {formatTimestamp(entry.timestamp)}
                    </span>
                </div>
                {!isQuote && (
                    <div style={{ marginLeft: 'auto', display: 'flex', alignItems: 'center', gap: 8 }}>
                        {canSyndicate ? (
                            <button
                                className="syndicate-button"
                                onClick={handleSyndicate}
                                disabled={isSyndicating}
                                title="Syndicate to Bluesky"
                                style={{
                                    background: 'var(--brand-primary)',
                                    color: 'white',
                                    border: 'none',
                                    borderRadius: 6,
                                    padding: '2px 8px',
                                    fontSize: '0.75rem',
                                    fontWeight: 600,
                                    cursor: isSyndicating ? 'wait' : 'pointer',
                                    display: 'flex',
                                    alignItems: 'center',
                                    gap: 4
                                }}
                            >
                                {isSyndicating ? '⏳' : '🌐'} Syndicat{isSyndicating ? 'ing...' : 'e'}
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

            {syndicateError && (
                <div style={{ color: 'var(--error)', fontSize: '0.75rem', marginTop: 4, background: 'var(--error-surface)', padding: '4px 8px', borderRadius: 4 }}>
                    ⚠️ {syndicateError}
                </div>
            )}

            {localSyndicatedUri && (
                <div style={{ fontSize: '0.75rem', marginTop: 4 }}>
                    <a href={localSyndicatedUri} target="_blank" rel="noopener noreferrer" style={{ color: 'var(--brand-primary)', textDecoration: 'none' }}>
                        🔗 View on Bluesky
                    </a>
                </div>
            )}

            {/* Location badge */}
            {location && (
                <div className="post-location" style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: '0.85em', opacity: 0.7, marginTop: 4 }}>
                    <span>📍</span>
                    <span>{location.name}</span>
                </div>
            )}

            {/* Title (if present, e.g. for articles) */}
            {entry.title && (
                <h3 className="item-title">{entry.title}</h3>
            )}

            {/* Content */}
            {displayContent && (
                <div className="post-content">{displayContent}</div>
            )}

            {/* Media grid */}
            {(hasMedia || isGif) && (
                <div className={`post-media grid-${Math.min(mediaSources.length || 1, 4)}`}>
                    {/* GIF Embedding */}
                    {isGif && (
                        <div className="media-item">
                            <img src={entry.link_thumbnail || entry.link_url || ''} alt="GIF" style={{ borderRadius: 8, width: '100%' }} />
                        </div>
                    )}
                    {!isGif && mediaSources.slice(0, 4).map((src, i) => {
                        const kind = entry.media_kinds?.[i] ?? inferContentKind(src);
                        const isInteractive = kind === 'video' || kind === 'audio';
                        return (
                            <div
                                key={i}
                                className={`media-item${isInteractive ? ' media-item--interactive' : ''}`}
                                onClick={isInteractive ? undefined : () => openGallery(i)}
                                style={isInteractive ? undefined : { cursor: 'pointer' }}
                            >
                                {kind === 'video' && (
                                    <VideoPlayer
                                        src={src}
                                        poster={thumbnailSources[i]}
                                        mime={entry.media_mimes?.[i]}
                                        style={{ maxWidth: '100%', maxHeight: 420, borderRadius: 8 }}
                                    />
                                )}
                                {kind === 'audio' && (
                                    <audio controls style={{ width: '100%' }}>
                                        <source src={src} />
                                    </audio>
                                )}
                                {kind === 'pdf' && (
                                    <a
                                        href={src}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        className="stat-chip link-chip"
                                        style={{ display: 'inline-flex' }}
                                    >
                                        📄 Open PDF
                                    </a>
                                )}
                                {kind !== 'video' && kind !== 'audio' && kind !== 'pdf' && (
                                    <BlobImage
                                        src={thumbnailSources[i] || src}
                                        alt={`Media ${i + 1}`}
                                        loading="lazy"
                                        mediaDir={mediaDir}
                                    />
                                )}
                            </div>
                        );
                    })}
                    {mediaSources.length > 4 && (
                        <button
                            type="button"
                            className="media-overflow"
                            onClick={() => openGallery(4)}
                            style={{ cursor: 'pointer' }}
                        >
                            +{mediaSources.length - 4} more
                        </button>
                    )}
                </div>
            )}

            {isGalleryOpen && mediaSources.length > 0 && typeof document !== 'undefined' && createPortal(
                <div
                    onClick={closeGallery}
                    style={{
                        position: 'fixed',
                        inset: 0,
                        background: 'rgba(0,0,0,0.85)',
                        zIndex: 1000,
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        padding: 24,
                    }}
                >
                    <div
                        onClick={(e) => e.stopPropagation()}
                        style={{ position: 'relative', maxWidth: '92vw', width: 'min(1100px, 92vw)' }}
                    >
                        <button
                            type="button"
                            onClick={closeGallery}
                            style={{ position: 'absolute', right: 0, top: -40, color: 'white', background: 'transparent', border: 'none', fontSize: 24, cursor: 'pointer' }}
                            aria-label="Close viewer"
                        >
                            ✕
                        </button>
                        {mediaSources.length > 1 && (
                            <>
                                <button
                                    type="button"
                                    onClick={showPrev}
                                    style={{ position: 'absolute', left: -12, top: '50%', transform: 'translateY(-50%)', zIndex: 2, border: 'none', borderRadius: 999, width: 42, height: 42, cursor: 'pointer' }}
                                    aria-label="Previous media"
                                >
                                    ←
                                </button>
                                <button
                                    type="button"
                                    onClick={showNext}
                                    style={{ position: 'absolute', right: -12, top: '50%', transform: 'translateY(-50%)', zIndex: 2, border: 'none', borderRadius: 999, width: 42, height: 42, cursor: 'pointer' }}
                                    aria-label="Next media"
                                >
                                    →
                                </button>
                            </>
                        )}

                        <div style={{ background: '#111', borderRadius: 12, padding: 12 }}>
                            {(() => {
                                const src = mediaSources[galleryIndex];
                                const kind = entry.media_kinds?.[galleryIndex] ?? inferContentKind(src);
                                if (kind === 'video') {
                                    return (
                                        <VideoPlayer
                                            src={src}
                                            poster={thumbnailSources[galleryIndex]}
                                            mime={entry.media_mimes?.[galleryIndex]}
                                            style={{ width: '100%', maxHeight: '75vh', borderRadius: 8 }}
                                        />
                                    );
                                }
                                if (kind === 'audio') {
                                    return (
                                        <audio controls style={{ width: '100%' }}>
                                            <source src={src} />
                                        </audio>
                                    );
                                }
                                if (kind === 'pdf') {
                                    return (
                                        <a href={src} target="_blank" rel="noopener noreferrer" className="stat-chip link-chip">
                                            📄 Open PDF
                                        </a>
                                    );
                                }
                                return (
                                    <BlobImage
                                        src={src}
                                        alt={`Media ${galleryIndex + 1}`}
                                        mediaDir={mediaDir}
                                        style={{ width: '100%', maxHeight: '75vh', objectFit: 'contain', borderRadius: 8 }}
                                    />
                                );
                            })()}
                            <div style={{ color: 'white', marginTop: 8, textAlign: 'center', fontSize: '0.85rem' }}>
                                {galleryIndex + 1} / {mediaSources.length}
                            </div>
                        </div>
                    </div>
                </div>,
                document.body
            )}

            {/* Quoted Post */}
            {entry.quoted_post && (
                <div style={{ marginTop: 12 }}>
                    <FeedEntryCard entry={entry.quoted_post} resolveMediaPath={resolveMediaPath} mediaDir={mediaDir} isQuote={true} />
                </div>
            )}

            {/* Link preview */}
            {entry.link_url && !entry.quoted_post && !isGif && (
                <a href={entry.link_url} target="_blank" rel="noopener noreferrer" className="link-preview" style={{
                    display: 'block',
                    marginTop: 12,
                    border: '1px solid var(--border)',
                    borderRadius: 8,
                    overflow: 'hidden',
                    textDecoration: 'none',
                    color: 'inherit'
                }}>
                    {entry.link_thumbnail && (
                        <div style={{ height: 140, overflow: 'hidden' }}>
                            <BlobImage src={entry.link_thumbnail} alt="" className="link-preview-img" style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
                        </div>
                    )}
                    <div style={{ padding: 12 }}>
                        <div style={{ fontWeight: 600, marginBottom: 4 }}>
                            {isPdfLink ? '📄 Open PDF' : (entry.link_title || entry.link_url)}
                        </div>
                        {!isPdfLink && entry.link_description && <div style={{ fontSize: '0.85rem', color: 'var(--text-secondary)' }}>{entry.link_description}</div>}
                        <div style={{ fontSize: '0.75rem', marginTop: 4, color: 'var(--text-muted)' }}>{externalHost}</div>
                    </div>
                </a>
            )}

            {/* Footer: type badge + engagement stats */}
            {!isQuote && (
                <div className="item-footer">
                    {entry.item_type && (
                        <span className="type-badge" title={entry.item_type || 'Status'}>
                            {typeIcon(entry.item_type)}
                        </span>
                    )}
                    {(entry.like_count ?? 0) > 0 && (
                        <span className="stat-chip">❤️ {entry.like_count}</span>
                    )}
                    {(entry.reply_count ?? 0) > 0 && (
                        <span className="stat-chip">💬 {entry.reply_count}</span>
                    )}
                    {(entry.repost_count ?? 0) > 0 && (
                        <span className="stat-chip">🔁 {entry.repost_count}</span>
                    )}
                    {entry.original_url && (
                        <a
                            href={entry.original_url}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="stat-chip link-chip"
                        >
                            🔗 Original
                        </a>
                    )}
                </div>
            )}
        </div>
    );
}
