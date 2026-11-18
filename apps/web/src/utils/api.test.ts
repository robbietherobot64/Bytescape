import { afterEach, describe, expect, it, vi } from 'vitest';

function fakeStorage(init: Record<string, string> = {}) {
  const map = new Map(Object.entries(init));
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => {
      map.set(k, v);
    },
    removeItem: (k: string) => {
      map.delete(k);
    },
    map,
  };
}

interface FetchCall {
  url: string;
  init?: RequestInit;
}

function installGlobals(tokens: Record<string, string>, response: (url: string) => { ok: boolean; status: number; body?: unknown }) {
  const storage = fakeStorage(tokens);
  const win = { location: { hostname: 'test.bytescape.social', href: '/app' } };
  const calls: FetchCall[] = [];
  const fetchMock = async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, init });
    const r = response(url);
    return {
      ok: r.ok,
      status: r.status,
      headers: { get: () => 'application/json' },
      json: async () => r.body ?? {},
      text: async () => JSON.stringify(r.body ?? {}),
    } as unknown as Response;
  };
  (globalThis as Record<string, unknown>).localStorage = storage;
  (globalThis as Record<string, unknown>).window = win;
  (globalThis as Record<string, unknown>).fetch = fetchMock;
  return { storage, win, calls };
}

const refreshCalls = (calls: FetchCall[]) =>
  calls.filter((c) => c.url.includes('/api/auth/refresh')).length;

describe('expired session: one refresh, then the honest terminal state', () => {
  afterEach(() => {
    vi.resetModules();
  });

  it('attempts exactly ONE refresh and stops all further requests', async () => {
    const { win, calls } = installGlobals(
      { bytescape_token: 'expired-access', bytescape_refresh_token: 'expired-refresh' },
      () => ({ ok: false, status: 401, body: { error: 'auth_error', message: 'Invalid token' } }),
    );
    // Dynamic import: each test needs a FRESH module state (the expired
    // latch + single-flight are module-level) — static import would share
    // it across tests and defeat the isolation.
    const api = await import('./api');

    // Five sequential calls — the chain's shape.
    const errors: Error[] = [];
    for (let i = 0; i < 5; i++) {
      await api.api('/api/profile/me').then(
        () => {
          throw new Error('must not succeed');
        },
        (e: Error) => errors.push(e),
      );
    }

    expect(refreshCalls(calls)).toBe(1);
    // One original request + one refresh — the other four calls short-circuit
    // on the terminal state and never touch the network.
    expect(calls.length).toBe(2);
    // The silent reload is gone: the page never navigates on its own.
    expect(win.location.href).toBe('/app');
    // The interstitial message is what callers see after the state flips.
    expect(errors.slice(1).every((e) => e.message === api.SESSION_EXPIRED_MSG)).toBe(true);
    expect(api.isSessionExpired()).toBe(true);
  });

  it('a missing refresh token for an existing session is terminal too', async () => {
    const { calls } = installGlobals({ bytescape_token: 'expired-access' }, () => ({
      ok: false,
      status: 401,
      body: { error: 'auth_error' },
    }));
    const api = await import('./api');

    await api.api('/api/profile/me').catch(() => {});
    await api.api('/api/settings').catch(() => {});

    expect(refreshCalls(calls)).toBe(0);
    // First call discovers the unrecoverable session; the second never fires.
    expect(calls.length).toBe(1);
    expect(api.isSessionExpired()).toBe(true);
  });

  it('a successful login resets the terminal state', async () => {
    const { storage, calls } = installGlobals(
      { bytescape_token: 'expired-access', bytescape_refresh_token: 'expired-refresh' },
      (url) => {
        // The expired session 401s; once the tokens rotate the API answers.
        const expired = storage.getItem('bytescape_token') === 'expired-access';
        return url.includes('/api/auth/refresh') || expired
          ? { ok: false, status: 401, body: { error: 'auth_error' } }
          : { ok: true, status: 200, body: { ok: true } };
      },
    );
    const api = await import('./api');

    await api.api('/api/profile/me').catch(() => {});
    expect(api.isSessionExpired()).toBe(true);

    api.setTokens('fresh-access', 'fresh-refresh');
    expect(api.isSessionExpired()).toBe(false);
    await api.api('/api/profile/me');
    // The recovered session makes real requests again.
    expect(calls.filter((c) => c.url.includes('/api/profile/me')).length).toBe(2);
    expect(refreshCalls(calls)).toBe(1);
  });

  it('a first-time visitor is never told the session expired', async () => {
    const { calls } = installGlobals({}, () => ({ ok: false, status: 401, body: { error: 'auth_error' } }));
    const api = await import('./api');

    await api.api('/api/profile/me').catch(() => {});
    await api.api('/api/profile/me').catch(() => {});

    expect(api.isSessionExpired()).toBe(false);
    expect(refreshCalls(calls)).toBe(0);
    expect(calls.length).toBe(2);
  });
});
