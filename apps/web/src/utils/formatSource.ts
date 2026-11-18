export function formatSource(source: string): string {
  const s = source.toLowerCase();
  if (s.startsWith('facebook') || s === 'fb') return 'Facebook';
  if (s.startsWith('instagram') || s === 'ig') return 'Instagram';
  if (s.startsWith('threads')) return 'Threads';
  if (s.startsWith('mastodon') || s.startsWith('masto')) return 'Mastodon';
  if (s.startsWith('bluesky') || s.startsWith('bsky')) return 'Bluesky';
  if (s.startsWith('google')) return 'Google';
  if (s.startsWith('lemmy')) return 'Lemmy';
  if (s.startsWith('piefed')) return 'Piefed';
  if (s.startsWith('rss')) return 'RSS';
  return source;
}

export function sourceEmoji(source: string): string {
  const s = source.toLowerCase();
  if (s.startsWith('bluesky') || s.startsWith('bsky')) return '🦋';
  if (s.startsWith('mastodon') || s.startsWith('masto')) return '🐘';
  if (s.startsWith('lemmy')) return '🐹';
  if (s.startsWith('piefed')) return '🥧';
  if (s.startsWith('facebook') || s === 'fb') return '📘';
  if (s.startsWith('instagram') || s === 'ig') return '📷';
  if (s.startsWith('threads')) return '🧵';
  if (s.startsWith('google')) return '🔍';
  if (s.startsWith('rss')) return '📰';
  if (s === 'bytescape' || s === '') return '🏠';
  return '📦';
}

export function formatEngagementCount(n: number | null | undefined): string {
  if (n == null) return '0';
  if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
  return n.toString();
}

export function formatSourceBadgeLabel(source: string): string {
  return `${sourceEmoji(source)} ${formatSource(source)}`;
}

export function isImported(source: string): boolean {
  const s = source.toLowerCase();
  return ['facebook', 'instagram', 'threads', 'google', 'fb', 'ig'].some(p => s.startsWith(p));
}
