import { useEffect, useState } from 'react';
import { feedgenApi, subscriptionApi, type FeedgenTopic } from '../utils/api';

interface Props {
  /** Refresh the user's subscriptions + feed after a subscribe/combo create. */
  onSubscribed: () => void | Promise<void>;
}

/**
 * Generated feeds catalog: the empty-state destination on the Feeds tab.
 * Grid of topic feed generators (subscribe each), plus a combo builder modal
 * that combines 2+ topics into a shareable feed and auto-subscribes you.
 */
export default function FeedCatalog({ onSubscribed }: Props) {
  const [topics, setTopics] = useState<FeedgenTopic[] | null>(null);
  const [loadError, setLoadError] = useState('');
  const [subscribing, setSubscribing] = useState<string | null>(null);
  const [comboOpen, setComboOpen] = useState(false);
  const [comboName, setComboName] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [creating, setCreating] = useState(false);
  const [created, setCreated] = useState<{ name: string; at_uri: string } | null>(null);
  const [copied, setCopied] = useState(false);

  const loadTopics = () => {
    setLoadError('');
    feedgenApi.topics()
      .then(r => setTopics(r.topics))
      .catch(e => setLoadError(e instanceof Error ? e.message : 'Failed to load feed generators'));
  };

  useEffect(() => {
    loadTopics();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const subscribeTopic = async (t: FeedgenTopic) => {
    setSubscribing(t.rkey);
    try {
      const { at_uri } = await feedgenApi.publish(t.rkey);
      await subscriptionApi.create({
        source: 'bytescape_feed',
        source_id: at_uri,
        label: t.name,
        icon: '✨',
        category: 'feeds',
      });
      window.dispatchEvent(new CustomEvent('bytescape:toast', { detail: { message: `Subscribed to ${t.name}!`, type: 'success' } }));
      await onSubscribed();
    } catch (e) {
      window.dispatchEvent(new CustomEvent('bytescape:toast', {
        detail: { message: e instanceof Error ? e.message : 'Failed to subscribe', type: 'error' },
      }));
    } finally {
      setSubscribing(null);
    }
  };

  const createCombo = async () => {
    setCreating(true);
    try {
      const combo = await feedgenApi.createCombo(comboName.trim(), Array.from(selected));
      await subscriptionApi.create({
        source: 'bytescape_feed',
        source_id: combo.at_uri,
        label: combo.name,
        icon: '✨',
        category: 'feeds',
      });
      setCreated({ name: combo.name, at_uri: combo.at_uri });
      window.dispatchEvent(new CustomEvent('bytescape:toast', { detail: { message: `Combo "${combo.name}" created!`, type: 'success' } }));
      await onSubscribed();
    } catch (e) {
      window.dispatchEvent(new CustomEvent('bytescape:toast', {
        detail: { message: e instanceof Error ? e.message : 'Failed to create combo', type: 'error' },
      }));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="feed-column">
      <div className="feed-column-header">
        <h1>GENERATED FEEDS</h1>
        <p>Topic feed generators for Bytescape — subscribe to one, or combine 2+ into a shareable combo.</p>
      </div>
      <div className="feed-column-scroll">
        {loadError && (
          <div className="empty-feed">
            <h2>COULD NOT LOAD GENERATORS</h2>
            <p>{loadError}</p>
            <button className="btn" onClick={loadTopics} style={{ marginTop: 12 }}>{'>'} RETRY</button>
          </div>
        )}
        {!loadError && topics === null && (
          <div className="empty-feed">
            <h2>LOADING...</h2>
            <p>Fetching topic feed generators.</p>
          </div>
        )}
        {!loadError && topics !== null && topics.length === 0 && (
          <div className="empty-feed">
            <h2>NO GENERATORS AVAILABLE</h2>
            <p>Topic generators will show up here once they're published.</p>
          </div>
        )}
        {!loadError && topics !== null && topics.length > 0 && (
          <>
            <div style={{
              display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))',
              gap: 12, maxWidth: 900, margin: '0 auto',
            }}>
              {topics.map(t => (
                <div key={t.rkey} style={{
                  display: 'flex', flexDirection: 'column', gap: 8,
                  border: '1px solid var(--border-dim)', borderRadius: 6,
                  background: 'var(--bg-secondary)', padding: 12,
                }}>
                  <div style={{ fontFamily: 'var(--font-retro)', fontSize: 16, color: 'var(--accent)' }}>
                    {t.name}
                  </div>
                  <p style={{ flex: 1, fontSize: 12, lineHeight: 1.4, color: 'var(--text-muted)' }}>
                    {t.description}
                  </p>
                  <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
                    {t.tags.map(tag => (
                      <span key={tag} className="chip" style={{ fontSize: 10, padding: '2px 8px' }}>
                        #{tag}
                      </span>
                    ))}
                  </div>
                  <button
                    className="btn"
                    disabled={subscribing === t.rkey}
                    onClick={() => subscribeTopic(t)}
                    style={{ alignSelf: 'flex-start', padding: '5px 12px', fontSize: 12 }}
                  >
                    {subscribing === t.rkey ? 'SUBSCRIBING…' : '📡 SUBSCRIBE'}
                  </button>
                </div>
              ))}
            </div>
            <div style={{ maxWidth: 900, margin: '16px auto 0', textAlign: 'center' }}>
              <button
                className="btn"
                onClick={() => { setCreated(null); setComboOpen(true); }}
                style={{ background: 'var(--accent)', color: '#000', fontWeight: 700 }}
              >
                🔨 Build combined feed
              </button>
            </div>
          </>
        )}
      </div>

      {comboOpen && (
        <div className="modal-overlay" onClick={() => { if (!creating && !created) setComboOpen(false); }}>
          <div className="modal-box" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 480 }}>
            {created ? (
              <>
                <h2>COMBO FEED READY</h2>
                <p>
                  <span style={{ color: 'var(--accent)' }}>{created.name}</span> is live and you're subscribed.
                  Anyone on Bluesky can follow this feed:
                </p>
                <code style={{
                  display: 'block', padding: '10px 12px', marginBottom: 12,
                  background: 'var(--bg-tertiary)', border: '1px solid var(--border-dim)',
                  color: 'var(--accent)', fontFamily: 'var(--font-mono)', fontSize: 12,
                  wordBreak: 'break-all',
                }}>
                  {created.at_uri}
                </code>
                <div className="modal-actions">
                  <button
                    className="btn"
                    onClick={async () => {
                      try {
                        await navigator.clipboard.writeText(created.at_uri);
                        setCopied(true);
                        setTimeout(() => setCopied(false), 1500);
                      } catch {
                        // Clipboard unavailable — the uri above stays selectable.
                      }
                    }}
                  >
                    {copied ? '✓ COPIED' : '⧉ COPY LINK'}
                  </button>
                  <button
                    className="btn"
                    onClick={() => { setComboOpen(false); setCreated(null); setComboName(''); setSelected(new Set()); setCopied(false); }}
                  >
                    DONE
                  </button>
                </div>
              </>
            ) : (
              <>
                <h2>BUILD COMBINED FEED</h2>
                <p>Pick at least 2 topics, name your combo — we'll generate it and subscribe you automatically.</p>
                <input
                  className="input-field"
                  placeholder="Combo name (e.g. Punk + Hockey)"
                  value={comboName}
                  onChange={(e) => setComboName(e.target.value)}
                  style={{ marginBottom: 12 }}
                />
                <div style={{ maxHeight: 260, overflowY: 'auto', display: 'flex', flexDirection: 'column', gap: 6, marginBottom: 10 }}>
                  {(topics ?? []).map(t => {
                    const isOn = selected.has(t.rkey);
                    return (
                      <label key={t.rkey} style={{
                        display: 'flex', gap: 8, alignItems: 'flex-start', cursor: 'pointer',
                        fontSize: 13, padding: '6px 8px',
                        border: `1px solid ${isOn ? 'var(--accent)' : 'var(--border-dim)'}`,
                        background: isOn ? 'rgba(0, 255, 65, 0.06)' : 'transparent',
                      }}>
                        <input
                          type="checkbox"
                          checked={isOn}
                          onChange={() => setSelected(prev => {
                            const next = new Set(prev);
                            if (next.has(t.rkey)) next.delete(t.rkey); else next.add(t.rkey);
                            return next;
                          })}
                          style={{ marginTop: 3 }}
                        />
                        <span>
                          <span style={{ color: 'var(--accent)', fontFamily: 'var(--font-retro)' }}>{t.name}</span>
                          <span style={{ display: 'block', color: 'var(--text-muted)', fontSize: 11 }}>{t.description}</span>
                        </span>
                      </label>
                    );
                  })}
                </div>
                <div style={{ fontSize: 12, color: selected.size >= 2 ? 'var(--accent)' : 'var(--text-muted)' }}>
                  {selected.size} topic{selected.size === 1 ? '' : 's'} selected
                  {selected.size < 2 && ' — pick at least 2'}
                </div>
                <div className="modal-actions">
                  <button className="btn" disabled={creating} onClick={() => setComboOpen(false)}>
                    CANCEL
                  </button>
                  <button
                    className="btn"
                    disabled={creating || selected.size < 2 || !comboName.trim()}
                    onClick={createCombo}
                    style={{ background: 'var(--accent)', color: '#000', fontWeight: 700 }}
                  >
                    {creating ? 'BUILDING…' : `> CREATE COMBO (${selected.size})`}
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
