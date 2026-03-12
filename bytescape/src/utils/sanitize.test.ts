import { describe, it, expect } from 'vitest';
import { sanitizeHtml, stripHtml, formatFileSize, INPUT_LIMITS } from './sanitize';

describe('sanitizeHtml', () => {
    it('strips <script> tags completely', () => {
        const dirty = '<script>alert("xss")</script>Hello';
        expect(sanitizeHtml(dirty)).toBe('Hello');
    });

    it('strips <script> with src attribute', () => {
        const dirty = '<script src="evil.js"></script>';
        expect(sanitizeHtml(dirty)).toBe('');
    });

    it('preserves allowed formatting tags: <b>, <i>, <em>, <strong>', () => {
        const input = '<b>bold</b> <i>italic</i> <em>em</em> <strong>strong</strong>';
        expect(sanitizeHtml(input)).toBe(input);
    });

    it('preserves <a> with href and target', () => {
        const input = '<a href="https://example.com" target="_blank">link</a>';
        expect(sanitizeHtml(input)).toBe(input);
    });

    it('removes javascript: URIs from <a> href', () => {
        const dirty = '<a href="javascript:alert(\'xss\')">click</a>';
        const result = sanitizeHtml(dirty);
        expect(result).not.toContain('javascript:');
        // DOMPurify removes the href entirely
        expect(result).toContain('<a>click</a>');
    });

    it('strips <img> tags (not in allowed list)', () => {
        const dirty = '<img src="x" onerror="alert(\'xss\')">';
        expect(sanitizeHtml(dirty)).toBe('');
    });

    it('strips <img> with onerror handler', () => {
        const dirty = 'Hello <img src=x onerror=alert("xss")> world';
        expect(sanitizeHtml(dirty)).toBe('Hello  world');
    });

    it('strips <iframe> tags', () => {
        const dirty = '<iframe src="evil.html"></iframe>content';
        expect(sanitizeHtml(dirty)).toBe('content');
    });

    it('strips event handlers from allowed tags', () => {
        const dirty = '<b onclick="alert(1)">bold</b>';
        expect(sanitizeHtml(dirty)).toBe('<b>bold</b>');
    });

    it('preserves <br>, <p>, <span>, lists', () => {
        const input = '<p>paragraph</p><br><ul><li>item</li></ul><ol><li>ordered</li></ol>';
        expect(sanitizeHtml(input)).toBe(input);
    });

    it('handles nested dangerous content', () => {
        const dirty = '<b><script>alert(1)</script>safe text</b>';
        expect(sanitizeHtml(dirty)).toBe('<b>safe text</b>');
    });

    it('handles empty string', () => {
        expect(sanitizeHtml('')).toBe('');
    });

    it('handles plain text without HTML', () => {
        const text = 'Just a normal post with no HTML';
        expect(sanitizeHtml(text)).toBe(text);
    });
});

describe('stripHtml', () => {
    it('removes all HTML tags, returns plain text', () => {
        const dirty = '<b>bold</b> and <a href="x">link</a>';
        expect(stripHtml(dirty)).toBe('bold and link');
    });

    it('strips script tags and their content', () => {
        const dirty = '<script>alert("xss")</script>Hello';
        expect(stripHtml(dirty)).toBe('Hello');
    });

    it('handles empty string', () => {
        expect(stripHtml('')).toBe('');
    });

    it('preserves plain text', () => {
        const text = 'No tags here';
        expect(stripHtml(text)).toBe(text);
    });
});

describe('formatFileSize', () => {
    it('formats bytes', () => {
        expect(formatFileSize(500)).toBe('500 B');
    });

    it('formats kilobytes', () => {
        expect(formatFileSize(2048)).toBe('2.0 KB');
    });

    it('formats megabytes', () => {
        expect(formatFileSize(49_576_755)).toBe('47.3 MB');
    });

    it('formats gigabytes', () => {
        expect(formatFileSize(1_500_000_000)).toBe('1.4 GB');
    });
});

describe('INPUT_LIMITS', () => {
    it('has expected values', () => {
        expect(INPUT_LIMITS.MAX_LOCAL_CHARS).toBe(10_000);
        expect(INPUT_LIMITS.WARN_CHARS).toBe(8_000);
        expect(INPUT_LIMITS.MAX_ATTACHMENTS).toBe(20);
        expect(INPUT_LIMITS.FILE_WARN_BYTES).toBe(50 * 1024 * 1024);
        expect(INPUT_LIMITS.FILE_MAX_BYTES).toBe(100 * 1024 * 1024);
    });
});
