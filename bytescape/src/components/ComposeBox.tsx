import { useState, useRef, useCallback, useEffect } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { INPUT_LIMITS, formatFileSize } from '../utils/sanitize';
import { useCharacterCount, TARGET_ICONS, STATUS_COLORS } from '../hooks/useCharacterCount';
import { getAutocompleteContext, applyAutocompleteSelection } from '../utils/richtext';
import type { AutocompleteContext } from '../utils/richtext';
import { ArticleEditor } from './ArticleEditor';
import type { ArticleEditorData } from './ArticleEditor';
import { AutocompleteDropdown, mentionToItem, hashtagToItem } from './AutocompleteDropdown';
import { LinkPreviewCard } from './LinkPreviewCard';
import type { LinkCard, LinkedAccount } from '../types';
import type { AutocompleteItem } from './AutocompleteDropdown';
import type { MentionSuggestion } from '../types';

/**
 * PURPOSE
 * - Provide the primary post composition surface for short posts and article-mode posts.
 * - Coordinate text, media attachments, platform targets, autocomplete, and link unfurl preview.
 *
 * HOW IT WORKS
 * - Tracks compose state locally and derives posting constraints from `useCharacterCount()`.
 * - Calls backend commands (`invoke`) for mention/hashtag suggestions, linked-account detection,
 *   link unfurling, and attachment file-size checks.
 * - Delegates final publish behavior to injected `onPublish` callback.
 *
 * RELATED FILES
 * - src/hooks/useCharacterCount.ts (multi-target limit logic)
 * - src/utils/richtext.ts (autocomplete parsing + insertion)
 * - src/components/AutocompleteDropdown.tsx (suggestion UI)
 * - src/components/ArticleEditor.tsx (article-mode editor)
 * - src-tauri/src/lib.rs (command registration for invoked backend APIs)
 *
 * CROSS-SYSTEM NOTES
 * - Engine interactions are through `invoke()` command boundary:
 *   `search_mentions`, `search_hashtags_autocomplete`, `unfurl_link`,
 *   `list_linked_accounts`, `mastodon_get_session`, `get_file_size`.
 * - Attachment preview URLs use Tauri's `convertFileSrc`, coupling rendering to local file access rules.
 */

/**
 * Local attachment descriptor used by compose/publish pipeline.
 */
export interface Attachment {
    path?: string;
    blob_hash?: string;
    mime_type?: string;
    preview?: string;
    file_size?: number;
}

/**
 * Comma-separated platform target string, e.g. `local,bluesky`.
 */
export type PostTarget = string; // Comma-separated platforms: "local", "local,bluesky", etc.

/**
 * Optional publish metadata for non-basic post flows (e.g., article + link card).
 */
export interface PublishOptions {
    itemType?: 'Status' | 'Article';
    title?: string;
    contentJson?: object;
    linkCard?: LinkCard;
}

interface ComposeBoxProps {
    onPublish: (text: string, target: PostTarget, attachments: Attachment[], options?: PublishOptions) => Promise<void>;
    isPosting: boolean;
    postStatus: string | null;
    initialText?: string;
    initialAttachments?: Attachment[];
}

/**
 * Main composer component.
 *
 * Responsibilities:
 * - Collect user input and enforce compose constraints.
 * - Surface inline assistance (autocomplete, link previews, target counters).
 * - Hand off publish request to parent orchestrator through `onPublish`.
 */
