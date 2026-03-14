import { useState, useRef } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { INPUT_LIMITS, formatFileSize } from '../utils/sanitize';
import { useCharacterCount, TARGET_ICONS, STATUS_COLORS } from '../hooks/useCharacterCount';
import { ArticleEditor } from './ArticleEditor';
import type { ArticleEditorData } from './ArticleEditor';

export interface Attachment {
    path?: string;
    blob_hash?: string;
    mime_type?: string;
    preview?: string;
    file_size?: number;
}

export type PostTarget = 'local' | 'bluesky' | 'activitypub' | 'all';

export interface PublishOptions {
    itemType?: 'Status' | 'Article';
    title?: string;
    contentJson?: object;
}

interface ComposeBoxProps {
    onPublish: (text: string, target: PostTarget, attachments: Attachment[], options?: PublishOptions) => Promise<void>;
    isPosting: boolean;
    postStatus: string | null;
    initialText?: string;
    initialAttachments?: Attachment[];
}

export function ComposeBox({ onPublish, isPosting, postStatus, initialText = '', initialAttachments = [] }: ComposeBoxProps) {
    const [composeText, setComposeText] = useState(initialText);
    const [postTarget, setPostTarget] = useState<PostTarget>('local');
    const [attachments, setAttachments] = useState<Attachment[]>(initialAttachments);
    const [fileSizeWarnings, setFileSizeWarnings] = useState<string[]>([]);
    const [articleMode, setArticleMode] = useState(false);
    const articleDataRef = useRef<ArticleEditorData | null>(null);

    // Input validation (MED-4) + multi-target character counting (C1.4)
    const charResult = useCharacterCount(composeText, postTarget);
    const isAtAttachmentLimit = attachments.length >= INPUT_LIMITS.MAX_ATTACHMENTS;

    // In article mode, character limits don't apply (articles are long-form)
    const isOverCharLimit = !articleMode && charResult.allOver;
    const isAnyTargetOver = !articleMode && charResult.anyOver;
    const isNearCharLimit = !articleMode && charResult.counts.some(c => c.status === 'warn' || c.status === 'danger');

    const hasContent = articleMode
        ? (articleDataRef.current?.content?.trim().length ?? 0) > 0 || (articleDataRef.current?.title?.trim().length ?? 0) > 0
        : composeText.trim().length > 0;
    const canPost = !isPosting && !isOverCharLimit && (hasContent || attachments.length > 0);

    const handlePublish = async () => {
        if (!canPost) return;
        if (articleMode && articleDataRef.current) {
            const { content, title, contentJson } = articleDataRef.current;
            await onPublish(content, postTarget, attachments, {
                itemType: 'Article',
                title,
                contentJson,
            });
            articleDataRef.current = null;
        } else {
            await onPublish(composeText, postTarget, attachments);
        }
        setComposeText('');
        setAttachments([]);
        setFileSizeWarnings([]);
    };

    const handleAddMedia = async () => {
        if (isAtAttachmentLimit) return;
        try {
            const selected = await open({
                multiple: true,
                filters: [{
                    name: 'Media',
                    extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif', 'mp4', 'mov']
                }]
            });

            const paths: string[] = [];
            if (selected && Array.isArray(selected)) {
                paths.push(...selected);
            } else if (selected) {
                paths.push(selected as string);
            }
            if (paths.length === 0) return;

            // Enforce max attachment count
            const slotsLeft = INPUT_LIMITS.MAX_ATTACHMENTS - attachments.length;
            const accepted = paths.slice(0, slotsLeft);

            // Check file sizes via Tauri
            const warnings: string[] = [];
            const validated: Attachment[] = [];
            for (const path of accepted) {
                let fileSize = 0;
                try {
                    fileSize = await invoke<number>('get_file_size', { path });
                } catch {
                    // If command not available, accept without size check
                }

                if (fileSize > INPUT_LIMITS.FILE_MAX_BYTES) {
                    warnings.push(`${path.split('/').pop() || path}: ${formatFileSize(fileSize)} exceeds 100 MB limit — skipped`);
                    continue;
                }
                if (fileSize > INPUT_LIMITS.FILE_WARN_BYTES) {
                    warnings.push(`${path.split('/').pop() || path}: ${formatFileSize(fileSize)} — large file`);
                }
                validated.push({
                    path,
                    preview: convertFileSrc(path),
                    mime_type: path.toLowerCase().endsWith('.mp4') || path.toLowerCase().endsWith('.mov') ? 'video/mp4' : 'image/jpeg',
                    file_size: fileSize || undefined,
                });
            }
            if (warnings.length > 0) setFileSizeWarnings(warnings);
            if (validated.length > 0) setAttachments([...attachments, ...validated]);

            if (paths.length > slotsLeft) {
                setFileSizeWarnings(prev => [...prev, `Only ${slotsLeft} of ${paths.length} files added (max ${INPUT_LIMITS.MAX_ATTACHMENTS})`]);
            }
        } catch (err) {
            console.error('Failed to open file dialog:', err);
        }
    };

    const removeAttachment = (index: number) => {
        setAttachments(attachments.filter((_, i) => i !== index));
    };

    return (
        <div className="feed-compose" style={{ marginBottom: 12 }}>
            <div className="card" style={{ marginBottom: 12, padding: 12 }}>
                <h3 style={{ marginBottom: 8, fontSize: '0.95rem', fontWeight: 600, display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                    Create Post
                    <button
                        className={`btn btn-ghost btn-sm ${articleMode ? 'active' : ''}`}
                        onClick={() => setArticleMode(!articleMode)}
                        title={articleMode ? 'Switch to short post' : 'Switch to article editor'}
                        style={{ padding: '2px 8px', fontSize: '0.72rem' } as React.CSSProperties}
                        type="button"
                    >
                        {articleMode ? '📝 Article' : '📄 Article Mode'}
                    </button>
                </h3>

                {articleMode ? (
                    <ArticleEditor
                        onChange={(data) => { articleDataRef.current = data; }}
                    />
                ) : (
                    <>
                        <textarea
                            value={composeText}
                            onChange={e => setComposeText(e.target.value)}
                            placeholder="Share something..."
                            rows={3}
                            style={{
                                resize: 'vertical',
                                marginBottom: 4,
                                width: '100%',
                                background: 'rgba(255,255,255,0.05)',
                                    border: `1px solid ${isOverCharLimit ? '#e74c3c' : isAnyTargetOver ? '#e67e22' : isNearCharLimit ? '#f39c12' : 'rgba(255,255,255,0.1)'}`,
                                borderRadius: 8,
                                padding: 10,
                                color: 'inherit',
                                fontSize: '0.9rem'
                            }}
                        />
                        <div style={{
                            display: 'flex',
                            justifyContent: 'space-between',
                            alignItems: 'center',
                            marginBottom: 8,
                            fontSize: '0.72rem',
                            color: 'var(--text-muted)',
                        }}>
                            <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
                                {composeText.length > 0 && charResult.counts.map(c => (
                                    <span key={c.target} style={{ color: STATUS_COLORS[c.status] }} title={`${c.target}: ${c.current}/${c.limit}`}>
                                        {TARGET_ICONS[c.target]} {c.current}/{c.limit}
                                    </span>
                                ))}
                            </div>
                            <span style={{ color: 'var(--text-muted)' }}>
                                {attachments.length > 0 && `${attachments.length} / ${INPUT_LIMITS.MAX_ATTACHMENTS} attachments`}
                            </span>
                        </div>
                        {isAnyTargetOver && !isOverCharLimit && (
                            <div style={{
                                fontSize: '0.72rem',
                                color: '#e67e22',
                                background: 'rgba(230,126,34,0.08)',
                                padding: '4px 8px',
                                borderRadius: 4,
                                marginBottom: 8,
                            }}>
                                ⚠️ Text exceeds limit on: {charResult.truncatedTargets.map(t =>
                                    `${TARGET_ICONS[t]} ${t} (by ${charResult.counts.find(c => c.target === t)?.overBy ?? 0})`
                                ).join(', ')}
                            </div>
                        )}
                    </>
                )}

                {attachments.length > 0 && (
                    <div style={{ display: 'flex', gap: 8, marginBottom: 12, overflowX: 'auto', paddingBottom: 8 } as React.CSSProperties}>
                        {attachments.map((att, i) => (
                            <div key={i} style={{ position: 'relative', flexShrink: 0 }}>
                                {att.mime_type?.startsWith('video/') ? (
                                    <video
                                        src={att.preview}
                                        style={{ width: 80, height: 80, objectFit: 'cover', borderRadius: 6 }}
                                    />
                                ) : (
                                    <img
                                        src={att.preview}
                                        style={{ width: 80, height: 80, objectFit: 'cover', borderRadius: 6 }}
                                    />
                                )}
                                <button
                                    onClick={() => removeAttachment(i)}
                                    style={{
                                        position: 'absolute',
                                        top: -4,
                                        right: -4,
                                        background: 'rgba(0,0,0,0.7)',
                                        color: 'white',
                                        border: 'none',
                                        borderRadius: '50%',
                                        width: 20,
                                        height: 20,
                                        cursor: 'pointer',
                                        fontSize: 12,
                                        display: 'flex',
                                        alignItems: 'center',
                                        justifyContent: 'center'
                                    }}
                                >
                                    ✕
                                </button>
                                {att.file_size && (
                                    <div style={{
                                        position: 'absolute',
                                        bottom: 2,
                                        left: 2,
                                        right: 2,
                                        background: att.file_size > INPUT_LIMITS.FILE_WARN_BYTES ? 'rgba(243,156,18,0.9)' : 'rgba(0,0,0,0.65)',
                                        color: '#fff',
                                        fontSize: 9,
                                        textAlign: 'center',
                                        borderRadius: 3,
                                        padding: '1px 0',
                                    } as React.CSSProperties}>
                                        {formatFileSize(att.file_size)}
                                    </div>
                                )}
                            </div>
                        ))}
                    </div>
                )}

                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center', justifyContent: 'space-between' }}>
                    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center' }}>
                        <button
                            className="btn btn-ghost btn-sm"
                            onClick={handleAddMedia}
                            disabled={isAtAttachmentLimit}
                            title={isAtAttachmentLimit ? `Max ${INPUT_LIMITS.MAX_ATTACHMENTS} attachments` : 'Add Images/Video'}
                            style={{ padding: '4px 8px', opacity: isAtAttachmentLimit ? 0.4 : 1 }}
                        >
                            🖼️ Media
                        </button>
                        <div style={{ width: 1, height: 20, background: 'rgba(255,255,255,0.1)', margin: '0 4px' }} />
                        {[
                            { id: 'local', label: '🔒 Local Only', title: 'Stored in your vault, not shared' },
                            { id: 'bluesky', label: '🦋 Bluesky', title: 'Posts to your Bluesky account' },
                            { id: 'all', label: '🌐 All Networks', title: 'Posts to all linked networks + P2P' },
                        ].map(opt => (
                            <button
                                key={opt.id}
                                className={`btn btn-ghost btn-sm ${postTarget === opt.id ? 'active' : ''}`}
                                onClick={() => setPostTarget(opt.id as typeof postTarget)}
                                title={opt.title}
                                style={{ padding: '2px 6px', fontSize: '0.75rem' }}
                            >
                                {opt.label}
                            </button>
                        ))}
                    </div>
                    <button
                        className="btn btn-primary"
                        onClick={handlePublish}
                        disabled={!canPost}
                        title={isOverCharLimit ? 'Post exceeds character limit on all targets' : isAnyTargetOver ? 'Post will be truncated on some targets' : ''}
                        style={{ padding: '6px 16px' }}
                    >
                        {isPosting ? 'Posting...' : 'Post'}
                    </button>
                </div>
                {postStatus && (
                    <div style={{ marginTop: 8, fontSize: '0.8rem', color: 'var(--text-secondary)', background: 'rgba(255,255,255,0.03)', padding: '4px 8px', borderRadius: 4 }}>
                        {postStatus}
                    </div>
                )}
                {fileSizeWarnings.length > 0 && (
                    <div style={{ marginTop: 6, fontSize: '0.75rem', color: '#f39c12', background: 'rgba(243,156,18,0.08)', padding: '6px 8px', borderRadius: 4 }}>
                        {fileSizeWarnings.map((w, i) => <div key={i}>⚠️ {w}</div>)}
                        <button
                            onClick={() => setFileSizeWarnings([])}
                            style={{ marginTop: 4, background: 'none', border: 'none', color: 'var(--text-muted)', cursor: 'pointer', fontSize: '0.7rem', padding: 0 }}
                        >
                            Dismiss
                        </button>
                    </div>
                )}
            </div>
        </div>
    );
}
