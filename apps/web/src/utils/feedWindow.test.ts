import { describe, expect, it } from 'vitest';
import {
  DEFAULT_WINDOW,
  loadWindowPref,
  normalizeWindowPreset,
  saveWindowPref,
  windowQuery,
  type WindowPref,
} from './feedWindow';

function fakeStorage(init: Record<string, string> = {}) {
  const map = new Map(Object.entries(init));
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => { map.set(k, v); },
    map,
  };
}

describe('time-period prefs', () => {
  it('defaults a fresh load to the last 24 hours', () => {
    expect(loadWindowPref(fakeStorage())).toEqual({ preset: '24h', from: '', to: '' });
    expect(windowQuery(DEFAULT_WINDOW)).toEqual({ window: '24h' });
  });

  it('keeps an explicitly stored all-time selection', () => {
    const storage = fakeStorage({ bytescape_feed_window: 'all' });
    expect(loadWindowPref(storage)).toEqual({ preset: 'all', from: '', to: '' });
    expect(windowQuery(loadWindowPref(storage))).toEqual({});
  });

  it('validates presets instead of trusting storage', () => {
    expect(normalizeWindowPreset('24h')).toBe('24h');
    expect(normalizeWindowPreset('CUSTOM')).toBe('24h');
    expect(normalizeWindowPreset('year')).toBe('24h');
    expect(normalizeWindowPreset(null)).toBe('24h');
  });

  it('round-trips preset plus custom dates through storage', () => {
    const storage = fakeStorage();
    const pref: WindowPref = { preset: 'custom', from: '2026-11-10', to: '2026-11-12' };
    saveWindowPref(pref, storage);
    expect(loadWindowPref(storage)).toEqual(pref);

    const preset: WindowPref = { preset: '1w', from: '', to: '' };
    saveWindowPref(preset, storage);
    expect(loadWindowPref(storage)).toEqual(preset);
  });

  it('drops malformed stored dates', () => {
    const storage = fakeStorage({
      bytescape_feed_window: 'custom',
      bytescape_feed_window_from: '11/10/2026',
      bytescape_feed_window_to: '2026-11-12',
    });
    expect(loadWindowPref(storage)).toEqual({ preset: 'custom', from: '', to: '2026-11-12' });
  });

  it('builds query params: neutral omitted, presets sent, custom endpoints trimmed', () => {
    expect(windowQuery({ preset: 'all', from: '', to: '' })).toEqual({});
    expect(windowQuery({ preset: '24h', from: '', to: '' })).toEqual({ window: '24h' });
    expect(windowQuery({ preset: 'custom', from: '', to: '' })).toEqual({});
    expect(windowQuery({ preset: 'custom', from: '2026-11-10', to: '' }))
      .toEqual({ window: 'custom', from: '2026-11-10', to: '' });
    expect(windowQuery({ preset: 'custom', from: '', to: '2026-11-12' }))
      .toEqual({ window: 'custom', from: '', to: '2026-11-12' });
    expect(windowQuery({ preset: 'custom', from: '2026-11-10', to: '2026-11-12' }))
      .toEqual({ window: 'custom', from: '2026-11-10', to: '2026-11-12' });
  });
});
