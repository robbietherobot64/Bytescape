/**
 * PURPOSE
 * - Render and manage an editable, section-based profile bio surface.
 * - Support section CRUD/reordering and user-generated guestbook activity.
 *
 * HOW IT WORKS
 * - Loads serialized widget payload from backend, merges/falls back to defaults,
 *   and renders visible sections in deterministic order.
 * - Edit mode works on a draft array (`editSections`) and commits atomically.
 * - Type-specific renderers provide specialized UX for links, guestbook, etc.
 *
 * RELATED FILES
 * - src/components/ProfileBioSections.css (visual styles)
 * - src/components/Win98Box.tsx (window/card container)
 * - src-tauri/src/lib.rs (Tauri command handlers invoked from this component)
 *
 * CROSS-SYSTEM NOTES
 * - Backend command interactions via `invoke()`:
 *   - `get_widget_data` / `save_widget_data` (profile widget persistence)
 *   - `list_guestbook_entries` / `add_guestbook_entry` / `delete_guestbook_entry`
 *   - `list_follows` / `list_contacts` (connection metadata)
 * - This file is a frontend lens over engine data; command names and payloads
 *   must stay synchronized with Rust command signatures.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { LinkedAccount } from '../types';
import { Win98Box } from './Win98Box';
import { BsPencilFill, BsCheckLg, BsXLg, BsPlusLg, BsArrowUp, BsArrowDown, BsTrash } from 'react-icons/bs';
import './ProfileBioSections.css';

// ═══════════════════════════════════════════════════════════════════════════
// Data Model
// ═══════════════════════════════════════════════════════════════════════════

/** Public link model used by the `links` section serializer. */
export interface ProfileLink {
    label: string;
    url: string;
}

/** Public section model persisted in widget data JSON. */
export interface ProfileSection {
    id: string;
    type: 'about' | 'connections' | 'now-playing' | 'guestbook' | 'links' | 'custom';
    title: string;
    content: string;
    order: number;
    visible: boolean;
}

interface GuestbookEntry {
    id: string;
    author: string;
    message: string;
    timestamp: number;
    node_id: string | null;
}

const DEFAULT_SECTIONS: ProfileSection[] = [
    { id: 'about', type: 'about', title: 'About Me', content: '', order: 0, visible: true },
    { id: 'now-playing', type: 'now-playing', title: 'Now Playing', content: '', order: 1, visible: true },
    { id: 'links', type: 'links', title: 'Links', content: '[]', order: 2, visible: true },
    { id: 'guestbook', type: 'guestbook', title: 'Guestbook', content: '', order: 3, visible: true },
    { id: 'connections', type: 'connections', title: 'Connections', content: '', order: 4, visible: true },
];

/** Maps section types to their profile grid column. */
const COLUMN_MAP: Record<string, 'left' | 'center' | 'right' | 'full'> = {
    about: 'left',
    'now-playing': 'left',
    links: 'left',
    guestbook: 'center',
    connections: 'right',
    custom: 'full',
};

