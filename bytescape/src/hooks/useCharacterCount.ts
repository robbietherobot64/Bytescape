import { useMemo } from 'react';

/** Per-target character limits */
export const TARGET_LIMITS = {
    bluesky: 300,     // 300 graphemes
    mastodon: 500,    // 500 characters (configurable per-instance, default 500)
    activitypub: 500, // 500 characters (configurable per-server, default 500)
    local: 10_000,    // 10K characters (MED-4 limit)
} as const;

export type CountTarget = keyof typeof TARGET_LIMITS;

export interface TargetCount {
    target: CountTarget;
    current: number;
    limit: number;
    remaining: number;
    /** 'ok' (0-80%), 'warn' (80-95%), 'danger' (95-100%), 'over' (>100%) */
    status: 'ok' | 'warn' | 'danger' | 'over';
    overBy: number;
}

export interface CharacterCountResult {
    counts: TargetCount[];
    /** True if ALL targets are over limit — post should be fully disabled */
    allOver: boolean;
    /** True if ANY target is over limit — show truncation warning */
    anyOver: boolean;
    /** Targets that will truncate the post */
    truncatedTargets: CountTarget[];
}

/**
 * Count graphemes in a string using Intl.Segmenter.
 * Bluesky uses grapheme clusters — emoji like 👨‍👩‍👧‍👦 count as 1.
 * Falls back to string length if Intl.Segmenter is not available.
 */
export function countGraphemes(text: string): number {
    if (typeof Intl !== 'undefined' && typeof Intl.Segmenter === 'function') {
        const segmenter = new Intl.Segmenter('en', { granularity: 'grapheme' });
        let count = 0;
        for (const _seg of segmenter.segment(text)) {
            count++;
        }
        return count;
    }
    // Fallback: spread into array to handle surrogate pairs (not perfect for ZWJ)
    return [...text].length;
}

function getStatus(current: number, limit: number): TargetCount['status'] {
    const ratio = current / limit;
    if (ratio > 1) return 'over';
    if (ratio > 0.95) return 'danger';
    if (ratio > 0.80) return 'warn';
    return 'ok';
}

/** Map a PostTarget selection (comma-separated platforms) to the set of CountTargets that apply */
export function resolveTargets(postTarget: string): CountTarget[] {
    const parts = postTarget.split(',').map(s => s.trim().toLowerCase());
    const targets = new Set<CountTarget>();

    for (const part of parts) {
        switch (part) {
            case 'all':
                targets.add('bluesky');
                targets.add('mastodon');
                targets.add('activitypub');
                targets.add('local');
                break;
            case 'bluesky':
                targets.add('bluesky');
                break;
            case 'mastodon':
                targets.add('mastodon');
                break;
            case 'activitypub':
                targets.add('activitypub');
                break;
            case 'local':
                targets.add('local');
                break;
        }
    }

    if (targets.size === 0) targets.add('local');
    return Array.from(targets);
}

/**
 * Compute character/grapheme counts for the given text against each active target.
 * Pure function — can be used outside React.
 */
export function computeCharacterCounts(text: string, targets: CountTarget[]): CharacterCountResult {
    const counts: TargetCount[] = targets.map(target => {
        // Bluesky uses grapheme counting; others use character length
        const current = target === 'bluesky' ? countGraphemes(text) : text.length;
        const limit = TARGET_LIMITS[target];
        const remaining = limit - current;
        const overBy = Math.max(0, current - limit);
        return {
            target,
            current,
            limit,
            remaining,
            status: getStatus(current, limit),
            overBy,
        };
    });

    const allOver = counts.length > 0 && counts.every(c => c.status === 'over');
    const anyOver = counts.some(c => c.status === 'over');
    const truncatedTargets = counts.filter(c => c.status === 'over').map(c => c.target);

    return { counts, allOver, anyOver, truncatedTargets };
}

/** Target display icons */
export const TARGET_ICONS: Record<CountTarget, string> = {
    bluesky: '🦋',
    mastodon: '🐘',
    activitypub: '🌐',
    local: '💾',
};

/** Status-to-color mapping */
export const STATUS_COLORS: Record<TargetCount['status'], string> = {
    ok: 'var(--text-muted, #888)',
    warn: '#f39c12',
    danger: '#e67e22',
    over: '#e74c3c',
};

/**
 * React hook for platform-aware character counting in ComposeBox.
 * Returns per-target counts with status colors and truncation warnings.
 */
export function useCharacterCount(text: string, postTarget: string): CharacterCountResult {
    return useMemo(() => {
        const targets = resolveTargets(postTarget);
        return computeCharacterCounts(text, targets);
    }, [text, postTarget]);
}
