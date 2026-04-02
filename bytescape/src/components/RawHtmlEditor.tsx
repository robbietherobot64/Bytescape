/**
 * PURPOSE
 * - Provide a power-user editor for raw profile markup (`HTML/CSS/JS`) in one place.
 * - Persist profile layout in `raw` render mode using existing profile layout commands.
 *
 * HOW IT WORKS
 * - Loads existing layout via `get_profile_layout` and hydrates local editor state.
 * - Saves/deletes layouts through Tauri `invoke()` profile commands.
 * - Builds a `srcDoc` preview document and renders it in a sandboxed iframe.
 * - Supports importing `.html` files and extracting inline `<style>`/`<script>` blocks.
 *
 * RELATED FILES
 * - `src/types/index.ts` (`ProfileLayout` shape)
 * - `src/screens/ProfileScreen.tsx` / profile builder flow (entrypoint and navigation)
 * - `src/components/Win98Box.tsx` (preview shell container)
 * - `src-tauri/src/lib.rs` (profile layout command handlers)
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { Win98Box } from '../components/Win98Box';
import { ProfileLayout } from '../types';
import { BsSave, BsEye, BsUpload, BsTrash, BsCodeSlash } from 'react-icons/bs';

// ═══════════════════════════════════════════════════════════════════════════════
// Default editor templates
// ═══════════════════════════════════════════════════════════════════════════════

const DEFAULT_HTML = `<div class="profile-page">
  <h1>Welcome to my page!</h1>
  <p>Edit this HTML to create your custom profile.</p>
  <div class="about-section">
    <h2>About Me</h2>
    <p>Write something about yourself here.</p>
  </div>
</div>`;

const DEFAULT_CSS = `/* Your custom styles */
.profile-page {
  max-width: 800px;
  margin: 0 auto;
  padding: 20px;
  font-family: 'Comic Sans MS', 'Segoe UI', sans-serif;
}

h1 {
  color: #ff69b4;
  text-shadow: 2px 2px 4px rgba(0, 0, 0, 0.5);
  text-align: center;
}

