/**
 * Guestbook Widget (B2.4)
 *
 * Allows visitors to sign the profile guestbook.
 * Entries are stored via Tauri commands and displayed in reverse chronological order.
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { GuestbookEntry } from '../../types';
import { BsTrash, BsPencilSquare } from 'react-icons/bs';

interface GuestbookWidgetProps {
    editable?: boolean;
}

export function GuestbookWidget({ editable = false }: GuestbookWidgetProps) {
    const [entries, setEntries] = useState<GuestbookEntry[]>([]);
    const [newAuthor, setNewAuthor] = useState('');
    const [newMessage, setNewMessage] = useState('');
    const [submitting, setSubmitting] = useState(false);

    const loadEntries = useCallback(async () => {
        try {
            const result = await invoke<GuestbookEntry[]>('list_guestbook_entries');
            setEntries(result);
        } catch {
            // Silently fail — no guestbook entries yet
        }
    }, []);

    useEffect(() => {
        loadEntries();
    }, [loadEntries]);

    const handleSign = async () => {
        if (!newAuthor.trim() || !newMessage.trim()) return;
        setSubmitting(true);
        try {
            await invoke('add_guestbook_entry', {
                author: newAuthor.trim().slice(0, 200),
                message: newMessage.trim().slice(0, 10000),
                nodeId: null,
            });
            setNewAuthor('');
            setNewMessage('');
            await loadEntries();
        } catch (err) {
            console.error('Failed to sign guestbook:', err);
        }
        setSubmitting(false);
    };

    const handleDelete = async (entryId: string) => {
        try {
            await invoke('delete_guestbook_entry', { entryId });
            await loadEntries();
        } catch (err) {
            console.error('Failed to delete entry:', err);
        }
    };

    const formatDate = (ts: number) => {
        const d = new Date(ts * 1000);
        return d.toLocaleDateString('en-US', {
            year: 'numeric',
            month: 'short',
            day: 'numeric',
            hour: '2-digit',
            minute: '2-digit',
        });
    };

    return (
        <div style={{
            background: 'var(--bg-secondary)',
            border: '2px solid var(--border-color)',
            borderRadius: 'var(--border-radius)',
            padding: 16,
        } as React.CSSProperties}>
            <div style={{
                fontSize: 14,
                fontWeight: 600,
                color: 'var(--text-primary)',
                marginBottom: 12,
                borderBottom: '1px solid var(--border-color)',
                paddingBottom: 6,
            } as React.CSSProperties}>
                📖 Guestbook ({entries.length})
            </div>

            {/* Sign Form */}
            <div style={{
                background: 'var(--bg-tertiary)',
                borderRadius: 6,
                padding: 12,
                marginBottom: 12,
                border: '1px solid var(--border-color)',
            } as React.CSSProperties}>
                <div style={{ display: 'flex', gap: 8, marginBottom: 8 } as React.CSSProperties}>
                    <input
                        type="text"
                        placeholder="Your name"
                        value={newAuthor}
                        onChange={e => setNewAuthor(e.target.value.slice(0, 200))}
                        style={{
                            flex: 1,
                            background: 'var(--bg-primary)',
                            border: '1px solid var(--border-color)',
                            borderRadius: 4,
                            padding: '6px 8px',
                            color: 'var(--text-primary)',
                            fontSize: 13,
                        } as React.CSSProperties}
                    />
                </div>
                <textarea
                    placeholder="Leave a message..."
                    value={newMessage}
                    onChange={e => setNewMessage(e.target.value.slice(0, 10000))}
                    rows={2}
                    style={{
                        width: '100%',
                        background: 'var(--bg-primary)',
                        border: '1px solid var(--border-color)',
                        borderRadius: 4,
                        padding: '6px 8px',
                        color: 'var(--text-primary)',
                        fontSize: 13,
                        resize: 'vertical',
                        marginBottom: 8,
                    } as React.CSSProperties}
                />
                <button
                    onClick={handleSign}
                    disabled={submitting || !newAuthor.trim() || !newMessage.trim()}
                    style={{
                        background: 'var(--accent-purple)',
                        color: '#fff',
                        border: 'none',
                        borderRadius: 4,
                        padding: '6px 16px',
                        cursor: 'pointer',
                        fontSize: 13,
                        display: 'flex',
                        alignItems: 'center',
                        gap: 6,
                        opacity: submitting || !newAuthor.trim() || !newMessage.trim() ? 0.5 : 1,
                    } as React.CSSProperties}
                >
                    <BsPencilSquare /> {submitting ? 'Signing...' : 'Sign Guestbook'}
                </button>
            </div>

            {/* Entries */}
            <div style={{ maxHeight: 300, overflowY: 'auto' } as React.CSSProperties}>
                {entries.length === 0 ? (
                    <div style={{ color: 'var(--text-muted)', fontSize: 12, textAlign: 'center', padding: 16 } as React.CSSProperties}>
                        No entries yet. Be the first to sign!
                    </div>
                ) : (
                    entries.map(entry => (
                        <div key={entry.id} style={{
                            borderBottom: '1px solid var(--border-color)',
                            padding: '10px 0',
                        } as React.CSSProperties}>
                            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4 } as React.CSSProperties}>
                                <span style={{ fontWeight: 600, color: 'var(--accent-purple)', fontSize: 13 } as React.CSSProperties}>
                                    {entry.author}
                                </span>
                                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                                    <span style={{ color: 'var(--text-muted)', fontSize: 11 } as React.CSSProperties}>
                                        {formatDate(entry.timestamp)}
                                    </span>
                                    {editable && (
                                        <button
                                            onClick={() => handleDelete(entry.id)}
                                            style={{ background: 'none', border: 'none', color: '#f44336', cursor: 'pointer', fontSize: 12, padding: 2 } as React.CSSProperties}
                                            title="Delete entry"
                                        >
                                            <BsTrash />
                                        </button>
                                    )}
                                </div>
                            </div>
                            <div style={{ color: 'var(--text-secondary)', fontSize: 13, lineHeight: 1.5, whiteSpace: 'pre-wrap' } as React.CSSProperties}>
                                {entry.message}
                            </div>
                        </div>
                    ))
                )}
            </div>
        </div>
    );
}