export function ComposeBox({ onPublish, isPosting, postStatus, initialText = '', initialAttachments = [] }: ComposeBoxProps) {
    const [composeText, setComposeText] = useState(initialText);
    const [selectedPlatforms, setSelectedPlatforms] = useState<Set<string>>(new Set(['local']));
    const [linkedPlatforms, setLinkedPlatforms] = useState<string[]>(['local']);
    const [platformDropdownOpen, setPlatformDropdownOpen] = useState(false);
    const platformDropdownRef = useRef<HTMLElement>(null);
    const platformPanelRef = useRef<HTMLDivElement>(null);
    const [attachments, setAttachments] = useState<Attachment[]>(initialAttachments);
    const [fileSizeWarnings, setFileSizeWarnings] = useState<string[]>([]);
    const [articleMode, setArticleMode] = useState(false);
    const articleDataRef = useRef<ArticleEditorData | null>(null);
    const textareaRef = useRef<HTMLTextAreaElement>(null);

    // Compute postTarget from selected platforms (comma-separated)
    const postTarget = Array.from(selectedPlatforms).join(',') || 'local';

    // ═══════════════════════════════════════════════════════════════════════
    // Autocomplete State (C1.2b / C1.3b)
    // ═══════════════════════════════════════════════════════════════════════
    const [acContext, setAcContext] = useState<AutocompleteContext | null>(null);
    const [acItems, setAcItems] = useState<AutocompleteItem[]>([]);
    const [acSelectedIndex, setAcSelectedIndex] = useState(0);
    const [acLoading, setAcLoading] = useState(false);
    const acVisible = acContext !== null && acContext.type !== null;
    const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    // ═══════════════════════════════════════════════════════════════════════
    // Link Preview State (C2.1)
    // ═══════════════════════════════════════════════════════════════════════
    const [linkCard, setLinkCard] = useState<LinkCard | null>(null);
    const [linkDismissed, setLinkDismissed] = useState<string | null>(null);
    const [linkLoading, setLinkLoading] = useState(false);
    const linkDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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

    // ═══════════════════════════════════════════════════════════════════════
    // Autocomplete Logic (C1.2b / C1.3b)
    // ═══════════════════════════════════════════════════════════════════════

    const handleTextChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
        const newText = e.target.value;
        setComposeText(newText);

        const cursorPos = e.target.selectionStart ?? newText.length;
        const ctx = getAutocompleteContext(newText, cursorPos);

        if (ctx.type) {
            setAcContext(ctx);
            setAcSelectedIndex(0);

            // Debounce the search
            if (debounceRef.current) clearTimeout(debounceRef.current);
            debounceRef.current = setTimeout(() => {
                fetchAutocompleteSuggestions(ctx);
            }, 200);
        } else {
            closeAutocomplete();
        }
    }, []);

    const fetchAutocompleteSuggestions = useCallback(async (ctx: AutocompleteContext) => {
        if (!ctx.type) return;
        setAcLoading(true);
        try {
            if (ctx.type === 'mention') {
                const results = await invoke<MentionSuggestion[]>('search_mentions', { query: ctx.query });
                setAcItems(results.map(mentionToItem));
            } else {
                const results = await invoke<string[]>('search_hashtags_autocomplete', { query: ctx.query });
                setAcItems(results.map(hashtagToItem));
            }
        } catch (err) {
            console.error('Autocomplete fetch failed:', err);
            setAcItems([]);
        } finally {
            setAcLoading(false);
        }
    }, []);

    const closeAutocomplete = useCallback(() => {
        setAcContext(null);
        setAcItems([]);
        setAcSelectedIndex(0);
        setAcLoading(false);
        if (debounceRef.current) {
            clearTimeout(debounceRef.current);
            debounceRef.current = null;
        }
    }, []);

    const handleAcSelect = useCallback((item: AutocompleteItem) => {
        if (!acContext || !acContext.type) return;
        const value = item.type === 'mention' ? item.handle : item.tag;
        const { text: newText, cursorPos } = applyAutocompleteSelection(composeText, acContext, value);
        setComposeText(newText);
        closeAutocomplete();

        // Restore cursor position after React re-render
        requestAnimationFrame(() => {
            if (textareaRef.current) {
                textareaRef.current.focus();
                textareaRef.current.selectionStart = cursorPos;
                textareaRef.current.selectionEnd = cursorPos;
            }
        });
    }, [acContext, composeText, closeAutocomplete]);

    const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLTextAreaElement>) => {
        if (!acVisible) return;

        if (e.key === 'ArrowDown') {
            e.preventDefault();
            setAcSelectedIndex(prev => Math.min(prev + 1, acItems.length - 1));
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            setAcSelectedIndex(prev => Math.max(prev - 1, 0));
        } else if (e.key === 'Enter' || e.key === 'Tab') {
            if (acItems.length > 0) {
                e.preventDefault();
                handleAcSelect(acItems[acSelectedIndex]);
            }
        } else if (e.key === 'Escape') {
            e.preventDefault();
            closeAutocomplete();
        }
    }, [acVisible, acItems, acSelectedIndex, handleAcSelect, closeAutocomplete]);

    // Cleanup debounce on unmount
    useEffect(() => {
        return () => {
            if (debounceRef.current) clearTimeout(debounceRef.current);
        };
    }, []);

    // ═══════════════════════════════════════════════════════════════════════
    // Link Preview Detection (C2.1)
    // ═══════════════════════════════════════════════════════════════════════
    useEffect(() => {
        if (articleMode) return;
        const urlMatch = composeText.match(/https?:\/\/[^\s<>"{}|\\^`[\]]+/);
        const detectedUrl = urlMatch ? urlMatch[0] : null;

        if (!detectedUrl) {
            setLinkCard(null);
            return;
        }
        if (detectedUrl === linkDismissed) return;
        if (linkCard?.url === detectedUrl) return;

        if (linkDebounceRef.current) clearTimeout(linkDebounceRef.current);
        linkDebounceRef.current = setTimeout(async () => {
            setLinkLoading(true);
            try {
                const card = await invoke<LinkCard>('unfurl_link', { url: detectedUrl });
                setLinkCard(card);
            } catch (err) {
                console.warn('Link unfurl failed:', err);
                setLinkCard(null);
            } finally {
                setLinkLoading(false);
            }
        }, 500);

        return () => {
            if (linkDebounceRef.current) clearTimeout(linkDebounceRef.current);
        };
    }, [composeText, articleMode, linkDismissed, linkCard?.url]);

    // ═══════════════════════════════════════════════════════════════════════
    // Detect Linked Platforms
    // ═══════════════════════════════════════════════════════════════════════
    useEffect(() => {
        async function detectPlatforms() {
            const platforms: string[] = ['local'];
            try {
                const accounts = await invoke<LinkedAccount[]>('list_linked_accounts');
                for (const acc of accounts) {
                    if (acc.active) {
                        const proto = acc.protocol?.toLowerCase();
                        if (proto === 'bluesky' && !platforms.includes('bluesky')) platforms.push('bluesky');
                        if (proto === 'mastodon' && !platforms.includes('mastodon')) platforms.push('mastodon');
                    }
                }
            } catch { /* list_linked_accounts may not be available */ }
            // Fallback: check mastodon session directly
            if (!platforms.includes('mastodon')) {
                try {
                    const session = await invoke('mastodon_get_session');
                    if (session) platforms.push('mastodon');
                } catch { /* no mastodon session */ }
            }
            setLinkedPlatforms(platforms);
        }
        detectPlatforms();
    }, []);

    // ═══════════════════════════════════════════════════════════════════════
    // Close Platform Dropdown on Outside Click
    // ═══════════════════════════════════════════════════════════════════════
    useEffect(() => {
        if (!platformDropdownOpen) return;
        const handleClickOutside = (e: MouseEvent) => {
            const target = e.target as Node;
            const inButton = platformDropdownRef.current?.contains(target);
            const inPanel = platformPanelRef.current?.contains(target);
            if (!inButton && !inPanel) {
                setPlatformDropdownOpen(false);
            }
        };
        document.addEventListener('mousedown', handleClickOutside);
        return () => document.removeEventListener('mousedown', handleClickOutside);
    }, [platformDropdownOpen]);

    // ═══════════════════════════════════════════════════════════════════════
    // Platform Toggle Handlers
    // ═══════════════════════════════════════════════════════════════════════
    const PLATFORM_INFO: Record<string, { icon: string; label: string }> = {
        local: { icon: '🔒', label: 'Local Vault' },
        bluesky: { icon: '🦋', label: 'Bluesky' },
        mastodon: { icon: '🐘', label: 'Mastodon' },
    };

    const togglePlatform = useCallback((platform: string) => {
        setSelectedPlatforms(prev => {
            const next = new Set(prev);
            if (next.has(platform)) {
                next.delete(platform);
                if (next.size === 0) next.add('local');
            } else {
                next.add(platform);
            }
            return next;
        });
    }, []);

    const allLinkedSelected = linkedPlatforms.every(p => selectedPlatforms.has(p));

    const toggleAll = useCallback(() => {
        if (allLinkedSelected) {
            setSelectedPlatforms(new Set(['local']));
        } else {
            setSelectedPlatforms(new Set(linkedPlatforms));
        }
    }, [allLinkedSelected, linkedPlatforms]);

    const platformSummary = allLinkedSelected && linkedPlatforms.length > 1
        ? 'All'
        : Array.from(selectedPlatforms).map(p => PLATFORM_INFO[p]?.icon || p).join(' ');

    const handlePublish = async () => {
        if (!canPost) return;
        if (articleMode && articleDataRef.current) {
            const { content, title, contentJson } = articleDataRef.current;
            await onPublish(content, postTarget, attachments, {
                itemType: 'Article',
                title,
                contentJson,
                linkCard: linkCard ?? undefined,
            });
            articleDataRef.current = null;
        } else {
            await onPublish(composeText, postTarget, attachments, linkCard ? { linkCard } : undefined);
        }
        setComposeText('');
        setAttachments([]);
        setFileSizeWarnings([]);
        setLinkCard(null);
        setLinkDismissed(null);
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
                        <div style={{ position: 'relative' }}>
                            <textarea
                                ref={textareaRef}
                                value={composeText}
                                onChange={handleTextChange}
                                onKeyDown={handleKeyDown}
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
                            <AutocompleteDropdown
                                items={acItems}
                                selectedIndex={acSelectedIndex}
                                onSelect={handleAcSelect}
                                onClose={closeAutocomplete}
                                visible={acVisible}
                                loading={acLoading}
                            />
                        </div>
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

                        {/* Link Preview Card (C2.1) */}
                        {linkCard && (
                            <LinkPreviewCard
                                card={linkCard}
                                onDismiss={() => {
                                    setLinkDismissed(linkCard.url);
                                    setLinkCard(null);
                                }}
                            />
                        )}
                        {linkLoading && (
                            <div style={{ fontSize: '0.72rem', color: 'var(--text-muted)', marginBottom: 8 }}>
                                🔗 Loading link preview…
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
                        {/* Platform Selector Toggle */}
                        <button
                            ref={platformDropdownRef as React.RefObject<HTMLButtonElement | null>}
                            className="btn btn-ghost btn-sm"
                            onClick={() => setPlatformDropdownOpen(!platformDropdownOpen)}
                            title="Select posting platforms"
                            style={{ padding: '2px 8px', fontSize: '0.75rem' } as React.CSSProperties}
                            type="button"
                        >
                            Platform: {platformSummary} {platformDropdownOpen ? '▴' : '▾'}
                        </button>
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

                {/* Platform checkboxes — renders inline below the toolbar when open */}
                {platformDropdownOpen && (
                    <div ref={platformPanelRef} style={{
                        display: 'flex',
                        flexWrap: 'wrap',
                        gap: '4px 16px',
                        alignItems: 'center',
                        marginTop: 8,
                        padding: '8px 10px',
                        background: 'var(--bg-secondary, #1a1a2e)',
                        border: '1px solid var(--border-color, #333)',
                        borderRadius: 6,
                    }}>
                        {linkedPlatforms.length > 1 && (
                            <label style={{
                                display: 'flex', alignItems: 'center', gap: 6,
                                cursor: 'pointer', fontSize: '0.8rem',
                                color: 'var(--text-primary, #eee)',
                                marginRight: 8,
                            }}>
                                <input
                                    type="checkbox"
                                    checked={allLinkedSelected}
                                    onChange={toggleAll}
                                    style={{ accentColor: 'var(--accent-purple, #9b59b6)' }}
                                />
                                ✨ All
                            </label>
                        )}
                        {linkedPlatforms.map(p => (
                            <label key={p} style={{
                                display: 'flex', alignItems: 'center', gap: 6,
                                cursor: 'pointer', fontSize: '0.8rem',
                                color: 'var(--text-primary, #eee)',
                            }}>
                                <input
                                    type="checkbox"
                                    checked={selectedPlatforms.has(p)}
                                    onChange={() => togglePlatform(p)}
                                    style={{ accentColor: 'var(--accent-purple, #9b59b6)' }}
                                />
                                {PLATFORM_INFO[p]?.icon} {PLATFORM_INFO[p]?.label}
                            </label>
                        ))}
                    </div>
                )}
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
