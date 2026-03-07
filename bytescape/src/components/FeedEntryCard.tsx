import { useState } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { FeedEntry, TaggedPerson, Audience, LikeResult, ThreadReply } from '../types';
import { BlobImage } from './BlobImage';
import { VideoPlayer } from './VideoPlayer';
import { AudiencePills } from './AudiencePills';
import { AudienceDrawer } from './AudienceDrawer';
import { ReplyCompose } from './ReplyCompose';
import { ReactionPicker } from './ReactionPicker';
import { QuoteRepostModal } from './QuoteRepostModal';
import { safeHostname, looksLikeGif } from '../utils/content';

interface FeedEntryCardProps {
    entry: FeedEntry;
    resolveMediaPath?: (p: string) => string;
    mediaDir?: string;
    onEntryUpdated?: (id: string, audiences: Audience[]) => void;
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

function parseTaggedPeople(input?: TaggedPerson[] | string): TaggedPerson[] {
    if (!input) return [];
    // New typed format: already an array
    if (Array.isArray(input)) return input;
    // Legacy string format: JSON-encoded
    try {
        return JSON.parse(input) as TaggedPerson[];
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



/* ─── Component ─── */

/** Resolve the AT-URI for a FeedEntry. Live posts use entry.id (already an AT-URI),
 *  but vault items store the CID as id and the AT-URI in original_url / syndicated_uri. */
function resolveAtUri(entry: FeedEntry): string {
    // AT-URIs start with 'at://'
    if (entry.id.startsWith('at://')) return entry.id;
    if (entry.original_url) return entry.original_url;
    if (entry.syndicated_uri) return entry.syndicated_uri;
    // Fallback: check share_history for a bluesky URI
    const bskyEvent = entry.share_history?.find(e => e.platform === 'bluesky' && e.active);
    if (bskyEvent?.uri) return bskyEvent.uri;
    return entry.id;
}

export function FeedEntryCard({ entry, resolveMediaPath, mediaDir, onEntryUpdated, isQuote = false }: FeedEntryCardProps) {
    const [isGalleryOpen, setIsGalleryOpen] = useState(false);
    const [galleryIndex, setGalleryIndex] = useState(0);
    const [drawerOpen, setDrawerOpen] = useState(false);
    const taggedPeople = parseTaggedPeople(entry.tagged_people);
    const tagLine = renderTaggedPeople(taggedPeople);

    // Interaction state
    const [liked, setLiked] = useState(!!entry.viewer_like);
    const [likeUri, setLikeUri] = useState<string | undefined>(entry.viewer_like);
    const [likeCount, setLikeCount] = useState(entry.like_count || 0);
    const [reposted, setReposted] = useState(!!entry.viewer_repost);
    const [repostUri, setRepostUri] = useState<string | undefined>(entry.viewer_repost);
    const [repostCount, setRepostCount] = useState(entry.repost_count || 0);
    const [replyOpen, setReplyOpen] = useState(false);
    const [threadReplies, setThreadReplies] = useState<ThreadReply[]>([]);
    const [threadOpen, setThreadOpen] = useState(false);
    const [threadLoading, setThreadLoading] = useState(false);
    const [quoteModalOpen, setQuoteModalOpen] = useState(false);
    const [reactionEmoji, setReactionEmoji] = useState<string | undefined>(
        entry.viewer_like ? '❤️' : undefined
    );
    const [repostMenuOpen, setRepostMenuOpen] = useState(false);

    const isBluesky = entry.source === 'bluesky';

    const mediaSources = entry.media_urls.map(src => resolveMediaPath ? resolveMediaPath(src) : src);
    const thumbnailSources = entry.media_thumbnails.map(src => resolveMediaPath ? resolveMediaPath(src) : src);

    const authorName = entry.author_handle || 'You';
    const displayContent = sanitizeDisplayContent(entry.content || '');
    const isGif = looksLikeGif(entry.link_url);
    const externalHost = entry.link_url ? safeHostname(entry.link_url) : '';
    const hasMedia = entry.media_urls.length > 0;
    const shareHistory = entry.share_history || [];

    // Compute active syndicatedURIs for explicit links and the Drawer
    const syndicatedUris = shareHistory
        .filter(event => event.active)
        .reduce((acc, event) => {
            acc[event.platform.toLowerCase()] = event.uri;
            return acc;
        }, {} as Record<string, string>);

    const openGallery = (index: number) => {
        setGalleryIndex(index);
        setIsGalleryOpen(true);
    };

    const handleShareClick = (e: React.MouseEvent) => {
        e.stopPropagation();
        setDrawerOpen(true);
    };

    const handleAudienceUpdated = (newAudiences: Audience[]) => {
        if (onEntryUpdated) onEntryUpdated(entry.id, newAudiences);
    };

    // ─── React (Like/Unlike via emoji) ───
    const handleReaction = async (emoji: string | null) => {
        if (!isBluesky) return;

        if (emoji === null && liked && likeUri) {
            // Remove reaction (unlike)
            setLiked(false);
            setReactionEmoji(undefined);
            setLikeCount(c => Math.max(0, c - 1));
            try {
                await invoke<boolean>('unlike_post', { likeUri });
                setLikeUri(undefined);
            } catch (err) {
                console.error('Unlike failed:', err);
                setLiked(true);
                setReactionEmoji('❤️');
                setLikeCount(c => c + 1);
            }
        } else if (emoji) {
            // Add reaction (like)
            setLiked(true);
            setReactionEmoji(emoji);
            setLikeCount(c => c + 1);
            try {
                const result = await invoke<LikeResult>('like_post', { uri: resolveAtUri(entry), cid: getCid() });
                setLikeUri(result.uri);
            } catch (err) {
                console.error('Like failed:', err);
                setLiked(false);
                setReactionEmoji(undefined);
                setLikeCount(c => Math.max(0, c - 1));
            }
        }
    };

    // ─── Repost / Quote Repost ───
    const handleRepostClick = (e: React.MouseEvent) => {
        e.stopPropagation();
        if (!isBluesky) return;

        if (reposted && repostUri) {
            // Undo repost directly
            handleDirectRepost(e);
        } else {
            // Show repost menu (repost vs quote)
            setRepostMenuOpen(prev => !prev);
        }
    };

    const handleDirectRepost = async (e: React.MouseEvent) => {
        e.stopPropagation();
        if (!isBluesky) return;
        setRepostMenuOpen(false);

        if (reposted && repostUri) {
            setReposted(false);
            setRepostCount(c => Math.max(0, c - 1));
            try {
                await invoke<boolean>('unrepost_post', { repostUri });
                setRepostUri(undefined);
            } catch (err) {
                console.error('Unrepost failed:', err);
                setReposted(true);
                setRepostCount(c => c + 1);
            }
        } else {
            setReposted(true);
            setRepostCount(c => c + 1);
            try {
                const result = await invoke<{ uri: string; cid: string }>('repost_post', { uri: resolveAtUri(entry), cid: getCid() });
                setRepostUri(result.uri);
            } catch (err) {
                console.error('Repost failed:', err);
                setReposted(false);
                setRepostCount(c => Math.max(0, c - 1));
            }
        }
    };

    const handleQuoteRepost = (e: React.MouseEvent) => {
        e.stopPropagation();
        setRepostMenuOpen(false);
        setQuoteModalOpen(true);
    };

    // ─── Reply Toggle ───
    const handleReplyToggle = (e: React.MouseEvent) => {
        e.stopPropagation();
        setReplyOpen(prev => !prev);
    };

    // ─── Thread Toggle ───
    const handleThreadToggle = async (e: React.MouseEvent) => {
        e.stopPropagation();
        if (threadOpen) {
            setThreadOpen(false);
            return;
        }
        if (!isBluesky) return;

        setThreadLoading(true);
        setThreadOpen(true);
        try {
            const postUri = resolveAtUri(entry);
            const replies = await invoke<ThreadReply[]>('get_post_thread', { uri: postUri, depth: 6 });
            setThreadReplies(replies);
        } catch (err) {
            console.error('Failed to load thread:', err);
            setThreadReplies([]);
        } finally {
            setThreadLoading(false);
        }
    };

    // Extract CID from entry
    const getCid = (): string => {
        return entry.cid || '';
    };

    // Callback when a reply is successfully posted
    const handleReplySent = () => {
        setReplyOpen(false);
        // Optimistically bump the reply count
        // The count state isn't tracked separately yet, but we could add it
    };

    // Determine repost banner text:
    //   1. If reason-based repost → "{handle} reposted"
    //   2. If viewer has reposted (no reason) → "You reposted"
    const isRepost = !isQuote && !!(entry.reposted_by_handle || entry.viewer_repost);
    const repostBannerText = entry.reposted_by_handle
        ? `${entry.reposted_by_handle} reposted`
        : (entry.viewer_repost ? 'You reposted' : null);

    return (
        <div 
            className={`feed-entry-card ${isQuote ? 'is-quote' : ''} ${isRepost ? 'is-repost' : ''}`}
        >
            {/* Repost Banner */}
            {isRepost && repostBannerText && (
                <div className="repost-banner">
                    <RepostIcon /> {repostBannerText}
                </div>
            )}

            {/* Reply Context */}
            {entry.reply_parent_handle && !isQuote && (
                <div className="reply-context-chip">
                    ↩ replying to @{entry.reply_parent_handle}
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
                        <AudiencePills
                            source={entry.source}
                            originalTimestamp={entry.timestamp}
                            shareHistory={shareHistory}
                            compact
                        />
                    </div>
                )}
            </div>

            {/* Syndicated links */}
            {Object.entries(syndicatedUris).length > 0 && (
                <div style={{ padding: '0 16px 4px', display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                    {Object.entries(syndicatedUris).map(([platform, uri]) => (
                        <a key={platform} href={uri} target="_blank" rel="noopener noreferrer"
                            style={{ fontSize: '0.72rem', color: 'var(--text-muted)', textDecoration: 'none' }}>
                            🔗 View on {platform.charAt(0).toUpperCase() + platform.slice(1)}
                        </a>
                    ))}
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
                        📍 {typeof entry.location === 'string' ? entry.location : entry.location.name}
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
                                                mediaDir={mediaDir}
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

            {/* Engagement Action Bar — React · Comment · Repost · Share */}
            {!isQuote && (
                <div className="engagement-bar">
                    <ReactionPicker
                        currentReaction={reactionEmoji}
                        onReact={handleReaction}
                        disabled={!isBluesky}
                    />
                    <span className="count">{formatCount(likeCount)}</span>

                    <button
                        className={`engagement-btn btn-reply ${replyOpen ? 'active' : ''}`}
                        title="Comment"
                        onClick={handleReplyToggle}
                        disabled={!isBluesky}
                    >
                        <ReplyIcon />
                        <span className="count">{formatCount(entry.reply_count)}</span>
                    </button>

                    <div className="repost-btn-container">
                        <button
                            className={`engagement-btn btn-repost ${reposted ? 'active' : ''}`}
                            title={reposted ? 'Undo repost' : 'Repost'}
                            onClick={handleRepostClick}
                            disabled={!isBluesky}
                        >
                            <RepostIcon />
                            <span className="count">{formatCount(repostCount)}</span>
                        </button>
                        {repostMenuOpen && (
                            <div className="repost-menu" onClick={e => e.stopPropagation()}>
                                <button className="repost-menu-item" onClick={handleDirectRepost}>
                                    🔁 Repost
                                </button>
                                <button className="repost-menu-item" onClick={handleQuoteRepost}>
                                    💬 Quote Post
                                </button>
                            </div>
                        )}
                    </div>

                    {isBluesky && entry.reply_count && entry.reply_count > 0 ? (
                        <button
                            className={`engagement-btn btn-thread ${threadOpen ? 'active' : ''}`}
                            title={threadOpen ? 'Hide replies' : 'Show replies'}
                            onClick={handleThreadToggle}
                        >
                            💬 <span className="count">{formatCount(entry.reply_count)}</span>
                        </button>
                    ) : null}

                    <button className="engagement-btn btn-share" title="Share" style={{ marginLeft: 'auto' }}
                        onClick={handleShareClick}
                    >
                        <ShareIcon />
                    </button>
                </div>
            )}

            {/* Inline Reply Compose */}
            {replyOpen && isBluesky && !isQuote && (
                <ReplyCompose
                    parentUri={resolveAtUri(entry)}
                    parentCid={getCid()}
                    rootUri={resolveAtUri(entry)}
                    rootCid={getCid()}
                    replyToHandle={entry.author_handle || 'unknown'}
                    onReplySent={handleReplySent}
                    onCancel={() => setReplyOpen(false)}
                />
            )}

            {/* Thread / Replies */}
            {threadOpen && !isQuote && (
                <div className="thread-replies">
                    {threadLoading ? (
                        <div className="thread-loading">Loading replies…</div>
                    ) : threadReplies.length === 0 ? (
                        <div className="thread-empty">No replies yet</div>
                    ) : (
                        threadReplies.map(reply => (
                            <div key={reply.uri} className="thread-reply">
                                <div className="thread-reply-header">
                                    <div className="avatar small">
                                        {reply.author_avatar ? (
                                            <img src={reply.author_avatar} alt="" />
                                        ) : (
                                            reply.author_handle.charAt(0).toUpperCase()
                                        )}
                                    </div>
                                    <span className="author-name">{reply.author_handle}</span>
                                    <span className="post-time">{formatTimestamp(reply.created_at)}</span>
                                </div>
                                <div className="thread-reply-body">{reply.text}</div>
                                {reply.image_urls.length > 0 && (
                                    <div className="thread-reply-images">
                                        {reply.image_urls.map((url, i) => (
                                            <img key={i} src={url} alt="" className="thread-reply-img" />
                                        ))}
                                    </div>
                                )}
                                <div className="thread-reply-stats">
                                    <span>❤️ {formatCount(reply.like_count)}</span>
                                    <span>💬 {formatCount(reply.reply_count)}</span>
                                    <span>🔁 {formatCount(reply.repost_count)}</span>
                                </div>
                            </div>
                        ))
                    )}
                </div>
            )}

            {/* Owner Replies – your comments on this post, rendered as full interactive cards */}
            {entry.owner_replies && entry.owner_replies.length > 0 && !isQuote && (
                <div className="owner-replies">
                    {entry.owner_replies.map(reply => (
                        <div key={reply.id} className="owner-reply-wrapper">
                            <FeedEntryCard
                                entry={reply}
                                resolveMediaPath={resolveMediaPath}
                                mediaDir={mediaDir}
                                onEntryUpdated={onEntryUpdated}
                            />
                        </div>
                    ))}
                </div>
            )}

            {/* Quote Repost Modal */}
            {quoteModalOpen && createPortal(
                <QuoteRepostModal
                    originalEntry={entry}
                    onClose={() => setQuoteModalOpen(false)}
                    onQuoted={(result) => {
                        console.log('Quote posted:', result);
                        setQuoteModalOpen(false);
                    }}
                />,
                document.body
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

            {/* Audience Drawer */}
            {drawerOpen && (
                <AudienceDrawer
                    itemId={entry.id}
                    shareHistory={shareHistory}
                    syndicatedUris={syndicatedUris}
                    onClose={() => setDrawerOpen(false)}
                    onUpdated={handleAudienceUpdated}
                />
            )}
        </div>
    );
}
