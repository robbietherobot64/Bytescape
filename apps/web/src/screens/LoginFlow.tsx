import { useState, useCallback, useEffect } from 'react';
import TerminalBoot from '../components/TerminalBoot';
import LoginScreen from './LoginScreen';
import UsernameComplete from './UsernameComplete';
import { userFromToken, apiErrorDisplay } from '../utils/api';

// Error boundary to catch TerminalBoot crash and log the real error
import { Component } from 'react';

class BootErrorBoundary extends Component<{ children: React.ReactNode }, { error: Error | null }> {
  constructor(props: any) { super(props); this.state = { error: null }; }
  static getDerivedStateFromError(error: Error) { return { error }; }
  render() {
    if (this.state.error) {
      console.error('[BootErrorBoundary]', this.state.error);
      return <div className="fullscreen"><div className="terminal-title">BOOT ERROR</div><pre style={{ color: '#f33' }}>{this.state.error.message}</pre></div>;
    }
    return this.props.children;
  }
}

type Phase = 'landing' | 'oauth-callback' | 'complete' | 'boot' | 'done';

interface Props {
  onComplete: (username: string) => void;
}

export default function LoginFlow({ onComplete }: Props) {
  const [phase, setPhase] = useState<Phase>('landing');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [pendingTempToken, setPendingTempToken] = useState('');
  const [pendingProvider, setPendingProvider] = useState('');
  // Invite code persists across reloads / OAuth redirects (sessionStorage) so a
  // page reload mid-flow never forces re-entering it.
  const [invite, setInvite] = useState(() => sessionStorage.getItem('bytescape_invite') || '');
  useEffect(() => {
    if (invite) sessionStorage.setItem('bytescape_invite', invite);
  }, [invite]);

  // Handle OAuth callback — when Bluesky redirects back
  useEffect(() => {
    // Session tokens arrive in the URL FRAGMENT (SEC-1: fragments never reach
    // servers, history or Referer). Query first is kept as the tolerant
    // fallback so redirects issued by a pre-deploy server (which put the
    // tokens in ?access_token=…) survive the deploy window.
    const search = new URLSearchParams(window.location.search);
    const hash = window.location.hash
      ? new URLSearchParams(window.location.hash.replace(/^#/, ''))
      : null;
    const params = new URLSearchParams();
    for (const [k, v] of search) params.set(k, v);
    if (hash) for (const [k, v] of hash) params.set(k, v);

    // New user auto-complete: backend issued JWT tokens directly
    if (params.get('oauth_complete') === 'true') {
      window.history.replaceState({}, '', '/app');
      const accessToken = params.get('access_token');
      const refreshToken = params.get('refresh_token');
      if (accessToken && refreshToken) {
        import('../utils/api').then(({ setTokens }) => {
          setTokens(accessToken, refreshToken);
          if (params.get('returning') === 'true') {
            // Returning user login: keep their cached interests. Onboarding
            // state is decided by the server `onboarded` flag — never reset it
            // here, or every logout+login re-prompts onboarding.
            sessionStorage.removeItem('onboarding_route_done');
          } else {
            // Fresh account session: drop any account-scoped cached state from
            // a previous account so onboarding runs and interests aren't inherited.
            localStorage.setItem('onboarding_complete', 'false');
            localStorage.removeItem('bytescape_interests');
            localStorage.removeItem('bytescape_interests_configured');
            sessionStorage.removeItem('onboarding_route_done');
          }
          setPhase('boot');
        });
      } else {
        setError('OAuth callback missing tokens');
        setPhase('landing');
      }
      return;
    }

    // Existing user returning: provider=bluesky&success=true
    if (params.get('provider') === 'bluesky' && params.get('success') === 'true') {
      window.history.replaceState({}, '', '/app');
      // Existing user — tokens should already be in localStorage from prior session
      const token = localStorage.getItem('bytescape_token');
      if (token) {
        setPhase('boot');
      } else {
        // No existing token — need to re-authenticate
        setError('Session expired. Please sign in again.');
        setPhase('landing');
      }
      return;
    }

    // OAuth error
    const oauthError = params.get('oauth_error');
    if (oauthError) {
      window.history.replaceState({}, '', '/app');
      setError(decodeURIComponent(oauthError));
      setPhase('landing');
      return;
    }

    // Legacy oauth_signup callback (Mastodon/Bluesky new signup) — complete with a chosen username.
    if (params.get('oauth_signup') === 'true') {
      window.history.replaceState({}, '', '/app');
      const tt = params.get('temp_token');
      if (tt) {
        setPendingTempToken(tt);
        setPendingProvider(params.get('provider') || '');
        setPhase('complete');
      } else {
        setError('Missing signup token. Please sign in again.');
        setPhase('landing');
      }
      return;
    }
  }, []);

  // Google OAuth popup callback — legacy, kept for compatibility
  useEffect(() => {
    const hash = window.location.hash;
    if (hash && window.opener) {
      const fragment = hash.replace('#', '');
      const hashParams = new URLSearchParams(fragment);
      const idToken = hashParams.get('id_token');
      if (idToken) {
        window.opener.postMessage({ type: 'google_oauth', id_token: idToken }, window.location.origin);
        window.close();
      }
    }
  }, []);

  // After boot animation finishes → enter the app
  const handleBootDone = useCallback(() => {
    // The invite code is consumed once the user is authenticated — drop it so it
    // doesn't linger (e.g. on a shared machine) or leak into a later signup.
    sessionStorage.removeItem('bytescape_invite');
    const u = userFromToken();
    onComplete(u?.username || 'user');
  }, [onComplete]);

  // Phase: Boot animation
  if (phase === 'boot') {
    return <BootErrorBoundary><TerminalBoot onComplete={handleBootDone} /></BootErrorBoundary>;
  }

  // Phase: Complete — pick a username to finish a Mastodon/Bluesky new signup.
  if (phase === 'complete') {
    return (
      <UsernameComplete
        tempToken={pendingTempToken}
        provider={pendingProvider}
        error={error}
        loading={loading}
        onDone={() => {
          // oauth_signup is always a FRESH account — reset onboarding state so
          // the Import onboarding screen is shown (mirrors the oauth_complete
          // fresh-account path + email finishFreshSignup).
          localStorage.setItem('onboarding_complete', 'false');
          localStorage.removeItem('bytescape_interests');
          localStorage.removeItem('bytescape_interests_configured');
          sessionStorage.removeItem('onboarding_route_done');
          setPhase('boot');
        }}
        onError={(m) => { setError(m); setPhase('landing'); }}
        setLoading={setLoading}
      />
    );
  }

  // Phase: Landing — multi-provider login (email-first, Bluesky + fediverse secondary)
  return <LoginScreen loading={loading} invite={invite} onInviteChange={setInvite} />;
}
