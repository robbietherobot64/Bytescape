/**
 * AutocompleteDropdown Component (C1.2b / C1.3b)
 *
 * A positioned dropdown for @mention and #hashtag suggestions.
 * Shows below the ComposeBox textarea with keyboard navigation support.
 */

import { useEffect, useRef, useCallback } from 'react';
import type { MentionSuggestion } from '../types';

// ─── Types ───

export interface MentionItem {
    type: 'mention';
    handle: string;
    did: string;
    avatarUrl?: string | null;
}

export interface HashtagItem {
    type: 'hashtag';
    tag: string;
}

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

export function mentionToItem(s: MentionSuggestion): MentionItem {
    return { type: 'mention', handle: s.handle, did: s.did, avatarUrl: s.avatar_url };
}

export function hashtagToItem(tag: string): HashtagItem {
    return { type: 'hashtag', tag };
}

// ─── Component ───

export function AutocompleteDropdown({
    items,
    selectedIndex,
    onSelect,
    onClose,
    visible,
    loading = false,
}: AutocompleteDropdownProps) {
    const listRef = useRef<HTMLDivElement>(null);

    // Scroll selected item into view
    useEffect(() => {
        if (!listRef.current) return;
        const selected = listRef.current.querySelector('.autocomplete-item.selected');
        if (selected) {
            selected.scrollIntoView({ block: 'nearest' });
        }
    }, [selectedIndex]);

    // Close on outside click
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
