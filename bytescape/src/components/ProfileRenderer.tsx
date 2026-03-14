/**
 * Profile Renderer (B2.4)
 *
 * Renders a saved GrapesJS profile layout with live React widget hydration.
 * Parses the saved HTML to find widget container elements (identified by CSS class),
 * then renders sanitized HTML segments interspersed with live React widget components.
 *
 * Security: Uses DOMPurify (HIGH-3 compliance) to sanitize all HTML content.
 * CSS is also sanitized to strip potentially dangerous constructs.
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import DOMPurify from 'dompurify';
import { ProfileLayout, WidgetData, AwayMessageConfig, MusicTrack } from '../types';
import {
    WIDGET_CLASSES,
    AwayMessageWidget,
    MusicPlayerWidget,
    FriendsListWidget,
    GuestbookWidget,
    VisitorCounterWidget,
    PostFeedWidget,
    MediaGridWidget,
} from './widgets';

interface ProfileRendererProps {
    editable?: boolean;
}

/** A segment of the layout — either raw HTML or a widget placeholder */
type LayoutSegment =
    | { type: 'html'; content: string }
    | { type: 'widget'; widgetClass: string };

/**
 * Parse the saved layout HTML into alternating HTML and widget segments.
 * Widget containers are identified by their CSS class (e.g., bs-away, bs-music).
 */
function parseLayoutIntoSegments(html: string): LayoutSegment[] {
    const segments: LayoutSegment[] = [];
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');
    const body = doc.body;

    function walk(node: Node) {
        if (node.nodeType === Node.ELEMENT_NODE) {
            const el = node as HTMLElement;
            // Check if this element is a widget container
            const widgetClass = WIDGET_CLASSES.find(cls => el.classList.contains(cls));
            if (widgetClass) {
                segments.push({ type: 'widget', widgetClass });
                return; // Don't recurse into widget containers
            }
        }

        if (node.nodeType === Node.TEXT_NODE) {
            const text = node.textContent || '';
            if (text.trim()) {
                segments.push({ type: 'html', content: text });
            }
            return;
        }

        if (node.nodeType === Node.ELEMENT_NODE) {
            const el = node as HTMLElement;
            // Check if any children contain widgets
            const childNodes = Array.from(el.childNodes);
            const hasWidgetChild = childNodes.some(child => {
                if (child.nodeType === Node.ELEMENT_NODE) {
                    const childEl = child as HTMLElement;
                    return WIDGET_CLASSES.some(cls => childEl.classList.contains(cls)) ||
                        childEl.querySelector(WIDGET_CLASSES.map(c => '.' + c).join(','));
                }
                return false;
            });

            if (hasWidgetChild) {
                // This element has widget children — recurse to split them out
                for (const child of childNodes) {
                    walk(child);
                }
            } else {
                // No widget children — output as a single HTML block
                segments.push({ type: 'html', content: el.outerHTML });
            }
        }
    }

    for (const child of Array.from(body.childNodes)) {
        walk(child);
    }

    // Merge adjacent HTML segments
    const merged: LayoutSegment[] = [];
    for (const seg of segments) {
        if (seg.type === 'html' && merged.length > 0 && merged[merged.length - 1].type === 'html') {
            (merged[merged.length - 1] as { type: 'html'; content: string }).content += seg.content;
        } else {
            merged.push(seg);
        }
    }

    return merged;
}

/**
 * Sanitize CSS to remove potentially dangerous constructs.
 */
