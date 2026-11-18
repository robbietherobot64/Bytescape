import { useState, useRef, useEffect, type ReactNode } from 'react';
import { importApi, accountApi } from '../utils/api';
import ImportResults from './ImportResults';

interface Props {
  onComplete: () => void;
  onSkip: () => void;
  /** Render as an embedded full-screen flow instead of a centered modal. */
  embedded?: boolean;
}

type ImportStepPhase = 'entry' | 'upload' | 'guide-platform' | 'waiting' | 'processing';

const metaDyiLink = 'https://accountscenter.facebook.com/info_and_permissions/dyi';

type Platform = 'facebook' | 'instagram' | 'threads' | 'mastodon';

const platformGuides: Record<Platform, { name: string; icon: string; steps: string[] }> = {
  facebook: {
    name: 'Facebook',
    icon: '📘',
    steps: [
      'Click "Create export" on the Meta Accounts Center page.',
      'Choose the Facebook profile you want to export.',
      'Click "Export to device".',
      'Set Date Range to "All time".',
      'Set Format to "JSON" (recommended for best import fidelity — HTML also works but JSON is more reliable).',
      'Set Media quality to "High".',
      'Click "Start export". You\'ll get an email when your archive is ready to download.',
    ],
  },
  instagram: {
    name: 'Instagram',
    icon: '📷',
    steps: [
      'Click "Create export" on the Meta Accounts Center page.',
      'Choose the Instagram profile you want to export.',
      'Click "Export to device".',
      'Set Date Range to "All time".',
      'Set Format to "JSON" (recommended for best import fidelity — HTML also works but JSON is more reliable).',
      'Set Media quality to "High".',
      'Click "Start export". You\'ll get an email when your archive is ready to download.',
    ],
  },
  threads: {
    name: 'Threads',
    icon: '🧵',
    steps: [
      'Click "Create export" on the Meta Accounts Center page.',
      'Choose the Threads profile you want to export.',
      'Click "Export to device".',
      'Set Date Range to "All time".',
      'Set Format to "JSON" (recommended for best import fidelity — HTML also works but JSON is more reliable).',
      'Set Media quality to "High".',
      'Click "Start export". You\'ll get an email when your archive is ready to download.',
    ],
  },
  mastodon: {
    name: 'Mastodon',
    icon: '🐘',
    steps: [
      'Open your Mastodon instance in a browser and sign in.',
      'Go to Settings → Import and export → Export.',
      'Click "Request archive" under "Post archive".',
      'Your instance will prepare the archive and email you when it is ready (this can take a few minutes to hours depending on the instance).',
      'Download the archive ZIP (it contains outbox.json, following.json, followers.json, and actor.json).',
    ],
  },
};

const TerminalWindow = ({ title, items, count, color }: { title: string; items: string[]; count: number; color: string }) => (
  <div style={{
    border: '1px solid var(--border-color)',
    borderRadius: 4,
    background: 'rgba(0,0,0,0.4)',
    overflow: 'hidden',
    flex: 1,
    minWidth: 0,
    display: 'flex',
    flexDirection: 'column',
    minHeight: 160,
  }}>
    <div style={{
      padding: '6px 10px',
      borderBottom: '1px solid var(--border-color)',
      fontSize: 11,
      fontFamily: 'var(--font-mono, monospace)',
      color,
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'center',
      background: 'rgba(0,0,0,0.3)',
    }}>
      <span>{title}</span>
      <span style={{ color: 'var(--text-muted)' }}>{count}</span>
    </div>
    <div style={{
      padding: '8px 10px',
      fontSize: 11,
      fontFamily: 'var(--font-mono, monospace)',
      color: 'var(--text-muted)',
      overflowY: 'auto',
      flex: 1,
      maxHeight: 220,
      lineHeight: 1.5,
    }}>
      {items.length === 0 ? (
        <span style={{ opacity: 0.5 }}>▋</span>
      ) : (
        items.map((item, i) => <div key={i} style={{ color: i === items.length - 1 ? color : undefined }}>{item}</div>)
      )}
    </div>
  </div>
);

