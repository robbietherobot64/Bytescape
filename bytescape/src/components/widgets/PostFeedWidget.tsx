/**
 * Post Feed Widget (B2.4)
 *
 * Displays the user's most recent vault posts in a compact list format.
 * Used as a widget block in the GrapesJS profile builder.
 */

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { BytescapeItem } from '../../types';

interface PagedResult {
    items: BytescapeItem[];
    total: number;
}

export function PostFeedWidget() {
    const [posts, setPosts] = useState<BytescapeItem[]>([]);

    useEffect(() => {
        invoke<PagedResult>('list_vault_posts_paged', { offset: 0, limit: 5, sourceFilter: null })
            .then(result => setPosts(result.items))
            .catch(() => {});
    }, []);

    const formatDate = (ts: number) => {
        const d = new Date(ts * 1000);
        return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    };

    return (
        <div style={{
            background: 'var(--bg-secondary)',
            border: '2px solid var(--border-color)',
            borderRadius: 'var(--border-radius)',
            padding: 12,
        } as React.CSSProperties}>
            <div style={{
                fontSize: 13,
                fontWeight: 600,
                color: 'var(--text-primary)',
                marginBottom: 8,
                borderBottom: '1px solid var(--border-color)',
                paddingBottom: 6,
            } as React.CSSProperties}>
                📝 Recent Posts
            </div>

            {posts.length === 0 ? (
                <div style={{ color: 'var(--text-muted)', fontSize: 12, textAlign: 'center', padding: 16 } as React.CSSProperties}>
                    No posts yet
                </div>
            ) : (
                posts.map(post => (
                    <div key={post.id} style={{
                        padding: '8px 0',
                        borderBottom: '1px solid var(--border-color)',
                    } as React.CSSProperties}>
                        <div style={{
                            fontSize: 13,
                            color: 'var(--text-primary)',
                            lineHeight: 1.4,
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            display: '-webkit-box',
                            WebkitLineClamp: 2,
                            WebkitBoxOrient: 'vertical',
                        } as React.CSSProperties}>
                            {post.content || post.title || '(no text)'}
                        </div>
                        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 } as React.CSSProperties}>
                            {formatDate(post.original_timestamp)} · {post.source}
                        </div>
                    </div>
                ))
            )}
        </div>
    );
}
