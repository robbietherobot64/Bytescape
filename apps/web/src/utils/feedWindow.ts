// Time-period filter (owner spec 2026-11-17): filters the feed by post age
// BEFORE sorting, then the chosen sort (new/popular) orders inside the window.
// Client-owned preference like sort/types/platform: persists in localStorage and
// is sent as `window=` (+ `from`/`to` for a custom range) on every feed call.

export type WindowPreset = 'all' | '12h' | '24h' | '48h' | '1w' | '1mo' | 'custom';

export interface WindowPref {
  preset: WindowPreset;
  /** Custom range start, YYYY-MM-DD (inclusive); '' = open-ended. */
  from: string;
  /** Custom range end, YYYY-MM-DD (inclusive); '' = open-ended. */
  to: string;
}

export interface WindowQuery {
  window?: string;
  from?: string;
  to?: string;
}

export const WINDOW_OPTIONS: { value: WindowPreset; label: string }[] = [
  { value: 'all', label: 'All time' },
  { value: '12h', label: '12 hrs' },
  { value: '24h', label: '24 hrs' },
  { value: '48h', label: '48 hrs' },
  { value: '1w', label: '1 week' },
  { value: '1mo', label: '1 month' },
  { value: 'custom', label: 'Custom' },
];

/**
 * Fresh-load default (owner 2026-11-17): the last 24 hours. `All time` stays a
 * selectable option and an explicitly stored `all` is honoured — only the
 * unset/invalid fallback moved from `all` to `24h`.
 */
export const DEFAULT_WINDOW: WindowPref = { preset: '24h', from: '', to: '' };

const PRESETS: Set<string> = new Set(WINDOW_OPTIONS.map((o) => o.value));
const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;

export function normalizeWindowPreset(raw: string | null | undefined): WindowPreset {
  return raw && PRESETS.has(raw) ? (raw as WindowPreset) : DEFAULT_WINDOW.preset;
}

function normalizeDate(raw: string | null | undefined): string {
  return raw && DATE_RE.test(raw) ? raw : '';
}

export function loadWindowPref(
  storage: Pick<Storage, 'getItem'> = localStorage,
): WindowPref {
  return {
    preset: normalizeWindowPreset(storage.getItem('bytescape_feed_window')),
    from: normalizeDate(storage.getItem('bytescape_feed_window_from')),
    to: normalizeDate(storage.getItem('bytescape_feed_window_to')),
  };
}

export function saveWindowPref(
  pref: WindowPref,
  storage: Pick<Storage, 'setItem'> = localStorage,
): void {
  storage.setItem('bytescape_feed_window', pref.preset);
  storage.setItem('bytescape_feed_window_from', pref.from);
  storage.setItem('bytescape_feed_window_to', pref.to);
}

/**
 * Query params for the combined feed. `all` and an empty custom form mean no
 * filter (omitted entirely, matching the backend's neutral defaults).
 */
export function windowQuery(pref: WindowPref): WindowQuery {
  const preset = normalizeWindowPreset(pref.preset);
  if (preset === 'all') return {};
  if (preset === 'custom') {
    const from = normalizeDate(pref.from);
    const to = normalizeDate(pref.to);
    if (!from && !to) return {};
    return { window: 'custom', from, to };
  }
  return { window: preset };
}
