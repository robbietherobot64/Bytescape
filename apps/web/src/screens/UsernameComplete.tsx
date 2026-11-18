import { useState } from 'react';
import { oauthApi, apiErrorDisplay, setTokens } from '../utils/api';
import { generateSigningKey } from '../utils/crypto';

interface Props {
  tempToken: string;
  provider: string;
  error?: string;
  loading?: boolean;
  onDone: () => void;
  onError: (msg: string) => void;
  setLoading: (v: boolean) => void;
}

/**
 * Finish a Mastodon/Bluesky new signup: the OAuth flow returned a temp_token;
 * the user now picks a Bytescape username and we complete account creation.
 */
export default function UsernameComplete({ tempToken, provider, error, loading, onDone, onError, setLoading }: Props) {
  const [username, setUsername] = useState('');
  const [localError, setLocalError] = useState('');

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLocalError('');
    const name = username.trim();
    if (name.length < 2 || name.length > 32) {
      setLocalError('Username must be 2-32 characters');
      return;
    }
    setLoading(true);
    try {
      // oauth-complete requires a public_key (64-hex). Generate an Ed25519
      // signing key; the encrypted private key stays in client custody.
      const { publicKeyHex, encryptedPrivateKey } = await generateSigningKey(crypto.randomUUID());
      // The invite code is persisted in sessionStorage by LoginFlow — pass it so
      // the oauth-complete invite gate (consume_temp_token) is satisfied.
      const invite = sessionStorage.getItem('bytescape_invite') || undefined;
      const res = await oauthApi.completeSignup(tempToken, name, publicKeyHex, invite);
      localStorage.setItem('bytescape_enc_privkey', encryptedPrivateKey);
      setTokens(res.access_token, res.refresh_token);
      onDone();
    } catch (err: any) {
      setLoading(false);
      const msg = apiErrorDisplay(err);
      setLocalError(msg);
    }
  };

  const shown = localError || error || '';

  return (
    <div
      className="fullscreen"
      style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', flexDirection: 'column', padding: 24 }}
    >
      <div className="terminal-title" style={{ marginBottom: 4 }}>BYTESCAPE</div>
      <p style={{ color: 'var(--text-muted)', fontSize: 14, marginBottom: 24, fontFamily: 'var(--font-mono)' }}>
        {provider ? `connected via ${provider} — ` : ''}pick your username
      </p>

      <form onSubmit={submit} style={{ display: 'flex', flexDirection: 'column', gap: 12, width: 280, maxWidth: '100%' }}>
        <input
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          placeholder="username"
          aria-label="Username"
          autoFocus
          style={{
            background: 'rgba(0,0,0,0.4)',
            border: '1px solid var(--accent)',
            color: 'var(--text)',
            padding: '12px 16px',
            fontFamily: 'var(--font-mono)',
            fontSize: 15,
            textAlign: 'center',
          }}
        />
        <button
          type="submit"
          className="btn"
          disabled={loading}
          style={{
            background: 'var(--accent)',
            color: '#000',
            border: 'none',
            fontWeight: 600,
            fontSize: 16,
            padding: '13px 24px',
            fontFamily: 'var(--font-retro)',
            letterSpacing: 1,
            cursor: 'pointer',
          }}
        >
          {loading ? '> CONNECTING...' : 'Create account'}
        </button>
      </form>

      {shown && (
        <div className="error-text" style={{ marginTop: 14, maxWidth: 320, textAlign: 'center' }}>
          {'>'} {shown}
        </div>
      )}
    </div>
  );
}
