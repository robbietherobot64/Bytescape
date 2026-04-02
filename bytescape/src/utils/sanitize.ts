import DOMPurify from 'dompurify';

/**
 * PURPOSE
 * Centralized sanitization and input-limit helpers for user-authored content.
 *
 * HOW IT WORKS
 * Wraps DOMPurify with an explicit allowlist policy for formatting tags/attrs,
 * and exposes shared size/length guardrails used by compose flows.
 *
 * RELATED FILES
 * - src/components/*Compose* (input validation and warnings)
 * - src/utils/content.ts (non-HTML text cleanup helpers)
 */
const ALLOWED_TAGS = ['b', 'i', 'em', 'strong', 'a', 'br', 'p', 'span', 'ul', 'ol', 'li'];
const ALLOWED_ATTR = ['href', 'target', 'rel'];

/**
 * Sanitize HTML string, allowing only safe formatting tags.
 * Use when you need to render user content with basic formatting preserved.
 * All `<script>`, `<img onerror>`, `javascript:` URIs etc. are stripped.
 *
 * Caveat:
 * This uses a fixed allowlist policy by design; unsupported tags/attributes are
 * always removed unless this module is explicitly updated.
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
 *
 * Caveat:
 * Result is plain text produced by sanitization rules, not a reversible HTML
 * transformation.
 */
export function stripHtml(dirty: string): string {
    return DOMPurify.sanitize(dirty, { ALLOWED_TAGS: [] });
}

/** Shared input validation constants for compose/posting workflows. */
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

/** Formats bytes into a compact human-readable label (for example, "47.3 MB"). */
export function formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}
