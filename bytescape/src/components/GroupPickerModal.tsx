import { useState, useEffect } from 'react';
import ReactDOM from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { LocalGroup } from '../types';

/**
 * ═══════════════════════════════════════════════════════════════════════════════
 * PURPOSE
 * ═══════════════════════════════════════════════════════════════════════════════
 * Presents a modal picker for selecting one joined group and sharing a single
 * feed item into that group.
 *
 * HOW IT WORKS
 * - Loads memberships on mount by calling backend command `get_user_group_memberships`.
 * - Filters groups client-side by `searchQuery`.
 * - On selection, calls backend command `share_to_group` with `itemId` + `targetGroupId`.
 * - Renders via portal into `document.body` so modal layering is independent of
 *   parent layout stacking contexts.
 * - Supports Escape and overlay-click close patterns.
 *
 * RELATED FILES
 * - src/types/index.ts (`LocalGroup` shape mirrored from backend payload)
 * - src/screens/FeedScreen.tsx (share affordance entry point)
 * - src-tauri/src/lib.rs (Tauri command registration for group commands)
 *
 * CROSS-SYSTEM NOTES
 * - Backend interaction is direct via Tauri `invoke()` (no intermediary store).
 * - Parent screen owns success lifecycle via `onShared()` (toast/UI update flow).
 */

interface GroupPickerModalProps {
    /** The item ID to share into the selected group */
    itemId: string;
    onClose: () => void;
    onShared: (groupId: string, groupName: string) => void;
}

/**
 * Modal that lets the user pick one of their joined groups
 * and shares a feed item into it via `share_to_group`.
 */
export function GroupPickerModal({ itemId, onClose, onShared }: GroupPickerModalProps) {
    const [groups, setGroups] = useState<LocalGroup[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [searchQuery, setSearchQuery] = useState('');
    const [sharingTo, setSharingTo] = useState<string | null>(null);

    // ═══ Data hydration: fetch joined groups from backend once on mount ═══
    useEffect(() => {
        invoke<LocalGroup[]>('get_user_group_memberships')
            .then(g => {
                setGroups(g);
                setLoading(false);
            })
            .catch(e => {
                console.error('Failed to load groups:', e);
                setError(typeof e === 'string' ? e : 'Failed to load groups');
                setLoading(false);
            });
    }, []);

    // ═══ Local projection: derive visible subset from search text ═══
    const filtered = searchQuery
        ? groups.filter(g => g.name.toLowerCase().includes(searchQuery.toLowerCase()))
        : groups;

    // ═══ Mutation: perform share operation and report success upward ═══
    const handleShare = async (group: LocalGroup) => {
        setSharingTo(group.group_id);
        try {
            await invoke('share_to_group', {
                itemId,
                targetGroupId: group.group_id,
            });
            onShared(group.group_id, group.name);
        } catch (e) {
            console.error('Failed to share to group:', e);
            setError(typeof e === 'string' ? e : 'Failed to share');
            setSharingTo(null);
        }
    };

    // ═══ Accessibility: close on Escape while mounted ═══
    useEffect(() => {
        const handler = (e: KeyboardEvent) => {
            if (e.key === 'Escape') onClose();
        };
        document.addEventListener('keydown', handler);
        return () => document.removeEventListener('keydown', handler);
    }, [onClose]);

    return ReactDOM.createPortal(
        <div className="modal-overlay" onClick={onClose}>
            <div
                className="group-picker-modal win98-box"
                onClick={e => e.stopPropagation()}
                role="dialog"
                aria-label="Share to Group"
            >
                <div className="group-picker-header">
                    <span className="group-picker-title">📋 Share to Group</span>
                    <button className="group-picker-close" onClick={onClose} aria-label="Close">✕</button>
                </div>

                {groups.length > 3 && (
                    <div className="group-picker-search">
                        <input
                            type="text"
                            placeholder="Search groups..."
                            value={searchQuery}
                            onChange={e => setSearchQuery(e.target.value)}
                            autoFocus
                        />
                    </div>
                )}

                <div className="group-picker-list">
                    {loading && (
                        <div className="group-picker-empty">Loading groups...</div>
                    )}

                    {!loading && error && (
                        <div className="group-picker-empty" style={{ color: 'var(--text-error, #e55)' }}>
                            {error}
                        </div>
                    )}

                    {!loading && !error && filtered.length === 0 && (
                        <div className="group-picker-empty">
                            {groups.length === 0
                                ? 'You haven\'t joined any groups yet.'
                                : 'No groups match your search.'}
                        </div>
                    )}

                    {!loading && !error && filtered.map(group => (
                        <button
                            key={group.group_id}
                            className="group-picker-item"
                            onClick={() => handleShare(group)}
                            disabled={sharingTo !== null}
                        >
                            <span className="group-picker-name">{group.name}</span>
                            <span className="group-picker-members">
                                {group.member_count} member{group.member_count !== 1 ? 's' : ''}
                            </span>
                            {sharingTo === group.group_id && (
                                <span className="group-picker-sharing">Sharing...</span>
                            )}
                        </button>
                    ))}
                </div>
            </div>
        </div>,
        document.body
    );
}
