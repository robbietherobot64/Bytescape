/**
 * Widget System Registry (B2.4)
 *
 * Maps GrapesJS block CSS classes to their corresponding React widget components.
 * Used by ProfileRenderer to hydrate widget placeholders with live components.
 *
 * Cross-system note:
 * - CSS class hooks originate from GrapesJS-authored profile markup.
 * - Widget type identifiers are consumed by frontend hydration orchestration.
 * - Component exports here are the canonical registry surface for profile widget rendering.
 */

// -----------------------------------------------------------------------------
// Widget component exports
// -----------------------------------------------------------------------------

/** Export: away-message widget component. */
export { AwayMessageWidget } from './AwayMessageWidget';
/** Export: music-player widget component. */
export { MusicPlayerWidget } from './MusicPlayerWidget';
/** Export: friends-list widget component. */
export { FriendsListWidget } from './FriendsListWidget';
/** Export: guestbook widget component. */
export { GuestbookWidget } from './GuestbookWidget';
/** Export: visitor-counter widget component. */
export { VisitorCounterWidget } from './VisitorCounterWidget';
/** Export: post-feed widget component. */
export { PostFeedWidget } from './PostFeedWidget';
/** Export: media-grid widget component. */
export { MediaGridWidget } from './MediaGridWidget';

/**
 * CSS class → widget type mapping.
 * These classes are applied by GrapesJS blocks in ProfileBuilderScreen.
 *
 * Keep this map synchronized with:
 * 1) GrapesJS block class names
 * 2) Profile renderer hydration selectors
 * 3) Widget type routing logic
 */
export const WIDGET_CLASS_MAP: Record<string, string> = {
    'bs-away': 'away-message',
    'bs-music': 'music-player',
    'bs-friends': 'friends-list',
    'bs-guestbook': 'guestbook',
    'bs-counter': 'visitor-counter',
    'bs-feed-widget': 'post-feed',
    'bs-media-grid': 'media-grid',
};

/**
 * All supported widget CSS classes for quick detection/scanning passes.
 * Derived directly from `WIDGET_CLASS_MAP` to avoid duplicated class lists.
 */
export const WIDGET_CLASSES = Object.keys(WIDGET_CLASS_MAP);
