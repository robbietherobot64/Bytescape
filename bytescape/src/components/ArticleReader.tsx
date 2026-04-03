import { useEffect, useState, useRef } from 'react';
import { sanitizeHtml } from '../utils/sanitize';
import { markdownToHtml, countWords, readingTime } from '../utils/markdown';
import { linkifyFacetsInHtml } from '../utils/richtext';

/**
 * Props for `ArticleReader`.
 *
 * Cross-system note:
 * - Input `content` is markdown-like text normalized from engine data models.
 * - Rendering pipeline (`markdownToHtml` -> `sanitizeHtml` -> facet linkify)
 *   keeps output safe while preserving rich text affordances.
 */
export interface ArticleReaderProps {
    title: string;
    content: string;        // Markdown string
    source?: string;
    createdAt?: number;      // Unix timestamp
    updatedAt?: number;      // Unix timestamp
}

/**
 * Exported article-view component with reading metrics and scroll progress.
 */
export function ArticleReader({ title, content, source, createdAt, updatedAt }: ArticleReaderProps) {
    // ===== Section: local UI state =====
    const [scrollProgress, setScrollProgress] = useState(0);
    const contentRef = useRef<HTMLDivElement>(null);

    // ===== Section: derived content metadata =====
    const html = linkifyFacetsInHtml(sanitizeHtml(markdownToHtml(content)));
    const wc = countWords(content);
    const rt = readingTime(wc);

    // ===== Section: scroll progress tracking =====
    useEffect(() => {
        const handleScroll = () => {
            if (!contentRef.current) return;
            const rect = contentRef.current.getBoundingClientRect();
            const windowHeight = window.innerHeight;
            const totalHeight = rect.height;

            if (totalHeight <= windowHeight) {
                setScrollProgress(100);
                return;
            }

            const scrolled = Math.max(0, -rect.top);
            const scrollable = totalHeight - windowHeight;
            const progress = Math.min(100, (scrolled / scrollable) * 100);
            setScrollProgress(progress);
        };

        window.addEventListener('scroll', handleScroll, { passive: true });
        handleScroll();
        return () => window.removeEventListener('scroll', handleScroll);
    }, []);

    // ===== Section: formatting helpers =====
    const formatDate = (ts: number) => {
        return new Date(ts * 1000).toLocaleDateString('en-US', {
            year: 'numeric',
            month: 'long',
            day: 'numeric',
        });
    };

    return (
        <>
            {/* ===== Section: top reading progress bar ===== */}
            <div
                className="article-scroll-progress"
                style={{ width: `${scrollProgress}%` } as React.CSSProperties}
                data-testid="scroll-progress"
            />

            {/* ===== Section: article frame + body ===== */}
            <div className="article-reader" ref={contentRef}>
                <div className="win98-box" style={{ padding: 24 } as React.CSSProperties}>
                    <h1 style={{ fontSize: 32, marginBottom: 8, color: 'var(--text-primary)' } as React.CSSProperties}>
                        {title}
                    </h1>

                    <div className="article-reader-meta">
                        {source && (
                            <span className="source-badge" style={{ textTransform: 'capitalize' } as React.CSSProperties}>
                                {source}
                            </span>
                        )}
                        {createdAt && <span>📅 {formatDate(createdAt)}</span>}
                        {updatedAt && updatedAt !== createdAt && (
                            <span>✏️ Updated {formatDate(updatedAt)}</span>
                        )}
                        <span>📖 {rt} min read</span>
                        <span>{wc} words</span>
                    </div>

                    <div
                        className="article-reader-content"
                        dangerouslySetInnerHTML={{ __html: html }}
                        style={{
                            fontSize: 16,
                            lineHeight: 1.8,
                            color: 'var(--text-secondary)',
                            fontFamily: 'var(--font-serif)',
                        } as React.CSSProperties}
                    />
                </div>
            </div>
        </>
    );
}
