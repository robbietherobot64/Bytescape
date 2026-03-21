import React, { useState, useMemo } from 'react';
import { openProfile } from '../utils/openInApp';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { FeedEntry, Audience, LikeResult, ThreadReply } from '../types';
import { AudiencePills } from './AudiencePills';
import { AudienceDrawer } from './AudienceDrawer';
import { ReplyCompose } from './ReplyCompose';
import { QuoteRepostModal } from './QuoteRepostModal';
import { safeHostname, looksLikeGif } from '../utils/content';
import { RichText } from './RichText';
import { VideoPlaybackManager } from '../utils/VideoPlaybackManager';
import {
    RepostIcon,
    parseTaggedPeople,
    renderTaggedPeople,
    formatTimestamp,
    sanitizeDisplayContent,
    resolveAtUri,
    MediaGrid,
    EngagementBar,
    ThreadReplies,
    GalleryOverlay,
} from './feed';

/** Extract unique hashtags from post text content */
function extractHashtags(text: string | undefined): string[] {
    if (!text) return [];
    const matches = text.match(/#[\w\u00C0-\u024F\u1E00-\u1EFF]+/g);
    if (!matches) return [];
    // Deduplicate case-insensitively, keep original casing of first occurrence
    const seen = new Set<string>();
    const result: string[] = [];
    for (const tag of matches) {
        const lower = tag.toLowerCase();
        if (!seen.has(lower)) {
            seen.add(lower);
            result.push(tag);
        }
    }
    return result;
}

/** Dispatch a custom event to navigate to a hashtag in the Subscribed tab */
function navigateToHashtag(tag: string) {
    const cleaned = tag.replace(/^#/, '');
    window.dispatchEvent(new CustomEvent('navigate-hashtag', { detail: cleaned }));
}

interface FeedEntryCardProps {
    entry: FeedEntry;
    resolveMediaPath?: (p: string) => string;
    mediaDir?: string;
    onEntryUpdated?: (id: string, audiences: Audience[]) => void;
    isQuote?: boolean;
}

/* ─── Component ─── */

function FeedEntryCardInner({ entry, resolveMediaPath, mediaDir, onEntryUpdated, isQuote = false }: FeedEntryCardProps) {
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

    // Extract hashtags from post content for chips
    const hashtags = useMemo(() => extractHashtags(entry.content), [entry.content]);

    // Compute active syndicatedURIs for explicit links and the Drawer
    const syndicatedUris = shareHistory
        .filter(event => event.active)
        .reduce((acc, event) => {
            acc[event.platform.toLowerCase()] = event.uri;
            return acc;
        }, {} as Record<string, string>);

    const openGallery = (index: number) => {
        VideoPlaybackManager.pauseAll();
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
                <div
                    className="avatar"
                    onClick={() => {
                        if (entry.author_handle && entry.author_handle !== 'You') {
                            openProfile(entry.author_handle, entry.source as any);
                        }
                    }}
                    style={{ cursor: entry.author_handle && entry.author_handle !== 'You' ? 'pointer' : 'default' } as React.CSSProperties}
                    title={entry.author_handle ? `View @${entry.author_handle}'s profile` : undefined}
                >
                    {entry.author_avatar ? (
                        <img src={entry.author_avatar} alt="" />
                    ) : (
                        authorName.charAt(0).toUpperCase()
                    )}
                </div>
                <div className="author-info">
                    <div
                        className="author-name"
                        onClick={() => {
                            if (entry.author_handle && entry.author_handle !== 'You') {
                                openProfile(entry.author_handle, entry.source as any);
                            }
                        }}
                        style={{ cursor: entry.author_handle && entry.author_handle !== 'You' ? 'pointer' : 'default' } as React.CSSProperties}
                        title={entry.author_handle ? `View @${entry.author_handle}'s profile` : undefined}
                    >
                        {authorName}
                    </div>
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

            {/* Hashtag Chips */}
            {hashtags.length > 0 && !isQuote && (
                <div className="hashtag-chips" style={{
                    padding: '0 16px 4px',
                    display: 'flex',
                    gap: 4,
                    flexWrap: 'wrap',
                } as React.CSSProperties}>
                    {hashtags.slice(0, 6).map(tag => (
                        <button
                            key={tag}
                            className="hashtag-chip"
                            onClick={(e) => {
                                e.stopPropagation();
                                navigateToHashtag(tag);
                            }}
                            title={`View posts tagged ${tag}`}
                            style={{
                                background: 'var(--bg-tertiary, #1a1a2e)',
                                border: '1px solid var(--border-color, #333)',
                                borderRadius: 12,
                                padding: '1px 8px',
                                fontSize: '0.7rem',
                                color: 'var(--accent-blue, #3498db)',
                                cursor: 'pointer',
                                lineHeight: 1.6,
                                whiteSpace: 'nowrap',
                            } as React.CSSProperties}
                        >
                            {tag}
                        </button>
                    ))}
                </div>
            )}

            {/* Content Body */}
            <div className="feed-entry-body">
                {displayContent && (
                    <div className="post-text">
                        <RichText text={displayContent} />
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
                    <MediaGrid
                        mediaUrls={entry.media_urls}
                        mediaKinds={entry.media_kinds}
                        mediaSources={mediaSources}
                        thumbnailSources={thumbnailSources}
                        mediaDir={mediaDir}
                        openGallery={openGallery}
                    />
                )}

                {/* Link Preview — show for article/external links even with media */}
                {entry.link_url && !isGif && (
                    <a href={entry.link_url} target="_blank" rel="noopener noreferrer" className="link-preview-card">
                        {entry.link_thumbnail && !hasMedia && (
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
                <EngagementBar
                    isBluesky={isBluesky}
                    likeCount={likeCount}
                    reactionEmoji={reactionEmoji}
                    replyOpen={replyOpen}
                    replyCount={entry.reply_count}
                    reposted={reposted}
                    repostCount={repostCount}
                    repostMenuOpen={repostMenuOpen}
                    threadOpen={threadOpen}
                    onReaction={handleReaction}
                    onReplyToggle={handleReplyToggle}
                    onRepostClick={handleRepostClick}
                    onDirectRepost={handleDirectRepost}
                    onQuoteRepost={handleQuoteRepost}
                    onThreadToggle={handleThreadToggle}
                    onShareClick={handleShareClick}
                />
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
            {!isQuote && (
                <ThreadReplies
                    threadOpen={threadOpen}
                    threadLoading={threadLoading}
                    threadReplies={threadReplies}
                />
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

            {/* Fullscreen Gallery */}
            {isGalleryOpen && (
                <GalleryOverlay
                    mediaSources={mediaSources}
                    thumbnailSources={thumbnailSources}
                    mediaKinds={entry.media_kinds}
                    galleryIndex={galleryIndex}
                    setGalleryIndex={setGalleryIndex}
                    onClose={() => setIsGalleryOpen(false)}
                />
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

/** Memoized FeedEntryCard — skips re-render when entry identity and interaction state haven't changed. */
export const FeedEntryCard = React.memo(FeedEntryCardInner, (prev, next) => {
    return (
        prev.entry.id === next.entry.id &&
        prev.entry.timestamp === next.entry.timestamp &&
        prev.entry.viewer_like === next.entry.viewer_like &&
        prev.entry.viewer_repost === next.entry.viewer_repost &&
        prev.entry.like_count === next.entry.like_count &&
        prev.entry.repost_count === next.entry.repost_count &&
        prev.entry.reply_count === next.entry.reply_count &&
        prev.isQuote === next.isQuote
    );
});