export default function ImportStep({ onComplete, onSkip, embedded }: Props) {
  const [phase, setPhase] = useState<ImportStepPhase>('entry');
  const [importFile, setImportFile] = useState<File | null>(null);
  const [importing, setImporting] = useState(false);
  const [importResult, setImportResult] = useState<{ imported: number; errors: string[]; session_id?: string; source?: string } | null>(null);
  const [showResults, setShowResults] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [guidePlatform, setGuidePlatform] = useState<Platform | null>(null);
  const [guideStep, setGuideStep] = useState(0);

  // Mastodon users arrive with their follow graph already mirrored into the
  // following list (live home-instance import at link/sign-up time). Say so
  // here — this screen was the first thing a fresh sign-up saw while the
  // following list was empty and unexplained.
  const [importedFollows, setImportedFollows] = useState(0);
  useEffect(() => {
    let alive = true;
    accountApi
      .list()
      .then((accounts) => {
        if (!alive) return;
        setImportedFollows(
          accounts
            .filter((a) => a.provider === 'mastodon')
            .reduce((sum, a) => sum + (a.imported_follows || 0), 0),
        );
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  // Import processing terminal state
  const [importPhase, setImportPhase] = useState('');
  const [terminalPosts, setTerminalPosts] = useState<string[]>([]);
  const [terminalImages, setTerminalImages] = useState<string[]>([]);
  const [terminalVideos, setTerminalVideos] = useState<string[]>([]);
  const [terminalTrackers, setTerminalTrackers] = useState<string[]>([]);
  const [terminalContacts, setTerminalContacts] = useState<string[]>([]);
  const [postCount, setPostCount] = useState(0);
  const [imageCount, setImageCount] = useState(0);
  const [videoCount, setVideoCount] = useState(0);
  const [trackerCount, setTrackerCount] = useState(0);
  const [contactCount, setContactCount] = useState(0);
  const [matchedCount, setMatchedCount] = useState(0);

  // Archive scan summary (files detected) — populated from the archive_scanned
  // phase so the user sees what was found before parsing begins.
  const [scanSummary, setScanSummary] = useState<{ media: number; html: number; json: number } | null>(null);

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    const file = e.dataTransfer.files?.[0];
    if (file && file.name.endsWith('.zip')) {
      setImportFile(file);
    }
  };

  const handleImportUpload = async () => {
    if (!importFile) return;
    setPhase('processing');
    setImportPhase('Starting...');
    setTerminalPosts([]);
    setTerminalImages([]);
    setTerminalVideos([]);
    setTerminalTrackers([]);
    setTerminalContacts([]);
    setPostCount(0);
    setImageCount(0);
    setVideoCount(0);
    setTrackerCount(0);
    setContactCount(0);
    setMatchedCount(0);
    setScanSummary(null);
    setImportResult(null);
    setImporting(true);

    try {
      const result = await importApi.uploadStream(importFile, (event) => {
        if (event.type === 'phase') {
          const labels: Record<string, string> = {
            scanning_archive: event.progress ? `Scanning archive... ${event.progress}/${event.total || 0}` : 'Scanning archive...',
            archive_scanned: `Archive scanned: ${event.media_count || 0} media, ${event.html_files || 0} HTML, ${event.json_files || 0} JSON`,
            extracting_media: 'Extracting referenced media...',
            parsing_posts: `Parsing posts... ${event.total || 0} files`,
            deduplicating: 'Removing duplicates...',
            inserting: 'Inserting into database...',
          };
          setImportPhase(labels[event.phase] || event.phase);
          if (event.phase === 'archive_scanned') {
            setScanSummary({ media: event.media_count || 0, html: event.html_files || 0, json: event.json_files || 0 });
          }
        } else if (event.type === 'media') {
          if (event.category === 'video') {
            setTerminalVideos(prev => [...prev.slice(-50), `  ${event.filename}`]);
            setVideoCount(c => c + 1);
          } else {
            setTerminalImages(prev => [...prev.slice(-50), `  ${event.filename}`]);
            setImageCount(c => c + 1);
          }
        } else if (event.type === 'post') {
          const title = event.title || '(no text)';
          const date = event.timestamp ? new Date(event.timestamp).toLocaleDateString() : '';
          setTerminalPosts(prev => [...prev.slice(-50), `  [${date}] ${title}${event.media_count > 0 ? ` (${event.media_count} media)` : ''}`]);
          setPostCount(c => c + 1);
        } else if (event.type === 'tracker') {
          setTerminalTrackers(prev => [...prev.slice(-50), `  ${event.category}: ${event.filename}`]);
          setTrackerCount(c => c + 1);
        } else if (event.type === 'social') {
          // Contacts saved to the user's account and matched/linked to other
          // users — surfaced live so the friend-matching payoff is visible.
          if (event.phase === 'mapping_connections') {
            const total = event.total_contacts || 0;
            setContactCount(total);
            setTerminalContacts(prev => [...prev.slice(-50), `  Saved ${total} contacts to your account`]);
          } else if (event.phase === 'connections_mapped') {
            const matched = event.matched || 0;
            setMatchedCount(matched);
            setTerminalContacts(prev => [...prev.slice(-50), `  Linked ${matched} contacts to users on the network (${event.handle_matched || 0} by handle, ${event.name_matched || 0} by name)`]);
          } else if (event.phase === 'fediverse_check') {
            setTerminalContacts(prev => [...prev.slice(-50), `  ${event.friends_on_fediverse || 0} friends found on the fediverse`]);
          }
        } else if (event.type === 'error') {
          setTerminalPosts(prev => [...prev.slice(-20), `  [ERROR] ${event.message}`]);
        }
      });
      setImportResult({ imported: result.imported, errors: result.errors, session_id: result.session_id, source: result.source });
      setImportPhase('Complete');
    } catch (e: any) {
      setImportResult({ imported: 0, errors: [e.message || 'Upload failed'] });
      setImportPhase('Failed');
    } finally {
      setImporting(false);
    }
  };

  const uploadArea = (
    <div
      style={{
        border: dragOver ? '2px solid var(--accent)' : '2px dashed var(--border-color)',
        borderRadius: 8,
        padding: 20,
        textAlign: 'center',
        marginBottom: 12,
        transition: 'border-color 0.2s',
        background: dragOver ? 'rgba(0,255,65,0.05)' : 'transparent',
      }}
      onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
      onDragLeave={() => setDragOver(false)}
      onDrop={handleDrop}
    >
      <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 10 }}>
        Drag your ZIP file here, or browse:
      </p>
      <input type="file" accept=".zip" style={{ fontSize: 13, marginBottom: 10 }} onChange={e => setImportFile(e.target.files?.[0] || null)} disabled={importing} />
      <br />
      <button className="btn" onClick={handleImportUpload} disabled={!importFile || importing} style={{ background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, fontSize: 12 }}>
        {importing ? 'Importing...' : 'Upload Archive'}
      </button>
    </div>
  );

  const resultArea = importResult && (
    <div style={{ border: '1px solid var(--accent)', borderRadius: 8, padding: 12, marginBottom: 12, background: 'var(--surface-secondary)' }}>
      {importResult.imported > 0
        ? <p style={{ color: '#4caf50', fontSize: 13, margin: 0 }}>✅ Imported {importResult.imported} items!</p>
        : <p style={{ color: 'var(--danger)', fontSize: 13, margin: 0 }}>❌ Import failed</p>
      }
      {importResult.errors.length > 0 && (
        <ul style={{ color: 'var(--danger)', fontSize: 11, paddingLeft: 16, margin: '6px 0 0' }}>
          {importResult.errors.slice(0, 3).map((err, i) => <li key={i}>{err}</li>)}
        </ul>
      )}
    </div>
  );

  // After a successful import with a session, show the friend-matching
  // results (ImportResults) before completing; otherwise complete directly.
  const finishImport = () => {
    if (importResult && importResult.imported > 0 && importResult.session_id) {
      setShowResults(true);
    } else {
      onComplete();
    }
  };

  // Shell: modal overlay by default, or an embedded full-screen panel when
  // rendered inside the dedicated Import screen (no dim/centering).
  const shell = (children: ReactNode) => {
    if (embedded) {
      return (
        <div style={{ flex: 1, minHeight: 0, overflow: 'auto', padding: 24, display: 'flex', justifyContent: 'center' }}>
          <div style={{ background: 'var(--bg-secondary, #1a1a2e)', border: '1px solid var(--border-color)', borderRadius: 8, padding: 24, width: '100%', maxWidth: 760, height: 'fit-content' }}>
            {children}
          </div>
        </div>
      );
    }
    return (
      <div style={{ position: 'fixed', inset: 0, zIndex: 10003, display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'rgba(0, 0, 0, 0.85)' }}>
        <div style={{ background: 'var(--bg-secondary, #1a1a2e)', border: '1px solid var(--accent)', borderRadius: 8, padding: 20, maxWidth: 600, width: '95%', maxHeight: '90vh', overflow: 'auto' }}>
          {children}
        </div>
      </div>
    );
  };

  if (showResults && importResult?.session_id) {
    return shell(
      <ImportResults
        sessionId={importResult.session_id}
        importedCount={importResult.imported}
        source={importResult.source || 'imported'}
        onContinue={onComplete}
      />
    );
  }

  return shell(
    <>
      <div style={{ fontFamily: 'var(--font-retro)', fontSize: 20, color: 'var(--accent)', marginBottom: 8 }}>
        Import your archive
      </div>
      <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 16 }}>
        Choose where your archive comes from and we'll walk you through getting and uploading it.
      </p>

      {/* ── Entry: equal source picker + quick actions ── */}
        {phase === 'entry' && (
          <div>
            {importedFollows > 0 && (
              <p style={{ color: '#4caf50', fontSize: 12, marginBottom: 12, marginTop: 0 }}>
                ✅ {importedFollows} {importedFollows === 1 ? 'person' : 'people'} you follow on
                Mastodon {importedFollows === 1 ? 'is' : 'are'} now in your following list.
              </p>
            )}
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 10, marginBottom: 12 }}>
              {(['instagram', 'facebook', 'threads', 'mastodon'] as Platform[]).map((p) => (
                <button
                  key={p}
                  className="btn"
                  style={{ textAlign: 'left', padding: '12px 14px', fontSize: 13, display: 'flex', flexDirection: 'column', gap: 4, alignItems: 'flex-start' }}
                  onClick={() => { setGuidePlatform(p); setGuideStep(0); setPhase('guide-platform'); }}
                >
                  <span style={{ fontSize: 16 }}>{platformGuides[p].icon} {platformGuides[p].name}</span>
                  <span style={{ color: 'var(--text-muted)', fontSize: 11, fontWeight: 400 }}>Walk me through the export →</span>
                </button>
              ))}
            </div>
            <button className="btn" style={{ background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, textAlign: 'left', padding: '10px 14px', fontSize: 13, width: '100%' }} onClick={() => setPhase('upload')}>
              📦 I already have my archive — upload it now
            </button>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 8 }}>
              <button className="btn-link" onClick={onSkip} style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                Skip for now
              </button>
            </div>
          </div>
        )}

        {/* ── Upload step ── */}
        {phase === 'upload' && (
          <>
            {uploadArea}
            {resultArea}
            {importResult && (
              <div style={{ display: 'flex', gap: 8 }}>
                {importResult.imported > 0 && (
                  <button className="btn" style={{ background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, fontSize: 12 }} onClick={finishImport}>
                    Done
                  </button>
                )}
                {importResult.imported === 0 && (
                  <button className="btn" onClick={finishImport} style={{ fontSize: 12 }}>
                    Continue without importing
                  </button>
                )}
              </div>
            )}
            {!importResult && (
              <button className="btn-link" onClick={() => setPhase('entry')} style={{ color: 'var(--text-muted)', fontSize: 12, marginTop: 8 }}>
                ← Back
              </button>
            )}
          </>
        )}

        {/* ── Guide: step-by-step walkthrough ── */}
        {phase === 'guide-platform' && guidePlatform && platformGuides[guidePlatform] && (
          <div>
            {guideStep === 0 && guidePlatform !== 'mastodon' && (
              <div style={{ textAlign: 'center', marginBottom: 12 }}>
                <a
                  href={metaDyiLink}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="btn"
                  style={{ background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, display: 'inline-block', textDecoration: 'none', padding: '10px 20px', fontSize: 13 }}
                >
                  Open Meta Accounts Center →
                </a>
                <p style={{ color: 'var(--text-muted)', fontSize: 11, marginTop: 6 }}>
                  Opens in a new tab. Follow the steps below.
                </p>
              </div>
            )}

            <div style={{ border: '1px solid var(--border-color)', borderRadius: 8, padding: 14, marginBottom: 12, background: 'var(--surface-secondary)' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
                <span style={{ fontSize: 18 }}>{platformGuides[guidePlatform].icon}</span>
                <span style={{ fontFamily: 'var(--font-retro)', fontSize: 13 }}>{platformGuides[guidePlatform].name} — Step {guideStep + 1} of {platformGuides[guidePlatform].steps.length}</span>
              </div>
              <p style={{ color: 'var(--text)', fontSize: 13, margin: 0, lineHeight: 1.5 }}>
                {platformGuides[guidePlatform].steps[guideStep]}
              </p>
            </div>

            {/* Progress dots */}
            <div style={{ display: 'flex', gap: 5, justifyContent: 'center', marginBottom: 12 }}>
              {platformGuides[guidePlatform].steps.map((_, i) => (
                <div key={i} style={{
                  width: 7, height: 7, borderRadius: '50%',
                  background: i === guideStep ? 'var(--accent)' : i < guideStep ? 'var(--accent)' : 'var(--border-color)',
                  opacity: i < guideStep ? 0.5 : 1,
                }} />
              ))}
            </div>

            <div style={{ display: 'flex', gap: 8, justifyContent: 'center' }}>
              {guideStep < platformGuides[guidePlatform].steps.length - 1 ? (
                <>
                  <button className="btn" onClick={() => setGuideStep(guideStep + 1)} style={{ fontSize: 12 }}>Done — Next →</button>
                  <button className="btn-link" onClick={() => guideStep > 0 ? setGuideStep(guideStep - 1) : setPhase('entry')} style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                    ← Back
                  </button>
                </>
              ) : (
                <>
                  <button className="btn" style={{ background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, fontSize: 12 }} onClick={() => { setPhase('upload'); setGuidePlatform(null); }}>
                    I already have my file →
                  </button>
                  <button className="btn-link" onClick={() => { setPhase('waiting'); setGuidePlatform(null); }} style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                    Still waiting for archive
                  </button>
                </>
              )}
            </div>
          </div>
        )}

        {/* ── Waiting for archive ── */}
        {phase === 'waiting' && (
          <>
            <div style={{ border: '1px solid var(--border-color)', borderRadius: 8, padding: 14, marginBottom: 12, background: 'var(--surface-secondary)', textAlign: 'center' }}>
              <p style={{ fontSize: 13, margin: '0 0 6px' }}>⏳ Your archive is being prepared</p>
              <p style={{ color: 'var(--text-muted)', fontSize: 12, margin: 0 }}>
                Meta will email you when your archive is ready to download. This usually takes 1–2 days.
              </p>
            </div>
            <p style={{ color: 'var(--text-muted)', fontSize: 12, textAlign: 'center', marginBottom: 10 }}>
              When you have the ZIP file, upload it here:
            </p>
            {uploadArea}
            {resultArea}
            {importResult && importResult.imported > 0 && (
              <button className="btn" style={{ background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, fontSize: 12 }} onClick={finishImport}>
                Done
              </button>
            )}
            <button className="btn-link" onClick={onSkip} style={{ color: 'var(--text-muted)', fontSize: 12, marginTop: 8 }}>
              Continue without importing for now →
            </button>
          </>
        )}

        {/* ── Processing: real-time terminal display ── */}
        {phase === 'processing' && (
          <div style={{ width: '100%' }}>
            <div style={{ textAlign: 'center', marginBottom: 12 }}>
              <div style={{
                fontFamily: 'var(--font-mono, monospace)',
                fontSize: 13,
                color: importPhase === 'Complete' ? '#4caf50' : importPhase === 'Failed' ? 'var(--danger)' : 'var(--accent)',
              }}>
                {importPhase === 'Complete' ? '✅ ' : importPhase === 'Failed' ? '❌ ' : '> '}{importPhase}
              </div>
              {(importPhase !== 'Complete' && importPhase !== 'Failed') && (
                <div style={{ marginTop: 6, height: 3, background: 'var(--border-color)', borderRadius: 2, overflow: 'hidden' }}>
                  <div style={{
                    height: '100%',
                    background: 'var(--accent)',
                    width: '30%',
                    animation: 'pulse 1.5s ease-in-out infinite',
                  }} />
                </div>
              )}
            </div>

            {/* Files detected in the archive — populated at scan time */}
            {scanSummary && (
              <div style={{ border: '1px solid var(--border-color)', borderRadius: 6, padding: '8px 12px', marginBottom: 12, background: 'var(--surface-secondary)', fontSize: 12, fontFamily: 'var(--font-mono, monospace)', color: 'var(--text-muted)' }}>
                <span style={{ color: 'var(--accent)' }}>Files detected:</span>{' '}
                {scanSummary.media} media · {scanSummary.html} HTML · {scanSummary.json} JSON
              </div>
            )}

            <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 12, width: '100%' }}>
              <div style={{ display: 'flex', gap: 8, width: '100%' }}>
                <TerminalWindow title="POSTS" items={terminalPosts} count={postCount} color="var(--accent)" />
                <TerminalWindow title="CONTACTS" items={terminalContacts} count={contactCount} color="#81c784" />
              </div>
              <div style={{ display: 'flex', gap: 8, width: '100%' }}>
                <TerminalWindow title="IMAGES" items={terminalImages} count={imageCount} color="#4fc3f7" />
                <TerminalWindow title="VIDEOS" items={terminalVideos} count={videoCount} color="#ff9800" />
              </div>
              <div style={{ display: 'flex', gap: 8, width: '100%' }}>
                <TerminalWindow title="TRACKERS" items={terminalTrackers} count={trackerCount} color="#e91e63" />
                <TerminalWindow title="LINKED" items={[]} count={matchedCount} color="#ba68c8" />
              </div>
            </div>

            {importResult && (
              <>
                {resultArea}
                <div style={{ display: 'flex', gap: 8 }}>
                  {importResult.imported > 0 && (
                    <button className="btn" style={{ background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, fontSize: 12 }} onClick={finishImport}>
                      Done
                    </button>
                  )}
                  {importResult.imported === 0 && (
                    <button className="btn" onClick={finishImport} style={{ fontSize: 12 }}>
                      Continue
                    </button>
                  )}
                </div>
              </>
            )}
          </div>
        )}

        {/* Close button — always visible */}
        <div style={{ marginTop: 12, textAlign: 'right' }}>
          <button className="btn-link" onClick={onSkip} style={{ color: 'var(--text-muted)', fontSize: 12 }}>
            Skip for now →
          </button>
        </div>
      </>
    );
  }
