import { describe, it, expect } from 'vitest';
import {
    countGraphemes,
    computeCharacterCounts,
    resolveTargets,
    TARGET_LIMITS,
} from './useCharacterCount';
import type { CountTarget } from './useCharacterCount';

describe('countGraphemes', () => {
    it('counts ASCII characters', () => {
        expect(countGraphemes('Hello')).toBe(5);
    });

    it('counts empty string as 0', () => {
        expect(countGraphemes('')).toBe(0);
    });

    it('counts single emoji as 1 grapheme', () => {
        expect(countGraphemes('😀')).toBe(1);
    });

    it('counts ZWJ family emoji as 1 grapheme', () => {
        // 👨‍👩‍👧‍👦 is a ZWJ sequence — should be 1 grapheme
        expect(countGraphemes('👨‍👩‍👧‍👦')).toBe(1);
    });

    it('counts flag emoji as 1 grapheme', () => {
        expect(countGraphemes('🇺🇸')).toBe(1);
    });

    it('counts mixed ASCII + emoji', () => {
        expect(countGraphemes('Hi 👋')).toBe(4); // H, i, space, 👋
    });

    it('counts CJK characters as 1 grapheme each', () => {
        expect(countGraphemes('日本語')).toBe(3);
    });

    it('counts skin-toned emoji as 1 grapheme', () => {
        expect(countGraphemes('👍🏽')).toBe(1);
    });

    it('counts keycap sequences as 1 grapheme', () => {
        expect(countGraphemes('#️⃣')).toBe(1);
    });

    it('handles long ZWJ sequences', () => {
        // 👩‍❤️‍💋‍👨
        expect(countGraphemes('👩‍❤️‍💋‍👨')).toBe(1);
    });
});

describe('resolveTargets', () => {
    it('returns local only for "local"', () => {
        expect(resolveTargets('local')).toEqual(['local']);
    });

    it('returns bluesky + local for "bluesky"', () => {
        expect(resolveTargets('bluesky')).toEqual(['bluesky', 'local']);
    });

    it('returns activitypub + local for "activitypub"', () => {
        expect(resolveTargets('activitypub')).toEqual(['activitypub', 'local']);
    });

    it('returns all targets for "all"', () => {
        expect(resolveTargets('all')).toEqual(['bluesky', 'activitypub', 'local']);
    });

    it('defaults to local for unknown target', () => {
        expect(resolveTargets('foobar')).toEqual(['local']);
    });
});

describe('computeCharacterCounts', () => {
    it('returns ok status for short text', () => {
        const result = computeCharacterCounts('Hello', ['bluesky']);
        expect(result.counts).toHaveLength(1);
        expect(result.counts[0].target).toBe('bluesky');
        expect(result.counts[0].current).toBe(5);
        expect(result.counts[0].limit).toBe(300);
        expect(result.counts[0].status).toBe('ok');
        expect(result.counts[0].overBy).toBe(0);
        expect(result.allOver).toBe(false);
        expect(result.anyOver).toBe(false);
    });

    it('returns warn at 80-95% of limit', () => {
        const text = 'a'.repeat(250); // 250/300 = 83%
        const result = computeCharacterCounts(text, ['bluesky']);
        expect(result.counts[0].status).toBe('warn');
    });

    it('returns danger at 95-100%', () => {
        const text = 'a'.repeat(290); // 290/300 = 96.7%
        const result = computeCharacterCounts(text, ['bluesky']);
        expect(result.counts[0].status).toBe('danger');
    });

    it('returns over when exceeding limit', () => {
        const text = 'a'.repeat(310);
        const result = computeCharacterCounts(text, ['bluesky']);
        expect(result.counts[0].status).toBe('over');
        expect(result.counts[0].overBy).toBe(10);
        expect(result.anyOver).toBe(true);
    });

    it('uses grapheme counting for bluesky', () => {
        // 300 emoji should count as 300 graphemes for Bluesky but many more chars
        const text = '😀'.repeat(300);
        const result = computeCharacterCounts(text, ['bluesky']);
        expect(result.counts[0].current).toBe(300);
        expect(result.counts[0].status).toBe('danger'); // 300/300 = 100%, ≤100% but > 95%
    });

    it('uses character length for activitypub', () => {
        const text = '😀'.repeat(250);
        const result = computeCharacterCounts(text, ['activitypub']);
        // Each emoji is 2 chars in JS string — 250 * 2 = 500
        expect(result.counts[0].current).toBe(500);
        expect(result.counts[0].limit).toBe(500);
    });

    it('multi-target: bluesky over but local ok', () => {
        const text = 'a'.repeat(350); // Over bluesky (300) but ok for local (10K)
        const result = computeCharacterCounts(text, ['bluesky', 'local']);
        expect(result.counts).toHaveLength(2);

        const bsky = result.counts.find(c => c.target === 'bluesky')!;
        const local = result.counts.find(c => c.target === 'local')!;

        expect(bsky.status).toBe('over');
        expect(bsky.overBy).toBe(50);
        expect(local.status).toBe('ok');

        expect(result.allOver).toBe(false);
        expect(result.anyOver).toBe(true);
        expect(result.truncatedTargets).toEqual(['bluesky']);
    });

    it('allOver is true when every target exceeds', () => {
        // Over 10K for local
        const text = 'a'.repeat(10_001);
        const result = computeCharacterCounts(text, ['bluesky', 'local'] as CountTarget[]);
        expect(result.allOver).toBe(true);
    });

    it('handles empty text', () => {
        const result = computeCharacterCounts('', ['bluesky', 'local']);
        expect(result.counts[0].current).toBe(0);
        expect(result.counts[0].status).toBe('ok');
        expect(result.allOver).toBe(false);
        expect(result.anyOver).toBe(false);
    });

    it('ZWJ emoji counts as 1 for bluesky but more for AP', () => {
        // 👨‍👩‍👧‍👦 is 1 grapheme but 11 chars (7 code points in JS)
        const text = '👨‍👩‍👧‍👦';
        const bskyResult = computeCharacterCounts(text, ['bluesky']);
        const apResult = computeCharacterCounts(text, ['activitypub']);

        expect(bskyResult.counts[0].current).toBe(1); // 1 grapheme
        expect(apResult.counts[0].current).toBeGreaterThan(1); // multiple JS chars
    });
});

describe('TARGET_LIMITS', () => {
    it('has correct Bluesky limit', () => {
        expect(TARGET_LIMITS.bluesky).toBe(300);
    });

    it('has correct AP limit', () => {
        expect(TARGET_LIMITS.activitypub).toBe(500);
    });

    it('has correct local limit', () => {
        expect(TARGET_LIMITS.local).toBe(10_000);
    });
});
