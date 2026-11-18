// Client-owned feed preferences: stars and saved lenses (feed-refocus design:
// localStorage v1, keyed by identity). Server persistence is a later phase.

export interface SavedLens {
  id: string;
  name: string;
  sources: string[]; // source keys: 'hashtag:x' | 'feed:at://…' | 'follows:all'
}

export interface FeedPrefs {
  stars: string[]; // starred source keys
  lenses: SavedLens[];
}

const KEY = 'bytescape.feedPrefs.v1';

function identityKey(): string {
  try {
    const token = localStorage.getItem('bytescape_token') || '';
    const payload = JSON.parse(atob(token.split('.')[1] || 'e30'));
    return payload.sub || 'anon';
  } catch {
    return 'anon';
  }
}

export function getPrefs(): FeedPrefs {
  try {
    const raw = localStorage.getItem(`${KEY}:${identityKey()}`);
    if (!raw) return { stars: [], lenses: [] };
    const p = JSON.parse(raw);
    return { stars: p.stars || [], lenses: p.lenses || [] };
  } catch {
    return { stars: [], lenses: [] };
  }
}

export function isStarred(key: string): boolean {
  const { stars } = getPrefs();
  return stars.length > 0 && stars.includes(key);
}

export function savePrefs(p: FeedPrefs) {
  // Normalize on save: stars are a set in practice.
  const normalized: FeedPrefs = { stars: [...new Set(p.stars)], lenses: p.lenses };
  localStorage.setItem(`${KEY}:${identityKey()}`, JSON.stringify(normalized));
}

export function toggleStar(key: string): boolean {
  const p = getPrefs();
  const i = p.stars.indexOf(key);
  if (i >= 0) p.stars.splice(i, 1);
  else p.stars.push(key);
  savePrefs(p);
  return i < 0;
}

export function pinLens(name: string, sources: string[]): SavedLens | null {
  const p = getPrefs();
  if (p.lenses.some((l) => l.sources.join('|') === sources.join('|'))) return null;
  const lens: SavedLens = {
    id: `lens-${Date.now()}`,
    name,
    sources,
  };
  p.lenses.push(lens);
  savePrefs(p);
  return lens;
}

export function removeLens(id: string) {
  const p = getPrefs();
  p.lenses = p.lenses.filter((l) => l.id !== id);
  savePrefs(p);
}

export function addSourceToLens(id: string, key: string) {
  const p = getPrefs();
  const l = p.lenses.find((x) => x.id === id);
  if (l && !l.sources.includes(key)) {
    l.sources.push(key);
    savePrefs(p);
  }
}

export function removeSourceFromLens(id: string, key: string) {
  const p = getPrefs();
  const l = p.lenses.find((x) => x.id === id);
  if (l) {
    l.sources = l.sources.filter((s) => s !== key);
    savePrefs(p);
  }
}

export interface ChipSource {
  label: string;
  kind: string;
  rank: number;
}

export function sourceKey(m: ChipSource): string {
  return m.kind === 'hashtag'
    ? `hashtag:${m.label}`
    : m.kind === 'feed'
      ? `feed:${m.label}`
      : 'follows:all';
}

