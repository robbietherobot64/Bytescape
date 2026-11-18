import { useState, useEffect, useCallback } from 'react';
import { accountApi, oauthApi, userFromToken, apiErrorDisplay, type LinkedAccount } from '../utils/api';

function displayExternalName(username: string): string {
  return username.replace(/@https?:\/\//g, '@').replace(/https?:\/\//g, '');
}

type AddMethod = 'none' | 'bluesky' | 'mastodon' | 'import' | 'import-guide';

/** One-per-network rule (account/connections spec): at most one Bluesky link
 *  and at most one Mastodon link per Bytescape account. The API enforces this;
 *  the UI mirrors it by hiding the Add card for a filled network. */
function hasNetwork(accounts: LinkedAccount[], network: 'bluesky' | 'mastodon'): boolean {
  return accounts.some((a) => a.provider === network);
}

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' });
  } catch { return iso; }
}

export default function AccountScreen() {
  const [accounts, setAccounts] = useState<LinkedAccount[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [addMethod, setAddMethod] = useState<AddMethod>('none');

  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState('');
  const [formSuccess, setFormSuccess] = useState('');
  const [deleting, setDeleting] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [deleteConfirmInput, setDeleteConfirmInput] = useState('');
  const [exporting, setExporting] = useState(false);

  // Mastodon link form
  const [mastoInstance, setMastoInstance] = useState('');

  // Import
  const [importFile, setImportFile] = useState<File | null>(null);
  const [importing, setImporting] = useState(false);
  const [importResult, setImportResult] = useState<{ imported: number; errors: string[] } | null>(null);
  const [importPhase, setImportPhase] = useState('');

  // Import guide
  const [guideStep, setGuideStep] = useState(0);

  const loadAccounts = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const accts = await accountApi.list();
      setAccounts(accts);
    } catch (e) {
      setError(apiErrorDisplay(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { loadAccounts(); }, [loadAccounts]);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.get('provider') && params.get('success') === 'true') {
      window.history.replaceState({}, '', '/app');
      setFormSuccess(`${params.get('provider')} account linked successfully!`);
      loadAccounts();
      window.dispatchEvent(new CustomEvent('bytescape:accountLinked'));
    }
    if (sessionStorage.getItem('oauth_pending') === 'true') {
      sessionStorage.removeItem('oauth_pending');
      sessionStorage.removeItem('oauth_method');
      sessionStorage.removeItem('oauth_instance');
      loadAccounts();
    }
  }, [loadAccounts]);

  const handleBlueskyLink = async () => {
    setSubmitting(true);
    setFormError('');
    setFormSuccess('');
    try {
      const result = await oauthApi.blueskyAuthorize();
      if (result.url) {
        sessionStorage.setItem('oauth_method', 'bluesky');
        window.location.href = result.url;
      } else {
        setFormError('Failed to start Bluesky OAuth flow');
        setSubmitting(false);
      }
    } catch (e) {
      setFormError(apiErrorDisplay(e));
      setSubmitting(false);
    }
  };

  const handleMastodonLink = async () => {
    const inst = mastoInstance.trim();
    if (!inst) return;
    setSubmitting(true);
    setFormError('');
    setFormSuccess('');
    try {
      const host = inst.includes('@') ? inst.split('@').filter(Boolean).pop()! : inst;
      sessionStorage.setItem('oauth_method', 'mastodon');
      const result = await oauthApi.mastodonStart(host);
      window.location.href = result.url;
    } catch (e) {
      setFormError(apiErrorDisplay(e));
      setSubmitting(false);
    }
  };

  const handleUnlink = async (id: string) => {
    if (!confirm('Unlink this account?')) return;
    try {
      await accountApi.unlink(id);
      loadAccounts();
    } catch (e) {
      setError(apiErrorDisplay(e));
    }
  };

  return (
    <div className="screen accounts-screen">
      {error && <div className="error-text" style={{ margin: '8px 20px' }}>{error}</div>}
      {formError && <div className="error-text" style={{ margin: '8px 20px' }}>{formError}</div>}
      {formSuccess && <div className="success-text" style={{ margin: '8px 20px' }}>{formSuccess}</div>}

      <div className="accounts-scroll">
        {/* ═══ Connections ═══ */}
        <section>
          <h2 className="section-title">Connections</h2>
          {loading ? (
            <div className="empty-feed"><p>Loading...</p></div>
          ) : accounts.length === 0 ? (
            <div className="empty-feed">
              <p>No accounts linked yet. Add one below.</p>
            </div>
          ) : (
            <div className="accounts-list">
              {accounts.map(acct => (
                <div key={acct.id} className="account-card">
                  <div className="account-card-top">
                    <div className="account-provider">
                      <span className={`provider-badge ${acct.provider}`}>
                        {acct.provider === 'bluesky' ? '🦋' : '🐘'} {acct.provider}
                      </span>
                    </div>
                    <div className="account-info">
                      <span className="account-handle">{displayExternalName(acct.external_username)}</span>
                      <span className="account-date">Linked {formatDate(acct.created_at)}</span>
                    </div>
                    <button className="btn" onClick={() => handleUnlink(acct.id)} style={{ padding: '4px 12px', fontSize: 11, flexShrink: 0 }}>UNLINK</button>
                  </div>
                  <div className="account-card-stats">
                    <div className="account-stat">
                      <span className="stat-value">{acct.posts}</span>
                      <span className="stat-label">Posts</span>
                    </div>
                    <div className="account-stat">
                      <span className="stat-value">{acct.following}</span>
                      <span className="stat-label">Following</span>
                    </div>
                    <div className="account-stat">
                      <span className="stat-value">{acct.followers}</span>
                      <span className="stat-label">Followers</span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}

          <div className="accounts-add-section">
            <h3 className="section-title" style={{ fontSize: 15 }}>Add a connection</h3>
            {addMethod === 'none' && (
              <div className="add-methods">
                {!hasNetwork(accounts, 'bluesky') && (
                  <div className="link-card" onClick={() => setAddMethod('bluesky')}>
                    <span className="link-card-icon">🦋</span>
                    <div className="link-card-info">
                      <span className="link-card-title">Bluesky</span>
                      <span className="link-card-desc">Link with OAuth (secure)</span>
                    </div>
                  </div>
                )}
                {!hasNetwork(accounts, 'mastodon') && (
                  <div className="link-card" onClick={() => setAddMethod('mastodon')}>
                    <span className="link-card-icon">🐘</span>
                    <div className="link-card-info">
                      <span className="link-card-title">Mastodon</span>
                      <span className="link-card-desc">Link your fediverse account</span>
                    </div>
                  </div>
                )}
                {(hasNetwork(accounts, 'bluesky') && hasNetwork(accounts, 'mastodon')) && (
                  <p style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                    One link per network — both Bluesky and Mastodon are linked.
                  </p>
                )}
              </div>
            )}

            {addMethod === 'bluesky' && (
              <div className="link-form">
                <h3 className="link-form-title">🦋 Link Bluesky Account</h3>
                <p className="link-form-help">
                  You'll be redirected to Bluesky to authorize Bytescape.
                  No password needed — uses secure OAuth.
                </p>
                <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
                  <button className="btn" onClick={handleBlueskyLink} disabled={submitting}>
                    {submitting ? 'Redirecting...' : '> CONNECT WITH BLUESKY'}
                  </button>
                  <button className="btn" onClick={() => { setAddMethod('none'); setFormError(''); }}>Cancel</button>
                </div>
              </div>
            )}

            {addMethod === 'mastodon' && (
              <div className="link-form">
                <h3 className="link-form-title">🐘 Link Mastodon Account</h3>
                <p className="link-form-help">
                  Enter your instance (or a full @user@instance handle). You'll be
                  redirected to your instance to authorize Bytescape.
                </p>
                <div style={{ display: 'flex', gap: 8, marginTop: 12, flexWrap: 'wrap' }}>
                  <input
                    className="input-field"
                    placeholder="instance (e.g. mastodon.social)"
                    value={mastoInstance}
                    onChange={e => setMastoInstance(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') handleMastodonLink(); }}
                    style={{ flex: 1, minWidth: 200 }}
                  />
                  <button className="btn" onClick={handleMastodonLink} disabled={submitting || !mastoInstance.trim()}>
                    {submitting ? 'Redirecting...' : '> CONNECT'}
                  </button>
                  <button className="btn" onClick={() => { setAddMethod('none'); setFormError(''); }}>Cancel</button>
                </div>
              </div>
            )}
          </div>
        </section>

        {/* ═══ Data ═══ */}
        <section className="accounts-import-section">
          <h2 className="section-title">Data</h2>

          <div className="account-card">
            <div className="account-card-header" style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '14px 16px 10px' }}>
              <span style={{ fontSize: 24 }}>📦</span>
              <div>
                <div style={{ fontFamily: 'var(--font-retro)', fontSize: 16, color: 'var(--text-primary)' }}>Import Data Archive</div>
                <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>Facebook, Instagram, or Threads GDPR export</div>
              </div>
            </div>
            <div className="account-card-body" style={{ padding: '0 16px 14px' }}>
              {addMethod !== 'import' && addMethod !== 'import-guide' && (
                <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 8 }}>
                  <button className="btn" onClick={() => setAddMethod('import')} style={{ fontSize: 13, padding: '8px 16px' }}>
                    📁 I have my archive
                  </button>
                  <button className="btn" onClick={() => setAddMethod('import-guide')} style={{ fontSize: 13, padding: '8px 16px' }}>
                    📋 Guide me to request data
                  </button>
                </div>
              )}

              {addMethod === 'import' && (
                <div style={{ marginTop: 12 }}>
                  <p style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 8 }}>
                    Upload your GDPR archive ZIP. We auto-detect Facebook, Instagram, or Threads.
                  </p>
                  <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                    <input
                      type="file"
                      accept=".zip"
                      style={{ fontSize: 13 }}
                      onChange={e => setImportFile(e.target.files?.[0] || null)}
                      disabled={importing}
                    />
                    <button
                      className="btn"
                      onClick={async () => {
                        if (!importFile) return;
                        setImporting(true);
                        setImportResult(null);
                        setImportPhase('Uploading...');
                        try {
                          const token = localStorage.getItem('bytescape_token') || '';
                          const formData = new FormData();
                          formData.append('file', importFile);
                          const res = await fetch('/api/import/stream', {
                            method: 'POST',
                            headers: { 'Authorization': `Bearer ${token}` },
                            body: formData,
                          });
                          const reader = res.body?.getReader();
                          if (!reader) throw new Error('No response body');
                          let imported = 0;
                          let errors: string[] = [];
                          const decoder = new TextDecoder();
                          let buf = '';
                          while (true) {
                            const { done, value } = await reader.read();
                            if (done) break;
                            buf += decoder.decode(value, { stream: true });
                            const lines = buf.split('\n');
                            buf = lines.pop() || '';
                            for (const line of lines) {
                              if (line.startsWith('data: ')) {
                                try {
                                  const evt = JSON.parse(line.slice(6));
                                  if (evt.type === 'phase') setImportPhase(evt.phase);
                                  if (evt.type === 'post') imported++;
                                  if (evt.type === 'error') errors.push(evt.message);
                                  if (evt.type === 'complete') {
                                    imported = evt.imported;
                                    errors = evt.errors || errors;
                                    setImportPhase('Complete');
                                  }
                                } catch {}
                              }
                            }
                          }
                          setImportResult({ imported, errors });
                          loadAccounts();
                          window.dispatchEvent(new CustomEvent('bytescape:refreshFeed'));
                        } catch (e) {
                          setImportResult({ imported: 0, errors: [apiErrorDisplay(e)] });
                          setImportPhase('Failed');
                        } finally {
                          setImporting(false);
                        }
                      }}
                      disabled={!importFile || importing}
                      style={{ fontSize: 13, padding: '8px 20px' }}
                    >
                      {importing ? 'Importing...' : 'Upload Archive'}
                    </button>
                    <button className="btn" onClick={() => { setAddMethod('none'); setImportResult(null); setImportPhase(''); }} style={{ fontSize: 13, padding: '8px 16px' }}>
                      Cancel
                    </button>
                  </div>
                  {importing && importPhase && (
                    <div style={{ marginTop: 8, fontSize: 13, color: 'var(--text-muted)' }}>
                      <span style={{ color: importPhase === 'Complete' ? '#4caf50' : importPhase === 'Failed' ? 'var(--danger)' : 'var(--accent)' }}>
                        {importPhase === 'Complete' ? '✅ ' : importPhase === 'Failed' ? '❌ ' : '> '}{importPhase}
                      </span>
                      {importPhase !== 'Complete' && importPhase !== 'Failed' && (
                        <span style={{ animation: 'blink 1s infinite', marginLeft: 2 }}>▋</span>
                      )}
                    </div>
                  )}
                  {importResult && (
                    <div style={{ marginTop: 8, color: importResult.errors.length > 0 && importResult.imported === 0 ? 'var(--danger)' : '#4caf50', fontSize: 13 }}>
                      {importResult.imported > 0 && `✅ Imported ${importResult.imported} items`}
                      {importResult.errors.length > 0 && (
                        <ul style={{ color: 'var(--danger)', fontSize: 12, paddingLeft: 16, margin: '4px 0 0' }}>
                          {importResult.errors.slice(0, 5).map((err, i) => <li key={i}>{err}</li>)}
                        </ul>
                      )}
                    </div>
                  )}
                </div>
              )}

              {addMethod === 'import-guide' && (
                <div style={{ marginTop: 12 }}>
                  <p style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 10 }}>
                    All Meta platforms use the same data request page:
                  </p>
                  <a href="https://accountscenter.facebook.com/info_and_permissions/dyi" target="_blank" rel="noopener" className="btn" style={{ fontSize: 13, padding: '8px 16px', textDecoration: 'none', display: 'inline-block', marginBottom: 10 }}>
                    Open Meta Data Request ↗
                  </a>
                  <ol style={{ fontSize: 12, color: 'var(--text-muted)', paddingLeft: 20, lineHeight: 1.8 }}>
                    <li>Click the link above and log into your Facebook/Instagram/Threads account</li>
                    <li>Select <strong>Download your information</strong></li>
                    <li>Choose <strong>JSON</strong> format (recommended for best import fidelity)</li>
                    <li>Select all categories you want to import</li>
                    <li>Click <strong>Submit request</strong> — it may take 1-2 days to prepare</li>
                    <li>You'll get an email when your archive is ready to download</li>
                    <li>Come back here and click <strong>I have my archive</strong> to upload it</li>
                  </ol>
                  <button className="btn" onClick={() => setAddMethod('import')} style={{ fontSize: 13, padding: '8px 16px', marginTop: 8 }}>
                    I have my archive ready →
                  </button>
                </div>
              )}
            </div>
          </div>

          <div style={{ marginTop: 16 }}>
            <button
              className="btn btn-block"
              disabled={exporting}
              onClick={async () => {
                setExporting(true);
                try {
                  await accountApi.exportData();
                } catch (e) {
                  alert('Export failed: ' + apiErrorDisplay(e));
                } finally {
                  setExporting(false);
                }
              }}
            >
              {exporting ? 'Exporting...' : '📤 Export My Data'}
            </button>
          </div>
        </section>

        {/* ═══ Deletion ═══ */}
        <section className="accounts-import-section">
          <h2 className="section-title">Deletion</h2>
          <p style={{ color: 'var(--text-muted)', fontSize: 12, marginBottom: 12 }}>
            Delete your account — immediately, or with a 30-day window to restore it.
          </p>
          <button
            className="btn"
            style={{ background: 'var(--danger)', color: '#fff', border: 'none', padding: '10px 24px' }}
            onClick={() => setShowDeleteModal(true)}
          >
            Delete Account
          </button>
        </section>
      </div>

      {showDeleteModal && (
        <div
          onClick={(e) => { if (e.target === e.currentTarget) setShowDeleteModal(false); }}
          style={{
            position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
            background: 'rgba(0,0,0,0.7)', display: 'flex', alignItems: 'center', justifyContent: 'center',
            zIndex: 2000,
          }}
        >
          <div style={{
            background: 'var(--bg-secondary, #1a1a2e)', border: '2px solid var(--danger)',
            borderRadius: 12, padding: 24, maxWidth: 440, width: '90%',
            maxHeight: '80vh', overflowY: 'auto',
          }}>
            <h2 style={{ color: 'var(--danger)', fontFamily: 'var(--font-retro)', fontSize: 18, marginTop: 0, marginBottom: 12 }}>
              Delete your account
            </h2>
            <p style={{ color: 'var(--text-primary)', fontSize: 13, marginBottom: 14 }}>
              This action cannot be undone. All your data will be permanently deleted.
            </p>
            <div style={{ marginBottom: 14 }}>
              <h3 style={{ fontSize: 12, color: 'var(--text-primary)', marginBottom: 6 }}>What will be deleted:</h3>
              <ul style={{ color: 'var(--text-muted)', fontSize: 12, paddingLeft: 18, margin: 0, lineHeight: 1.6 }}>
                <li>Your profile, posts, and media</li>
                <li>All messages (E2EE)</li>
                <li>All imported data and connections</li>
                <li>Linked platform connections (not the accounts themselves)</li>
              </ul>
            </div>
            <div style={{ marginBottom: 14, padding: '8px 12px', background: 'var(--surface-secondary)', borderRadius: 6 }}>
              <h3 style={{ fontSize: 12, color: 'var(--text-primary)', marginBottom: 4 }}>What will NOT be deleted:</h3>
              <ul style={{ color: 'var(--text-muted)', fontSize: 12, paddingLeft: 18, margin: 0, lineHeight: 1.6 }}>
                <li>Posts federated to other instances (they control their copy)</li>
                <li>Other people's comments on your posts</li>
              </ul>
            </div>
            <div className="form-group" style={{ marginBottom: 14 }}>
              <label style={{ color: 'var(--text-primary)', fontSize: 12, marginBottom: 6, display: 'block' }}>
                Type your username to confirm: <strong>@{userFromToken()?.username || '...'}</strong>
              </label>
              <input
                className="input-field"
                placeholder={`@${userFromToken()?.username || ''}`}
                value={deleteConfirmInput}
                onChange={e => setDeleteConfirmInput(e.target.value)}
                autoComplete="off"
                style={{ borderColor: 'var(--danger)' }}
              />
            </div>
            <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
              <button
                className="btn"
                style={{
                  background: deleteConfirmInput === `@${userFromToken()?.username}` ? 'var(--danger)' : '#555',
                  color: '#fff', border: 'none',
                  opacity: deleteConfirmInput === `@${userFromToken()?.username}` ? 1 : 0.5,
                  cursor: deleteConfirmInput === `@${userFromToken()?.username}` ? 'pointer' : 'not-allowed',
                }}
                disabled={deleting || deleteConfirmInput !== `@${userFromToken()?.username}`}
                onClick={async () => {
                  setDeleting(true);
                  try {
                    await accountApi.deleteAccount(true);
                    localStorage.clear();
                    window.location.reload();
                  } catch (e) {
                    alert('Delete failed: ' + apiErrorDisplay(e));
                  } finally {
                    setDeleting(false);
                  }
                }}
              >
                {deleting ? 'Processing...' : 'Delete immediately'}
              </button>
              <button
                className="btn"
                style={{
                  background: '#555', color: '#fff', border: 'none',
                  opacity: deleteConfirmInput === `@${userFromToken()?.username}` ? 1 : 0.5,
                  cursor: deleteConfirmInput === `@${userFromToken()?.username}` ? 'pointer' : 'not-allowed',
                }}
                disabled={deleting || deleteConfirmInput !== `@${userFromToken()?.username}`}
                onClick={async () => {
                  setDeleting(true);
                  try {
                    const result = await accountApi.deleteAccount(false);
                    const delDate = result.deleted_at ? new Date(result.deleted_at).toLocaleDateString() : 'in 30 days';
                    alert(`Account scheduled for deletion on ${delDate}. You can restore by logging back in.`);
                    setShowDeleteModal(false);
                    setDeleteConfirmInput('');
                  } catch (e) {
                    alert('Delete failed: ' + apiErrorDisplay(e));
                  } finally {
                    setDeleting(false);
                  }
                }}
              >
                Delete in 30 days
              </button>
              <button className="btn" onClick={() => { setShowDeleteModal(false); setDeleteConfirmInput(''); }} disabled={deleting} style={{ marginLeft: 'auto' }}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
