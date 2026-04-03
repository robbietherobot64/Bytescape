import { useState, useRef, useEffect } from 'react';

/**
 * UI-visible emoji set for the lightweight reaction picker.
 *
 * Cross-system note:
 * - Backend like-state currently supports binary like/unlike semantics.
 * - Emoji choice is frontend display metadata layered on top of that state.
 */
const REACTION_EMOJIS = ['❤️', '😂', '😮', '😢', '😡', '👍', '🔥', '🎉'] as const;

/** Props for `ReactionPicker`. */
interface ReactionPickerProps {
    /** Currently selected reaction, if any */
    currentReaction?: string;
    /** When user picks a reaction (null = deselect) */
    onReact: (emoji: string | null) => void;
    /** Disable for non-Bluesky sources */
    disabled?: boolean;
}

/**
 * ReactionPicker — small popup emoji grid that appears on hover/click
 * over the React (heart) button. Maps to Bluesky like_post on the backend.
 * Locally tracks which emoji was chosen for Bytescape-specific display.
 *
 * Exported symbol used by feed engagement surfaces.
 */
export function ReactionPicker({ currentReaction, onReact, disabled }: ReactionPickerProps) {
     // ===== Section: local popup state =====
    const [open, setOpen] = useState(false);
    const containerRef = useRef<HTMLDivElement>(null);

     // ===== Section: side effects =====
     // Close on outside click.
    useEffect(() => {
        if (!open) return;
        const handler = (e: MouseEvent) => {
            if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
                setOpen(false);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, [open]);

    // Close on Escape
    useEffect(() => {
        if (!open) return;
        const handler = (e: KeyboardEvent) => {
            if (e.key === 'Escape') setOpen(false);
        };
        document.addEventListener('keydown', handler);
        return () => document.removeEventListener('keydown', handler);
    }, [open]);

    // ===== Section: interaction handlers =====
    const handleToggle = (e: React.MouseEvent) => {
        e.stopPropagation();
        if (disabled) return;

        // If already reacted, quick-tap removes the reaction
        if (currentReaction && !open) {
            onReact(null);
            return;
        }

        setOpen(prev => !prev);
    };

    const handlePick = (emoji: string, e: React.MouseEvent) => {
        e.stopPropagation();
        onReact(emoji);
        setOpen(false);
    };

    return (
        // ===== Section: trigger + popup composition =====
        <div className="reaction-picker-container" ref={containerRef}>
            <button
                className={`engagement-btn btn-like ${currentReaction ? 'active' : ''}`}
                title={currentReaction ? `Reacted ${currentReaction} — click to remove` : 'React'}
                onClick={handleToggle}
                disabled={disabled}
            >
                <span className="reaction-current">
                    {currentReaction || '❤️'}
                </span>
            </button>

            {open && (
                <div className="reaction-picker-popup" onClick={e => e.stopPropagation()}>
                    {REACTION_EMOJIS.map(emoji => (
                        <button
                            key={emoji}
                            className={`reaction-picker-emoji ${emoji === currentReaction ? 'selected' : ''}`}
                            onClick={(e) => handlePick(emoji, e)}
                            title={emoji}
                        >
                            {emoji}
                        </button>
                    ))}
                </div>
            )}
        </div>
    );
}