function getColumnKey(type: string): 'left' | 'center' | 'right' | 'full' {
    return COLUMN_MAP[type] || 'full';
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

function generateId(): string {
    return `section-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function parseBioData(raw: string | null): ProfileSection[] | null {
    if (!raw) return null;
    try {
        const parsed = JSON.parse(raw);
        if (parsed && Array.isArray(parsed.sections)) {
            return parsed.sections as ProfileSection[];
        }
    } catch {
        // Invalid JSON — fall back to defaults
    }
    return null;
}

function serializeBioData(sections: ProfileSection[]): string {
    return JSON.stringify({ version: 1, sections });
}

// ═══════════════════════════════════════════════════════════════════════════
// Props
// ═══════════════════════════════════════════════════════════════════════════

interface ProfileBioSectionsProps {
    handle?: string | null;
    did?: string | null;
    source?: string | null;
    /** When provided, external parent controls edit mode (profile screen toggle). */
    editMode?: boolean;
}

// ═══════════════════════════════════════════════════════════════════════════
// Component
// ═══════════════════════════════════════════════════════════════════════════

/**
 * Editable profile bio sections surface.
 *
 * Behavior summary:
 * - View mode: shows visible sections in saved order.
 * - Edit mode: allows rename/reorder/content editing/add/remove operations.
 * - Save action serializes current draft and persists it through backend commands.
 */
export function ProfileBioSections({ handle, did, source, editMode: externalEditMode }: ProfileBioSectionsProps) {
    const [sections, setSections] = useState<ProfileSection[]>([]);
    const [editing, setEditing] = useState(false);
    const [editSections, setEditSections] = useState<ProfileSection[]>([]);
    const [editingSection, setEditingSection] = useState<string | null>(null);
    const [editingTitle, setEditingTitle] = useState<string | null>(null);
    const [saveStatus, setSaveStatus] = useState<string | null>(null);
    const [loaded, setLoaded] = useState(false);
    const [showAddMenu, setShowAddMenu] = useState(false);

    // Guestbook state
    const [guestbookEntries, setGuestbookEntries] = useState<GuestbookEntry[]>([]);
    const [guestbookMessage, setGuestbookMessage] = useState('');
    const [guestbookSubmitting, setGuestbookSubmitting] = useState(false);

    // Connections state
    const [followCount, setFollowCount] = useState(0);
    const [contactCount, setContactCount] = useState(0);
    const [connectionHandles, setConnectionHandles] = useState<string[]>([]);

    // Linked accounts for links auto-populate
    const [linkedAccounts, setLinkedAccounts] = useState<LinkedAccount[]>([]);

    // Add-section menu ref for click-outside
    const addMenuRef = useRef<HTMLDivElement>(null);

    // Load sections from backend on mount
    useEffect(() => {
        invoke<string | null>('get_widget_data')
            .then((raw) => {
                const parsed = parseBioData(raw);
                setSections(parsed || DEFAULT_SECTIONS.map(s => ({ ...s })));
                setLoaded(true);
            })
            .catch(() => {
                setSections(DEFAULT_SECTIONS.map(s => ({ ...s })));
                setLoaded(true);
            });
    }, []);

    // Load guestbook entries
    useEffect(() => {
        invoke<GuestbookEntry[]>('list_guestbook_entries')
            .then(setGuestbookEntries)
            .catch(() => {});
    }, []);

    // Load connection counts
    useEffect(() => {
        if (did) {
            invoke<Array<{ did: string; handle: string; displayName?: string }>>('list_follows', { actor: did })
                .then((follows) => {
                    setFollowCount(follows.length);
                    setConnectionHandles(follows.slice(0, 8).map(f => f.handle || f.displayName || f.did.slice(0, 12)));
                })
                .catch(() => {});
        }
        invoke<Array<{ did: string; handle: string }>>('list_contacts')
            .then((contacts) => setContactCount(contacts.length))
            .catch(() => {});
    }, [did]);

    // Load linked accounts for links auto-populate
    useEffect(() => {
        invoke<LinkedAccount[]>('list_linked_accounts')
            .then((accts) => { if (Array.isArray(accts)) setLinkedAccounts(accts); })
            .catch(() => {});
    }, []);

    // Click-outside to close add menu
    useEffect(() => {
        if (!showAddMenu) return;
        function handleClick(e: MouseEvent) {
            if (addMenuRef.current && !addMenuRef.current.contains(e.target as Node)) {
                setShowAddMenu(false);
            }
        }
        document.addEventListener('mousedown', handleClick);
        return () => document.removeEventListener('mousedown', handleClick);
    }, [showAddMenu]);

    // Sync editing state with external editMode prop
    useEffect(() => {
        if (externalEditMode !== undefined) {
            if (externalEditMode && !editing) {
                startEditing();
            } else if (!externalEditMode && editing) {
                cancelEdits();
            }
        }
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [externalEditMode]);

    // Save sections to backend
    const saveSections = useCallback(async (updated: ProfileSection[]) => {
        try {
            await invoke('save_widget_data', { widgetData: serializeBioData(updated) });
            setSections(updated);
            setSaveStatus('✓ Saved');
            setTimeout(() => setSaveStatus(null), 2000);
        } catch (err) {
            console.error('[ProfileBio] Save failed:', err);
            setSaveStatus('✗ Save failed');
            setTimeout(() => setSaveStatus(null), 3000);
        }
    }, []);

    // Enter edit mode
    const startEditing = useCallback(() => {
        setEditSections(sections.map(s => ({ ...s })));
        setEditing(true);
        setEditingSection(null);
        setEditingTitle(null);
    }, [sections]);

    // Save edits
    const saveEdits = useCallback(async () => {
        // Reassign order values to match array index
        const ordered = editSections.map((s, i) => ({ ...s, order: i }));
        await saveSections(ordered);
        setEditing(false);
        setEditingSection(null);
        setEditingTitle(null);
    }, [editSections, saveSections]);

    // Cancel edits
    const cancelEdits = useCallback(() => {
        setEditing(false);
        setEditSections([]);
        setEditingSection(null);
        setEditingTitle(null);
    }, []);

    // Move section up/down
    const moveSection = useCallback((id: string, direction: -1 | 1) => {
        setEditSections(prev => {
            const section = prev.find(s => s.id === id);
            if (!section) return prev;
            const col = getColumnKey(section.type);
            // Gather visible sections in the same column, in current order
            const colSections = prev.filter(s => s.visible !== false && getColumnKey(s.type) === col);
            const colIdx = colSections.findIndex(s => s.id === id);
            const targetColIdx = colIdx + direction;
            if (targetColIdx < 0 || targetColIdx >= colSections.length) return prev;
            // Swap positions in the full list
            const idxA = prev.findIndex(s => s.id === id);
            const idxB = prev.findIndex(s => s.id === colSections[targetColIdx].id);
            const copy = [...prev];
            [copy[idxA], copy[idxB]] = [copy[idxB], copy[idxA]];
            return copy;
        });
    }, []);

    // Delete section
    const deleteSection = useCallback((id: string) => {
        setEditSections(prev => prev.filter(s => s.id !== id));
    }, []);

    // Update section content
    const updateSectionContent = useCallback((id: string, content: string) => {
        setEditSections(prev => prev.map(s => s.id === id ? { ...s, content } : s));
    }, []);

    // Update section title
    const updateSectionTitle = useCallback((id: string, title: string) => {
        setEditSections(prev => prev.map(s => s.id === id ? { ...s, title } : s));
    }, []);

    // Add a new section
    const addSection = useCallback((type: ProfileSection['type'], title: string) => {
        const newSection: ProfileSection = {
            id: generateId(),
            type,
            title,
            content: type === 'links' ? '[]' : '',
            order: editSections.length,
            visible: true,
        };
        setEditSections(prev => [...prev, newSection]);
        setShowAddMenu(false);
    }, [editSections.length]);

    // Add guestbook entry
    const addGuestbookEntry = useCallback(async () => {
        if (!guestbookMessage.trim()) return;
        setGuestbookSubmitting(true);
        try {
            const entry = await invoke<GuestbookEntry>('add_guestbook_entry', {
                author: handle || 'Anonymous',
                message: guestbookMessage.trim(),
                nodeId: null,
            });
            setGuestbookEntries(prev => [entry, ...prev]);
            setGuestbookMessage('');
        } catch (err) {
            console.error('[Guestbook] Add failed:', err);
        } finally {
            setGuestbookSubmitting(false);
        }
    }, [guestbookMessage, handle]);

    // Delete guestbook entry
    const deleteGuestbookEntry = useCallback(async (entryId: string) => {
        try {
            await invoke('delete_guestbook_entry', { entryId });
            setGuestbookEntries(prev => prev.filter(e => e.id !== entryId));
        } catch (err) {
            console.error('[Guestbook] Delete failed:', err);
        }
    }, []);

    // ═══════════════════════════════════════════════════════════════════════
    // Section renderers
    // ═══════════════════════════════════════════════════════════════════════

    const visibleSections = editing
        ? editSections
        : sections.filter(s => s.visible).sort((a, b) => a.order - b.order);

    // Group sections by their assigned column
    const leftSections = visibleSections.filter(s => getColumnKey(s.type) === 'left');
    const centerSections = visibleSections.filter(s => getColumnKey(s.type) === 'center');
    const rightSections = visibleSections.filter(s => getColumnKey(s.type) === 'right');
    const fullSections = visibleSections.filter(s => getColumnKey(s.type) === 'full');
    const hasAnyColumn = leftSections.length > 0 || centerSections.length > 0 || rightSections.length > 0;

    /** Render a single section card (used in each column). */
    function renderSectionCard(section: ProfileSection, colSections: ProfileSection[]) {
        const colIdx = colSections.indexOf(section);
        return (
            <Win98Box
                key={section.id}
                variant="window"
                title={
                    editing && editingTitle === section.id
                        ? undefined
                        : section.title
                }
                className={`bio-section-card ${editing ? 'bio-section-card--editing' : ''}`}
                onClose={editing ? () => deleteSection(section.id) : undefined}
            >
                {editing && editingTitle === section.id && (
                    <input
                        className="bio-section-title-editor"
                        value={section.title}
                        onChange={(e) => updateSectionTitle(section.id, e.target.value)}
                        onBlur={() => setEditingTitle(null)}
                        onKeyDown={(e) => { if (e.key === 'Enter') setEditingTitle(null); }}
                        autoFocus
                    />
                )}

                {editing && (
                    <div
                        className="bio-section-controls"
                        style={{ padding: '4px 8px', borderBottom: '1px solid var(--border-outset-dark, #444)' } as React.CSSProperties}
                    >
                        <button
                            onClick={() => moveSection(section.id, -1)}
                            disabled={colIdx === 0}
                            title="Move up"
                        >
                            <BsArrowUp size={10} />
                        </button>
                        <button
                            onClick={() => moveSection(section.id, 1)}
                            disabled={colIdx === colSections.length - 1}
                            title="Move down"
                        >
                            <BsArrowDown size={10} />
                        </button>
                        <button
                            onClick={() => setEditingTitle(editingTitle === section.id ? null : section.id)}
                            title="Rename section"
                        >
                            Aa
                        </button>
                        <button
                            onClick={() => setEditingSection(editingSection === section.id ? null : section.id)}
                            title="Edit content"
                        >
                            <BsPencilFill size={9} />
                        </button>
                        <span style={{ flex: 1 } as React.CSSProperties} />
                        <button
                            className="bio-section-delete"
                            onClick={() => deleteSection(section.id)}
                            title="Delete section"
                        >
                            <BsTrash size={10} />
                        </button>
                    </div>
                )}

                <div className="bio-section-body">
                    {renderSectionContent(section)}
                </div>
            </Win98Box>
        );
    }

    function renderSectionContent(section: ProfileSection) {
        // If editing this section's content, show textarea
        if (editing && editingSection === section.id) {
            if (section.type === 'links') {
                return renderLinksEditor(section);
            }
            return (
                <textarea
                    className="bio-section-editor"
                    value={section.content}
                    onChange={(e) => updateSectionContent(section.id, e.target.value)}
                    placeholder={`Write your ${section.title.toLowerCase()} here...`}
                    autoFocus
                />
            );
        }

        switch (section.type) {
            case 'about':
                return renderAbout(section);
            case 'connections':
                return renderConnections();
            case 'now-playing':
                return renderNowPlaying(section);
            case 'guestbook':
                return renderGuestbook();
            case 'links':
                return renderLinks(section);
            case 'custom':
                return renderCustom(section);
            default:
                return <p className="bio-section-placeholder">Unknown section type</p>;
        }
    }

    function renderAbout(section: ProfileSection) {
        if (!section.content) {
            return (
                <p className="bio-section-placeholder">
                    {editing ? 'Click ✏️ to write your bio...' : 'Tell the world about yourself.'}
                </p>
            );
        }
        return (
            <div>
                {section.content.split('\n').map((line, i) => (
                    <p key={i}>{line || '\u00A0'}</p>
                ))}
            </div>
        );
    }

    function renderConnections() {
        const totalConnections = followCount + contactCount;
        const displayLimit = 8;
        const showAll = connectionHandles.length > displayLimit;

        return (
            <div className="bio-connections-section">
                {/* Summary badges */}
                <div className="bio-connections-summary">
                    {source === 'bluesky' && handle && (
                        <span className="bio-connection-badge bio-connection-badge--self">
                            🦋 {handle}
                        </span>
                    )}
                    {followCount > 0 && (
                        <span className="bio-connection-badge">
                            👥 {followCount} following
                        </span>
                    )}
                    {contactCount > 0 && (
                        <span className="bio-connection-badge">
                            🤝 {contactCount} contacts
                        </span>
                    )}
                </div>

                {/* Connection handles grid */}
                {connectionHandles.length > 0 && (
                    <div className="bio-connections-grid">
                        {connectionHandles.slice(0, displayLimit).map((h) => (
                            <span key={h} className="bio-connection-badge">
                                <span className="bio-connection-avatar">
                                    {h[0]?.toUpperCase() || '?'}
                                </span>
                                {h}
                            </span>
                        ))}
                        {showAll && (
                            <span className="bio-connection-badge bio-connection-badge--more">
                                +{connectionHandles.length - displayLimit} more
                            </span>
                        )}
                    </div>
                )}

                {totalConnections === 0 && connectionHandles.length === 0 && (
                    <p className="bio-section-placeholder">No connections yet. Add friends to see them here!</p>
                )}
            </div>
        );
    }

    /** Detect embed-worthy URLs and return an iframe, else null. */
    function detectEmbed(content: string): React.ReactNode | null {
        const trimmed = content.trim();
        // Bandcamp album/track
        const bcMatch = trimmed.match(/https?:\/\/([a-z0-9-]+)\.bandcamp\.com\/(track|album)\/([a-z0-9-]+)/i);
        if (bcMatch) {
            return (
                <iframe
                    title="Bandcamp embed"
                    style={{ border: 0, width: '100%', height: 120 } as React.CSSProperties}
                    src={`https://bandcamp.com/EmbeddedPlayer/${bcMatch[2]}=${bcMatch[3]}/size=large/bgcol=333333/linkcol=e99708/tracklist=false/artwork=small/transparent=true/`}
                    seamless
                />
            );
        }
        // SoundCloud
        if (/soundcloud\.com\/[a-z0-9_-]+\/[a-z0-9_-]+/i.test(trimmed)) {
            return (
                <iframe
                    title="SoundCloud embed"
                    width="100%"
                    height={166}
                    scrolling="no"
                    frameBorder="no"
                    allow="autoplay"
                    src={`https://w.soundcloud.com/player/?url=${encodeURIComponent(trimmed)}&color=%23ff5500&auto_play=false&hide_related=true&show_comments=false&show_user=true&show_reposts=false&show_teaser=false`}
                />
            );
        }
        // Spotify track/album/playlist
        const spotMatch = trimmed.match(/https?:\/\/open\.spotify\.com\/(track|album|playlist)\/([a-zA-Z0-9]+)/);
        if (spotMatch) {
            return (
                <iframe
                    title="Spotify embed"
                    style={{ borderRadius: 12, border: 0, width: '100%', height: 152 } as React.CSSProperties}
                    src={`https://open.spotify.com/embed/${spotMatch[1]}/${spotMatch[2]}?theme=0`}
                    allow="autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture"
                    loading="lazy"
                />
            );
        }
        // YouTube
        const ytMatch = trimmed.match(/(?:youtube\.com\/watch\?v=|youtu\.be\/)([a-zA-Z0-9_-]{11})/);
        if (ytMatch) {
            return (
                <iframe
                    title="YouTube embed"
                    width="100%"
                    height={180}
                    src={`https://www.youtube.com/embed/${ytMatch[1]}`}
                    frameBorder="0"
                    allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                    allowFullScreen
                    style={{ borderRadius: 4 } as React.CSSProperties}
                />
            );
        }
        return null;
    }

    function renderNowPlaying(section: ProfileSection) {
        if (!section.content) {
            return (
                <div className="bio-now-playing">
                    <span className="bio-now-playing-icon">💿</span>
                    <span className="bio-section-placeholder">
                        {editing ? 'Click ✏️ to set what you\'re listening to (paste a URL for embeds)...' : 'Nothing playing right now.'}
                    </span>
                </div>
            );
        }
        const embed = detectEmbed(section.content);
        if (embed) {
            return (
                <div className="bio-now-playing">
                    {embed}
                </div>
            );
        }
        return (
            <div className="bio-now-playing">
                <span className="bio-now-playing-icon">💿</span>
                <span className="bio-now-playing-text">{section.content}</span>
            </div>
        );
    }

    function renderGuestbook() {
        return (
            <div>
                <div className="bio-guestbook-form">
                    <input
                        className="bio-guestbook-input"
                        type="text"
                        placeholder="Leave a message..."
                        value={guestbookMessage}
                        onChange={(e) => setGuestbookMessage(e.target.value)}
                        onKeyDown={(e) => { if (e.key === 'Enter') addGuestbookEntry(); }}
                        maxLength={500}
                    />
                    <button
                        className="bio-guestbook-send"
                        onClick={addGuestbookEntry}
                        disabled={guestbookSubmitting || !guestbookMessage.trim()}
                    >
                        Sign
                    </button>
                </div>
                {guestbookEntries.length === 0 && (
                    <p className="bio-section-placeholder" style={{ marginTop: 8 } as React.CSSProperties}>
                        Be the first to sign the guestbook! ✍️
                    </p>
                )}
                <div className="bio-guestbook-entries" style={{ marginTop: guestbookEntries.length > 0 ? 8 : 0 } as React.CSSProperties}>
                    {guestbookEntries.slice(0, 10).map((entry) => (
                        <div key={entry.id} className="bio-guestbook-entry">
                            <div className="bio-guestbook-entry-header">
                                <span className="bio-guestbook-author">{entry.author}</span>
                                <span className="bio-guestbook-time">
                                    {new Date(entry.timestamp * 1000).toLocaleDateString()}
                                </span>
                            </div>
                            <div className="bio-guestbook-message">{entry.message}</div>
                            {editing && (
                                <button
                                    className="bio-section-controls"
                                    style={{ marginTop: 4, fontSize: '0.75rem', color: '#ef4444', background: 'none', border: 'none', cursor: 'pointer', padding: 0 } as React.CSSProperties}
                                    onClick={() => deleteGuestbookEntry(entry.id)}
                                >
                                    🗑 Remove
                                </button>
                            )}
                        </div>
                    ))}
                </div>
            </div>
        );
    }

    function renderLinks(section: ProfileSection) {
        let links: ProfileLink[] = [];
        try {
            links = JSON.parse(section.content || '[]');
        } catch {
            links = [];
        }

        // Auto-populate from linked accounts when no custom links exist
        const autoLinks: ProfileLink[] = linkedAccounts
            .filter(a => a.active)
            .map(a => {
                const proto = a.protocol.charAt(0).toUpperCase() + a.protocol.slice(1);
                return {
                    label: `${proto}: ${a.display_handle}`,
                    url: a.protocol === 'bluesky'
                        ? `https://bsky.app/profile/${a.display_handle.replace('@', '')}`
                        : a.protocol === 'mastodon' && a.platform_identifier
                            ? `https://${a.platform_identifier.split('@').pop()}/@${a.platform_identifier.split('@')[1]}`
                            : '#',
                };
            });

        const allLinks = links.length > 0 ? links : autoLinks;

        if (allLinks.length === 0) {
            return (
                <p className="bio-section-placeholder">
                    {editing ? 'Click ✏️ to add your links...' : 'No links added yet.'}
                </p>
            );
        }

        return (
            <div className="bio-links-list">
                {links.length === 0 && autoLinks.length > 0 && (
                    <div className="bio-links-auto-label">Auto-detected from linked accounts:</div>
                )}
                {allLinks.map((link, i) => (
                    <div key={i} className="bio-link-item">
                        🔗 <a href={link.url} target="_blank" rel="noopener noreferrer">
                            {link.label || link.url}
                        </a>
                    </div>
                ))}
            </div>
        );
    }

    function renderLinksEditor(section: ProfileSection) {
        let links: ProfileLink[] = [];
        try {
            links = JSON.parse(section.content || '[]');
        } catch {
            links = [];
        }

        const updateLink = (index: number, field: 'label' | 'url', value: string) => {
            const updated = [...links];
            updated[index] = { ...updated[index], [field]: value };
            updateSectionContent(section.id, JSON.stringify(updated));
        };

        const addLink = () => {
            const updated = [...links, { label: '', url: '' }];
            updateSectionContent(section.id, JSON.stringify(updated));
        };

        const removeLink = (index: number) => {
            const updated = links.filter((_, i) => i !== index);
            updateSectionContent(section.id, JSON.stringify(updated));
        };

        return (
            <div>
                {links.map((link, i) => (
                    <div key={i} className="bio-link-edit-row">
                        <input
                            className="bio-link-input"
                            type="text"
                            placeholder="Label"
                            value={link.label}
                            onChange={(e) => updateLink(i, 'label', e.target.value)}
                        />
                        <input
                            className="bio-link-input"
                            type="text"
                            placeholder="https://..."
                            value={link.url}
                            onChange={(e) => updateLink(i, 'url', e.target.value)}
                        />
                        <button
                            className="bio-toolbar-btn bio-toolbar-btn--cancel"
                            onClick={() => removeLink(i)}
                            style={{ padding: '4px 8px' } as React.CSSProperties}
                        >
                            ✕
                        </button>
                    </div>
                ))}
                <button
                    className="bio-toolbar-btn"
                    onClick={addLink}
                    style={{ marginTop: 4, padding: '4px 10px', fontSize: '0.8rem' } as React.CSSProperties}
                >
                    <BsPlusLg size={10} /> Add Link
                </button>
            </div>
        );
    }

    function renderCustom(section: ProfileSection) {
        if (!section.content) {
            return (
                <p className="bio-section-placeholder">
                    {editing ? 'Click ✏️ to add content...' : 'Empty section.'}
                </p>
            );
        }
        return (
            <div>
                {section.content.split('\n').map((line, i) => (
                    <p key={i}>{line || '\u00A0'}</p>
                ))}
            </div>
        );
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Render
    // ═══════════════════════════════════════════════════════════════════════

    if (!loaded) {
        return <div className="bio-section-placeholder" style={{ padding: 20 } as React.CSSProperties}>Loading profile...</div>;
    }

    // Determine which types are already present (for add menu filtering)
    const presentTypes = new Set(editing ? editSections.map(s => s.type) : sections.map(s => s.type));

    const addableTypes: Array<{ type: ProfileSection['type']; label: string; emoji: string }> = [
        { type: 'about', label: 'About Me', emoji: '📝' },
        { type: 'connections', label: 'Connections', emoji: '👥' },
        { type: 'now-playing', label: 'Now Playing', emoji: '🎵' },
        { type: 'guestbook', label: 'Guestbook', emoji: '💬' },
        { type: 'links', label: 'Links', emoji: '🔗' },
        { type: 'custom', label: 'Custom Section', emoji: '✨' },
    ];

    // Filter out already-present non-custom types (custom can be added multiple times)
    const availableTypes = addableTypes.filter(
        t => t.type === 'custom' || !presentTypes.has(t.type)
    );

    return (
        <div className="bio-sections-root">
            {/* Toolbar — hidden when parent controls edit mode (externally toggled) */}
            <div className="bio-toolbar" style={externalEditMode !== undefined && !editing ? { display: 'none' } : undefined}>
                {!editing ? (
                    <button className="bio-toolbar-btn" onClick={startEditing}>
                        <BsPencilFill size={12} /> Edit Bio
                    </button>
                ) : (
                    <>
                        <button className="bio-toolbar-btn bio-toolbar-btn--save" onClick={saveEdits}>
                            <BsCheckLg size={12} /> Save
                        </button>
                        <button className="bio-toolbar-btn bio-toolbar-btn--cancel" onClick={cancelEdits}>
                            <BsXLg size={10} /> Cancel
                        </button>
                        <div className="bio-add-menu" ref={addMenuRef}>
                            <button
                                className="bio-toolbar-btn"
                                onClick={() => setShowAddMenu(!showAddMenu)}
                            >
                                <BsPlusLg size={10} /> Add Section
                            </button>
                            {showAddMenu && (
                                <div className="bio-add-dropdown">
                                    {availableTypes.map((t) => (
                                        <button
                                            key={t.type + t.label}
                                            onClick={() => addSection(t.type, t.type === 'custom' ? 'My Section' : t.label)}
                                        >
                                            {t.emoji} {t.label}
                                        </button>
                                    ))}
                                </div>
                            )}
                        </div>
                    </>
                )}
                <span className="bio-toolbar-spacer" />
                {saveStatus && <span className="bio-save-status">{saveStatus}</span>}
            </div>

            {/* Sections — 3-column grid layout */}
            {hasAnyColumn ? (
                <div className="bio-sections-columns">
                    <div className="bio-column bio-column--left">
                        {leftSections.map(s => renderSectionCard(s, leftSections))}
                    </div>
                    <div className="bio-column bio-column--center">
                        {centerSections.map(s => renderSectionCard(s, centerSections))}
                    </div>
                    <div className="bio-column bio-column--right">
                        {rightSections.map(s => renderSectionCard(s, rightSections))}
                    </div>
                </div>
            ) : null}

            {/* Full-width custom sections below the grid */}
            {fullSections.length > 0 && (
                <div className="bio-sections-full">
                    {fullSections.map(s => renderSectionCard(s, fullSections))}
                </div>
            )}

            {visibleSections.length === 0 && (
                <p className="bio-section-placeholder" style={{ textAlign: 'center', padding: 20 } as React.CSSProperties}>
                    No sections yet. {editing ? 'Click "Add Section" to get started!' : 'Click "Edit Bio" to add sections.'}
                </p>
            )}
        </div>
    );
}
