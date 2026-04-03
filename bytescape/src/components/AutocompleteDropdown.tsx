/**
 * ════════════════════════════════════════════════════════════════════════════
 * PURPOSE
 * ════════════════════════════════════════════════════════════════════════════
 * Provide a generic autocomplete list UI for Compose mention/hashtag flows.
 *
 * HOW IT WORKS
 * - Accepts precomputed items and active selection index from parent state.
 * - Renders mention/hashtag variants under a shared `AutocompleteItem` union.
 * - Scrolls selected row into view as keyboard selection changes.
 * - Adds an outside-click listener while visible to trigger `onClose`.
 * - Emits selection to parent on `onMouseDown` to avoid textarea blur race.
 *
 * RELATED FILES
 * - `src/components/ComposeBox.tsx` (input parsing, keyboard nav, state owner)
 * - `src/types/index.ts` (`MentionSuggestion` backend-normalized type)
 * - Compose styles in App/CSS component class definitions
 *
 * CROSS-SYSTEM NOTES
 * - No direct store/provider/invoke calls exist in this component.
 * - Backend suggestion fetches (if any) and caching policies are intentionally
 *   owned by parent hooks/components, not by this dropdown renderer.
 */

import { useEffect, useRef, useCallback } from 'react';
import type { MentionSuggestion } from '../types';

// ─── Types ───

/**
 * Mention-shaped UI item used by the dropdown renderer.
 */
export interface MentionItem {
    type: 'mention';
    handle: string;
    did: string;
    avatarUrl?: string | null;
}

/**
 * Hashtag-shaped UI item used by the dropdown renderer.
 */
export interface HashtagItem {
    type: 'hashtag';
    tag: string;
}

/**
 * Discriminated union consumed by `AutocompleteDropdown`.
 */
export type AutocompleteItem = MentionItem | HashtagItem;

interface AutocompleteDropdownProps {
    items: AutocompleteItem[];
    selectedIndex: number;
    onSelect: (item: AutocompleteItem) => void;
    onClose: () => void;
    visible: boolean;
    loading?: boolean;
}

// ─── Helpers ───

/**
 * Convert a backend/domain `MentionSuggestion` into a renderable dropdown item.
 */
export function mentionToItem(s: MentionSuggestion): MentionItem {
    return { type: 'mention', handle: s.handle, did: s.did, avatarUrl: s.avatar_url };
}

/**
 * Convert a raw hashtag token into a renderable dropdown item.
 */
export function hashtagToItem(tag: string): HashtagItem {
    return { type: 'hashtag', tag };
}

// ─── Component ───

/**
 * ════════════════════════════════════════════════════════════════════════════
 * AutocompleteDropdown
 * ════════════════════════════════════════════════════════════════════════════
 * Stateless suggestion-list renderer controlled by parent Compose state.
 */
export function AutocompleteDropdown({
    items,
    selectedIndex,
    onSelect,
    onClose,
    visible,
    loading = false,
}: AutocompleteDropdownProps) {
    const listRef = useRef<HTMLDivElement>(null);

    // ═══ Keep keyboard-selected row visible within scroll container ═══
    useEffect(() => {
        if (!listRef.current) return;
        const selected = listRef.current.querySelector('.autocomplete-item.selected');
        if (selected) {
            selected.scrollIntoView({ block: 'nearest' });
        }
    }, [selectedIndex]);

    // ═══ Close when pointer interaction occurs outside the dropdown ═══
    const handleClickOutside = useCallback((e: MouseEvent) => {
        if (listRef.current && !listRef.current.contains(e.target as Node)) {
            onClose();
        }
    }, [onClose]);

    useEffect(() => {
        if (visible) {
            document.addEventListener('mousedown', handleClickOutside);
            return () => document.removeEventListener('mousedown', handleClickOutside);
        }
    }, [visible, handleClickOutside]);

    if (!visible) return null;

    return (
        <div
            className="autocomplete-dropdown"
            ref={listRef}
            role="listbox"
            aria-label="Autocomplete suggestions"
        >
            {loading && items.length === 0 && (
                <div className="autocomplete-loading">Searching…</div>
            )}
            {!loading && items.length === 0 && (
                <div className="autocomplete-empty">No results</div>
            )}
            {items.map((item, i) => (
                <div
                    key={item.type === 'mention' ? item.handle : item.tag}
                    className={`autocomplete-item ${i === selectedIndex ? 'selected' : ''}`}
                    role="option"
                    aria-selected={i === selectedIndex}
                    onMouseDown={(e) => {
                        e.preventDefault(); // Prevent textarea blur
                        onSelect(item);
                    }}
                    onMouseEnter={() => {
                        // Handled via parent state — no-op for now
                    }}
                >
                    {item.type === 'mention' ? (
                        <div className="autocomplete-mention-item">
                            <div className="autocomplete-avatar">
                                {item.avatarUrl ? (
                                    <img src={item.avatarUrl} alt="" />
                                ) : (
                                    <span>{item.handle.charAt(0).toUpperCase()}</span>
                                )}
                            </div>
                            <div className="autocomplete-mention-info">
                                <span className="autocomplete-handle">@{item.handle}</span>
                            </div>
                        </div>
                    ) : (
                        <div className="autocomplete-hashtag-item">
                            <span className="autocomplete-hashtag-icon">#</span>
                            <span className="autocomplete-tag-text">{item.tag}</span>
                        </div>
                    )}
                </div>
            ))}
        </div>
    );
}
