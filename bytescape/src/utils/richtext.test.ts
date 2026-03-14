/**
 * Tests for Rich Text Facet Detection & Segmentation (C1.1b)
 */

import { describe, it, expect } from 'vitest';
import {
    detectFacets,
    segmentText,
    truncateUrl,
    linkifyFacetsInHtml,
    getAutocompleteContext,
    applyAutocompleteSelection,
} from './richtext';

// ─── detectFacets ───

describe('detectFacets', () => {
    it('detects @mentions with domain-like handles', () => {
        const facets = detectFacets('Hello @alice.bsky.social how are you?');
        expect(facets).toHaveLength(1);
        expect(facets[0].type).toBe('mention');
        expect(facets[0].value).toBe('alice.bsky.social');
        expect(facets[0].text).toBe('@alice.bsky.social');
    });

    it('detects multiple @mentions', () => {
        const facets = detectFacets('@alice.bsky.social and @bob.test');
        expect(facets).toHaveLength(2);
        expect(facets[0].value).toBe('alice.bsky.social');
        expect(facets[1].value).toBe('bob.test');
    });

    it('ignores @handles without dots', () => {
        const facets = detectFacets('Hello @alice how are you?');
        expect(facets).toHaveLength(0);
    });

    it('detects #hashtags', () => {
        const facets = detectFacets('Check out #bytescape today!');
        expect(facets).toHaveLength(1);
        expect(facets[0].type).toBe('hashtag');
        expect(facets[0].value).toBe('bytescape');
        expect(facets[0].text).toBe('#bytescape');
    });

    it('strips trailing punctuation from hashtags', () => {
        const facets = detectFacets('Love #coding!');
        expect(facets).toHaveLength(1);
        expect(facets[0].value).toBe('coding');
    });

    it('ignores hashtags starting with digit', () => {
        const facets = detectFacets('Check #1thing');
        expect(facets).toHaveLength(0);
    });

    it('detects URLs', () => {
        const facets = detectFacets('Visit https://example.com for more');
        expect(facets).toHaveLength(1);
        expect(facets[0].type).toBe('link');
        expect(facets[0].value).toBe('https://example.com');
    });

    it('strips trailing punctuation from URLs', () => {
        const facets = detectFacets('See https://example.com/page.');
        expect(facets).toHaveLength(1);
        expect(facets[0].value).toBe('https://example.com/page');
    });

    it('handles mixed facets', () => {
        const facets = detectFacets('Hey @alice.bsky.social check #art at https://art.gallery');
        expect(facets).toHaveLength(3);
        expect(facets[0].type).toBe('mention');
        expect(facets[1].type).toBe('hashtag');
        expect(facets[2].type).toBe('link');
    });

    it('removes overlapping facets (keeps first)', () => {
        // URL containing what looks like a mention — URL should win since it starts earlier
        const facets = detectFacets('Check https://example.com/@user.name/post');
        expect(facets).toHaveLength(1);
        expect(facets[0].type).toBe('link');
    });

    it('handles empty string', () => {
        expect(detectFacets('')).toHaveLength(0);
    });

    it('handles text with no facets', () => {
        expect(detectFacets('Just plain text here')).toHaveLength(0);
    });

    it('detects mention at start of text', () => {
        const facets = detectFacets('@alice.bsky.social hello');
        expect(facets).toHaveLength(1);
        expect(facets[0].value).toBe('alice.bsky.social');
        expect(facets[0].start).toBe(0);
    });

    it('detects hashtag at start of text', () => {
        const facets = detectFacets('#hello world');
        expect(facets).toHaveLength(1);
        expect(facets[0].value).toBe('hello');
        expect(facets[0].start).toBe(0);
    });

    it('handles URLs with parentheses correctly', () => {
        const facets = detectFacets('See https://en.wikipedia.org/wiki/Test_(page) for info');
        expect(facets).toHaveLength(1);
        // URL has matching parens, so trailing ) should be kept
        expect(facets[0].value).toBe('https://en.wikipedia.org/wiki/Test_(page)');
    });

    it('strips trailing ) from URL without matching (', () => {
        const facets = detectFacets('(see https://example.com)');
        expect(facets).toHaveLength(1);
        expect(facets[0].value).toBe('https://example.com');
    });

    it('handles emoji in surrounding text', () => {
        const facets = detectFacets('😀 @alice.bsky.social 🎉 #art');
        expect(facets).toHaveLength(2);
        expect(facets[0].type).toBe('mention');
        expect(facets[1].type).toBe('hashtag');
    });
});

// ─── segmentText ───

