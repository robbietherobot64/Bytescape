import { useState } from 'react';
import { oauthApi, apiErrorDisplay } from '../utils/api';

interface Props {
  loading?: boolean;
}

export default function BlueskyLanding({ loading: externalLoading }: Props) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [invite, setInvite] = useState('');

  const handleSignIn = async () => {
    setLoading(true);
    setError('');
    try {
      const code = invite.trim();
      if (!code) {
        throw new Error('Invite code required');
      }
      sessionStorage.setItem('oauth_signup_pending', 'true');
      sessionStorage.setItem('oauth_method', 'bluesky');
      // Use the signup flow (not the linking flow) — this is the sole auth method
      const result = await oauthApi.blueskySignup(code);
      window.location.href = result.url;
    } catch (e: any) {
      setError(apiErrorDisplay(e));
      setLoading(false);
    }
  };

  const busy = loading || externalLoading;

  return (
    <div className="fullscreen" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column' }}>
      <div className="terminal-title" style={{ marginBottom: 4 }}>BYTESCAPE</div>
      <p style={{
        color: 'var(--text-muted)',
        fontSize: 14,
        marginBottom: 32,
        fontFamily: 'var(--font-mono)',
      }}>
        your home on the open web
      </p>

      <input
        value={invite}
        onChange={(e) => setInvite(e.target.value)}
        placeholder="invite code"
        aria-label="Invite code"
        style={{
          background: 'rgba(0,0,0,0.4)',
          border: '1px solid var(--accent)',
          color: 'var(--text)',
          padding: '12px 16px',
          marginBottom: 16,
          fontFamily: 'var(--font-mono)',
          fontSize: 15,
          width: 260,
          textAlign: 'center',
        }}
      />

      <button
        className="btn"
        onClick={handleSignIn}
        disabled={busy}
        style={{
          background: 'var(--accent)',
          color: '#000',
          border: 'none',
          fontWeight: 600,
          fontSize: 16,
          padding: '14px 32px',
          fontFamily: 'var(--font-retro)',
          letterSpacing: 1,
        }}
      >
        {busy ? '> CONNECTING...' : '🦋  Sign in with Bluesky'}
      </button>

      {error && (
        <div className="error-text" style={{ marginTop: 12 }}>
          {'>'} {error}
        </div>
      )}

      <a
        href="https://bsky.social"
        target="_blank"
        rel="noopener noreferrer"
        className="btn-link"
        style={{ marginTop: 20, fontSize: 13, color: 'var(--text-muted)' }}
      >
        no account? get one at bsky.social
      </a>
    </div>
  );
}
