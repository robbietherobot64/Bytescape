import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { convertFileSrc } from '@tauri-apps/api/core';

export interface Attachment {
    path?: string;
    blob_hash?: string;
    mime_type?: string;
    preview?: string;
}

interface ComposeBoxProps {
    onPublish: (text: string, target: 'local' | 'bluesky' | 'activitypub' | 'all', attachments: Attachment[]) => Promise<void>;
    isPosting: boolean;
    postStatus: string | null;
    initialText?: string;
    initialAttachments?: Attachment[];
}

export function ComposeBox({ onPublish, isPosting, postStatus, initialText = '', initialAttachments = [] }: ComposeBoxProps) {
    const [composeText, setComposeText] = useState(initialText);
    const [postTarget, setPostTarget] = useState<'local' | 'bluesky' | 'activitypub' | 'all'>('local');
    const [attachments, setAttachments] = useState<Attachment[]>(initialAttachments);

    const handlePublish = async () => {
        if (!composeText.trim() && attachments.length === 0) return;
        await onPublish(composeText, postTarget, attachments);
        setComposeText('');
        setAttachments([]);
    };

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
                const newAttachments: Attachment[] = selected.map(path => ({
                    path,
                    preview: convertFileSrc(path),
                    mime_type: path.toLowerCase().endsWith('.mp4') || path.toLowerCase().endsWith('.mov') ? 'video/mp4' : 'image/jpeg'
                }));
                setAttachments([...attachments, ...newAttachments]);
            } else if (selected) {
                const path = selected as string;
                setAttachments([...attachments, {
                    path,
                    preview: convertFileSrc(path),
                    mime_type: path.toLowerCase().endsWith('.mp4') || path.toLowerCase().endsWith('.mov') ? 'video/mp4' : 'image/jpeg'
                }]);
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
                <h3 style={{ marginBottom: 8, fontSize: '0.95rem', fontWeight: 600 }}>Create Post</h3>
                <textarea
                    value={composeText}
                    onChange={e => setComposeText(e.target.value)}
                    placeholder="Share something..."
                    rows={3}
                    style={{
                        resize: 'vertical',
                        marginBottom: 10,
                        width: '100%',
                        background: 'rgba(255,255,255,0.05)',
                        border: '1px solid rgba(255,255,255,0.1)',
                        borderRadius: 8,
                        padding: 10,
                        color: 'inherit',
                        fontSize: '0.9rem'
                    }}
                />

                {attachments.length > 0 && (
                    <div style={{ display: 'flex', gap: 8, marginBottom: 12, overflowX: 'auto', paddingBottom: 8 }}>
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
                            </div>
                        ))}
                    </div>
                )}

                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center', justifyContent: 'space-between' }}>
                    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center' }}>
                        <button
                            className="btn btn-ghost btn-sm"
                            onClick={handleAddMedia}
                            title="Add Images/Video"
                            style={{ padding: '4px 8px' }}
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
                        disabled={isPosting || (!composeText.trim() && attachments.length === 0)}
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
            </div>
        </div>
    );
}
