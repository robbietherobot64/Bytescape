import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { convertFileSrc } from '@tauri-apps/api/core';
import { Win98Box } from './Win98Box';
import { FeedEntry, PublishResult } from '../types';
import { stripHtml } from '../utils/sanitize';

interface QuoteRepostModalProps {
    /** The post being quoted */
    originalEntry: FeedEntry;
    /** Close the modal */
    onClose: () => void;
    /** Called after a successful quote-repost */
    onQuoted: (result: PublishResult) => void;
}

interface QuoteAttachment {
    path: string;
    preview: string;
    mime_type: string;
}

/**
 * QuoteRepostModal — Win98 window-style compose modal for quote posts.
 * Shows an embedded preview of the original post and a compose area above.
 * Publishes via publish_post_v2 with a quote embed.
 */
export function QuoteRepostModal({ originalEntry, onClose, onQuoted }: QuoteRepostModalProps) {
    const [text, setText] = useState('');
    const [attachments, setAttachments] = useState<QuoteAttachment[]>([]);
    const [isPosting, setIsPosting] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const handleAddMedia = async () => {
        try {
            const selected = await open({
                multiple: true,
                filters: [{
                    name: 'Media',
                    extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'mp4', 'mov']
                }]
            });

            if (selected && Array.isArray(selected)) {
                const newAttachments: QuoteAttachment[] = selected.map(path => ({
                    path,
                    preview: convertFileSrc(path),
                    mime_type: path.toLowerCase().endsWith('.mp4') || path.toLowerCase().endsWith('.mov')
                        ? 'video/mp4' : 'image/jpeg'
                }));
                setAttachments(prev => [...prev, ...newAttachments].slice(0, 4));
            } else if (selected) {
                const path = selected as string;
                setAttachments(prev => [...prev, {
                    path,
                    preview: convertFileSrc(path),
                    mime_type: path.toLowerCase().endsWith('.mp4') || path.toLowerCase().endsWith('.mov')
                        ? 'video/mp4' : 'image/jpeg'
                }].slice(0, 4));
            }
        } catch (err) {
            console.error('Failed to open file dialog:', err);
        }
    };

    const removeAttachment = (index: number) => {
        setAttachments(prev => prev.filter((_, i) => i !== index));
    };

    const handlePost = async () => {
        if (!text.trim() && attachments.length === 0) return;
        setIsPosting(true);
        setError(null);

        try {
            // Build attachment data for publish_post_v2
            const attachmentData = attachments.map(a => ({
                path: a.path,
                mime_type: a.mime_type,
            }));

            // publish_post_v2 handles bluesky quote embedding via the quoteUri param
            const result = await invoke<PublishResult>('publish_post_v2', {
                text: text.trim(),
                attachments: attachmentData,
                destination: 'bluesky',
                // Quote post context — the backend will add this as an embed
                quoteUri: originalEntry.id,
                quoteCid: originalEntry.cid || '',
            });

            onQuoted(result);
            onClose();
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            setError(msg);
            console.error('Quote repost failed:', err);
        } finally {
            setIsPosting(false);
        }
    };

    const charCount = text.length;
    const maxChars = 300;
    const isOverLimit = charCount > maxChars;

    return (
        <div className="quote-repost-overlay" onClick={onClose}>
            <div className="quote-repost-dialog" onClick={e => e.stopPropagation()}>
                <Win98Box variant="window" title="Quote Post" onClose={onClose}>
                    <div className="quote-compose-area">
                        <textarea
                            className="quote-compose-input"
                            value={text}
                            onChange={e => setText(e.target.value)}
                            placeholder="Add your thoughts…"
                            rows={3}
                            autoFocus
                            maxLength={maxChars + 50}
                        />
                        <div className="quote-compose-toolbar">
                            <button
                                className="btn btn-ghost btn-sm"
                                onClick={handleAddMedia}
                                disabled={isPosting || attachments.length >= 4}
                                title="Add Media"
                            >
                                🖼️ Media
                            </button>
                            <span className={`quote-char-count ${isOverLimit ? 'over-limit' : ''}`}>
                                {charCount}/{maxChars}
                            </span>
                        </div>

                        {/* Attachment Previews */}
                        {attachments.length > 0 && (
                            <div className="quote-attachments">
                                {attachments.map((att, i) => (
                                    <div key={i} className="quote-attachment-thumb">
                                        {att.mime_type.startsWith('video/') ? (
                                            <video src={att.preview} />
                                        ) : (
                                            <img src={att.preview} alt="" />
                                        )}
                                        <button
                                            className="reply-attachment-remove"
                                            onClick={() => removeAttachment(i)}
                                        >
                                            ✕
                                        </button>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>

                    {/* Embedded Original Post Preview */}
                    <div className="quote-original-preview">
                        <div className="quote-original-header">
                            <div className="avatar small">
                                {originalEntry.author_avatar ? (
                                    <img src={originalEntry.author_avatar} alt="" />
                                ) : (
                                    (originalEntry.author_handle || 'U').charAt(0).toUpperCase()
                                )}
                            </div>
                            <span className="author-name">
                                {originalEntry.author_handle || 'Unknown'}
                            </span>
                        </div>
                        {originalEntry.content && (
                            <div className="quote-original-text">
                                {(() => {
                                    const clean = stripHtml(originalEntry.content);
                                    return clean.length > 200 ? clean.substring(0, 200) + '…' : clean;
                                })()}
                            </div>
                        )}
                        {originalEntry.media_urls.length > 0 && (
                            <div className="quote-original-media-hint">
                                📎 {originalEntry.media_urls.length} attachment{originalEntry.media_urls.length > 1 ? 's' : ''}
                            </div>
                        )}
                    </div>

                    {error && <div className="quote-error">{error}</div>}

                    <div className="quote-compose-actions">
                        <button
                            className="btn btn-ghost"
                            onClick={onClose}
                            disabled={isPosting}
                        >
                            Cancel
                        </button>
                        <button
                            className="btn btn-primary"
                            onClick={handlePost}
                            disabled={isPosting || isOverLimit || (!text.trim() && attachments.length === 0)}
                        >
                            {isPosting ? 'Posting…' : 'Quote Post'}
                        </button>
                    </div>
                </Win98Box>
            </div>
        </div>
    );
}
