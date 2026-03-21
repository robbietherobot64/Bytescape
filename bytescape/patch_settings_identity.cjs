const fs = require('fs');
let content = fs.readFileSync('/home/korendian/Bytescape/bytescape/src/screens/SettingsScreen.tsx', 'utf-8');

const targetRegex = /<h3>🪪 Identity<\/h3>[\s\S]*?(?=<\/div>\s*<\/div>\s*{\/\* Appearance \/ Theme \*\/})/;

const replacement = `<h3>🪪 Identity</h3>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 16 }}>
                    <div>
                        <p className="text-muted text-sm" style={{ margin: 0 }}>
                            Signed in as <strong>{auth?.local_username || auth?.handle}</strong>
                        </p>
                        <p className="text-xs text-muted" style={{ opacity: 0.7 }}>
                            Master Identity: {auth?.did?.substring(0, 16)}...
                            <button
                                onClick={() => {
                                    navigator.clipboard.writeText(auth?.did || '');
                                    alert('DID copied to clipboard');
                                }}
                                style={{ background: 'none', border: 'none', color: 'var(--accent-purple)', cursor: 'pointer', marginLeft: 8 }}
                            >
                                Copy
                            </button>
                        </p>
                    </div>
                    <button className="btn btn-danger" onClick={onLogout}>
                        Sign Out
                    </button>
                </div>

                {/* Linked Accounts */}
                <div style={{ marginTop: 12 }}>
                    <div style={{ fontSize: '0.85rem', fontWeight: 600, marginBottom: 8, color: 'var(--text-secondary)' }}>
                        Linked Accounts
                    </div>
                    
                    {linkedAccounts.map(account => (
                        <div key={account.identifier} style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 14px', background: 'var(--bg-panel)', border: 'var(--border-default)', marginBottom: 8 }}>
                            <span style={{ fontSize: '1.2rem' }}>{account.protocol === 'bluesky' ? '🦋' : '🐘'}</span>
                            <div style={{ flex: 1 }}>
                                <strong style={{ fontSize: '0.9rem' }}>{account.display_handle}</strong>
                                <p className="text-xs text-muted" style={{ margin: 0 }}>{account.protocol.charAt(0).toUpperCase() + account.protocol.slice(1)} · Connected</p>
                            </div>
                            <span style={{ color: 'var(--accent-green)', fontSize: '0.82rem', fontWeight: 600 }}>✓ Linked</span>
                            <button
                                onClick={() => handleDisconnect(account)}
                                style={{
                                    padding: '4px 10px',
                                    fontSize: '0.75rem',
                                    background: 'transparent',
                                    border: '1px solid var(--accent-red, #ff5555)',
                                    color: 'var(--accent-red, #ff5555)',
                                    cursor: 'pointer',
                                    fontFamily: 'inherit',
                                    fontWeight: 500,
                                }}
                            >
                                Disconnect
                            </button>
                        </div>
                    ))}

                    {/* Green Plus — Connect Account */}
                    {!showLinkBluesky && !showLinkMastodon ? (
                        <div style={{ display: 'flex', gap: 10, flexDirection: 'column' }}>
                            <div style={{ display: 'flex', gap: 10 }}>
                                <button
                                    onClick={() => setShowLinkBluesky(true)}
                                    style={{
                                        flex: 1,
                                        display: 'flex',
                                        alignItems: 'center',
                                        justifyContent: 'center',
                                        gap: 8,
                                        padding: '12px',
                                        background: 'rgba(34, 197, 94, 0.1)',
                                        border: '2px dashed #22c55e',
                                        cursor: 'pointer',
                                        fontFamily: 'inherit',
                                        fontSize: '0.9rem',
                                        fontWeight: 600,
                                        color: '#22c55e',
                                        transition: 'background 0.15s',
                                    }}
                                >
                                    ＋ Connect Bluesky
                                </button>
                                <button
                                    onClick={() => setShowLinkMastodon(true)}
                                    style={{
                                        flex: 1,
                                        display: 'flex',
                                        alignItems: 'center',
                                        justifyContent: 'center',
                                        gap: 8,
                                        padding: '12px',
                                        background: 'rgba(34, 197, 94, 0.1)',
                                        border: '2px dashed #22c55e',
                                        cursor: 'pointer',
                                        fontFamily: 'inherit',
                                        fontSize: '0.9rem',
                                        fontWeight: 600,
                                        color: '#22c55e',
                                        transition: 'background 0.15s',
                                    }}
                                >
                                    ＋ Connect Mastodon
                                </button>
                            </div>
                        </div>
                    ) : showLinkBluesky ? (
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 14, background: 'var(--bg-panel)', border: 'var(--border-default)' }}>
                            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                <strong style={{ fontSize: '0.9rem' }}>🦋 Link Bluesky</strong>
                                <button className="btn btn-ghost" onClick={() => { setShowLinkBluesky(false); setLinkError(''); }} style={{ padding: '2px 8px', fontSize: '0.8rem' }}>
                                    Cancel
                                </button>
                            </div>
                            <input
                                className="input-field"
                                type="text"
                                placeholder="Handle (e.g. user.bsky.social)"
                                value={linkIdentifier}
                                onChange={(e) => setLinkIdentifier(e.target.value)}
                                onKeyDown={(e) => e.key === 'Enter' && handleLinkBluesky()}
                            />
                            <input
                                className="input-field"
                                type="password"
                                placeholder="App Password"
                                value={linkPassword}
                                onChange={(e) => setLinkPassword(e.target.value)}
                                onKeyDown={(e) => e.key === 'Enter' && handleLinkBluesky()}
                            />
                            <button
                                className="btn btn-primary"
                                onClick={handleLinkBluesky}
                                disabled={linkLoading}
                            >
                                {linkLoading ? "Linking..." : "Connect & Link"}
                            </button>
                            {linkError && <p className="text-sm" style={{ color: 'var(--accent-red)', margin: 0 }}>{linkError}</p>}
                        </div>
                    ) : (
                        <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 14, background: 'var(--bg-panel)', border: 'var(--border-default)' }}>
                            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                                <strong style={{ fontSize: '0.9rem' }}>🐘 Link Mastodon</strong>
                                <button className="btn btn-ghost" onClick={() => { setShowLinkMastodon(false); setLinkError(''); setMastodonStep('instance'); }} style={{ padding: '2px 8px', fontSize: '0.8rem' }}>
                                    Cancel
                                </button>
                            </div>
                            {mastodonStep === 'instance' ? (
                                <>
                                    <input
                                        className="input-field"
                                        type="text"
                                        placeholder="Instance (e.g. mastodon.social)"
                                        value={mastodonInstance}
                                        onChange={(e) => setMastodonInstance(e.target.value)}
                                        onKeyDown={(e) => e.key === 'Enter' && startMastodonAuth()}
                                    />
                                    <button
                                        className="btn btn-primary"
                                        onClick={startMastodonAuth}
                                        disabled={linkLoading}
                                    >
                                        {linkLoading ? "Loading..." : "Authorize in Browser"}
                                    </button>
                                </>
                            ) : (
                                <>
                                    <input
                                        className="input-field"
                                        type="text"
                                        placeholder="Paste Auth Code"
                                        value={mastodonAuthCode}
                                        onChange={(e) => setMastodonAuthCode(e.target.value)}
                                        onKeyDown={(e) => e.key === 'Enter' && finalizeMastodonAuth()}
                                    />
                                    <button
                                        className="btn btn-primary"
                                        onClick={finalizeMastodonAuth}
                                        disabled={linkLoading}
                                    >
                                        {linkLoading ? "Connecting..." : "Complete Connection"}
                                    </button>
                                </>
                            )}
                            {linkError && <p className="text-sm" style={{ color: 'var(--accent-red)', margin: 0 }}>{linkError}</p>}
                        </div>
                    )}`;

const newContent = content.replace(targetRegex, replacement);

if (newContent === content) {
    console.error("Replacement failed, target regex did not match!");
} else {
    fs.writeFileSync('/home/korendian/Bytescape/bytescape/src/screens/SettingsScreen.tsx', newContent);
    console.log("Replaced successfully!");
}
