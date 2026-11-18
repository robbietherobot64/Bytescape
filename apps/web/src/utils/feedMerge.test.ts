import { describe, expect, it } from 'vitest';
import type { FeedPost } from './api';
import { maxCreatedAtMs, parseCreatedAt, planFeedMerge } from './feedMerge';

function post(id: string, created_at: string): FeedPost {
  return { id, created_at } as FeedPost;
}

describe('parseCreatedAt', () => {
  it('parses RFC3339 with Z and with numeric offsets to the same instant', () => {
    expect(parseCreatedAt('2026-11-17T10:00:00Z')).toBe(
      parseCreatedAt('2026-11-17T12:00:00+02:00'),
    );
  });

  it('returns null for garbage instead of a NaN comparison', () => {
    expect(parseCreatedAt('not-a-date')).toBeNull();
    expect(parseCreatedAt('')).toBeNull();
  });
});

describe('planFeedMerge', () => {
  const t = (iso: string) => parseCreatedAt(iso)!;

  it('drops already-loaded posts and intra-batch duplicates everywhere', () => {
    const loaded = new Set(['a', 'dup']);
    const plan = planFeedMerge(
      [post('a', '2026-11-17T10:00:00Z'), post('dup', '2026-11-17T09:00:00Z'), post('x', '2026-11-17T08:00:00Z'), post('x', '2026-11-17T08:00:00Z')],
      loaded,
      t('2026-11-17T10:00:00Z'),
      'new',
      'live',
    );
    expect(plan.atTop).toEqual([]);
    expect(plan.held).toEqual([]);
    expect(plan.append.map((p) => p.id)).toEqual(['x']);
  });

  it('New sort, live batch: arrivals go to the top, older posts append below', () => {
    const plan = planFeedMerge(
      [
        post('newer', '2026-11-17T11:00:00Z'),
        post('older', '2026-11-17T09:00:00Z'),
        post('same-instant', '2026-11-17T10:00:00Z'),
      ],
      new Set(['loaded']),
      t('2026-11-17T10:00:00Z'),
      'new',
      'live',
    );
    expect(plan.atTop.map((p) => p.id)).toEqual(['newer']);
    expect(plan.append.map((p) => p.id)).toEqual(['older', 'same-instant']);
    expect(plan.held).toEqual([]);
  });

  it('Popular sort, live batch: the WHOLE unseen batch is held back, never injected', () => {
    const plan = planFeedMerge(
      [post('fresh', '2026-11-17T11:00:00Z'), post('below', '2026-11-17T09:00:00Z')],
      new Set(),
      t('2026-11-17T10:00:00Z'),
      'popular',
      'live',
    );
    expect(plan.held.map((p) => p.id)).toEqual(['fresh', 'below']);
    expect(plan.atTop).toEqual([]);
    expect(plan.append).toEqual([]);
  });

  it('Popular sort, cursor page: continuation appends below (pagination never stalls)', () => {
    const plan = planFeedMerge(
      [post('p1', '2026-11-17T09:00:00Z'), post('p2', '2026-11-17T08:00:00Z')],
      new Set(),
      t('2026-11-17T10:00:00Z'),
      'popular',
      'page',
    );
    expect(plan.append.map((p) => p.id)).toEqual(['p1', 'p2']);
    expect(plan.held).toEqual([]);
    expect(plan.atTop).toEqual([]);
  });

  it('New sort, cursor page: newer-than-top still routes to the top', () => {
    const plan = planFeedMerge(
      [post('stray', '2026-11-17T11:00:00Z'), post('old', '2026-11-17T07:00:00Z')],
      new Set(),
      t('2026-11-17T10:00:00Z'),
      'new',
      'page',
    );
    expect(plan.atTop.map((p) => p.id)).toEqual(['stray']);
    expect(plan.append.map((p) => p.id)).toEqual(['old']);
    expect(plan.held).toEqual([]);
  });

  it('treats unknown sort modes like Popular (live batch held back)', () => {
    const plan = planFeedMerge(
      [post('fresh', '2026-11-17T11:00:00Z')],
      new Set(),
      t('2026-11-17T10:00:00Z'),
      undefined,
      'live',
    );
    expect(plan.held.map((p) => p.id)).toEqual(['fresh']);
    expect(plan.atTop).toEqual([]);
    expect(plan.append).toEqual([]);
  });

  it('on an empty feed every parseable post is an arrival; unparseable ones append', () => {
    const plan = planFeedMerge(
      [post('ok', '2026-11-17T11:00:00Z'), post('bad', 'garbage')],
      new Set(),
      null,
      'new',
      'live',
    );
    expect(plan.atTop.map((p) => p.id)).toEqual(['ok']);
    expect(plan.append.map((p) => p.id)).toEqual(['bad']);
    expect(plan.held).toEqual([]);
  });

  it('compares timestamps as instants, not strings (offset formats interleave)', () => {
    // '2026-11-17T11:00:00+02:00' is 09:00Z — strictly OLDER than the
    // loaded top 10:00Z despite its raw string reading "later".
    const plan = planFeedMerge(
      [post('offset', '2026-11-17T11:00:00+02:00')],
      new Set(),
      t('2026-11-17T10:00:00Z'),
      'new',
      'live',
    );
    expect(plan.atTop).toEqual([]);
    expect(plan.append.map((p) => p.id)).toEqual(['offset']);
  });

  it('keeps server order within each bucket', () => {
    const plan = planFeedMerge(
      [
        post('n2', '2026-11-17T12:30:00Z'),
        post('n1', '2026-11-17T12:00:00Z'),
        post('o2', '2026-11-17T08:00:00Z'),
        post('o1', '2026-11-17T07:00:00Z'),
      ],
      new Set(),
      t('2026-11-17T10:00:00Z'),
      'new',
      'live',
    );
    expect(plan.atTop.map((p) => p.id)).toEqual(['n2', 'n1']);
    expect(plan.append.map((p) => p.id)).toEqual(['o2', 'o1']);
  });
});

describe('maxCreatedAtMs', () => {
  it('is the newest parseable instant, ignoring garbage', () => {
    const posts = [post('a', '2026-11-17T08:00:00Z'), post('b', 'nope'), post('c', '2026-11-17T09:00:00Z')];
    expect(maxCreatedAtMs(posts)).toBe(parseCreatedAt('2026-11-17T09:00:00Z'));
    expect(maxCreatedAtMs([])).toBeNull();
    expect(maxCreatedAtMs([post('b', 'nope')])).toBeNull();
  });
});
