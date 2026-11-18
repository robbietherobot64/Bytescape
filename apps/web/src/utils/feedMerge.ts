/**
 * Sort-aware merge planning for feed batches (initial load vs live refresh).
 *
 * Every batch arriving after the first page goes through `planFeedMerge`
 * before it touches the visible list. The plan keeps the active sort honest:
 *
 * - `atTop`: live arrivals under New sort — insert above the loaded list.
 * - `append`: pagination continuation — append below the loaded list. These
 *   come from cursor pages (server-guaranteed older-than-cursor) or from the
 *   older tail of a live batch under New sort, so appending never disturbs
 *   the loaded order under either sort.
 * - `held`: the unseen part of a LIVE batch under Popular sort — never
 *   injected into the ranked list; surfaced via the "N new posts" affordance
 *   instead (click refreshes/re-sorts).
 *
 * Already-loaded posts (by id) and intra-batch duplicates are dropped in all
 * three buckets, so nothing can render twice.
 *
 * The batch's *kind* matters as much as its timestamps: the two-phase
 * loader's "full" fetch is a live refresh (fresh content re-ranked), while
 * cursor fetches are pagination. Under Popular the whole unseen live batch is
 * "the new batch" the reader must be told about — rank-ordered pagination is
 * too leaky to silently splice it into the visible list.
 */
import type { FeedPost } from './api';

/** What kind of fetch produced the batch. */
export type FeedMergeMode = 'live' | 'page';

export interface FeedMergePlan {
  /** Insert at the very top of the visible list (New-sort live arrivals). */
  atTop: FeedPost[];
  /** Append below the loaded list (pagination continuation). */
  append: FeedPost[];
  /** Hold back from the list (Popular-sort live batch). */
  held: FeedPost[];
}

/** Parse a feed timestamp; returns null when unparseable. */
export function parseCreatedAt(iso: string): number | null {
  const ms = Date.parse(iso);
  return Number.isFinite(ms) ? ms : null;
}

/**
 * Partition `incoming` against what is already loaded.
 *
 * @param incoming   newly fetched batch, in server sort order (kept per bucket)
 * @param loadedIds  ids of every post already in the visible list
 * @param maxLoadedMs newest `created_at` among loaded posts, or null if the
 *                   list is empty (then every valid post counts as "newer")
 * @param sortMode   active sort ('new' | 'popular' | anything else = popular)
 * @param mode       'live' for the two-phase loader's full/live-refresh
 *                   batches; 'page' for cursor pagination batches
 */
export function planFeedMerge(
  incoming: FeedPost[],
  loadedIds: ReadonlySet<string>,
  maxLoadedMs: number | null,
  sortMode: string | undefined,
  mode: FeedMergeMode,
): FeedMergePlan {
  const plan: FeedMergePlan = { atTop: [], append: [], held: [] };
  const seen = new Set<string>();
  const atTopCapable = sortMode === 'new';

  for (const post of incoming) {
    if (loadedIds.has(post.id) || seen.has(post.id)) continue;
    seen.add(post.id);

    const ts = parseCreatedAt(post.created_at);
    // Unparseable timestamps are treated as "not newer": they may never jump
    // to the top of the list on a guess.
    const isNewer = ts !== null && (maxLoadedMs === null || ts > maxLoadedMs);

    if (isNewer && atTopCapable) {
      plan.atTop.push(post);
    } else if (mode === 'live' && !atTopCapable) {
      // Popular sort: the whole unseen live batch is "new posts" — held back
      // for the affordance so the ranked list is never silently reshuffled.
      plan.held.push(post);
    } else {
      plan.append.push(post);
    }
  }

  return plan;
}

/** Newest parseable `created_at` in ms across `posts`; null when none parse. */
export function maxCreatedAtMs(posts: FeedPost[]): number | null {
  let max: number | null = null;
  for (const p of posts) {
    const ts = parseCreatedAt(p.created_at);
    if (ts !== null && (max === null || ts > max)) max = ts;
  }
  return max;
}
