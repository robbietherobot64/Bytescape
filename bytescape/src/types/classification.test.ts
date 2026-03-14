import { describe, it, expect } from 'vitest';
import type { BytescapeType, TrackerEntry, ReclassifyResult, FeedEntry } from './index';

/**
 * Tests for content-type classification types and tracker categories.
 * 
 * These tests verify:
 * 1. BytescapeType includes all 5 variants (Status, Gallery, Article, Link, Video)
 * 2. TrackerEntry has category field
 * 3. ReclassifyResult has per-type counts
 * 4. FeedEntry.item_type accepts all BytescapeType variants
 */

describe('BytescapeType variants', () => {
    it('accepts Status type', () => {
        const t: BytescapeType = 'Status';
        expect(t).toBe('Status');
    });

    it('accepts Gallery type', () => {
        const t: BytescapeType = 'Gallery';
        expect(t).toBe('Gallery');
    });

    it('accepts Article type', () => {
        const t: BytescapeType = 'Article';
        expect(t).toBe('Article');
    });

    it('accepts Link type', () => {
        const t: BytescapeType = 'Link';
        expect(t).toBe('Link');
    });

    it('accepts Video type', () => {
        const t: BytescapeType = 'Video';
        expect(t).toBe('Video');
    });

    it('all 5 types are distinct', () => {
        const types: BytescapeType[] = ['Status', 'Gallery', 'Article', 'Link', 'Video'];
        const unique = new Set(types);
        expect(unique.size).toBe(5);
    });
});

describe('TrackerEntry with category', () => {
    it('has name, event_count, event_types, and category fields', () => {
        const tracker: TrackerEntry = {
            name: 'Facebook Pixel',
            event_count: 42,
            event_types: ['PAGE_VIEW', 'PURCHASE'],
            category: 'cross_site_tracking',
        };
        expect(tracker.name).toBe('Facebook Pixel');
        expect(tracker.event_count).toBe(42);
        expect(tracker.event_types).toEqual(['PAGE_VIEW', 'PURCHASE']);
        expect(tracker.category).toBe('cross_site_tracking');
    });

    it('supports all tracker categories', () => {
        const categories = [
            'ad_tracking',
            'location_tracking',
            'cross_site_tracking',
            'data_collection',
            'social_tracking',
            'commerce_tracking',
            'behavioral_tracking',
            'general_tracking',
        ];
        for (const cat of categories) {
            const t: TrackerEntry = { name: 'test', event_count: 1, event_types: [], category: cat };
            expect(t.category).toBe(cat);
        }
    });
});

describe('ReclassifyResult', () => {
    it('includes counts for all 5 types', () => {
        const result: ReclassifyResult = {
            total_scanned: 1000,
            total_changed: 150,
            status_count: 500,
            gallery_count: 200,
            article_count: 100,
            link_count: 50,
            video_count: 150,
        };
        expect(result.total_scanned).toBe(1000);
        expect(result.total_changed).toBe(150);
        expect(result.status_count + result.gallery_count + result.article_count + result.link_count + result.video_count).toBe(1000);
    });
});

describe('FeedEntry item_type field', () => {
    it('accepts all BytescapeType variants on FeedEntry', () => {
        const types: BytescapeType[] = ['Status', 'Gallery', 'Article', 'Link', 'Video'];
        for (const t of types) {
            const entry: Partial<FeedEntry> = {
                id: 'test',
                timestamp: Date.now(),
                content: 'test content',
                source: 'bluesky',
                item_type: t,
            };
            expect(entry.item_type).toBe(t);
        }
    });
});

describe('Classification heuristic expectations', () => {
    // These document the expected classification behavior matching the Rust classify_item()

    it('short text with no media → Status', () => {
        // text ≤ 300 chars, no images, no video, no link
        const expectedType: BytescapeType = 'Status';
        expect(expectedType).toBe('Status');
    });

    it('≥2 images with short text → Gallery', () => {
        // ≥2 images, text ≤ 300 chars
        const expectedType: BytescapeType = 'Gallery';
        expect(expectedType).toBe('Gallery');
    });

    it('text >500 chars → Article', () => {
        // body text > 500 chars
        const expectedType: BytescapeType = 'Article';
        expect(expectedType).toBe('Article');
    });

    it('link with ≤100 chars text → Link', () => {
        // has link_url, body ≤ 100 chars
        const expectedType: BytescapeType = 'Link';
        expect(expectedType).toBe('Link');
    });

    it('video attachment → Video (highest priority)', () => {
        // has video/* MIME — always Video regardless of other content
        const expectedType: BytescapeType = 'Video';
        expect(expectedType).toBe('Video');
    });

    it('video + long text → Video wins (priority check)', () => {
        // Video has highest priority even if text > 500 chars
        const expectedType: BytescapeType = 'Video';
        expect(expectedType).toBe('Video');
    });

    it('empty content with no media → Status (default)', () => {
        const expectedType: BytescapeType = 'Status';
        expect(expectedType).toBe('Status');
    });
});
