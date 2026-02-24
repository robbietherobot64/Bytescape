import { useState } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { AuthStatus, Protocol, PROTOCOLS } from '../types';

interface LoginScreenProps {
    onLoginSuccess: (auth: AuthStatus, shouldImport: boolean) => void;
    onNavigateImport: () => void;
}

export function LoginScreen({ onLoginSuccess, onNavigateImport }: LoginScreenProps) {
    const [selectedProtocol, setSelectedProtocol] = useState<Protocol>("atproto");
    const [identifier, setIdentifier] = useState("");
    const [password, setPassword] = useState("");
    const [importOnLogin, setImportOnLogin] = useState(true);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState("");

    const protocol = PROTOCOLS.find((p) => p.id === selectedProtocol)!;

    async function handleLogin() {
        if (selectedProtocol !== "atproto") {
            if (!protocol.hasApi) {
                // Archive-only platforms go straight to import
                onNavigateImport();
                return;
            }
            setError(`${protocol.label} login coming soon!`);
            return;
        }

        setLoading(true);
        setError("");
        try {
            const status = await invoke<AuthStatus>("login", {
                identifier: identifier.trim(),
                password: password.trim(),
            });

            onLoginSuccess(status, importOnLogin && protocol.hasApi);
        } catch (e: any) {
            setError(typeof e === "string" ? e : e.message || "Login failed");
        } finally {
            setLoading(false);
        }
    }

    return (
        <div className="app-container">
            <div className="login-screen">
                <div className="card login-card card-glow">
                    <h2>bytescape</h2>
                    <p className="login-subtitle">Your Social Vault</p>

                    {/* Protocol Selector */}
                    <div className="protocol-selector">
                        {PROTOCOLS.map((p) => (
                            <button
                                key={p.id}
                                className={`protocol-option ${selectedProtocol === p.id ? "active" : ""}`}
                                onClick={() => { setSelectedProtocol(p.id); setError(""); }}
                            >
                                <span className="protocol-emoji">{p.emoji}</span>
                                <span className="protocol-label">{p.label}</span>
                                <span className="protocol-sub">{p.subtitle}</span>
                            </button>
                        ))}
                    </div>

                    <div className="login-form">
                        <input
                            type="text"
                            placeholder={protocol.field1Placeholder}
                            value={identifier}
                            onChange={(e) => setIdentifier(e.target.value)}
                            onKeyDown={(e) => e.key === "Enter" && handleLogin()}
                        />
                        {protocol.showPassword && (
                            <input
                                type="password"
                                placeholder={protocol.field2Placeholder}
                                value={password}
                                onChange={(e) => setPassword(e.target.value)}
                                onKeyDown={(e) => e.key === "Enter" && handleLogin()}
                            />
                        )}

                        {/* Import checkbox */}
                        <label className="import-checkbox">
                            <input
                                type="checkbox"
                                checked={protocol.hasApi ? importOnLogin : true}
                                onChange={(e) => setImportOnLogin(e.target.checked)}
                                disabled={!protocol.hasApi}
                            />
                            <span>Import data on login</span>
                            {!protocol.hasApi && (
                                <span className="text-muted text-sm"> (archive upload required)</span>
                            )}
                        </label>

                        <button
                            className="btn btn-primary btn-block"
                            onClick={handleLogin}
                            disabled={loading || (!protocol.hasApi && selectedProtocol !== "meta" && selectedProtocol !== "x")}
                        >
                            {loading ? "Signing in..." : (!protocol.hasApi ? `Upload ${protocol.label} Archive` : "Sign In")}
                        </button>
                        {error && <p style={{ color: "var(--accent-red)", fontSize: "0.85rem" }}>{error}</p>}
                    </div>
                    <p className="login-hint">
                        {protocol.hasApi
                            ? "Your identity stays yours. Data never leaves your device."
                            : "No API available — import via data export archive."}
                    </p>
                </div>
            </div>
        </div>
    );
}
