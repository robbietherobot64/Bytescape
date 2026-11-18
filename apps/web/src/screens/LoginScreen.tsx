import { useState, useEffect } from 'react';
import { authApi, oauthApi, apiErrorDisplay, setTokens } from '../utils/api';
import { generateSigningKey } from '../utils/crypto';

/**
 * Multi-screen login flow (first-run impression).
 *
 *   Landing        →  invite code + [Create Account] [Login]
 *   Create Account →  [E-mail Signup] [Bluesky Signup] [Fediverse Signup]
 *   Login          →  [E-mail Login]   [Bluesky Login]   [Fediverse Login]
 *   Method screens →  the chosen method's actual form / OAuth redirect
 *
 * Visually identical between Create Account and Login; only the labels differ.
 * All paths converge on the same account end-state. E-mail Login asks only
 * email + password (existing user); E-mail Signup adds username.
 */

type Mode = 'signup' | 'login';
type Screen =
  | 'landing'
  | 'choose'          // Create Account or Login (three method buttons)
  | 'email'
  | 'fediverse';

interface Props {
  loading?: boolean;
  /** Shared invite code captured on the landing screen. */
  invite: string;
  onInviteChange: (v: string) => void;
}

export default function LoginScreen({ loading: externalLoading, invite, onInviteChange }: Props) {
  const [mode, setMode] = useState<Mode>('signup');
  const [screen, setScreen] = useState<Screen>('landing');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const ebusy = busy || !!externalLoading;

  const requireInvite = () => {
    if (!invite.trim()) {
      setError('Invite code required');
      return false;
    }
    return true;
  };

  // ── Bluesky OAuth (same flow for signup and login) ──
  const handleBluesky = async () => {
    setError('');
    if (!requireInvite()) return;
    setBusy(true);
    try {
      sessionStorage.setItem('oauth_signup_pending', mode === 'signup' ? 'true' : 'false');
      sessionStorage.setItem('oauth_method', 'bluesky');
      const result = await oauthApi.blueskySignup(invite.trim());
      window.location.href = result.url;
    } catch (err: any) {
      setError(apiErrorDisplay(err));
      setBusy(false);
    }
  };

  return (
    <div className="fullscreen" style={wrapStyle}>
      <div className="terminal-title" style={{ marginBottom: 4 }}>BYTESCAPE</div>
      <p style={{ color: 'var(--text-muted)', fontSize: 14, marginBottom: 24, fontFamily: 'var(--font-mono)' }}>
        your home on the open web
      </p>

      <div style={{ width: 320, maxWidth: '100%' }}>
        {screen === 'landing' && (
          <Landing
            invite={invite}
            onInviteChange={onInviteChange}
            onCreate={() => { setMode('signup'); setError(''); if (requireInvite()) setScreen('choose'); }}
            onLogin={() => { setMode('login'); setError(''); if (requireInvite()) setScreen('choose'); }}
          />
        )}

        {screen === 'choose' && (
          <Choose
            mode={mode}
            onEmail={() => { setError(''); setScreen('email'); }}
            onBluesky={handleBluesky}
            onFediverse={() => { setError(''); setScreen('fediverse'); }}
            onBack={() => { setError(''); setScreen('landing'); }}
            busy={ebusy}
          />
        )}

        {screen === 'email' && (
          <EmailForm
            mode={mode}
            invite={invite}
            busy={ebusy}
            error={error}
            setError={setError}
            setBusy={setBusy}
            onBack={() => { setError(''); setScreen('choose'); }}
          />
        )}

        {screen === 'fediverse' && (
          <FediverseForm
            mode={mode}
            invite={invite}
            busy={ebusy}
            error={error}
            setError={setError}
            setBusy={setBusy}
            onBack={() => { setError(''); setScreen('choose'); }}
          />
        )}
      </div>

      {error && screen !== 'email' && screen !== 'fediverse' && (
        <div className="error-text" style={{ marginTop: 14, maxWidth: 320, textAlign: 'center' }}>{'> '}{error}</div>
      )}
    </div>
  );
}

