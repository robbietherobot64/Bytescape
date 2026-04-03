import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { convertFileSrc } from '@tauri-apps/api/core';
import { ReplyResult } from '../types';

/**
 * PURPOSE
 * Inline composer for post replies across supported social backends.
 * Supports text + optional media attachments, and dispatches to the
 * correct backend command based on source platform.
 *
 * HOW IT WORKS
 * 1) Tracks local draft text/attachments/loading/error UI state.
 * 2) Uses Tauri dialog to pick media from disk and creates preview URLs.
 * 3) Sends either:
 *    - `mastodon_reply` for Mastodon replies, or
 *    - `reply_to_post` for Bluesky-style replies.
 * 4) Resets compose state and notifies parent via callbacks.
 *
 * RELATED FILES
 * - src/types/index.ts (`ReplyResult` + post/reply model types)
 * - Rust commands registered in src-tauri/src/lib.rs (`mastodon_reply`, `reply_to_post`)
 * - Parent feed/thread components that open this compose surface and refresh timeline state
 *
 * CROSS-SYSTEM NOTE
 * This component is intentionally store-agnostic. It relies on parent-provided identifiers
 * and callback hooks, while using Tauri `invoke` as the UI⇄Engine boundary.
 */

interface ReplyComposeProps {
    parentUri: string;
    parentCid: string;
    rootUri: string;
    rootCid: string;
    replyToHandle: string;
    source?: string;
    mastodonStatusId?: string;
    onReplySent: () => void;
    onCancel: () => void;
}

/**
 * Public props contract for `ReplyCompose`.
 *
 * Parent supplies full reply threading context (`parent*` + `root*`) and handles post-send
 * cache/timeline refresh in `onReplySent`.
 */

interface ReplyAttachment {
    path?: string;
    blob_hash?: string;
    mime_type?: string;
    preview?: string;
}

/**
 * Local attachment shape used only by UI compose state.
 * `blob_hash` exists for forward compatibility with pre-uploaded attachment flows.
 */

export function ReplyCompose({
    parentUri, parentCid, rootUri, rootCid,
    replyToHandle, source, mastodonStatusId, onReplySent, onCancel
}: ReplyComposeProps) {
    // ═══════════════════════════════════════════════════════════════════════════
    // Local compose state
    // ═══════════════════════════════════════════════════════════════════════════
    const [text, setText] = useState('');
    const [attachments, setAttachments] = useState<ReplyAttachment[]>([]);
    const [isPosting, setIsPosting] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // ═══════════════════════════════════════════════════════════════════════════
    // Attachment selection (native dialog via Tauri plugin)
    // ═══════════════════════════════════════════════════════════════════════════
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
                const newAttachments: ReplyAttachment[] = selected.map(path => ({
                    path,
                    preview: convertFileSrc(path),
                    mime_type: path.toLowerCase().endsWith('.mp4') || path.toLowerCase().endsWith('.mov')
                        ? 'video/mp4' : 'image/jpeg'
                }));
                setAttachments(prev => [...prev, ...newAttachments]);
            } else if (selected) {
                const path = selected as string;
                setAttachments(prev => [...prev, {
                    path,
                    preview: convertFileSrc(path),
                    mime_type: path.toLowerCase().endsWith('.mp4') || path.toLowerCase().endsWith('.mov')
                        ? 'video/mp4' : 'image/jpeg'
                }]);
            }
        } catch (err) {
            console.error('Failed to open file dialog:', err);
        }
    };

    const removeAttachment = (index: number) => {
        setAttachments(prev => prev.filter((_, i) => i !== index));
    };

    // ═══════════════════════════════════════════════════════════════════════════
    // Send pipeline (UI ⇄ Tauri invoke commands)
    // ═══════════════════════════════════════════════════════════════════════════
    const handleSend = async () => {
        if (!text.trim() && attachments.length === 0) return;
        setIsPosting(true);
        setError(null);

        try {
            if (source === 'mastodon' && mastodonStatusId) {
                // Mastodon reply: use mastodon_reply command
                await invoke('mastodon_reply', {
                    statusId: mastodonStatusId,
                    content: text.trim(),
                });
            } else {
                // Bluesky reply: use reply_to_post command
                const attachmentData = attachments
                    .filter(a => a.path)
                    .map(a => ({
                        path: a.path!,
                        mime_type: a.mime_type || 'image/jpeg'
                    }));

                await invoke<ReplyResult>('reply_to_post', {
                    parentUri,
                    parentCid,
                    rootUri,
                    rootCid,
                    text: text.trim(),
                    attachments: attachmentData,
                });
            }

            setText('');
            setAttachments([]);
            onReplySent();
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            setError(msg);
            console.error('Reply failed:', err);
        } finally {
            setIsPosting(false);
        }
    };

    return (
        <div className="reply-compose" onClick={e => e.stopPropagation()}>
            <div className="reply-compose-context">
                Replying to <span className="reply-handle">@{replyToHandle}</span>
            </div>

            <textarea
                className="reply-compose-input"
                value={text}
                onChange={e => setText(e.target.value)}
                placeholder="Write a reply…"
                rows={2}
                autoFocus
            />

            {attachments.length > 0 && (
                <div className="reply-compose-attachments">
                    {attachments.map((att, i) => (
                        <div key={i} className="reply-attachment-thumb">
                            {att.mime_type?.startsWith('video/') ? (
                                <video src={att.preview} />
                            ) : (
                                <img src={att.preview} alt="" />
                            )}
                            <button className="reply-attachment-remove" onClick={() => removeAttachment(i)}>✕</button>
                        </div>
                    ))}
                </div>
            )}

            {error && (
                <div className="reply-compose-error">{error}</div>
            )}

            <div className="reply-compose-actions">
                <button
                    className="btn btn-ghost btn-sm"
                    onClick={handleAddMedia}
                    title="Add Media"
                    disabled={isPosting}
                >
                    🖼️ Media
                </button>
                <div className="reply-compose-right">
                    <button
                        className="btn btn-ghost btn-sm"
                        onClick={onCancel}
                        disabled={isPosting}
                    >
                        Cancel
                    </button>
                    <button
                        className="btn btn-primary btn-sm"
                        onClick={handleSend}
                        disabled={isPosting || (!text.trim() && attachments.length === 0)}
                    >
                        {isPosting ? 'Sending…' : 'Reply'}
                    </button>
                </div>
            </div>
        </div>
    );
}
