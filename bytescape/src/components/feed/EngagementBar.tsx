import React from 'react';
import { ReplyIcon, RepostIcon, ShareIcon } from './icons';
import { ReactionPicker } from '../ReactionPicker';
import { formatCount } from './helpers';

interface EngagementBarProps {
    isBluesky: boolean;
    likeCount: number;
    reactionEmoji: string | undefined;
    replyOpen: boolean;
    replyCount?: number;
    reposted: boolean;
    repostCount: number;
    repostMenuOpen: boolean;
    threadOpen: boolean;
    onReaction: (emoji: string | null) => void;
    onReplyToggle: (e: React.MouseEvent) => void;
    onRepostClick: (e: React.MouseEvent) => void;
    onDirectRepost: (e: React.MouseEvent) => void;
    onQuoteRepost: (e: React.MouseEvent) => void;
    onThreadToggle: (e: React.MouseEvent) => void;
    onShareClick: (e: React.MouseEvent) => void;
}

export function EngagementBar({
    isBluesky,
    likeCount,
    reactionEmoji,
    replyOpen,
    replyCount,
    reposted,
    repostCount,
    repostMenuOpen,
    threadOpen,
    onReaction,
    onReplyToggle,
    onRepostClick,
    onDirectRepost,
    onQuoteRepost,
    onThreadToggle,
    onShareClick,
}: EngagementBarProps) {
    return (
        <div className="engagement-bar">
            <ReactionPicker
                currentReaction={reactionEmoji}
                onReact={onReaction}
                disabled={!isBluesky}
            />
            <span className="count">{formatCount(likeCount)}</span>

            <button
                className={`engagement-btn btn-reply ${replyOpen ? 'active' : ''}`}
                title="Comment"
                onClick={onReplyToggle}
                disabled={!isBluesky}
            >
                <ReplyIcon />
                <span className="count">{formatCount(replyCount)}</span>
            </button>

            <div className="repost-btn-container">
                <button
                    className={`engagement-btn btn-repost ${reposted ? 'active' : ''}`}
                    title={reposted ? 'Undo repost' : 'Repost'}
                    onClick={onRepostClick}
                    disabled={!isBluesky}
                >
                    <RepostIcon />
                    <span className="count">{formatCount(repostCount)}</span>
                </button>
                {repostMenuOpen && (
                    <div className="repost-menu" onClick={e => e.stopPropagation()}>
                        <button className="repost-menu-item" onClick={onDirectRepost}>
                            🔁 Repost
                        </button>
                        <button className="repost-menu-item" onClick={onQuoteRepost}>
                            💬 Quote Post
                        </button>
                    </div>
                )}
            </div>

            {isBluesky && replyCount && replyCount > 0 ? (
                <button
                    className={`engagement-btn btn-thread ${threadOpen ? 'active' : ''}`}
                    title={threadOpen ? 'Hide replies' : 'Show replies'}
                    onClick={onThreadToggle}
                >
                    💬 <span className="count">{formatCount(replyCount)}</span>
                </button>
            ) : null}

            <button className="engagement-btn btn-share" title="Share" style={{ marginLeft: 'auto' } as React.CSSProperties}
                onClick={onShareClick}
            >
                <ShareIcon />
            </button>
        </div>
    );
}
