/**
 * PURPOSE
 * Utility helpers for classifying content URLs and cleaning imported vault text.
 *
 * HOW IT WORKS
 * Uses lightweight extension/path heuristics and regex-based text cleanup to
 * remove common archive noise and pull out a first embedded URL.
 *
 * RELATED FILES
 * - src/utils/sanitize.ts (HTML sanitization for rendered/entered content)
 * - src/screens/* (feed and detail renderers consuming normalized text/media)
 */
export type ContentKind = 'video' | 'audio' | 'pdf' | 'image' | 'unknown';

/**
 * Returns the hostname portion of a URL, or the original string if parsing fails.
 */
export function safeHostname(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url;
  }
}

/**
 * Heuristic detection for GIF-style links commonly hosted by GIF platforms.
 */
export function looksLikeGif(url?: string | null): boolean {
  if (!url) return false;
  const u = url.toLowerCase();
  return u.includes('tenor.com') || u.includes('giphy.com') || u.endsWith('.gif');
}

/**
 * Infers a best-effort content kind from URL/file path text.
 */
export function inferContentKind(src: string): ContentKind {
  const clean = src.toLowerCase().split('?')[0];
  if (/\.(mp4|mov|webm|m4v|avi|mkv|m3u8)$/.test(clean)) return 'video';
  if (/\.(mp3|wav|ogg|flac|m4a|aac)$/.test(clean)) return 'audio';
  if (/\.(pdf)$/.test(clean)) return 'pdf';
  if (/\.(jpg|jpeg|png|gif|webp|bmp|svg)$/.test(clean)) return 'image';
  if (/(^|\/)(videos?|video_uploads?|your_videos)(\/|$)/.test(clean)) return 'video';
  if (/(^|\/)(audio|voice|music)(\/|$)/.test(clean)) return 'audio';
  if (clean.includes('video') && !clean.includes('thumbnail')) return 'video';
  return 'unknown';
}

/**
 * Cleans archive/vault text of metadata noise and surfaces one primary link.
 *
 * Caveat:
 * This is intentionally regex-based and tuned for known export artifacts
 * (for example Facebook "Taken..." / "Upload IP address..."). Unexpected
 * phrasing may survive cleanup, and similarly worded real content may be
 * removed.
 */
export function sanitizeVaultContent(raw: string): { text: string; link?: string } {
  let text = raw || '';

  // Strip common Facebook export noise
  text = text.replace(/Taken [^\n]*Upload IP address[^\n]*/gi, '');
  text = text.replace(/Taken [^\n]*/gi, '');
  text = text.replace(/Upload IP address[^\n]*/gi, '');
  text = text.replace(/\s*Updated\s*$/i, '');

  // Extract first URL to treat as link preview
  const urlMatch = text.match(/https?:\/\/\S+/i);
  const link = urlMatch ? urlMatch[0] : undefined;
  if (link) {
    text = text.replace(link, '');
  }

  // Collapse excess whitespace
  text = text.replace(/\s+/g, ' ').trim();

  return { text, link };
}
