import { useState, useEffect, useRef } from 'react';
import PostCard from './PostCard';
import GroupPostCard from './GroupPostCard';
import TerminalLoader from './TerminalLoader';
import FeedSkeleton from './FeedSkeleton';
import NotificationBell from './NotificationBell';
import { INTEREST_CATEGORIES, getTotalInterestCount, MAX_INTERESTS } from '../utils/interests';
import { currentUserHandle, subscriptionApi, type FeedPost } from '../utils/api';
import { DEFAULT_WINDOW, WINDOW_OPTIONS, type WindowPref } from '../utils/feedWindow';

interface Props {
  posts: FeedPost[];
  loading: boolean;
  error: string;
  feedMode: 'aggregated' | 'discover' | 'interests' | 'combined' | 'sources' | 'rss';
  viewMode?: 'feed' | 'interests';
  loadingMore?: boolean;
  hasMore?: boolean;
  sentinelRef?: React.RefObject<HTMLDivElement | null>;
  onRetry: () => void;
  onInterestsSelected?: (ids: string[]) => Promise<void>;
  onCloseInterests?: () => void;
  activeFilters?: Set<string>;
  onClearFilters?: () => void;
  sortMode?: string;
  onSortChange?: (sort: string) => void;
  typeFilter?: string[];
  /** Selected platform bucket ('all' | bytescape | bluesky | mastodon | lemmy | pixelfed). */
  platform?: string;
  /** Bucket key → scope-wide count, delivered with the feed response. */
  platformCounts?: Record<string, number>;
  onPlatformChange?: (platform: string) => void;
  /** Time-period filter (presets + custom range), persisted by FeedLayout. */
  windowPref?: WindowPref;
  onWindowChange?: (pref: WindowPref) => void;
  /** Open the Notifications section (mobile top-cluster bell). */
  onNotifications?: () => void;
}

/** Synchronous read from localStorage cache (for initial render / fallback). */
export function loadCachedInterests(): string[] {
  try {
    const raw = localStorage.getItem('bytescape_interests');
    return raw ? JSON.parse(raw) : [];
  } catch { return []; }
}

// ── /api/profile/me single-flight ──────────────────────────────────────────
// isOnboarded() and loadSavedInterests() are two questions about ONE endpoint
// and the startup chain asks both before the seed fetch can start. Without
// sharing, every answer cost its own serial round trip before first paint.
// Single-flight + a short TTL: concurrent/recent callers share one fetch;
// cacheInterests() (the universal post-write mirror) keeps the shared cache
// coherent with interest writes so no reader resolves with pre-write data.
const PROFILE_ME_TTL_MS = 10_000;
interface ProfileMe { interests: string[]; onboarded: boolean }
let profileMeCache: ProfileMe | null = null;
let profileMeCachedAt = 0;
let profileMeInflight: Promise<ProfileMe | null> | null = null;

