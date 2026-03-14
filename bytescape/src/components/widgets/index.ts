/**
 * Widget System Registry (B2.4)
 *
 * Maps GrapesJS block CSS classes to their corresponding React widget components.
 * Used by ProfileRenderer to hydrate widget placeholders with live components.
 */

export { AwayMessageWidget } from './AwayMessageWidget';
export { MusicPlayerWidget } from './MusicPlayerWidget';
export { FriendsListWidget } from './FriendsListWidget';
export { GuestbookWidget } from './GuestbookWidget';
export { VisitorCounterWidget } from './VisitorCounterWidget';
export { PostFeedWidget } from './PostFeedWidget';
export { MediaGridWidget } from './MediaGridWidget';

/**
 * CSS class → widget type mapping.
 * These classes are applied by GrapesJS blocks in ProfileBuilderScreen.
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

/** All widget CSS classes as an array for detection */
export const WIDGET_CLASSES = Object.keys(WIDGET_CLASS_MAP);