// ── Landing: invite code + Create Account + Login ──
function Landing({ invite, onInviteChange, onCreate, onLogin }: {
  invite: string;
  onInviteChange: (v: string) => void;
  onCreate: () => void;
  onLogin: () => void;
}) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <input
        value={invite}
        onChange={(e) => onInviteChange(e.target.value)}
        placeholder="invite code"
        aria-label="Invite code"
        style={inputStyle}
      />
      <button type="button" className="btn" onClick={onCreate} style={primaryBtnStyle}>Create Account</button>
      <button type="button" className="btn" onClick={onLogin} style={secondaryBtnStyle}>Login</button>
    </div>
  );
}

// ── Choose a method (three buttons) — identical layout for both modes ──
function Choose({ mode, onEmail, onBluesky, onFediverse, onBack, busy }: {
  mode: Mode;
  onEmail: () => void;
  onBluesky: () => void;
  onFediverse: () => void;
  onBack: () => void;
  busy: boolean;
}) {
  const label = (method: string) => (mode === 'signup' ? `${method} Signup` : `${method} Login`);
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <button type="button" className="btn" onClick={onEmail} disabled={busy} style={primaryBtnStyle}>
        ✉️  {label('E-mail')}
      </button>
      <button type="button" className="btn" onClick={onBluesky} disabled={busy} style={secondaryBtnStyle}>
        🦋  {label('Bluesky')}
      </button>
      <button type="button" className="btn" onClick={onFediverse} disabled={busy} style={secondaryBtnStyle}>
        🐘  {label('Fediverse')}
      </button>
      <button type="button" onClick={onBack} style={linkBtnStyle}>&#8592; back</button>
    </div>
  );
}

// ── E-mail: signup (username + email + password) or login (email + password) ──
function EmailForm({ mode, invite, busy, error, setError, setBusy, onBack }: {
  mode: Mode;
  invite: string;
  busy: boolean;
  error: string;
  setError: (v: string) => void;
  setBusy: (v: boolean) => void;
  onBack: () => void;
}) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [username, setUsername] = useState('');

  // Fresh signup must reach the Import onboarding screen — mirror the OAuth
  // fresh-account reset so a stale per-session guard never skips it.
  const finishFreshSignup = (access: string, refresh: string) => {
    setTokens(access, refresh);
    localStorage.setItem('onboarding_complete', 'false');
    localStorage.removeItem('bytescape_interests');
    localStorage.removeItem('bytescape_interests_configured');
    sessionStorage.removeItem('onboarding_route_done');
    window.location.href = '/';
  };

  const finishLogin = (access: string, refresh: string) => {
    setTokens(access, refresh);
    sessionStorage.removeItem('onboarding_route_done');
    window.location.href = '/';
  };

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    setBusy(true);
    try {
      if (mode === 'login') {
        const res = await authApi.login(email.trim(), password);
        finishLogin(res.access_token, res.refresh_token);
      } else {
        const { publicKeyHex, encryptedPrivateKey } = await generateSigningKey(password);
        const res = await authApi.signup({
          username: username.trim(),
          email: email.trim(),
          password,
          public_key: publicKeyHex,
          invite_code: invite.trim(),
        });
        if (res.status === 'active' && res.access_token && res.refresh_token) {
          localStorage.setItem('bytescape_enc_privkey', encryptedPrivateKey);
          finishFreshSignup(res.access_token, res.refresh_token);
        } else {
          setBusy(false);
          setError(
            res.code
              ? `Verification code (SMTP not configured): ${res.code}`
              : 'Check your email for the verification code, then log in.',
          );
        }
      }
    } catch (err: any) {
      setError(apiErrorDisplay(err));
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div style={headerStyle}>{mode === 'signup' ? 'E-mail Signup' : 'E-mail Login'}</div>
      {mode === 'signup' && (
        <input value={username} onChange={(e) => setUsername(e.target.value)} placeholder="username" aria-label="Username"
          autoComplete="username" style={inputStyle} />
      )}
      <input value={email} onChange={(e) => setEmail(e.target.value)} type="email" placeholder="email" aria-label="Email"
        autoComplete="email" style={inputStyle} />
      <input value={password} onChange={(e) => setPassword(e.target.value)} type="password" placeholder="password" aria-label="Password"
        autoComplete={mode === 'signup' ? 'new-password' : 'current-password'} style={inputStyle} />
      <button type="submit" className="btn" disabled={busy} style={primaryBtnStyle}>
        {busy ? '> CONNECTING...' : mode === 'signup' ? 'Create account' : 'Log in'}
      </button>
      {error && <div className="error-text" style={{ marginTop: 4 }}>{'> '}{error}</div>}
      <button type="button" onClick={onBack} style={linkBtnStyle}>&#8592; back</button>
    </form>
  );
}