function sanitizeCss(css: string): string {
    return css
        .replace(/@import\b[^;]+;/gi, '/* import removed */')
        .replace(/expression\s*\(/gi, '/* expression removed */(')
        .replace(/javascript\s*:/gi, '/* js removed */:')
        .replace(/behavior\s*:/gi, '/* behavior removed */:')
        .replace(/-moz-binding\s*:/gi, '/* binding removed */:');
}

export function ProfileRenderer({ editable = false }: ProfileRendererProps) {
    const [layout, setLayout] = useState<ProfileLayout | null>(null);
    const [widgetData, setWidgetData] = useState<WidgetData>({});
    const [loading, setLoading] = useState(true);

    // Load layout and widget data
    useEffect(() => {
        Promise.all([
            invoke<ProfileLayout | null>('get_profile_layout'),
            invoke<string | null>('get_widget_data'),
        ]).then(([layoutResult, widgetDataRaw]) => {
            setLayout(layoutResult);
            if (widgetDataRaw) {
                try {
                    setWidgetData(JSON.parse(widgetDataRaw));
                } catch {
                    // Invalid JSON — ignore
                }
            }
            setLoading(false);
        }).catch(() => setLoading(false));
    }, []);

    // Auto-save widget data when it changes
    const saveWidgetData = useCallback((newData: WidgetData) => {
        setWidgetData(newData);
        invoke('save_widget_data', { widgetData: JSON.stringify(newData) }).catch(console.error);
    }, []);

    if (loading) {
        return (
            <div style={{ padding: 40, textAlign: 'center', color: 'var(--text-muted)' } as React.CSSProperties}>
                Loading profile...
            </div>
        );
    }

    if (!layout || !layout.html) {
        return (
            <div style={{ padding: 40, textAlign: 'center', color: 'var(--text-muted)' } as React.CSSProperties}>
                No custom layout found.
            </div>
        );
    }

    const segments = parseLayoutIntoSegments(layout.html);
    const sanitizedCss = sanitizeCss(layout.css || '');

    const renderWidget = (widgetClass: string, key: number) => {
        switch (widgetClass) {
            case 'bs-away':
                return (
                    <AwayMessageWidget
                        key={key}
                        config={widgetData.away_message as AwayMessageConfig | undefined}
                        editable={editable}
                        onUpdate={(config: AwayMessageConfig) => saveWidgetData({ ...widgetData, away_message: config })}
                    />
                );
            case 'bs-music':
                return (
                    <MusicPlayerWidget
                        key={key}
                        tracks={widgetData.music_playlist as MusicTrack[] | undefined}
                        editable={editable}
                        onUpdate={(tracks: MusicTrack[]) => saveWidgetData({ ...widgetData, music_playlist: tracks })}
                    />
                );
            case 'bs-friends':
                return <FriendsListWidget key={key} editable={editable} />;
            case 'bs-guestbook':
                return <GuestbookWidget key={key} editable={editable} />;
            case 'bs-counter':
                return <VisitorCounterWidget key={key} count={widgetData.visitor_count as number | undefined} />;
            case 'bs-feed-widget':
                return <PostFeedWidget key={key} />;
            case 'bs-media-grid':
                return <MediaGridWidget key={key} />;
            default:
                return null;
        }
    };

    return (
        <div className="profile-renderer">
            {/* Inject sanitized CSS */}
            <style>{sanitizedCss}</style>

            {/* Render segments */}
            {segments.map((segment, i) => {
                if (segment.type === 'widget') {
                    return renderWidget(segment.widgetClass, i);
                }
                // HTML segment — sanitize with DOMPurify
                const cleanHtml = DOMPurify.sanitize(segment.content, {
                    ALLOWED_TAGS: [
                        'div', 'span', 'p', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
                        'a', 'img', 'br', 'hr', 'ul', 'ol', 'li', 'strong', 'em',
                        'b', 'i', 'u', 'blockquote', 'pre', 'code', 'table', 'tr',
                        'td', 'th', 'thead', 'tbody', 'section', 'article', 'header',
                        'footer', 'nav', 'main', 'figure', 'figcaption', 'marquee',
                    ],
                    ALLOWED_ATTR: [
                        'class', 'id', 'style', 'href', 'src', 'alt', 'title',
                        'width', 'height', 'target', 'rel', 'colspan', 'rowspan',
                    ],
                    ALLOW_DATA_ATTR: false,
                });
                return (
                    <div
                        key={i}
                        dangerouslySetInnerHTML={{ __html: cleanHtml }}
                    />
                );
            })}
        </div>
    );
}
