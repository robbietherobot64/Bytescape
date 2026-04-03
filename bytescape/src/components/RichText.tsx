/**
 * RichText Component (C1.1b)
 *
 * Renders plain text with detected @mentions, #hashtags, and URLs as
 * interactive elements. Uses the facet detection logic from utils/richtext.ts.
 *
 * Cross-system note:
 * - Facet parsing is local lens logic (`segmentText`) and does not call Tauri commands.
 * - Link navigation and hashtag events are intended to feed higher-level app routing/state.
 */

import { useMemo } from 'react';
import { segmentText, truncateUrl } from '../utils/richtext';

export interface RichTextProps {
    /** The plain text content to render with facets */
    text: string;
    /** Optional CSS class for the wrapper span */
    className?: string;
}

// ============================================================
// RichText Renderer
// ============================================================

/**
 * Exported presentation component for inline facet rendering.
 *
 * Renders text with @mentions, #hashtags, and URLs as styled interactive elements.
 * - Mentions render as purple-tinted links
 * - Hashtags render as teal-tinted links
 * - URLs render as classic blue links with truncated display text
 */
export function RichText({ text, className }: RichTextProps) {
    /** Derived segments are memoized to keep render cost predictable for long posts. */
    const segments = useMemo(() => segmentText(text), [text]);

    return (
        <span className={className}>
            {segments.map((seg, i) => {
                switch (seg.type) {
                    case 'mention':
                        return (
                            // Mention tokens are rendered as non-navigating link affordances for now.
                            <a
                                key={i}
                                className="facet-mention"
                                title={`View @${seg.value}`}
                                role="link"
                                tabIndex={0}
                            >
                                {seg.text}
                            </a>
                        );
                    case 'hashtag':
                        return (
                            // Hashtag navigation uses a global browser event consumed elsewhere in the lens.
                            <a
                                key={i}
                                className="facet-hashtag"
                                title={`View #${seg.value} in Subscribed`}
                                role="link"
                                tabIndex={0}
                                onClick={() => {
                                    window.dispatchEvent(new CustomEvent('navigate-hashtag', { detail: seg.value }));
                                }}
                                style={{ cursor: 'pointer' } as React.CSSProperties}
                            >
                                {seg.text}
                            </a>
                        );
                    case 'link':
                        return (
                            // External URLs open in a new tab/window with safe rel attributes.
                            <a
                                key={i}
                                className="facet-link"
                                href={seg.value}
                                target="_blank"
                                rel="noopener noreferrer"
                                title={seg.value}
                            >
                                {truncateUrl(seg.value || seg.text)}
                            </a>
                        );
                    default:
                        // Plain text fragment passthrough.
                        return <span key={i}>{seg.text}</span>;
                }
            })}
        </span>
    );
}
