import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Editor } from 'grapesjs';
import { Win98Box } from '../components/Win98Box';
import { RawHtmlEditor } from '../components/RawHtmlEditor';
import { useNavStore } from '../store';
import { BsPalette2, BsEye, BsPencilSquare, BsSave, BsTrash, BsArrowLeft, BsCodeSlash } from 'react-icons/bs';
import type { ProfileLayout } from '../types';

/**
 * ProfileBuilderScreen — GrapesJS-powered drag-and-drop profile page builder.
 *
 * Users can design their own MySpace/GeoCities-style profile page by dragging
 * Bytescape-specific blocks (About Me, Post Feed, Media Grid, Friends List,
 * Music Player, Guestbook, Away Message) onto a visual canvas.
 *
 * The output HTML + CSS is saved via Tauri commands to redb and rendered on
 * the ProfileScreen in a sandboxed iframe.
 */
export function ProfileBuilderScreen() {
    const editorRef = useRef<Editor | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const [isPreview, setIsPreview] = useState(false);
    const [saving, setSaving] = useState(false);
    const [saveStatus, setSaveStatus] = useState<string | null>(null);
    const [loaded, setLoaded] = useState(false);
    const [hasLayout, setHasLayout] = useState(false);
    const [rawMode, setRawMode] = useState(false);
    const { setScreen, setActiveSidebar } = useNavStore();

    // Check if existing layout is raw mode on mount
    useEffect(() => {
        invoke<ProfileLayout | null>('get_profile_layout').then((layout) => {
            if (layout && layout.render_mode === 'raw') {
                setRawMode(true);
            }
        }).catch(() => {});
    }, []);

    // Initialize GrapesJS editor
    useEffect(() => {
        let editor: Editor | null = null;
        let cancelled = false;

        async function initEditor() {
            // Dynamic imports — GrapesJS is a large library
            const grapesjs = await import('grapesjs');
            await import('grapesjs/dist/css/grapes.min.css');

            if (cancelled || !containerRef.current) return;

            editor = grapesjs.default.init({
                container: containerRef.current,
                fromElement: false,
                width: 'auto',
                height: '100%',
                storageManager: false, // We handle storage via Tauri
                plugins: [],
                canvas: {
                    styles: [
                        // Inject the current theme colors into the canvas
                    ],
                },
                // Default panel config — we override below
                panels: { defaults: [] },
                deviceManager: {
                    devices: [
                        { name: 'Desktop', width: '' },
                        { name: 'Tablet', width: '768px', widthMedia: '992px' },
                        { name: 'Mobile', width: '375px', widthMedia: '480px' },
                    ],
                },
                styleManager: {
                    sectors: [
                        {
                            name: 'Layout',
                            open: true,
                            properties: [
                                'display', 'flex-direction', 'justify-content',
                                'align-items', 'gap', 'flex-wrap',
                            ],
                        },
                        {
                            name: 'Spacing',
                            open: false,
                            properties: ['padding', 'margin'],
                        },
                        {
                            name: 'Size',
                            open: false,
                            properties: ['width', 'max-width', 'min-height', 'height'],
                        },
                        {
                            name: 'Typography',
                            open: false,
                            properties: [
                                'font-family', 'font-size', 'font-weight',
                                'color', 'text-align', 'line-height',
                                'letter-spacing', 'text-decoration',
                            ],
                        },
                        {
                            name: 'Background',
                            open: false,
                            properties: [
                                'background-color', 'background-image',
                                'background-size', 'background-position',
                            ],
                        },
                        {
                            name: 'Borders',
                            open: false,
                            properties: [
                                'border', 'border-radius',
                                'box-shadow',
                            ],
                        },
                    ],
                },
            });

            // ─── Register Bytescape-specific blocks ───
            const bm = editor.Blocks;

            bm.add('about-me', {
                label: '📝 About Me',
                category: 'Bytescape',
                content: `
                    <div class="bs-about" style="background: #1a1a2e; border: 2px solid #7c3aed; border-radius: 4px; padding: 16px; margin: 8px 0;">
                        <h2 style="color: #a78bfa; font-family: 'Comic Sans MS', cursive, sans-serif; margin: 0 0 8px 0; font-size: 18px;">✨ About Me ✨</h2>
                        <p style="color: #e2e8f0; font-size: 14px; line-height: 1.6;">
                            Welcome to my corner of the internet! I'm keeping my digital memories safe from corporate surveillance. All data stored locally. 🛡️
                        </p>
                    </div>
                `,
                attributes: { class: 'fa fa-user' },
            });

            bm.add('post-feed', {
                label: '📰 Post Feed',
                category: 'Bytescape',
                content: `
                    <div class="bs-feed-widget" style="background: #16213e; border: 2px solid #3b82f6; border-radius: 4px; padding: 16px; margin: 8px 0;">
                        <h3 style="color: #93c5fd; font-family: 'Comic Sans MS', cursive, sans-serif; margin: 0 0 12px 0; font-size: 16px;">📰 My Recent Posts</h3>
                        <div style="border-top: 1px solid #334155; padding-top: 8px;">
                            <div style="padding: 8px 0; border-bottom: 1px solid #1e293b;">
                                <span style="color: #94a3b8; font-size: 12px;">📅 Today</span>
                                <p style="color: #e2e8f0; font-size: 13px; margin: 4px 0 0 0;">This feed will show your recent vault posts when rendered.</p>
                            </div>
                            <div style="padding: 8px 0; border-bottom: 1px solid #1e293b;">
                                <span style="color: #94a3b8; font-size: 12px;">📅 Yesterday</span>
                                <p style="color: #e2e8f0; font-size: 13px; margin: 4px 0 0 0;">Another post from your digital vault...</p>
                            </div>
                        </div>
                    </div>
                `,
                attributes: { class: 'fa fa-list' },
            });

            bm.add('media-grid', {
                label: '🖼️ Media Grid',
                category: 'Bytescape',
                content: `
                    <div class="bs-media-grid" style="background: #1a1a2e; border: 2px solid #ec4899; border-radius: 4px; padding: 16px; margin: 8px 0;">
                        <h3 style="color: #f9a8d4; font-family: 'Comic Sans MS', cursive, sans-serif; margin: 0 0 12px 0; font-size: 16px;">🖼️ My Gallery</h3>
                        <div style="display: grid; grid-template-columns: repeat(3, 1fr); gap: 4px;">
                            <div style="aspect-ratio: 1; background: #2d1b4e; border-radius: 2px; display: flex; align-items: center; justify-content: center; color: #a78bfa;">📷</div>
                            <div style="aspect-ratio: 1; background: #2d1b4e; border-radius: 2px; display: flex; align-items: center; justify-content: center; color: #a78bfa;">📷</div>
                            <div style="aspect-ratio: 1; background: #2d1b4e; border-radius: 2px; display: flex; align-items: center; justify-content: center; color: #a78bfa;">📷</div>
                            <div style="aspect-ratio: 1; background: #2d1b4e; border-radius: 2px; display: flex; align-items: center; justify-content: center; color: #a78bfa;">📷</div>
                            <div style="aspect-ratio: 1; background: #2d1b4e; border-radius: 2px; display: flex; align-items: center; justify-content: center; color: #a78bfa;">📷</div>
                            <div style="aspect-ratio: 1; background: #2d1b4e; border-radius: 2px; display: flex; align-items: center; justify-content: center; color: #a78bfa;">📷</div>
                        </div>
                    </div>
                `,
                attributes: { class: 'fa fa-th' },
            });

            bm.add('friends-list', {
                label: '👥 Friends List',
                category: 'Bytescape',
                content: `
                    <div class="bs-friends" style="background: #16213e; border: 2px solid #10b981; border-radius: 4px; padding: 16px; margin: 8px 0;">
                        <h3 style="color: #6ee7b7; font-family: 'Comic Sans MS', cursive, sans-serif; margin: 0 0 12px 0; font-size: 16px;">👥 My Friends</h3>
                        <div style="display: flex; flex-wrap: wrap; gap: 8px;">
                            <div style="width: 48px; height: 48px; border-radius: 50%; background: #2d1b4e; border: 2px solid #7c3aed; display: flex; align-items: center; justify-content: center; font-size: 20px;">👤</div>
                            <div style="width: 48px; height: 48px; border-radius: 50%; background: #2d1b4e; border: 2px solid #7c3aed; display: flex; align-items: center; justify-content: center; font-size: 20px;">👤</div>
                            <div style="width: 48px; height: 48px; border-radius: 50%; background: #2d1b4e; border: 2px solid #7c3aed; display: flex; align-items: center; justify-content: center; font-size: 20px;">👤</div>
                            <div style="width: 48px; height: 48px; border-radius: 50%; background: #2d1b4e; border: 2px solid #7c3aed; display: flex; align-items: center; justify-content: center; font-size: 20px;">➕</div>
                        </div>
                    </div>
                `,
                attributes: { class: 'fa fa-users' },
            });

            bm.add('music-player', {
                label: '🎵 Music Player',
                category: 'Bytescape',
                content: `
                    <div class="bs-music" style="background: #1a1a2e; border: 2px solid #f59e0b; border-radius: 4px; padding: 16px; margin: 8px 0;">
                        <h3 style="color: #fcd34d; font-family: 'Comic Sans MS', cursive, sans-serif; margin: 0 0 8px 0; font-size: 16px;">🎵 Now Playing</h3>
                        <div style="display: flex; align-items: center; gap: 12px;">
                            <div style="width: 48px; height: 48px; background: #2d1b4e; border-radius: 4px; display: flex; align-items: center; justify-content: center; font-size: 24px;">💿</div>
                            <div>
                                <div style="color: #e2e8f0; font-size: 14px; font-weight: bold;">Song Title</div>
                                <div style="color: #94a3b8; font-size: 12px;">Artist Name</div>
                            </div>
                        </div>
                        <div style="margin-top: 12px; display: flex; gap: 8px; justify-content: center;">
                            <span style="color: #94a3b8; cursor: pointer; font-size: 18px;">⏮</span>
                            <span style="color: #fcd34d; cursor: pointer; font-size: 18px;">▶️</span>
                            <span style="color: #94a3b8; cursor: pointer; font-size: 18px;">⏭</span>
                        </div>
                    </div>
                `,
                attributes: { class: 'fa fa-music' },
            });

            bm.add('guestbook', {
                label: '📖 Guestbook',
                category: 'Bytescape',
                content: `
                    <div class="bs-guestbook" style="background: #16213e; border: 2px solid #8b5cf6; border-radius: 4px; padding: 16px; margin: 8px 0;">
                        <h3 style="color: #c4b5fd; font-family: 'Comic Sans MS', cursive, sans-serif; margin: 0 0 12px 0; font-size: 16px;">📖 Sign My Guestbook!</h3>
                        <div style="border: 1px solid #334155; border-radius: 4px; padding: 8px; margin-bottom: 8px;">
                            <div style="color: #a78bfa; font-size: 12px; font-weight: bold;">CoolVisitor2024</div>
                            <div style="color: #e2e8f0; font-size: 13px;">Great page! Love the vibes~ ⭐</div>
                        </div>
                        <div style="border: 1px solid #334155; border-radius: 4px; padding: 8px;">
                            <div style="color: #a78bfa; font-size: 12px; font-weight: bold;">AnonFriend</div>
                            <div style="color: #e2e8f0; font-size: 13px;">You were here! 🌟</div>
                        </div>
                    </div>
                `,
                attributes: { class: 'fa fa-book' },
            });

            bm.add('away-message', {
                label: '💬 Away Message',
                category: 'Bytescape',
                content: `
                    <div class="bs-away" style="background: linear-gradient(135deg, #1a1a2e 0%, #2d1b4e 100%); border: 2px dashed #7c3aed; border-radius: 4px; padding: 16px; margin: 8px 0; text-align: center;">
                        <div style="font-size: 24px; margin-bottom: 8px;">🌙</div>
                        <h3 style="color: #a78bfa; font-family: 'Comic Sans MS', cursive, sans-serif; margin: 0 0 8px 0; font-size: 16px;">Away Message</h3>
                        <p style="color: #e2e8f0; font-size: 14px; font-style: italic;">
                            "Currently offline. Leave me a message in the guestbook! BRB ✌️"
                        </p>
                    </div>
                `,
                attributes: { class: 'fa fa-comment' },
            });

            bm.add('visitor-counter', {
                label: '👀 Visitor Counter',
                category: 'Bytescape',
                content: `
                    <div class="bs-counter" style="background: #0f0f23; border: 2px solid #6366f1; border-radius: 4px; padding: 12px; margin: 8px 0; text-align: center;">
                        <div style="color: #818cf8; font-family: 'Courier New', monospace; font-size: 12px; margin-bottom: 4px;">VISITORS</div>
                        <div style="color: #22d3ee; font-family: 'Courier New', monospace; font-size: 24px; letter-spacing: 4px; text-shadow: 0 0 8px rgba(34, 211, 238, 0.5);">001337</div>
                    </div>
                `,
                attributes: { class: 'fa fa-eye' },
            });

            bm.add('divider', {
                label: '── Divider',
                category: 'Bytescape',
                content: `
                    <hr style="border: none; border-top: 2px solid #7c3aed; margin: 16px 0; opacity: 0.5;" />
                `,
                attributes: { class: 'fa fa-minus' },
            });

            bm.add('marquee-banner', {
                label: '⭐ Marquee Banner',
                category: 'Bytescape',
                content: `
                    <div class="bs-marquee" style="background: linear-gradient(90deg, #7c3aed, #3b82f6, #ec4899, #7c3aed); padding: 8px; border-radius: 4px; margin: 8px 0; text-align: center; overflow: hidden;">
                        <div style="color: #fff; font-family: 'Comic Sans MS', cursive, sans-serif; font-size: 14px; white-space: nowrap;">
                            ⭐ Welcome to my page! ⭐ Built with Bytescape ⭐ Best viewed on any screen ⭐
                        </div>
                    </div>
                `,
                attributes: { class: 'fa fa-star' },
            });

            bm.add('two-column', {
                label: '▥ Two Columns',
                category: 'Layout',
                content: `
                    <div style="display: flex; gap: 16px; padding: 8px 0;" data-gjs-droppable="true">
                        <div style="flex: 1; min-height: 80px; border: 1px dashed #475569; border-radius: 4px; padding: 8px;" data-gjs-droppable="true">
                            <p style="color: #94a3b8; font-size: 12px; text-align: center;">Drop blocks here</p>
                        </div>
                        <div style="flex: 1; min-height: 80px; border: 1px dashed #475569; border-radius: 4px; padding: 8px;" data-gjs-droppable="true">
                            <p style="color: #94a3b8; font-size: 12px; text-align: center;">Drop blocks here</p>
                        </div>
                    </div>
                `,
                attributes: { class: 'fa fa-columns' },
            });

            bm.add('section', {
                label: '☐ Section',
                category: 'Layout',
                content: `
                    <div style="min-height: 80px; border: 1px dashed #475569; border-radius: 4px; padding: 16px; margin: 8px 0;" data-gjs-droppable="true">
                        <p style="color: #94a3b8; font-size: 12px; text-align: center;">Drop blocks here</p>
                    </div>
                `,
                attributes: { class: 'fa fa-square-o' },
            });

            bm.add('heading', {
                label: 'H Heading',
                category: 'Basic',
                content: `<h2 style="color: #a78bfa; font-family: 'Comic Sans MS', cursive, sans-serif; margin: 8px 0;">Section Title</h2>`,
                attributes: { class: 'fa fa-header' },
            });

            bm.add('text-block', {
                label: '¶ Text',
                category: 'Basic',
                content: `<p style="color: #e2e8f0; font-size: 14px; line-height: 1.6;">Your text here. Tell the world about yourself!</p>`,
                attributes: { class: 'fa fa-paragraph' },
            });

            bm.add('image', {
                label: '🖼 Image',
                category: 'Basic',
                content: { type: 'image' },
                attributes: { class: 'fa fa-picture-o' },
            });

            bm.add('link', {
                label: '🔗 Link',
                category: 'Basic',
                content: `<a href="#" style="color: #93c5fd; text-decoration: underline; font-size: 14px;">My Cool Link</a>`,
                attributes: { class: 'fa fa-link' },
            });

            // Inject canvas styles that match the Bytescape theme
            const canvasDoc = editor.Canvas.getDocument();
            if (canvasDoc) {
                const style = canvasDoc.createElement('style');
                style.innerHTML = `
                    body {
                        background: #0f0f23;
                        color: #e2e8f0;
                        font-family: 'Segoe UI', 'Helvetica Neue', Arial, sans-serif;
                        margin: 0;
                        padding: 16px;
                        min-height: 100vh;
                    }
                    * { box-sizing: border-box; }
                `;
                canvasDoc.head.appendChild(style);
            }

            // Load existing layout if available
            try {
                const layout = await invoke<ProfileLayout | null>('get_profile_layout');
                if (layout) {
                    if (layout.components_json) {
                        editor.loadData({
                            components: JSON.parse(layout.components_json),
                            styles: layout.styles_json ? JSON.parse(layout.styles_json) : [],
                        });
                    } else {
                        editor.setComponents(layout.html);
                        editor.setStyle(layout.css);
                    }
                    setHasLayout(true);
                }
            } catch (e) {
                console.warn('Failed to load profile layout:', e);
            }

            editorRef.current = editor;
            setLoaded(true);
        }

        initEditor();

        return () => {
            cancelled = true;
            if (editor) {
                editor.destroy();
                editorRef.current = null;
            }
        };
    }, []);

    // Save layout to redb via Tauri
    const handleSave = useCallback(async () => {
        const editor = editorRef.current;
        if (!editor) return;

        setSaving(true);
        setSaveStatus(null);

        try {
            const html = editor.getHtml() || '';
            const css = editor.getCss() || '';
            const data = editor.storeData();
            const componentsJson = JSON.stringify(data.components || []);
            const stylesJson = JSON.stringify(data.styles || []);

            await invoke('save_profile_layout', {
                html,
                css,
                componentsJson,
                stylesJson,
                widgetData: null,
                renderMode: null,
                js: null,
            });

            setHasLayout(true);
            setSaveStatus('✅ Layout saved!');
            setTimeout(() => setSaveStatus(null), 3000);
        } catch (e) {
            console.error('Failed to save profile layout:', e);
            setSaveStatus(`❌ Save failed: ${e}`);
        } finally {
            setSaving(false);
        }
    }, []);

    // Delete layout
    const handleDelete = useCallback(async () => {
        try {
            await invoke('delete_profile_layout');
            setHasLayout(false);
            const editor = editorRef.current;
            if (editor) {
                editor.setComponents('');
                editor.setStyle('');
            }
            setSaveStatus('🗑️ Layout deleted. Profile reverted to default.');
            setTimeout(() => setSaveStatus(null), 3000);
        } catch (e) {
            console.error('Failed to delete profile layout:', e);
            setSaveStatus(`❌ Delete failed: ${e}`);
        }
    }, []);

    // Toggle preview mode
    const handlePreviewToggle = useCallback(() => {
        const editor = editorRef.current;
        if (!editor) return;

        if (isPreview) {
            editor.stopCommand('preview');
        } else {
            editor.runCommand('preview');
        }
        setIsPreview(!isPreview);
    }, [isPreview]);

    // Navigate back to profile
    const handleBack = useCallback(() => {
        setScreen('profile');
        setActiveSidebar('profile');
    }, [setScreen, setActiveSidebar]);

    const toolbarStyle: React.CSSProperties = {
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '8px 12px',
        borderBottom: '2px solid var(--border-color, #2d2d4e)',
        background: 'var(--bg-secondary, #1a1a2e)',
        flexWrap: 'wrap',
    };

    const btnStyle: React.CSSProperties = {
        display: 'flex',
        alignItems: 'center',
        gap: 6,
        padding: '6px 14px',
        border: '1px solid var(--border-color, #2d2d4e)',
        borderRadius: 'var(--border-radius, 4px)',
        background: 'var(--bg-tertiary, #16213e)',
        color: 'var(--text-primary, #e2e8f0)',
        cursor: 'pointer',
        fontSize: 13,
        fontWeight: 500,
    };

    const primaryBtnStyle: React.CSSProperties = {
        ...btnStyle,
        background: 'var(--accent-purple, #7c3aed)',
        borderColor: 'var(--accent-purple, #7c3aed)',
        color: '#fff',
    };

    const dangerBtnStyle: React.CSSProperties = {
        ...btnStyle,
        background: 'transparent',
        borderColor: '#ef4444',
        color: '#ef4444',
    };

    return (
        <div className="profile-builder-screen" style={{
            display: 'flex',
            flexDirection: 'column',
            height: '100%',
            background: 'var(--bg-primary, #0f0f23)',
        } as React.CSSProperties}>
            {/* If raw mode, render the Raw HTML Editor instead of GrapesJS */}
            {rawMode ? (
                <RawHtmlEditor onBack={() => {
                    setRawMode(false);
                    // If user switches back from raw mode, they may want GrapesJS
                    // Don't auto-navigate — let them choose
                }} />
            ) : (
            <>
            {/* Mobile warning — shown only on small screens via CSS */}
            <div className="profile-builder-mobile-warning">
                <Win98Box variant="raised">
                    <div style={{
                        padding: '24px 16px',
                        textAlign: 'center',
                    } as React.CSSProperties}>
                        <div style={{ fontSize: 32, marginBottom: 12 }}>🖥️</div>
                        <h3 style={{ color: 'var(--accent-yellow)', margin: '0 0 8px 0' } as React.CSSProperties}>
                            Desktop Recommended
                        </h3>
                        <p style={{ color: 'var(--text-secondary)', fontSize: 13, lineHeight: 1.5, margin: '0 0 16px 0' } as React.CSSProperties}>
                            The Profile Builder uses a drag-and-drop canvas that works best on larger screens.
                            For the best experience, switch to a desktop or tablet in landscape mode.
                        </p>
                        <button
                            onClick={() => { setScreen('profile'); setActiveSidebar('profile'); }}
                            style={{
                                ...btnStyle,
                                background: 'var(--accent-purple, #7c3aed)',
                                borderColor: 'var(--accent-purple, #7c3aed)',
                                color: '#fff',
                            } as React.CSSProperties}
                        >
                            <BsArrowLeft /> Back to Profile
                        </button>
                    </div>
                </Win98Box>
            </div>

            {/* ═══ Toolbar ═══ */}
            <Win98Box variant="raised" style={{ borderRadius: 0 } as React.CSSProperties}>
                <div style={toolbarStyle}>
                    <button
                        style={btnStyle}
                        onClick={handleBack}
                        title="Back to Profile"
                    >
                        <BsArrowLeft /> Back
                    </button>

                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, flex: 1 } as React.CSSProperties}>
                        <BsPalette2 size={20} style={{ color: 'var(--accent-purple, #7c3aed)' }} />
                        <span style={{
                            color: 'var(--text-primary, #e2e8f0)',
                            fontWeight: 700,
                            fontSize: 15,
                            fontFamily: "'Comic Sans MS', cursive, sans-serif",
                        }}>
                            ✨ Profile Builder ✨
                        </span>
                    </div>

                    <button
                        style={btnStyle}
                        onClick={handlePreviewToggle}
                        title={isPreview ? 'Edit Mode' : 'Preview'}
                    >
                        {isPreview ? <><BsPencilSquare /> Edit</> : <><BsEye /> Preview</>}
                    </button>

                    <button
                        style={primaryBtnStyle}
                        onClick={handleSave}
                        disabled={saving}
                        title="Save Layout"
                    >
                        <BsSave /> {saving ? 'Saving...' : 'Save'}
                    </button>

                    {hasLayout && (
                        <button
                            style={dangerBtnStyle}
                            onClick={handleDelete}
                            title="Delete Layout & Revert to Default"
                        >
                            <BsTrash /> Reset
                        </button>
                    )}

                    <button
                        style={btnStyle}
                        onClick={() => setRawMode(true)}
                        title="Switch to Raw HTML Editor"
                    >
                        <BsCodeSlash /> Raw HTML
                    </button>

                    {saveStatus && (
                        <span style={{
                            color: saveStatus.startsWith('✅') || saveStatus.startsWith('🗑️')
                                ? '#6ee7b7'
                                : '#f87171',
                            fontSize: 13,
                            marginLeft: 8,
                        }}>
                            {saveStatus}
                        </span>
                    )}
                </div>
            </Win98Box>

            {/* ═══ Loading State ═══ */}
            {!loaded && (
                <div style={{
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    flex: 1,
                    color: 'var(--text-muted, #94a3b8)',
                    fontSize: 16,
                } as React.CSSProperties}>
                    <div style={{ textAlign: 'center' } as React.CSSProperties}>
                        <div style={{ fontSize: 32, marginBottom: 12 }}>🎨</div>
                        <p>Loading Profile Builder...</p>
                    </div>
                </div>
            )}

            {/* ═══ GrapesJS Editor Container ═══ */}
            <div
                ref={containerRef}
                className="profile-builder-editor"
                style={{
                    flex: 1,
                    overflow: 'hidden',
                    display: loaded ? 'block' : 'none',
                } as React.CSSProperties}
            />
            </>
            )}
        </div>
    );
}