function fetchProfileMe(): Promise<ProfileMe | null> {
  if (profileMeCache && Date.now() - profileMeCachedAt < PROFILE_ME_TTL_MS) {
    return Promise.resolve(profileMeCache);
  }
  if (profileMeInflight) return profileMeInflight;
  const p = (async (): Promise<ProfileMe | null> => {
    try {
      const token = localStorage.getItem('bytescape_token') || '';
      const res = await fetch('/api/profile/me', {
        headers: token ? { 'Authorization': `Bearer ${token}` } : {},
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const profile = await res.json();
      const me: ProfileMe = {
        interests: profile.interests || [],
        onboarded: profile.onboarded === true,
      };
      profileMeCache = me;
      profileMeCachedAt = Date.now();
      return me;
    } catch {
      return null;
    } finally {
      profileMeInflight = null;
    }
  })();
  profileMeInflight = p;
  return p;
}

/** Mirror the authoritative interest selection into the localStorage cache. */
export function cacheInterests(ids: string[]) {
  localStorage.setItem('bytescape_interests', JSON.stringify(ids));
  localStorage.setItem('bytescape_interests_configured', 'true');
  if (profileMeCache) {
    profileMeCache = { ...profileMeCache, interests: ids };
    profileMeCachedAt = Date.now();
  }
}

/** Authoritative fetch from backend DB. Caches in localStorage. Falls back to cache on error. */
export async function loadSavedInterests(): Promise<string[]> {
  const me = await fetchProfileMe();
  if (!me) return loadCachedInterests();
  cacheInterests(me.interests);
  return me.interests;
}

/**
 * Whether this account has completed onboarding. Authoritative = the server
 * `users.onboarded` flag (survives new browsers, and is correctly reset for a
 * re-created account). Falls back to the localStorage flag only when the server
 * can't be reached. We deliberately do NOT auto-migrate a stale localStorage
 * interest cache into the account here — that seeded a freshly-created account
 * with a deleted account's interests and skipped onboarding.
 */
export async function isOnboarded(): Promise<boolean> {
  const me = await fetchProfileMe();
  if (!me) return localStorage.getItem('bytescape_interests_configured') === 'true';
  return me.onboarded;
}

export function hasConfiguredInterests(): boolean {
  return localStorage.getItem('bytescape_interests_configured') === 'true';
}

/**
 * Single batch write for an interest selection. bootstrap() is the ONE write:
 * the server sets the interests and reconciles the source groupings (removed
 * interests lose their feeds + hashtags). A separate profile PUT would run
 * first and hide the deselection delta — the server does both atomically.
 */
export async function saveInterests(ids: string[]) {
  cacheInterests(ids);
  try {
    await subscriptionApi.bootstrap(ids);
  } catch (e) {
    console.warn('Failed to persist interests to backend:', e);
  }
}

const PLATFORM_LABELS: Record<string, string> = {
  all: 'All',
  bytescape: 'Bytescape',
  bluesky: 'Bluesky',
  mastodon: 'Mastodon & friends',
  lemmy: 'Lemmy/PieFed',
  pixelfed: 'Pixelfed',
};
/** Compact trigger text — the panel keeps the full label. */
const PLATFORM_SHORT: Record<string, string> = {
  all: 'Platform',
  bytescape: 'Bytescape',
  bluesky: 'Bluesky',
  mastodon: 'Mastodon',
  lemmy: 'Lemmy',
  pixelfed: 'Pixelfed',
};
const PLATFORM_ORDER = Object.keys(PLATFORM_LABELS);

export default function FeedColumn({ posts, loading, error, feedMode, viewMode, loadingMore, hasMore, sentinelRef, onRetry, onInterestsSelected, onCloseInterests, activeFilters, onClearFilters, sortMode, onSortChange, typeFilter, platform, platformCounts, onPlatformChange, windowPref, onWindowChange, onNotifications }: Props) {
  const [configured] = useState(hasConfiguredInterests);
  const [savedIds, setSavedIds] = useState<string[]>(loadCachedInterests);
  const [expandedCat, setExpandedCat] = useState<string | null>(INTEREST_CATEGORIES[0]?.id || null);
  const [draftIds, setDraftIds] = useState<string[]>(savedIds);
  const [bootstrapping, setBootstrapping] = useState(false);

  // Mobile toolbar state. The platform selection itself is server-driven
  // (FeedLayout owns it and sends `platform=` on the feed query); this
  // component only renders the dropdown.
  const [openDropdown, setOpenDropdown] = useState<'platforms' | null>(null);
  const toolbarRef = useRef<HTMLDivElement>(null);

  // Close dropdown on click outside
  useEffect(() => {
    if (!openDropdown) return;
    const handler = (e: MouseEvent) => {
      if (toolbarRef.current && !toolbarRef.current.contains(e.target as Node)) {
        setOpenDropdown(null);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [openDropdown]);

  const selectedPlatform = platform || 'all';
  const platformLabel = PLATFORM_SHORT[selectedPlatform] || 'Platforms';
  const totalPlatformPosts = platformCounts
    ? Object.values(platformCounts).reduce((sum, n) => sum + n, 0)
    : undefined;

  // The picker opens only from explicit user actions (gear / nav) — never
  // automatically on load, regardless of cached interests or linked accounts.
  const showInterestsPicker = viewMode === 'interests';

  // Loading states (initial load, filter refetch, post-interests bootstrap)
  // no longer early-return into a bare interstitial: they fall through to the
  // normal render so the feed chrome (mobile toolbar, filter bar) paints
  // immediately and the scroll area fills with the terminal log + skeleton
  // cards instead of nothing (owner QA 2026-11-17 — no stall/null states).
  // Precedence below preserves the old ordering: bootstrapping > loading >
  // error > picker; the scroll content switch mirrors it.
  if (error && !bootstrapping) {
    return (
      <div className="feed-column">
        <div className="feed-column-header"><h1>FEED</h1><p>Connection error</p></div>
        <div className="feed-column-scroll">
          <div className="empty-feed">
            <h2>CONNECTION ERROR</h2><p>{error}</p>
            <button className="btn" onClick={onRetry} style={{ marginTop: 12 }}>{'>'} RETRY</button>
          </div>
        </div>
      </div>
    );
  }

  // Interest picker (initial onboarding or explicit edit mode)
  if (showInterestsPicker && !bootstrapping) {
    const editing = configured;
    return (
      <div className="feed-column">
        <div className="feed-column-header">
          <h1>{editing ? 'EDIT INTERESTS' : 'WELCOME'}</h1>
          <p>{editing ? 'Modify your interests to change your feed.' : 'Pick some interests to populate your feed. You can change these anytime.'}</p>
        </div>
        <div className="feed-column-scroll">
          <div className="inline-interest-picker">
            {INTEREST_CATEGORIES.map(cat => (
              <div key={cat.id} className="onboarding-cat">
                <div
                  className={`onboarding-cat-header ${expandedCat === cat.id ? 'expanded' : ''}`}
                  onClick={() => setExpandedCat(expandedCat === cat.id ? null : cat.id)}
                >
                  <span className="onboarding-cat-label">{cat.label}</span>
                  <span className="onboarding-cat-count">
                    {draftIds.filter(id => cat.interests.some(i => i.id === id)).length}/{cat.interests.length}
                  </span>
                  <span className={`onboarding-cat-arrow ${expandedCat === cat.id ? 'expanded' : ''}`}>▶</span>
                </div>
                {expandedCat === cat.id && (
                  <div className="onboarding-cat-interests">
                    {cat.interests.map(interest => {
                      const sel = draftIds.includes(interest.id);
                      return (
                        <span
                          key={interest.id}
                          className={`chip ${sel ? 'selected' : ''}`}
                          onClick={() => setDraftIds(prev =>
                            prev.includes(interest.id)
                              ? prev.filter(id => id !== interest.id)
                              : [...prev, interest.id]
                          )}
                        >
                          {interest.emoji} {interest.label}
                        </span>
                      );
                    })}
                  </div>
                )}
              </div>
            ))}
          </div>
          <div className="onboarding-footer">
            <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>
              {draftIds.length} of {getTotalInterestCount()} selected · max {MAX_INTERESTS}
            </span>
            <button
              className="btn"
              disabled={bootstrapping}
              onClick={async () => {
                if (!onInterestsSelected) return;
                setBootstrapping(true);
                const limited = draftIds.slice(0, MAX_INTERESTS);
                try {
                  // Batch write: PUT /api/profile/me {interests} → bootstrap subscriptions
                  await saveInterests(limited);
                  setSavedIds(limited);
                  // Refresh feed + subscriptions, then close the picker
                  await onInterestsSelected(limited);
                } catch {
                  // Keep the picker open for retry / cancel.
                } finally {
                  // Every exit path must clear this — a throw from
                  // saveInterests (once outside the try) stranded the user
                  // on the bootstrapping screen with no way out.
                  setBootstrapping(false);
                }
              }}
              style={{ marginLeft: 16 }}
            >
              {draftIds.length > 0 ? `> FOLLOW ${draftIds.length} INTERESTS` : '> SAVE'}
            </button>
            {editing && (
              <button
                className="btn"
                onClick={() => { onCloseInterests?.(); setDraftIds([...savedIds]); }}
                style={{ marginLeft: 8 }}
              >
                CANCEL
              </button>
            )}
          </div>
        </div>
      </div>
    );
  }

  // Empty-feed copy. Kept on the normal render path (not an early return) so
  // the mobile toolbar survives an empty result — a platform bucket with 0
  // posts must stay switchable back to All.
  // A type filter with no matches is not a sources problem — say so instead
  // of pointing at accounts/interests.
  const emptyMsg = selectedPlatform !== 'all'
    ? `No ${PLATFORM_LABELS[selectedPlatform] || selectedPlatform} posts in this feed yet \u{2014} try Platforms: All.`
    : typeFilter && typeFilter.length > 0
    ? 'No content of the selected type in your feed yet \u{2014} try \u{2728} All content.'
    : feedMode === 'rss'
    ? 'No RSS items yet. Add RSS feeds from Accounts.'
    : feedMode === 'combined'
    ? 'Connect more accounts or add interests to populate your feed.'
    : 'Content matching your interests will appear here once we connect to sources.';

  return (
    <div className="feed-column">
      <div className="feed-column-header feed-chrome-mobile">
        <div className="feed-toolbar" ref={toolbarRef}>
          {/* Interests gear and sort moved to the action bar (2026-10-14) */}

          {/* ── Mobile toolbar (shown only at ≤550px) ── */}
          <div className="feed-mobile-toolbar">
            {/* Platforms dropdown — mix counts + single-select bucket filter */}
            <div className="feed-dropdown-wrapper">
              <button
                className={`feed-dropdown-btn${openDropdown === 'platforms' ? ' open' : ''}${selectedPlatform !== 'all' ? ' filtered' : ''}`}
                onClick={() => setOpenDropdown(openDropdown === 'platforms' ? null : 'platforms')}
                aria-haspopup="listbox"
                aria-expanded={openDropdown === 'platforms'}
              >
                {selectedPlatform === 'all' ? 'Platform' : platformLabel} <span className="feed-dropdown-arrow">▼</span>
              </button>
              {openDropdown === 'platforms' && (
                <div className="feed-dropdown-panel" role="listbox" aria-label="Filter feed by platform">
                  {PLATFORM_ORDER.map((key) => {
                    const selected = selectedPlatform === key;
                    const count = key === 'all' ? totalPlatformPosts : platformCounts?.[key];
                    return (
                      <button
                        key={key}
                        type="button"
                        role="option"
                        aria-selected={selected}
                        className={`feed-dropdown-item feed-dropdown-option${selected ? ' selected' : ''}${key === 'all' ? ' feed-dropdown-all' : ''}`}
                        onClick={() => {
                          onPlatformChange?.(key);
                          setOpenDropdown(null);
                        }}
                      >
                        <span className="feed-dropdown-check">{selected ? '✓' : ''}</span>
                        <span className="feed-dropdown-label">{PLATFORM_LABELS[key]}</span>
                        <span className="feed-dropdown-count">{count ?? 0}</span>
                      </button>
                    );
                  })}
                </div>
              )}
            </div>

            {/* Popular / New toggle */}
            <div className="feed-sort-toggle">
              <button
                className={`feed-sort-btn${(sortMode || 'popular') === 'popular' ? ' active' : ''}`}
                onClick={() => {
                  const current = sortMode || 'popular';
                  if (current !== 'popular') {
                    localStorage.setItem('bytescape_feed_sort', 'popular');
                    onSortChange?.('popular');
                  }
                }}
              >Popular</button>
              <button
                className={`feed-sort-btn${(sortMode || 'popular') === 'new' ? ' active' : ''}`}
                onClick={() => {
                  const current = sortMode || 'popular';
                  if (current !== 'new') {
                    localStorage.setItem('bytescape_feed_sort', 'new');
                    onSortChange?.('new');
                  }
                }}
              >New</button>
            </div>

            {/* Time period — filters first, then the sort above orders inside
                the window. Neutral default keeps today's unfiltered feed. */}
            <select
              className="feed-window-select feed-window-select-mobile"
              value={windowPref?.preset || DEFAULT_WINDOW.preset}
              onChange={(e) => onWindowChange?.({ ...(windowPref || DEFAULT_WINDOW), preset: e.target.value as WindowPref['preset'] })}
              title="Time period"
              aria-label="Time period"
            >
              {WINDOW_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>

            {/* New post — the same action as the desktop action-bar button,
                pinned right so the mobile header is one row. */}
            <button
              className="feed-compose-btn feed-compose-btn-mobile"
              onClick={() => window.dispatchEvent(new Event('bytescape:openComposer'))}
              title="New post"
            >
              ✎ Post
            </button>

            {/* Owner QA 2026-11-17: the bell joins the mobile top cluster at
                its right end (the desktop placement is the search row). */}
            <NotificationBell onOpenAll={() => onNotifications?.()} />
          </div>
        </div>
        {windowPref?.preset === 'custom' && (
          <div className="feed-window-custom feed-window-custom-mobile">
            <input
              type="date"
              aria-label="From date"
              value={windowPref.from}
              onChange={(e) => onWindowChange?.({ ...windowPref, from: e.target.value })}
            />
            <span className="feed-window-custom-sep">–</span>
            <input
              type="date"
              aria-label="To date"
              value={windowPref.to}
              onChange={(e) => onWindowChange?.({ ...windowPref, to: e.target.value })}
            />
          </div>
        )}
        {activeFilters && activeFilters.size > 0 && (
          <div className="active-filters-bar">
            {Array.from(activeFilters).map(filter => {
              const [type, ...rest] = filter.split(':');
              const id = rest.join(':');
              const displayLabel = type === 'hashtag' ? `#${id}`
                : type === 'follows' ? `@${id.split('/').pop() || id}`
                : id.length > 20 ? id.slice(0, 20) + '...' : id;
              return (
                <span key={filter} className="active-filter-chip" onClick={() => onClearFilters?.()}>
                  {displayLabel}
                  <span className="filter-chip-x">✕</span>
                </span>
              );
            })}
            <button className="clear-filters-btn" onClick={() => onClearFilters?.()}>Clear all</button>
          </div>
        )}
      </div>
      <div className="feed-column-scroll">
        <div className="post-list">
            {bootstrapping ? (
              <div className="empty-feed">
                <h2>SETTING UP YOUR FEED</h2>
                <p>Subscribing to hashtags and feeds for your selected interests...</p>
              </div>
            ) : loading ? (
              <>
                <TerminalLoader sources={savedIds} />
                <FeedSkeleton />
              </>
            ) : posts.length === 0 ? (
              <div className="empty-feed">
                <h2>NO CONTENT YET</h2>
                <p>{emptyMsg}</p>
              </div>
            ) : posts.map(post => (
              post.group ? (
                <GroupPostCard
                  key={post.id}
                  post={post}
                  group={post.group}
                  currentUserHandle={currentUserHandle() || undefined}
                />
              ) : (
                <PostCard key={post.id} post={post} currentUserHandle={currentUserHandle() || undefined} />
              )
            ))}
          {loadingMore && <div className="loading-more">Loading more...</div>}
          <div ref={sentinelRef} style={{ height: 1 }} />
          {hasMore && !loadingMore && posts.length > 0 && (
            <button
              className="btn load-more-btn"
              style={{ display: 'block', margin: '12px auto' }}
              onClick={() => window.dispatchEvent(new CustomEvent('bytescape:loadMore'))}
            >
              {'>'} LOAD MORE POSTS
            </button>
          )}
          {loadingMore && (
            <div style={{ textAlign: 'center', padding: '8px', color: 'var(--text-muted)', fontSize: 13 }}>
              Loading more posts...
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
