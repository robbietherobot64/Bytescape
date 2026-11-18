import { useEffect, useState } from 'react';
import { oauthApi } from '../utils/api';

interface Props {
  onDone: () => void;
}

export default function OAuthCallback({ onDone }: Props) {
  const [status, setStatus] = useState('Connecting account...');

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const provider = params.get('provider');
    const success = params.get('success');

    if (!provider || success !== 'true') {
      setStatus('OAuth failed — missing parameters');
      return;
    }

    const check = async () => {
      // Poll briefly until the account is registered on server side
      for (let i = 0; i < 10; i++) {
        await new Promise(r => setTimeout(r, 500));
        try {
          const checkFn = oauthApi.blueskyCheck;
          const result = await checkFn();
          if (result.linked) {
            setStatus(`✅ ${provider} account linked!`);
            // Replace the URL to remove query params
            window.history.replaceState({}, '', '/');
            setTimeout(onDone, 1000);
            return;
          }
        } catch {}
      }
      setStatus(`❌ Failed to link ${provider} account. Try again.`);
    };

    check();
  }, [onDone]);

  return (
    <div className="screen">
      <div className="empty-feed">
        <h2>OAUTH CALLBACK</h2>
        <p>{status}</p>
      </div>
    </div>
  );
}
