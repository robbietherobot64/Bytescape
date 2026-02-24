import { useState } from 'react';

interface ComposeBoxProps {
    onPublish: (text: string, target: 'local' | 'bluesky' | 'activitypub' | 'all') => Promise<void>;
    isPosting: boolean;
    postStatus: string | null;
}

export function ComposeBox({ onPublish, isPosting, postStatus }: ComposeBoxProps) {
    const [composeText, setComposeText] = useState('');
    const [postTarget, setPostTarget] = useState<'local' | 'bluesky' | 'activitypub' | 'all'>('local');

    const handlePublish = async () => {
        if (!composeText.trim()) return;
        await onPublish(composeText, postTarget);
        setComposeText('');
    };

    return (
        <div className="feed-compose" style={{ marginBottom: 12 }}>
            <div className="card" style={{ marginBottom: 12 }}>
                <h3 style={{ marginBottom: 8 }}>Create Post</h3>
                <textarea
                    value={composeText}
                    onChange={e => setComposeText(e.target.value)}
                    placeholder="Share something..."
                    rows={3}
                    style={{ resize: 'vertical', marginBottom: 10 }}
                />
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center', justifyContent: 'space-between' }}>
                    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
                        {[
                            { id: 'local', label: '🔒 Local P2P' },
                            { id: 'bluesky', label: '🦋 Bluesky' },
                            { id: 'activitypub', label: '🐘 ActivityPub (Soon)' },
                            { id: 'all', label: '🌐 All' },
                        ].map(opt => (
                            <button
                                key={opt.id}
                                className={`btn btn-ghost btn-sm ${postTarget === opt.id ? 'active' : ''}`}
                                onClick={() => setPostTarget(opt.id as typeof postTarget)}
                                title={opt.id === 'activitypub' ? 'Placeholder target, not implemented yet' : undefined}
                            >
                                {opt.label}
                            </button>
                        ))}
                    </div>
                    <button className="btn btn-primary" onClick={handlePublish} disabled={isPosting || !composeText.trim()}>
                        {isPosting ? 'Posting...' : 'Post'}
                    </button>
                </div>
                {postStatus && (
                    <div style={{ marginTop: 8, fontSize: '0.85rem', color: 'var(--text-secondary)' }}>
                        {postStatus}
                    </div>
                )}
            </div>
        </div>
    );
}
