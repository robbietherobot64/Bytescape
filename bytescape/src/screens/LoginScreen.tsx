import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AuthStatus, IdentityRecoveryStartResult, IdentityRecoveryVerifyResult, IdentityRecoveryFinalizeResult } from '../types';
import { Win98Box } from '../components/Win98Box';

interface LoginScreenProps {
    onLoginSuccess: (auth: AuthStatus, shouldImport: boolean) => void;
    onNavigateImport: () => void;
}

export function LoginScreen({ onLoginSuccess, onNavigateImport }: LoginScreenProps) {
    const [mode, setMode] = useState<'welcome' | 'connect-bluesky' | 'connect-mastodon' | 'import-only' | 'local-vault' | 'recover'>('welcome');
    const [identifier, setIdentifier] = useState("");
    const [password, setPassword] = useState("");
    const [importOnLogin, setImportOnLogin] = useState(true);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState("");
    const [oauthPending, setOauthPending] = useState(false);
    
    // Mastodon Flow State
    const [isMastodonPending, setIsMastodonPending] = useState(false);
    const [mastodonAuthCode, setMastodonAuthCode] = useState("");
    const [mastodonOauthPending, setMastodonOauthPending] = useState(false);

    // Recovery flow state
    const [recoveryStep, setRecoveryStep] = useState<'paste' | 'code' | 'confirm'>('paste');
    const [bundleInput, setBundleInput] = useState('');
    const [didHint, setDidHint] = useState('');
    const [recoveryCode, setRecoveryCode] = useState('');
    const [verifiedDid, setVerifiedDid] = useState('');
    const [verifiedPubkey, setVerifiedPubkey] = useState('');

    // Listen for OAuth completion/error events from the backend
    useEffect(() => {
        let unlistenComplete: UnlistenFn | undefined;
        let unlistenError: UnlistenFn | undefined;
        let timeoutId: ReturnType<typeof setTimeout> | undefined;

        if (oauthPending) {
            // Set a 5-minute timeout for the OAuth flow
            timeoutId = setTimeout(() => {
                setOauthPending(false);
                setLoading(false);
                setError("OAuth login timed out. Please try again.");
                invoke('cancel_bluesky_oauth').catch(() => {});
            }, 5 * 60 * 1000);

            listen<AuthStatus>('oauth-complete', (event) => {
                setOauthPending(false);
                setLoading(false);
                if (timeoutId) clearTimeout(timeoutId);
                onLoginSuccess(event.payload, importOnLogin);
            }).then(fn => { unlistenComplete = fn; });

            listen<string>('oauth-error', (event) => {
                setOauthPending(false);
                setLoading(false);
                if (timeoutId) clearTimeout(timeoutId);
                setError(typeof event.payload === 'string' ? event.payload : 'OAuth login failed');
            }).then(fn => { unlistenError = fn; });
        }

        return () => {
            if (timeoutId) clearTimeout(timeoutId);
            unlistenComplete?.();
            unlistenError?.();
        };
    }, [oauthPending, importOnLogin, onLoginSuccess]);

    // Listen for Mastodon OAuth callback events from the backend
    useEffect(() => {
        let unlistenComplete: UnlistenFn | undefined;
        let unlistenError: UnlistenFn | undefined;
        let timeoutId: ReturnType<typeof setTimeout> | undefined;

        if (mastodonOauthPending) {
            timeoutId = setTimeout(() => {
                setMastodonOauthPending(false);
                setLoading(false);
                setError("Mastodon login timed out. Please try again.");
            }, 5 * 60 * 1000);

            listen<AuthStatus>('mastodon-auth-complete', (event) => {
                setMastodonOauthPending(false);
                setIsMastodonPending(false);
                setLoading(false);
                if (timeoutId) clearTimeout(timeoutId);
                onLoginSuccess(event.payload, importOnLogin);
            }).then(fn => { unlistenComplete = fn; });

            listen<string>('mastodon-auth-error', (event) => {
                setMastodonOauthPending(false);
                setLoading(false);
                if (timeoutId) clearTimeout(timeoutId);
                setError(typeof event.payload === 'string' ? event.payload : 'Mastodon login failed');
            }).then(fn => { unlistenError = fn; });
        }

        return () => {
            if (timeoutId) clearTimeout(timeoutId);
            unlistenComplete?.();
            unlistenError?.();
        };
    }, [mastodonOauthPending, importOnLogin, onLoginSuccess]);

    async function handleBlueskyOAuth() {
        if (!identifier.trim()) {
            setError("Enter your Bluesky handle (e.g. user.bsky.social)");
            return;
        }
        setLoading(true);
        setError("");
        try {
            const authUrl = await invoke<string>("start_bluesky_oauth", {
                handle: identifier.trim(),
            });
            setOauthPending(true);
            // Open the auth URL in the system browser
            await openUrl(authUrl);
        } catch (e: any) {
            setLoading(false);
            setError(typeof e === "string" ? e : e.message || "Failed to start OAuth");
        }
    }

    async function cancelOAuth() {
        setOauthPending(false);
        setLoading(false);
        setError("");
        try {
            await invoke('cancel_bluesky_oauth');
        } catch (_) {}
    }

    async function handleBlueskyLogin() {
        if (!identifier.trim() || !password.trim()) {
            setError("Handle and app password are required.");
            return;
        }
        setLoading(true);
        setError("");
        try {
            const status = await invoke<AuthStatus>("login", {
                identifier: identifier.trim(),
                password: password.trim(),
            });
            onLoginSuccess(status, importOnLogin);
        } catch (e: any) {
            setError(typeof e === "string" ? e : e.message || "Login failed");
        } finally {
            setLoading(false);
        }
    }

    async function handleMastodonStart() {
        setLoading(true);
        setError("");
        try {
            // Trim and normalize instance URL
            let instance = identifier.trim();
            if (!instance) throw new Error("Instance URL is required");
            if (instance.startsWith("http://")) instance = instance.slice(7);
            if (instance.startsWith("https://")) instance = instance.slice(8);
            instance = instance.split('/')[0];
            
            setIdentifier(instance);

            const authUrl = await invoke<string>('mastodon_register_app', { instance });
            setMastodonOauthPending(true);
            await openUrl(authUrl);
        } catch (e: any) {
            console.error("Mastodon auth start error:", e);
            setError(e.toString());
        } finally {
            setLoading(false);
        }
    }

    async function handleMastodonComplete() {
        if (!mastodonAuthCode.trim()) {
            setError("Please enter the authorization code");
            return;
        }
        setLoading(true);
        setError("");
        try {
            const result = await invoke<AuthStatus>('mastodon_complete_auth', { 
                instance: identifier, 
                authCode: mastodonAuthCode.trim() 
            });
            onLoginSuccess(result, importOnLogin);
        } catch (e: any) {
            console.error("Mastodon auth complete error:", e);
            setError(e.toString());
        } finally {
            setLoading(false);
        }
    }

    async function handleCreateLocalVault() {
        setLoading(true);
        setError("");
        try {
            const status = await invoke<AuthStatus>("create_local_identity");
            onLoginSuccess(status, false);
        } catch (e: any) {
            setError(typeof e === "string" ? e : e.message || "Failed to create local vault");
        } finally {
            setLoading(false);
        }
    }

    async function handleRecoveryStart() {
        const trimmed = bundleInput.trim();
        if (!trimmed) {
            setError("Paste your recovery bundle JSON.");
            return;
        }
        // Quick JSON validation
        try { JSON.parse(trimmed); } catch {
            setError("Invalid JSON. Paste the full escrow bundle.");
            return;
        }
        setLoading(true);
        setError("");
        try {
            const result = await invoke<IdentityRecoveryStartResult>('identity_start_recovery', {
                escrowBundle: trimmed,
            });
            setDidHint(result.did_hint);
            setRecoveryStep('code');
        } catch (e: any) {
            setError(typeof e === 'string' ? e : e.message || 'Failed to parse bundle');
        } finally {
            setLoading(false);
        }
    }

    async function handleRecoveryVerify() {
        if (!recoveryCode.trim()) {
            setError("Enter your recovery code.");
            return;
        }
        setLoading(true);
        setError("");
        try {
            const result = await invoke<IdentityRecoveryVerifyResult>('identity_verify_recovery', {
                recoveryCode: recoveryCode.trim(),
            });
            if (result.verified) {
                setVerifiedDid(result.did);
                setVerifiedPubkey(result.public_key);
                setRecoveryStep('confirm');
            } else {
                setError("Verification failed — wrong recovery code.");
            }
        } catch (e: any) {
            setError(typeof e === 'string' ? e : e.message || 'Verification failed — check your recovery code');
        } finally {
            setLoading(false);
        }
    }

    async function handleRecoveryFinalize() {
        setLoading(true);
        setError("");
        try {
            const result = await invoke<IdentityRecoveryFinalizeResult>('identity_finalize_recovery');
            if (result.restored) {
                // Re-check auth to get full status
                const status = await invoke<AuthStatus | null>('check_auth_status');
                if (status) {
                    onLoginSuccess(status, false);
                }
            } else {
                setError("Restore failed unexpectedly.");
            }
        } catch (e: any) {
            setError(typeof e === 'string' ? e : e.message || 'Failed to finalize recovery');
        } finally {
            setLoading(false);
        }
    }

    function resetRecovery() {
        setRecoveryStep('paste');
        setBundleInput('');
        setDidHint('');
        setRecoveryCode('');
        setVerifiedDid('');
        setVerifiedPubkey('');
        setError('');
    }

    return (
        <div className="app-container">
            <div className="login-screen">
                <Win98Box variant="window" title="Welcome to Bytescape" className="login-card card-glow">
                    <div className="login-inner">
                        <h2 className="login-title">bytescape</h2>
                        <p className="login-subtitle">Your Social Vault</p>

                        {mode === 'welcome' && (
                            <div className="login-welcome">
                                <p className="login-description">
                                    One identity. All your social data. Owned by you.
                                </p>

                                <div className="login-options">
                                    <Win98Box variant="raised" className="login-option-card" onClick={() => setMode('connect-bluesky')}>
                                        <span className="login-option-icon">🦋</span>
                                        <div className="login-option-text">
                                            <strong>Connect with Bluesky</strong>
                                            <span className="text-muted">Sign in and import your feed</span>
                                        </div>
                                    </Win98Box>

                                    <Win98Box variant="raised" className="login-option-card" onClick={() => setMode('connect-mastodon')}>
                                        <span className="login-option-icon">🌐</span>
                                        <div className="login-option-text">
                                            <strong>Connect with Mastodon</strong>
                                            <span className="text-muted">Federate with the open web</span>
                                        </div>
                                    </Win98Box>

                                    <Win98Box variant="raised" className="login-option-card" onClick={() => setMode('import-only')}>
                                        <span className="login-option-icon">📦</span>
                                        <div className="login-option-text">
                                            <strong>Import an Archive</strong>
                                            <span className="text-muted">Facebook, Instagram, Twitter exports</span>
                                        </div>
                                    </Win98Box>

                                    <Win98Box variant="raised" className="login-option-card" onClick={() => setMode('local-vault')}>
                                        <span className="login-option-icon">🔒</span>
                                        <div className="login-option-text">
                                            <strong>Create Local Vault</strong>
                                            <span className="text-muted">Offline-only identity, no accounts needed</span>
                                        </div>
                                    </Win98Box>

                                    <Win98Box variant="raised" className="login-option-card" onClick={() => { setMode('recover'); resetRecovery(); }}>
                                        <span className="login-option-icon">🔑</span>
                                        <div className="login-option-text">
                                            <strong>Recover Identity</strong>
                                            <span className="text-muted">Restore from a recovery bundle</span>
                                        </div>
                                    </Win98Box>
                                </div>

                                <p className="login-hint">
                                    Link additional accounts later in Settings.
                                </p>
                            </div>
                        )}

                        {mode === 'connect-bluesky' && (
                            <div className="login-form">
                                <div className="login-form-header">
                                    <button className="btn btn-ghost login-back-btn" onClick={() => { setMode('welcome'); setError(''); setOauthPending(false); cancelOAuth(); }}>
                                        ← Back
                                    </button>
                                    <span className="login-form-title">🦋 Connect Bluesky</span>
                                </div>

                                <input
                                    className="input-field"
                                    type="text"
                                    placeholder="Handle (e.g. user.bsky.social)"
                                    value={identifier}
                                    onChange={(e) => setIdentifier(e.target.value)}
                                    onKeyDown={(e) => e.key === "Enter" && !oauthPending && handleBlueskyOAuth()}
                                    disabled={oauthPending}
                                />

                                <label className="import-checkbox">
                                    <input
                                        type="checkbox"
                                        checked={importOnLogin}
                                        onChange={(e) => setImportOnLogin(e.target.checked)}
                                        disabled={oauthPending}
                                    />
                                    <span>Import data on login</span>
                                </label>

                                {!oauthPending ? (
                                    <>
                                        <button
                                            className="btn btn-primary btn-block"
                                            onClick={handleBlueskyOAuth}
                                            disabled={loading}
                                        >
                                            {loading ? "Connecting..." : "Login with Bluesky"}
                                        </button>

                                        <details className="login-app-password-fallback" style={{ marginTop: '12px' }}>
                                            <summary style={{ cursor: 'pointer', color: 'var(--text-muted)', fontSize: '13px' }}>
                                                Use App Password instead
                                            </summary>
                                            <div style={{ marginTop: '8px' }}>
                                                <input
                                                    className="input-field"
                                                    type="password"
                                                    placeholder="App Password"
                                                    value={password}
                                                    onChange={(e) => setPassword(e.target.value)}
                                                    onKeyDown={(e) => e.key === "Enter" && handleBlueskyLogin()}
                                                />
                                                <button
                                                    className="btn btn-block"
                                                    onClick={handleBlueskyLogin}
                                                    disabled={loading}
                                                    style={{ marginTop: '8px', background: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}
                                                >
                                                    {loading ? "Connecting..." : "Sign In with App Password"}
                                                </button>
                                            </div>
                                        </details>
                                    </>
                                ) : (
                                    <div style={{ textAlign: 'center', padding: '16px 0' }}>
                                        <p style={{ color: 'var(--accent-purple)', marginBottom: '8px' }}>
                                            ⏳ Waiting for authentication...
                                        </p>
                                        <p style={{ color: 'var(--text-muted)', fontSize: '13px', marginBottom: '16px' }}>
                                            Complete the login in your browser, then return here.
                                        </p>
                                        <button
                                            className="btn btn-ghost"
                                            onClick={cancelOAuth}
                                        >
                                            Cancel
                                        </button>
                                    </div>
                                )}

                                {error && <p className="login-error">{error}</p>}

                                <p className="login-hint">
                                    Your data never leaves your device. Bytescape stores everything locally.
                                </p>
                            </div>
                        )}

                        {mode === 'connect-mastodon' && (
                            <div className="login-form">
                                <div className="login-form-header">
                                    <button className="btn btn-ghost login-back-btn" onClick={() => { 
                                        setMode('welcome'); 
                                        setError('');
                                        setIsMastodonPending(false); 
                                    }}>
                                        ← Back
                                    </button>
                                    <span className="login-form-title">🌐 Connect Mastodon</span>
                                </div>
                                <p style={{ fontSize: '0.9rem', marginBottom: 15, color: 'var(--text-muted)' }}>
                                    Enter your Mastodon instance URL to authorize Bytescape. Support and unified feed commands coming shortly.
                                </p>

                                {!isMastodonPending ? (
                                    <>
                                        <div className="field-group">
                                            <label>Instance URL</label>
                                            <input 
                                                type="text" 
                                                placeholder="e.g. mastodon.social" 
                                                className="retro-input" 
                                                value={identifier} 
                                                onChange={e => setIdentifier(e.target.value)}
                                                onKeyDown={(e) => { 
                                                    if (e.key === 'Enter') handleMastodonStart();
                                                }}
                                                disabled={loading}
                                            />
                                        </div>
                                        {error && <div className="login-error">{error}</div>}
                                        <button
                                            className="btn btn-primary"
                                            style={{ width: '100%', marginTop: 10 }}
                                            onClick={handleMastodonStart}
                                            disabled={loading}
                                        >
                                            {loading ? 'Connecting...' : 'Proceed to Instance'}
                                        </button>
                                    </>
                                ) : (
                                    <>
                                        <div className="field-group">
                                            <label>Authorization Code</label>
                                            <p className="login-hint" style={{ marginTop: 0, marginBottom: 8 }}>
                                                Paste the code from your browser below.
                                            </p>
                                            <input 
                                                type="text" 
                                                placeholder="Paste code here" 
                                                className="retro-input" 
                                                value={mastodonAuthCode} 
                                                onChange={e => setMastodonAuthCode(e.target.value)}
                                                onKeyDown={(e) => { 
                                                    if (e.key === 'Enter') handleMastodonComplete();
                                                }}
                                                disabled={loading}
                                            />
                                        </div>

                                        <div className="login-options-row" style={{ marginTop: 15 }}>
                                            <label style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: '0.9rem' }}>
                                                <input 
                                                    type="checkbox" 
                                                    checked={importOnLogin} 
                                                    onChange={e => setImportOnLogin(e.target.checked)} 
                                                />
                                                Sync Feed on Login
                                            </label>
                                        </div>

                                        {error && <div className="login-error">{error}</div>}
                                        <button
                                            className="btn btn-primary"
                                            style={{ width: '100%', marginTop: 10 }}
                                            onClick={handleMastodonComplete}
                                            disabled={loading}
                                        >
                                            {loading ? 'Completing...' : 'Complete Login'}
                                        </button>
                                    </>
                                )}
                            </div>
                        )}

                        {mode === 'import-only' && (
                            <div className="login-form">
                                <div className="login-form-header">
                                    <button className="btn btn-ghost login-back-btn" onClick={() => { setMode('welcome'); setError(''); }}>
                                        ← Back
                                    </button>
                                    <span className="login-form-title">📦 Import Archive</span>
                                </div>

                                <p className="login-description">
                                    Upload a data export from Facebook, Instagram, Twitter, or Google Takeout. No account login required.
                                </p>

                                <button
                                    className="btn btn-primary btn-block"
                                    onClick={onNavigateImport}
                                >
                                    Choose Archive File
                                </button>

                                <p className="login-hint">
                                    You can connect accounts later to get live feeds.
                                </p>
                            </div>
                        )}

                        {mode === 'local-vault' && (
                            <div className="login-form">
                                <div className="login-form-header">
                                    <button className="btn btn-ghost login-back-btn" onClick={() => { setMode('welcome'); setError(''); }}>
                                        ← Back
                                    </button>
                                    <span className="login-form-title">🔒 Create Local Vault</span>
                                </div>

                                <p className="login-description">
                                    Generate a standalone local identity. Your data stays entirely on this device — no external accounts needed.
                                </p>

                                <button
                                    className="btn btn-primary btn-block"
                                    onClick={handleCreateLocalVault}
                                    disabled={loading}
                                >
                                    {loading ? "Generating identity..." : "Create My Vault"}
                                </button>
                                {error && <p className="login-error">{error}</p>}

                                <p className="login-hint">
                                    You'll choose a username next. Connect external accounts anytime in Settings.
                                </p>
                            </div>
                        )}

                        {mode === 'recover' && (
                            <div className="login-form">
                                <div className="login-form-header">
                                    <button className="btn btn-ghost login-back-btn" onClick={() => { setMode('welcome'); resetRecovery(); }}>
                                        ← Back
                                    </button>
                                    <span className="login-form-title">🔑 Recover Identity</span>
                                </div>

                                {/* Step indicator */}
                                <div className="recovery-steps-bar">
                                    <span className={`recovery-step-dot ${recoveryStep === 'paste' ? 'active' : 'done'}`}>1</span>
                                    <span className="recovery-step-line" />
                                    <span className={`recovery-step-dot ${recoveryStep === 'code' ? 'active' : recoveryStep === 'confirm' ? 'done' : ''}`}>2</span>
                                    <span className="recovery-step-line" />
                                    <span className={`recovery-step-dot ${recoveryStep === 'confirm' ? 'active' : ''}`}>3</span>
                                </div>

                                {recoveryStep === 'paste' && (
                                    <>
                                        <p className="login-description">
                                            Paste the recovery bundle you saved when you set up your identity.
                                        </p>
                                        <textarea
                                            className="input-field recovery-bundle-input"
                                            placeholder='{"version":1,"created_at":...,"did":"did:key:z6Mk...",...}'
                                            value={bundleInput}
                                            onChange={(e) => { setBundleInput(e.target.value); setError(''); }}
                                            rows={5}
                                        />
                                        <button
                                            className="btn btn-primary btn-block"
                                            onClick={handleRecoveryStart}
                                            disabled={loading || !bundleInput.trim()}
                                        >
                                            {loading ? 'Validating...' : 'Validate Bundle'}
                                        </button>
                                    </>
                                )}

                                {recoveryStep === 'code' && (
                                    <>
                                        <p className="login-description">
                                            Bundle verified! This will restore the identity:
                                        </p>
                                        <div className="recovery-did-hint">
                                            <span className="recovery-did-label">DID</span>
                                            <span className="recovery-did-value" title={didHint}>
                                                {didHint.slice(0, 20)}...{didHint.slice(-8)}
                                            </span>
                                        </div>
                                        <p className="login-description">
                                            Enter the recovery code you used when creating this bundle.
                                        </p>
                                        <input
                                            className="input-field"
                                            type="password"
                                            placeholder="Your recovery code"
                                            value={recoveryCode}
                                            onChange={(e) => { setRecoveryCode(e.target.value); setError(''); }}
                                            onKeyDown={(e) => e.key === 'Enter' && handleRecoveryVerify()}
                                            autoFocus
                                        />
                                        <button
                                            className="btn btn-primary btn-block"
                                            onClick={handleRecoveryVerify}
                                            disabled={loading || !recoveryCode.trim()}
                                        >
                                            {loading ? 'Decrypting...' : 'Verify Code'}
                                        </button>
                                    </>
                                )}

                                {recoveryStep === 'confirm' && (
                                    <>
                                        <div className="recovery-success-icon">✅</div>
                                        <p className="login-description">
                                            Recovery code verified! Ready to restore your identity.
                                        </p>
                                        <div className="recovery-did-hint">
                                            <span className="recovery-did-label">DID</span>
                                            <span className="recovery-did-value" title={verifiedDid}>
                                                {verifiedDid.slice(0, 20)}...{verifiedDid.slice(-8)}
                                            </span>
                                        </div>
                                        <div className="recovery-did-hint">
                                            <span className="recovery-did-label">Key</span>
                                            <span className="recovery-did-value" title={verifiedPubkey}>
                                                {verifiedPubkey.slice(0, 16)}...{verifiedPubkey.slice(-8)}
                                            </span>
                                        </div>
                                        <button
                                            className="btn btn-primary btn-block"
                                            onClick={handleRecoveryFinalize}
                                            disabled={loading}
                                        >
                                            {loading ? 'Restoring...' : '🔓 Restore My Identity'}
                                        </button>
                                    </>
                                )}

                                {error && <p className="login-error">{error}</p>}

                                <p className="login-hint">
                                    {recoveryStep === 'paste' && "Don't have a bundle? You'll need to create a new identity instead."}
                                    {recoveryStep === 'code' && "The recovery code decrypts the escrow bundle to recover your private key."}
                                    {recoveryStep === 'confirm' && "This will replace any existing identity on this device."}
                                </p>
                            </div>
                        )}

                        <div className="login-visitor-counter">
                            🌐 visitor #004,891
                        </div>
                    </div>
                </Win98Box>
            </div>
        </div>
    );
}
