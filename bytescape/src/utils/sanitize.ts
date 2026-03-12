import DOMPurify from 'dompurify';

const ALLOWED_TAGS = ['b', 'i', 'em', 'strong', 'a', 'br', 'p', 'span', 'ul', 'ol', 'li'];
const ALLOWED_ATTR = ['href', 'target', 'rel'];

/**
 * Sanitize HTML string, allowing only safe formatting tags.
 * Use when you need to render user content with basic formatting preserved.
 * All `<script>`, `<img onerror>`, `javascript:` URIs etc. are stripped.
 */
export function sanitizeHtml(dirty: string): string {
    return DOMPurify.sanitize(dirty, {
        ALLOWED_TAGS,
        ALLOWED_ATTR,
        ADD_ATTR: ['target'],
    });
}

/**
 * Strip ALL HTML — returns plain text only.
 * Use for previews, search indexes, character counts, and any context where
 * markup is not needed.
 */
export function stripHtml(dirty: string): string {
    return DOMPurify.sanitize(dirty, { ALLOWED_TAGS: [] });
}

/** Input validation constants for ComposeBox / posting */
export const INPUT_LIMITS = {
    /** Hard cap — post button is disabled above this */
    MAX_LOCAL_CHARS: 10_000,
    /** Soft warning threshold */
    WARN_CHARS: 8_000,
    /** Maximum file attachments per post */
    MAX_ATTACHMENTS: 20,
    /** Per-file warning threshold in bytes (50 MB) */
    FILE_WARN_BYTES: 50 * 1024 * 1024,
    /** Per-file hard block in bytes (100 MB) */
    FILE_MAX_BYTES: 100 * 1024 * 1024,
} as const;

/** Format bytes into a human-readable string, e.g. "47.3 MB" */
export function formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
