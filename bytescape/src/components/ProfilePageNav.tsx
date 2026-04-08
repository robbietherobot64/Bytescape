/**
 * PURPOSE
 * Profile page navigation bar — shows tabs for each user-created profile page.
 * Includes an "Add Page" button when in edit mode.
 *
 * RELATED FILES
 * - src/hooks/useProfilePages.ts (page CRUD)
 * - src/screens/ProfileScreen.tsx (consumer)
 * - src/types/index.ts (ProfilePage)
 */

import { useState } from 'react';
import { ProfilePage } from '../types';
import { BsPlusLg, BsTrash } from 'react-icons/bs';

interface ProfilePageNavProps {
    pages: ProfilePage[];
    activePage: string | null;
    editMode: boolean;
    onSelectPage: (pageId: string | null) => void;
    onCreatePage?: (title: string, contentType: ProfilePage['content_type']) => void;
    onDeletePage?: (pageId: string) => void;
}

export function ProfilePageNav({ pages, activePage, editMode, onSelectPage, onCreatePage, onDeletePage }: ProfilePageNavProps) {
    const [showNewForm, setShowNewForm] = useState(false);
    const [newTitle, setNewTitle] = useState('');
    const [newType, setNewType] = useState<ProfilePage['content_type']>('rich-text');

    const visiblePages = editMode ? pages : pages.filter(p => p.visible);

    if (visiblePages.length === 0 && !editMode) return null;

    const handleCreate = () => {
        if (!newTitle.trim()) return;
        onCreatePage?.(newTitle.trim(), newType);
        setNewTitle('');
        setNewType('rich-text');
        setShowNewForm(false);
    };

    return (
        <div className="profile-page-nav">
            {/* Home tab (main profile) */}
            <button
                className={`profile-page-tab ${activePage === null ? 'active' : ''}`}
                onClick={() => onSelectPage(null)}
            >
                🏠 Home
            </button>

            {/* Page tabs */}
            {visiblePages.map(page => (
                <div key={page.id} className="profile-page-tab-wrap">
                    <button
                        className={`profile-page-tab ${activePage === page.id ? 'active' : ''}`}
                        onClick={() => onSelectPage(page.id)}
                    >
                        {page.icon || '📄'} {page.title}
                    </button>
                    {editMode && onDeletePage && (
                        <button
                            className="profile-page-tab-delete"
                            onClick={(e) => { e.stopPropagation(); onDeletePage(page.id); }}
                            title={`Delete "${page.title}"`}
                        >
                            <BsTrash size={10} />
                        </button>
                    )}
                </div>
            ))}

            {/* Add page button */}
            {editMode && (
                <>
                    <button
                        className="profile-page-tab profile-page-tab--add"
                        onClick={() => setShowNewForm(!showNewForm)}
                    >
                        <BsPlusLg size={10} /> Add Page
                    </button>
                    {showNewForm && (
                        <div className="profile-page-new-form">
                            <input
                                type="text"
                                placeholder="Page title..."
                                value={newTitle}
                                onChange={(e) => setNewTitle(e.target.value)}
                                onKeyDown={(e) => { if (e.key === 'Enter') handleCreate(); }}
                                className="profile-page-new-input"
                                autoFocus
                            />
                            <select
                                value={newType}
                                onChange={(e) => setNewType(e.target.value as ProfilePage['content_type'])}
                                className="profile-page-new-select"
                            >
                                <option value="rich-text">Rich Text</option>
                                <option value="blog">Blog</option>
                                <option value="gallery">Gallery</option>
                            </select>
                            <button className="profile-page-new-btn" onClick={handleCreate}>
                                Create
                            </button>
                        </div>
                    )}
                </>
            )}
        </div>
    );
}