// ── Fediverse: username + instance → official Mastodon OAuth linking flow ──
function FediverseForm({ mode, invite, busy, error, setError, setBusy, onBack }: {
  mode: Mode;
  invite: string;
  busy: boolean;
  error: string;
  setError: (v: string) => void;
  setBusy: (v: boolean) => void;
  onBack: () => void;
}) {
  const [instance, setInstance] = useState('');

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    const inst = instance.trim();
    if (!inst) {
      setError('Enter your instance (e.g. mastodon.social)');
      return;
    }
    setBusy(true);
    try {
      sessionStorage.setItem('oauth_signup_pending', mode === 'signup' ? 'true' : 'false');
      sessionStorage.setItem('oauth_method', 'mastodon');
      // Accept a bare instance host or a full @user@instance handle. Identity is
      // determined by the instance's OAuth, not a client-side username field.
      const host = inst.includes('@') ? inst.split('@').filter(Boolean).pop()! : inst;
      const result = await oauthApi.mastodonStart(host, invite.trim());
      window.location.href = result.url;
    } catch (err: any) {
      setError(apiErrorDisplay(err));
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div style={headerStyle}>{mode === 'signup' ? 'Fediverse Signup' : 'Fediverse Login'}</div>
      <input value={instance} onChange={(e) => setInstance(e.target.value)} placeholder="instance (e.g. mastodon.social)"
        aria-label="Instance" style={inputStyle} />
      <button type="submit" className="btn" disabled={busy} style={primaryBtnStyle}>
        {busy ? '> CONNECTING...' : 'Continue'}
      </button>
      {error && <div className="error-text" style={{ marginTop: 4 }}>{'> '}{error}</div>}
      <button type="button" onClick={onBack} style={linkBtnStyle}>&#8592; back</button>
    </form>
  );
}

// ── shared styles ──
const wrapStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column', padding: 24,
};
const inputStyle: React.CSSProperties = {
  background: 'rgba(0,0,0,0.4)', border: '1px solid var(--accent)', color: 'var(--text)',
  padding: '12px 16px', fontFamily: 'var(--font-mono)', fontSize: 15, width: '100%',
  textAlign: 'center', boxSizing: 'border-box',
};
const primaryBtnStyle: React.CSSProperties = {
  background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, fontSize: 16,
  padding: '13px 24px', fontFamily: 'var(--font-retro)', letterSpacing: 1, cursor: 'pointer', width: '100%',
};
const secondaryBtnStyle: React.CSSProperties = {
  ...primaryBtnStyle, background: 'transparent', color: 'var(--accent)', border: '1px solid var(--accent)',
  padding: '11px 16px', fontSize: 15,
};
const linkBtnStyle: React.CSSProperties = {
  background: 'none', border: 'none', color: 'var(--text-muted)', cursor: 'pointer',
  fontFamily: 'var(--font-mono)', fontSize: 13, marginTop: 4,
};
const headerStyle: React.CSSProperties = {
  color: 'var(--accent)', fontFamily: 'var(--font-retro)', fontSize: 18, letterSpacing: 1,
  textAlign: 'center', marginBottom: 4,
};
