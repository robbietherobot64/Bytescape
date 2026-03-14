/**
 * RichText Component (C1.1b)
 *
 * Renders plain text with detected @mentions, #hashtags, and URLs as
 * interactive elements. Uses the facet detection logic from utils/richtext.ts.
 */

import { useMemo } from 'react';
import { segmentText, truncateUrl } from '../utils/richtext';

export interface RichTextProps {
    /** The plain text content to render with facets */
    text: string;
    /** Optional CSS class for the wrapper span */
    className?: string;
}

/**
 * Renders text with @mentions, #hashtags, and URLs as styled interactive elements.
 * - Mentions render as purple-tinted links
 * - Hashtags render as teal-tinted links
 * - URLs render as classic blue links with truncated display text
 */
export function RichText({ text, className }: RichTextProps) {
    const segments = useMemo(() => segmentText(text), [text]);

    return (
        <span className={className}>
            {segments.map((seg, i) => {
                switch (seg.type) {
                    case 'mention':
                        return (
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
                            <a
                                key={i}
                                className="facet-hashtag"
                                title={`Search #${seg.value}`}
                                role="link"
                                tabIndex={0}
                            >
                                {seg.text}
                            </a>
                        );
                    case 'link':
                        return (
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
                        return <span key={i}>{seg.text}</span>;
                }
            })}
        </span>
    );
}
