/**
 * PURPOSE
 * Inline page content editor and renderer for user-created profile pages.
 * Supports rich-text and blog content types.
 *
 * RELATED FILES
 * - src/hooks/useProfilePages.ts (page CRUD)
 * - src/components/ProfilePageNav.tsx (navigation)
 * - src/screens/ProfileScreen.tsx (consumer)
 * - src/types/index.ts (ProfilePage)
 */

import { useState, useEffect } from 'react';
import { ProfilePage } from '../types';
import { Win98Box } from './Win98Box';
import { BsPencilFill, BsCheckLg } from 'react-icons/bs';

interface ProfilePageEditorProps {
    page: ProfilePage;
    editMode: boolean;
    onSave: (page: ProfilePage) => void;
}

export function ProfilePageEditor({ page, editMode, onSave }: ProfilePageEditorProps) {
    const [content, setContent] = useState(page.content);
    const [isEditing, setIsEditing] = useState(false);

    useEffect(() => {
        setContent(page.content);
        setIsEditing(false);
    }, [page.id, page.content]);

    const handleSave = () => {
        onSave({ ...page, content });
        setIsEditing(false);
    };

    if (page.content_type === 'blog') {
        return renderBlogPage();
    }

    // Default: rich-text page
    return (
        <Win98Box title={page.title}>
            <div className="profile-page-content">
                {editMode && !isEditing && (
                    <button
                        className="profile-page-edit-btn"
                        onClick={() => setIsEditing(true)}
                    >
                        <BsPencilFill size={12} /> Edit
                    </button>
                )}
                {isEditing ? (
                    <div className="profile-page-editor">
                        <textarea
                            className="profile-page-textarea"
                            value={content}
                            onChange={(e) => setContent(e.target.value)}
                            rows={12}
                            placeholder="Write your page content here... (Markdown supported)"
                        />
                        <div className="profile-page-editor-actions">
                            <button className="profile-page-save-btn" onClick={handleSave}>
                                <BsCheckLg size={12} /> Save
                            </button>
                            <button
                                className="profile-page-cancel-btn"
                                onClick={() => { setContent(page.content); setIsEditing(false); }}
                            >
                                Cancel
                            </button>
                        </div>
                    </div>
                ) : (
                    <div className="profile-page-rendered">
                        {content ? (
                            <div className="profile-page-text">{content}</div>
                        ) : (
                            <p className="bio-section-placeholder">
                                {editMode ? 'Click Edit to add content to this page.' : 'This page is empty.'}
                            </p>
                        )}
                    </div>
                )}
            </div>
        </Win98Box>
    );

    function renderBlogPage() {
        // Blog pages store entries as JSON array
        let entries: Array<{ title: string; date: string; body: string }> = [];
        try {
            entries = JSON.parse(content || '[]');
        } catch {
            entries = [];
        }

        return (
            <Win98Box title={`📝 ${page.title}`}>
                <div className="profile-page-content">
                    {editMode && (
                        <button
                            className="profile-page-edit-btn"
                            onClick={() => {
                                const title = prompt('Blog entry title:');
                                if (!title) return;
                                const body = prompt('Blog entry content:');
                                if (!body) return;
                                const newEntry = {
                                    title,
                                    date: new Date().toISOString().split('T')[0],
                                    body,
                                };
                                const updated = [newEntry, ...entries];
                                const newContent = JSON.stringify(updated);
                                setContent(newContent);
                                onSave({ ...page, content: newContent });
                            }}
                        >
                            <BsPencilFill size={12} /> New Entry
                        </button>
                    )}
                    {entries.length === 0 ? (
                        <p className="bio-section-placeholder">
                            {editMode ? 'Click "New Entry" to start your blog.' : 'No blog entries yet.'}
                        </p>
                    ) : (
                        <div className="profile-blog-entries">
                            {entries.map((entry, i) => (
                                <div key={i} className="profile-blog-entry">
                                    <h3 className="profile-blog-entry-title">{entry.title}</h3>
                                    <div className="profile-blog-entry-date">{entry.date}</div>
                                    <div className="profile-blog-entry-body">{entry.body}</div>
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </Win98Box>
        );
    }
}