describe('segmentText', () => {
    it('returns single text segment for plain text', () => {
        const segments = segmentText('Hello world');
        expect(segments).toHaveLength(1);
        expect(segments[0]).toEqual({ text: 'Hello world', type: 'text' });
    });

    it('returns empty array for empty string', () => {
        expect(segmentText('')).toHaveLength(0);
    });

    it('segments text with a mention', () => {
        const segments = segmentText('Hello @alice.bsky.social!');
        expect(segments).toHaveLength(3);
        expect(segments[0]).toEqual({ text: 'Hello ', type: 'text' });
        expect(segments[1]).toEqual({ text: '@alice.bsky.social', type: 'mention', value: 'alice.bsky.social' });
        expect(segments[2]).toEqual({ text: '!', type: 'text' });
    });

    it('segments text with a hashtag', () => {
        const segments = segmentText('Check #bytescape now');
        expect(segments).toHaveLength(3);
        expect(segments[0]).toEqual({ text: 'Check ', type: 'text' });
        expect(segments[1]).toEqual({ text: '#bytescape', type: 'hashtag', value: 'bytescape' });
        expect(segments[2]).toEqual({ text: ' now', type: 'text' });
    });

    it('segments text with a link', () => {
        const segments = segmentText('Visit https://example.com today');
        expect(segments).toHaveLength(3);
        expect(segments[0]).toEqual({ text: 'Visit ', type: 'text' });
        expect(segments[1]).toEqual({ text: 'https://example.com', type: 'link', value: 'https://example.com' });
        expect(segments[2]).toEqual({ text: ' today', type: 'text' });
    });

    it('segments text with mixed facets', () => {
        const segments = segmentText('Hey @alice.bsky.social #art https://gallery.com');
        expect(segments.length).toBeGreaterThanOrEqual(5);
        const types = segments.map(s => s.type);
        expect(types).toContain('mention');
        expect(types).toContain('hashtag');
        expect(types).toContain('link');
    });

    it('handles text starting with a facet', () => {
        const segments = segmentText('@alice.bsky.social said hi');
        expect(segments[0].type).toBe('mention');
        expect(segments[0].text).toBe('@alice.bsky.social');
    });

    it('handles text ending with a facet', () => {
        const segments = segmentText('Follow @alice.bsky.social');
        expect(segments).toHaveLength(2);
        expect(segments[1].type).toBe('mention');
    });
});

// ─── truncateUrl ───

describe('truncateUrl', () => {
    it('strips protocol', () => {
        expect(truncateUrl('https://example.com')).toBe('example.com');
    });

    it('truncates long URLs', () => {
        const long = 'https://example.com/very/long/path/that/exceeds/the/limit/by/far';
        const result = truncateUrl(long, 40);
        expect(result.length).toBeLessThanOrEqual(40);
        expect(result).toContain('…');
    });

    it('does not truncate short URLs', () => {
        expect(truncateUrl('https://example.com', 40)).toBe('example.com');
    });
});

// ─── linkifyFacetsInHtml ───

describe('linkifyFacetsInHtml', () => {
    it('linkifies mentions in plain text', () => {
        const result = linkifyFacetsInHtml('Hello @alice.bsky.social!');
        expect(result).toContain('class="facet-mention"');
        expect(result).toContain('@alice.bsky.social');
    });

    it('linkifies hashtags in plain text', () => {
        const result = linkifyFacetsInHtml('Check #bytescape');
        expect(result).toContain('class="facet-hashtag"');
        expect(result).toContain('#bytescape');
    });

    it('does not modify content inside existing <a> tags', () => {
        const html = '<a href="https://example.com">@alice.bsky.social</a>';
        const result = linkifyFacetsInHtml(html);
        // Should NOT have a nested anchor
        expect(result).not.toContain('facet-mention');
        expect(result).toBe(html);
    });

    it('handles HTML with mixed content', () => {
        const html = '<p>Hello @alice.bsky.social, check <a href="https://example.com">this link</a> and #art</p>';
        const result = linkifyFacetsInHtml(html);
        expect(result).toContain('facet-mention');
        expect(result).toContain('facet-hashtag');
    });

    it('preserves existing HTML structure', () => {
        const html = '<p>Just text</p>';
        expect(linkifyFacetsInHtml(html)).toBe(html);
    });
});

// ─── getAutocompleteContext ───

describe('getAutocompleteContext', () => {
    it('detects @mention trigger with 2+ chars', () => {
        const ctx = getAutocompleteContext('Hello @al', 9);
        expect(ctx.type).toBe('mention');
        expect(ctx.query).toBe('al');
    });

    it('does not trigger with 1 char after @', () => {
        const ctx = getAutocompleteContext('Hello @a', 8);
        expect(ctx.type).toBeNull();
    });

    it('detects #hashtag trigger with 1+ chars', () => {
        const ctx = getAutocompleteContext('Check #b', 8);
        expect(ctx.type).toBe('hashtag');
        expect(ctx.query).toBe('b');
    });

    it('returns null when no trigger', () => {
        const ctx = getAutocompleteContext('Hello world', 11);
        expect(ctx.type).toBeNull();
    });

    it('returns null when trigger is mid-word', () => {
        const ctx = getAutocompleteContext('email@test', 10);
        expect(ctx.type).toBeNull();
    });

    it('detects trigger at start of text', () => {
        const ctx = getAutocompleteContext('@alice', 6);
        expect(ctx.type).toBe('mention');
        expect(ctx.query).toBe('alice');
    });

    it('detects trigger after newline', () => {
        const ctx = getAutocompleteContext('Hello\n@bob', 10);
        expect(ctx.type).toBe('mention');
        expect(ctx.query).toBe('bob');
    });
});

// ─── applyAutocompleteSelection ───

describe('applyAutocompleteSelection', () => {
    it('replaces @mention with selected handle', () => {
        const ctx = { type: 'mention' as const, query: 'al', triggerStart: 6, triggerEnd: 9 };
        const result = applyAutocompleteSelection('Hello @al', ctx, 'alice.bsky.social');
        expect(result.text).toBe('Hello @alice.bsky.social ');
        expect(result.cursorPos).toBe('Hello @alice.bsky.social '.length);
    });

    it('replaces #hashtag with selected tag', () => {
        const ctx = { type: 'hashtag' as const, query: 'by', triggerStart: 6, triggerEnd: 9 };
        const result = applyAutocompleteSelection('Check #by', ctx, 'bytescape');
        expect(result.text).toBe('Check #bytescape ');
    });

    it('preserves text after the trigger', () => {
        const ctx = { type: 'mention' as const, query: 'al', triggerStart: 0, triggerEnd: 3 };
        const result = applyAutocompleteSelection('@al world', ctx, 'alice.bsky.social');
        expect(result.text).toBe('@alice.bsky.social  world');
    });
});
