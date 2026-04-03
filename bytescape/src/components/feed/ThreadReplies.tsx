import { ThreadReply } from '../../types';
import { RichText } from '../RichText';
import { formatTimestamp, formatCount } from './helpers';

/**
 * PURPOSE:
 * Renders nested reply/thread context for a feed post.
 *
 * HOW IT WORKS:
 * - Early returns when thread UI is closed.
 * - Shows loading, empty, or mapped reply content states.
 * - Uses shared helper formatters to keep display behavior consistent.
 *
 * RELATED FILES:
 * - src/components/feed/helpers.ts
 * - src/components/RichText.tsx
 * - src/components/FeedEntryCard.tsx (thread toggle and data wiring)
 * - src/hooks/*, src/store/* (thread fetch/state ownership)
 */

interface ThreadRepliesProps {
    threadOpen: boolean;
    threadLoading: boolean;
    threadReplies: ThreadReply[];
}

/**
 * Exported thread replies panel for feed cards.
 */
export function ThreadReplies({ threadOpen, threadLoading, threadReplies }: ThreadRepliesProps) {
    if (!threadOpen) return null;

    return (
        <div className="thread-replies">
            {/* ── Loading / empty states ─────────────────────────────────────── */}
            {threadLoading ? (
                <div className="thread-loading">Loading replies…</div>
            ) : threadReplies.length === 0 ? (
                <div className="thread-empty">No replies yet</div>
            ) : (
                // ── Reply list ────────────────────────────────────────────────
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
                        <div className="thread-reply-body"><RichText text={reply.text} /></div>
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
    );
}