.about-section {
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid #444;
  border-radius: 8px;
  padding: 16px;
  margin-top: 16px;
}`;

const DEFAULT_JS = `// Your custom JavaScript (runs in sandbox)
// Note: No access to parent page or Tauri APIs
console.log('Profile page loaded!');`;

interface RawHtmlEditorProps {
    /** Return to the profile builder surface. */
    onBack: () => void;
}

/**
 * Raw HTML editor component.
 *
 * Cross-system notes:
 * - Backend interaction: profile layouts are read/written through Tauri `invoke()`.
 * - UI composition: preview is wrapped in `Win98Box` for consistent desktop shell chrome.
 * - Type coupling: `ProfileLayout` must remain aligned with Rust serialization contracts.
 */
export function RawHtmlEditor({ onBack }: RawHtmlEditorProps) {
    // ═══════════════════════════════════════════════════════════════════════════
    // Local state and refs
    // ═══════════════════════════════════════════════════════════════════════════

    const [html, setHtml] = useState(DEFAULT_HTML);
    const [css, setCss] = useState(DEFAULT_CSS);
    const [js, setJs] = useState(DEFAULT_JS);
    const [activeTab, setActiveTab] = useState<'html' | 'css' | 'js'>('html');
    const [showPreview, setShowPreview] = useState(false);
    const [saving, setSaving] = useState(false);
    const [saveStatus, setSaveStatus] = useState<string | null>(null);
    const [hasExisting, setHasExisting] = useState(false);
    const iframeRef = useRef<HTMLIFrameElement>(null);

    // ═══════════════════════════════════════════════════════════════════════════
    // Lifecycle: load persisted raw layout
    // ═══════════════════════════════════════════════════════════════════════════

    // Load existing raw layout on mount
    useEffect(() => {
        invoke<ProfileLayout | null>('get_profile_layout').then((layout) => {
            if (layout && layout.render_mode === 'raw') {
                setHtml(layout.html || DEFAULT_HTML);
                setCss(layout.css || DEFAULT_CSS);
                setJs(layout.js || DEFAULT_JS);
                setHasExisting(true);
            }
        }).catch(() => {});
    }, []);

    // ═══════════════════════════════════════════════════════════════════════════
    // Command handlers (save/delete/upload)
    // ═══════════════════════════════════════════════════════════════════════════

    const handleSave = useCallback(async () => {
        setSaving(true);
        setSaveStatus(null);
        try {
            await invoke('save_profile_layout', {
                html,
                css,
                componentsJson: null,
                stylesJson: null,
                widgetData: null,
                renderMode: 'raw',
                js,
            });
            setHasExisting(true);
            setSaveStatus('✅ Raw layout saved!');
            setTimeout(() => setSaveStatus(null), 3000);
        } catch (e) {
            setSaveStatus(`❌ Save failed: ${e}`);
        } finally {
            setSaving(false);
        }
    }, [html, css, js]);

    const handleDelete = useCallback(async () => {
        if (!confirm('Delete your raw HTML layout? This cannot be undone.')) return;
        try {
            await invoke('delete_profile_layout');
            setHtml(DEFAULT_HTML);
            setCss(DEFAULT_CSS);
            setJs(DEFAULT_JS);
            setHasExisting(false);
            setSaveStatus('🗑️ Layout deleted');
            setTimeout(() => setSaveStatus(null), 3000);
        } catch (e) {
            setSaveStatus(`❌ Delete failed: ${e}`);
        }
    }, []);

    const handleUpload = useCallback(async () => {
        try {
            const file = await open({
                filters: [{ name: 'HTML', extensions: ['html', 'htm'] }],
                multiple: false,
            });
            if (!file) return;
            // Read via Tauri fs
            const response = await fetch(`asset://localhost/${file}`);
            const text = await response.text();

            // Parse uploaded HTML to extract style/script
            const parser = new DOMParser();
            const doc = parser.parseFromString(text, 'text/html');

            // Extract styles
            const styles = Array.from(doc.querySelectorAll('style'))
                .map(s => s.textContent || '')
                .join('\n');
            if (styles) setCss(styles);

            // Extract scripts
            const scripts = Array.from(doc.querySelectorAll('script'))
                .map(s => s.textContent || '')
                .join('\n');
            if (scripts) setJs(scripts);

            // Remove style/script from body and use remaining HTML
            doc.querySelectorAll('style, script').forEach(el => el.remove());
            const bodyHtml = doc.body.innerHTML.trim();
            if (bodyHtml) setHtml(bodyHtml);

            setSaveStatus('📄 HTML file imported — review and save');
            setTimeout(() => setSaveStatus(null), 5000);
        } catch (e) {
            console.error('Upload failed:', e);
            setSaveStatus(`❌ Upload failed: ${e}`);
        }
    }, []);

    // ═══════════════════════════════════════════════════════════════════════════
    // Preview/document assembly helpers
    // ═══════════════════════════════════════════════════════════════════════════

    const buildPreviewSrcDoc = () => `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
body {
    background: #0f0f23;
    color: #e2e8f0;
    font-family: 'Segoe UI', 'Helvetica Neue', Arial, sans-serif;
    margin: 0;
    padding: 16px;
}
* { box-sizing: border-box; }
${css}
</style>
</head>
<body>
${html}
<script>${js}<\/script>
</body>
</html>`;

    // ═══════════════════════════════════════════════════════════════════════════
    // Inline style objects
    // ═══════════════════════════════════════════════════════════════════════════

    const textareaStyle: React.CSSProperties = {
        width: '100%',
        height: 'calc(100% - 40px)',
        background: '#1a1a2e',
        color: '#e0e0e0',
        border: '1px solid var(--border-color)',
        borderRadius: 4,
        fontFamily: "'Fira Code', 'Cascadia Code', 'Consolas', monospace",
        fontSize: 13,
        lineHeight: 1.6,
        padding: 12,
        resize: 'none',
        outline: 'none',
        tabSize: 2,
    };

    const tabBtnStyle = (active: boolean): React.CSSProperties => ({
        padding: '6px 16px',
        background: active ? 'var(--accent-purple)' : 'var(--bg-tertiary)',
        color: active ? '#fff' : 'var(--text-secondary)',
        border: 'none',
        borderRadius: '4px 4px 0 0',
        cursor: 'pointer',
        fontSize: 13,
        fontWeight: active ? 600 : 400,
        fontFamily: 'monospace',
    });

    // ═══════════════════════════════════════════════════════════════════════════
    // Render tree
    // ═══════════════════════════════════════════════════════════════════════════

    return (
        <div style={{
            display: 'flex',
            flexDirection: 'column',
            height: '100%',
            overflow: 'hidden',
        } as React.CSSProperties}>
            {/* Toolbar */}
            <div style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                padding: '8px 12px',
                background: 'var(--bg-secondary)',
                borderBottom: '1px solid var(--border-color)',
                flexShrink: 0,
            } as React.CSSProperties}>
                <button
                    onClick={onBack}
                    style={{
                        background: 'none',
                        border: 'none',
                        color: 'var(--text-secondary)',
                        cursor: 'pointer',
                        fontSize: 13,
                        display: 'flex',
                        alignItems: 'center',
                        gap: 4,
                    } as React.CSSProperties}
                >
                    ← Back to Builder
                </button>
                <div style={{ flex: 1 }} />
                <span style={{ color: 'var(--text-muted)', fontSize: 12 } as React.CSSProperties}>
                    <BsCodeSlash /> Raw HTML Mode
                </span>
                <div style={{ flex: 1 }} />
                {saveStatus && (
                    <span style={{ fontSize: 12, color: 'var(--text-secondary)' } as React.CSSProperties}>
                        {saveStatus}
                    </span>
                )}
                <button
                    onClick={handleUpload}
                    style={{
                        background: 'var(--bg-tertiary)',
                        border: '1px solid var(--border-color)',
                        borderRadius: 4,
                        padding: '4px 10px',
                        color: 'var(--text-primary)',
                        cursor: 'pointer',
                        fontSize: 12,
                        display: 'flex',
                        alignItems: 'center',
                        gap: 4,
                    } as React.CSSProperties}
                    title="Upload HTML file"
                >
                    <BsUpload /> Upload
                </button>
                <button
                    onClick={() => setShowPreview(!showPreview)}
                    style={{
                        background: showPreview ? 'var(--accent-purple)' : 'var(--bg-tertiary)',
                        border: '1px solid var(--border-color)',
                        borderRadius: 4,
                        padding: '4px 10px',
                        color: showPreview ? '#fff' : 'var(--text-primary)',
                        cursor: 'pointer',
                        fontSize: 12,
                        display: 'flex',
                        alignItems: 'center',
                        gap: 4,
                    } as React.CSSProperties}
                >
                    <BsEye /> Preview
                </button>
                <button
                    onClick={handleSave}
                    disabled={saving}
                    style={{
                        background: 'var(--accent-purple)',
                        border: 'none',
                        borderRadius: 4,
                        padding: '4px 14px',
                        color: '#fff',
                        cursor: 'pointer',
                        fontSize: 12,
                        fontWeight: 600,
                        display: 'flex',
                        alignItems: 'center',
                        gap: 4,
                        opacity: saving ? 0.6 : 1,
                    } as React.CSSProperties}
                >
                    <BsSave /> {saving ? 'Saving...' : 'Save'}
                </button>
                {hasExisting && (
                    <button
                        onClick={handleDelete}
                        style={{
                            background: 'none',
                            border: '1px solid #f44336',
                            borderRadius: 4,
                            padding: '4px 10px',
                            color: '#f44336',
                            cursor: 'pointer',
                            fontSize: 12,
                            display: 'flex',
                            alignItems: 'center',
                            gap: 4,
                        } as React.CSSProperties}
                    >
                        <BsTrash /> Delete
                    </button>
                )}
            </div>

            {/* Main content */}
            <div style={{
                flex: 1,
                display: 'flex',
                overflow: 'hidden',
            } as React.CSSProperties}>
                {/* Code Editor */}
                <div style={{
                    flex: showPreview ? 1 : 1,
                    display: 'flex',
                    flexDirection: 'column',
                    overflow: 'hidden',
                    borderRight: showPreview ? '1px solid var(--border-color)' : 'none',
                } as React.CSSProperties}>
                    {/* Tabs */}
                    <div style={{
                        display: 'flex',
                        gap: 2,
                        padding: '4px 8px 0',
                        background: 'var(--bg-secondary)',
                    } as React.CSSProperties}>
                        <button onClick={() => setActiveTab('html')} style={tabBtnStyle(activeTab === 'html')}>
                            HTML
                        </button>
                        <button onClick={() => setActiveTab('css')} style={tabBtnStyle(activeTab === 'css')}>
                            CSS
                        </button>
                        <button onClick={() => setActiveTab('js')} style={tabBtnStyle(activeTab === 'js')}>
                            JS
                        </button>
                    </div>

                    {/* Editor area */}
                    <div style={{ flex: 1, padding: 8, overflow: 'hidden' } as React.CSSProperties}>
                        {activeTab === 'html' && (
                            <textarea
                                value={html}
                                onChange={e => setHtml(e.target.value)}
                                style={textareaStyle}
                                spellCheck={false}
                                placeholder="<div>Your HTML here...</div>"
                            />
                        )}
                        {activeTab === 'css' && (
                            <textarea
                                value={css}
                                onChange={e => setCss(e.target.value)}
                                style={textareaStyle}
                                spellCheck={false}
                                placeholder="/* Your CSS styles */"
                            />
                        )}
                        {activeTab === 'js' && (
                            <textarea
                                value={js}
                                onChange={e => setJs(e.target.value)}
                                style={textareaStyle}
                                spellCheck={false}
                                placeholder="// Your JavaScript (runs sandboxed)"
                            />
                        )}
                    </div>
                </div>

                {/* Preview iframe */}
                {showPreview && (
                    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', overflow: 'hidden' } as React.CSSProperties}>
                        <div style={{
                            padding: '8px 12px',
                            background: 'var(--bg-secondary)',
                            borderBottom: '1px solid var(--border-color)',
                            fontSize: 12,
                            color: 'var(--text-muted)',
                        } as React.CSSProperties}>
                            Preview (sandboxed)
                        </div>
                        <Win98Box variant="window" title="✨ Live Preview ✨">
                            <iframe
                                ref={iframeRef}
                                sandbox="allow-scripts"
                                title="Raw HTML Preview"
                                srcDoc={buildPreviewSrcDoc()}
                                style={{
                                    width: '100%',
                                    height: '100%',
                                    minHeight: 400,
                                    border: 'none',
                                    background: '#0f0f23',
                                } as React.CSSProperties}
                            />
                        </Win98Box>
                    </div>
                )}
            </div>

            {/* Security notice */}
            <div style={{
                padding: '6px 12px',
                background: 'var(--bg-secondary)',
                borderTop: '1px solid var(--border-color)',
                fontSize: 11,
                color: 'var(--text-muted)',
                textAlign: 'center',
            } as React.CSSProperties}>
                🔒 Raw HTML is rendered in a sandboxed iframe — your scripts cannot access the main app, Tauri APIs, or parent page.
            </div>
        </div>
    );
}
