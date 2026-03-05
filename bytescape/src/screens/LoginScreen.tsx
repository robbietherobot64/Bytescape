import { useState } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { AuthStatus } from '../types';
import { Win98Box } from '../components/Win98Box';

interface LoginScreenProps {
    onLoginSuccess: (auth: AuthStatus, shouldImport: boolean) => void;
    onNavigateImport: () => void;
}

export function LoginScreen({ onLoginSuccess, onNavigateImport }: LoginScreenProps) {
    const [mode, setMode] = useState<'welcome' | 'connect-bluesky' | 'import-only'>('welcome');
    const [identifier, setIdentifier] = useState("");
    const [password, setPassword] = useState("");
    const [importOnLogin, setImportOnLogin] = useState(true);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState("");

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

                                    <Win98Box variant="raised" className="login-option-card" onClick={() => setMode('import-only')}>
                                        <span className="login-option-icon">📦</span>
                                        <div className="login-option-text">
                                            <strong>Import an Archive</strong>
                                            <span className="text-muted">Facebook, Instagram, Twitter exports</span>
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
                                    <button className="btn btn-ghost login-back-btn" onClick={() => { setMode('welcome'); setError(''); }}>
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
                                    onKeyDown={(e) => e.key === "Enter" && handleBlueskyLogin()}
                                />
                                <input
                                    className="input-field"
                                    type="password"
                                    placeholder="App Password"
                                    value={password}
                                    onChange={(e) => setPassword(e.target.value)}
                                    onKeyDown={(e) => e.key === "Enter" && handleBlueskyLogin()}
                                />

                                <label className="import-checkbox">
                                    <input
                                        type="checkbox"
                                        checked={importOnLogin}
                                        onChange={(e) => setImportOnLogin(e.target.checked)}
                                    />
                                    <span>Import data on login</span>
                                </label>

                                <button
                                    className="btn btn-primary btn-block"
                                    onClick={handleBlueskyLogin}
                                    disabled={loading}
                                >
                                    {loading ? "Connecting..." : "Connect & Sign In"}
                                </button>
                                {error && <p className="login-error">{error}</p>}

                                <p className="login-hint">
                                    Your data never leaves your device. Bytescape stores everything locally.
                                </p>
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

                        <div className="login-visitor-counter">
                            🌐 visitor #004,891
                        </div>
                    </div>
                </Win98Box>
            </div>
        </div>
    );
}
