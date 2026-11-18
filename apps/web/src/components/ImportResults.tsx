import { useState, useEffect } from 'react';
import { importApi, followApi, rssApi, SocialGraphResponse } from '../utils/api';

interface Props {
  sessionId: string;
  importedCount: number;
  source: string;
  onContinue: () => void;
}

const SOURCE_LABELS: Record<string, string> = {
  facebook: 'Facebook',
  instagram: 'Instagram',
  threads: 'Threads',
};

const SOURCE_ICONS: Record<string, string> = {
  facebook: '📘',
  instagram: '📷',
  threads: '🧵',
};

export default function ImportResults({ sessionId, importedCount, source, onContinue }: Props) {
  const [socialGraph, setSocialGraph] = useState<SocialGraphResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [followedIds, setFollowedIds] = useState<Set<string>>(new Set());
  const [followAllDone, setFollowAllDone] = useState(false);
  const [followAllLoading, setFollowAllLoading] = useState(false);
  const [followAllCount, setFollowAllCount] = useState(0);
  const [showReview, setShowReview] = useState(false);
  const [animIn, setAnimIn] = useState(false);
  const [rssSubscribed, setRssSubscribed] = useState<Set<string>>(new Set());
  const [rssSubscribing, setRssSubscribing] = useState(false);

  const platformLabel = SOURCE_LABELS[source] || source || 'imported';
  const platformIcon = SOURCE_ICONS[source] || '📦';

  useEffect(() => {
    loadSocialGraph();
    setTimeout(() => setAnimIn(true), 50);
  }, [sessionId]);

  async function loadSocialGraph() {
    try {
      const data = await importApi.socialGraph(sessionId);
      setSocialGraph(data);
    } catch (e: any) {
      setError(e.message || 'Failed to load social graph');
    } finally {
      setLoading(false);
    }
  }

  async function handleFollowAll() {
    if (!socialGraph || socialGraph.matches.length === 0) return;
    setFollowAllLoading(true);
    const results = await Promise.allSettled(
      socialGraph.matches.map(m => followApi.follow(m.handle))
    );
    const succeeded = results.filter(r => r.status === 'fulfilled').length;
    const newFollowed = new Set(followedIds);
    socialGraph.matches.forEach(m => newFollowed.add(m.user_id));
    setFollowedIds(newFollowed);
    setFollowAllCount(succeeded);
    setFollowAllDone(true);
    setFollowAllLoading(false);
  }

  async function handleFollowOne(match: { user_id: string; handle: string }) {
    if (followedIds.has(match.user_id)) return;
    try {
      await followApi.follow(match.handle);
      setFollowedIds(prev => new Set(prev).add(match.user_id));
    } catch {}
  }

  async function handleUnfollowOne(match: { user_id: string; handle: string }) {
    try {
      await followApi.unfollow(match.handle);
      setFollowedIds(prev => {
        const next = new Set(prev);
        next.delete(match.user_id);
        return next;
      });
    } catch {}
  }

  async function handleSubscribeRss(candidate: { feed_url: string; contact_name: string; source: string }) {
    if (rssSubscribed.has(candidate.feed_url)) return;
    try {
      await rssApi.create(candidate.feed_url);
      setRssSubscribed(prev => new Set(prev).add(candidate.feed_url));
    } catch {}
  }

  async function handleSubscribeAllRss() {
    if (!socialGraph?.rss_candidates) return;
    setRssSubscribing(true);
    const results = await Promise.allSettled(
      socialGraph.rss_candidates.map(c => rssApi.create(c.feed_url))
    );
    const newSubscribed = new Set(rssSubscribed);
    socialGraph.rss_candidates.forEach(c => newSubscribed.add(c.feed_url));
    setRssSubscribed(newSubscribed);
    setRssSubscribing(false);
  }

  const cardStyle: React.CSSProperties = {
    border: '1px solid var(--border-color)',
    borderRadius: 8,
    padding: 20,
    background: 'var(--surface-secondary)',
  };

  const headingStyle: React.CSSProperties = {
    fontFamily: 'var(--font-retro)',
    fontSize: 16,
    color: 'var(--text-primary)',
    marginBottom: 8,
  };

  // ── Loading state ──
  if (loading) {
    return (
      <div className="fullscreen" style={{ overflowY: 'auto', padding: '20px 16px' }}>
        <div style={{ maxWidth: 600, margin: '0 auto', textAlign: 'center', paddingTop: 40 }}>
          <div style={{ fontFamily: 'var(--font-mono)', fontSize: 14, color: 'var(--accent)' }}>
            {'>'} Loading your import results...
          </div>
          <div style={{ marginTop: 16, height: 3, background: 'var(--border-color)', borderRadius: 2, overflow: 'hidden', maxWidth: 200, marginLeft: 'auto', marginRight: 'auto' }}>
            <div style={{ height: '100%', background: 'var(--accent)', width: '30%', animation: 'pulse 1.5s ease-in-out infinite' }} />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div style={{ overflowY: 'auto', padding: '16px 12px', width: '100%', maxWidth: 600, margin: '0 auto', minHeight: '100vh' }}>
         {/* ── Hero Stats ── */}
         <div style={{
           textAlign: 'center',
           marginBottom: 20,
           opacity: animIn ? 1 : 0,
           transform: animIn ? 'translateY(0)' : 'translateY(20px)',
           transition: 'opacity 0.5s ease, transform 0.5s ease',
         }}>
           {socialGraph && socialGraph.matches.length > 0 && (
             <div style={{ fontSize: 36, marginBottom: 4, display: 'inline-block', animation: animIn ? 'scaleIn 0.4s ease-out' : 'none' }}>{platformIcon}</div>
           )}
           <div style={{ fontFamily: 'var(--font-retro)', fontSize: 18, color: 'var(--text-primary)', marginBottom: 4 }}>
             Your {platformLabel} archive is here!
           </div>
           <p style={{ color: 'var(--text-muted)', fontSize: 12, margin: 0 }}>
             We found {importedCount} posts from your {platformLabel} history.
           </p>
         </div>

         {/* ── Stat Cards ── */}
         <div style={{
           display: 'flex',
           gap: 8,
           marginBottom: 20,
           flexWrap: 'wrap',
           opacity: animIn ? 1 : 0,
           transform: animIn ? 'translateY(0)' : 'translateY(15px)',
           transition: 'opacity 0.6s ease 0.15s, transform 0.6s ease 0.15s',
         }}>
           <div style={{ ...cardStyle, flex: 1, minWidth: 120, textAlign: 'center', padding: 16 }}>
             <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--accent)', fontFamily: 'var(--font-mono)' }}>
               {importedCount}
             </div>
             <div style={{ color: 'var(--text-muted)', fontSize: 11, marginTop: 4 }}>Posts imported</div>
           </div>
           <div style={{ ...cardStyle, flex: 1, minWidth: 120, textAlign: 'center', padding: 16 }}>
             <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--accent)', fontFamily: 'var(--font-mono)' }}>
               {socialGraph?.stats.total_contacts ?? 0}
             </div>
             <div style={{ color: 'var(--text-muted)', fontSize: 11, marginTop: 4 }}>Connections mapped</div>
           </div>
           <div style={{ ...cardStyle, flex: 1, minWidth: 120, textAlign: 'center', padding: 16 }}>
             <div style={{ fontSize: 22, fontWeight: 700, color: 'var(--accent)', fontFamily: 'var(--font-mono)' }}>
               {socialGraph?.stats.matched_count ?? 0}
             </div>
             <div style={{ color: 'var(--text-muted)', fontSize: 11, marginTop: 4 }}>Bytescape friends matched</div>
           </div>
         </div>

        {error && (
          <div style={{ ...cardStyle, marginBottom: 16, borderColor: 'var(--danger)' }}>
            <p style={{ color: 'var(--danger)', fontSize: 13, margin: 0 }}>{error}</p>
          </div>
        )}

        {/* ── Social Graph Discovery — matches exist ── */}
        {socialGraph && socialGraph.matches.length > 0 && (
          <div style={{ ...cardStyle, marginBottom: 16 }}>
            <div style={headingStyle}>
              {socialGraph.matches.length} of your friends are already on the fediverse!
            </div>
            <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 16 }}>
              People you know who've joined Bytescape or the fediverse.
            </p>

            {/* Matched accounts list */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10, marginBottom: 16 }}>
              {socialGraph.matches.map(match => {
                const isFollowed = followedIds.has(match.user_id);
                const username = match.handle.replace('@', '');
                return (
                  <div key={match.user_id} style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 12,
                    padding: '10px 12px',
                    border: '1px solid var(--border-color)',
                    borderRadius: 6,
                    background: 'rgba(0,0,0,0.2)',
                  }}>
                    {/* Avatar */}
                    <div style={{
                      width: 36,
                      height: 36,
                      borderRadius: '50%',
                      background: match.avatar ? 'transparent' : 'var(--border-color)',
                      overflow: 'hidden',
                      flexShrink: 0,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      fontSize: 14,
                      color: '#000',
                      fontWeight: 600,
                    }}>
                      {match.avatar
                        ? <img src={match.avatar} alt="" style={{ width: 36, height: 36, objectFit: 'cover' }} />
                        : match.display_name.charAt(0).toUpperCase()
                      }
                    </div>

                    {/* Name and handle */}
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontSize: 13, color: 'var(--text-primary)', fontWeight: 600 }}>{match.display_name}</div>
                      <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>{match.handle}</div>
                    </div>

                    {/* Match type badge */}
                    <span style={{
                      fontSize: 10,
                      padding: '2px 6px',
                      borderRadius: 4,
                      background: match.match_type === 'handle_match' ? 'rgba(0,255,65,0.15)' : 'rgba(0,200,255,0.15)',
                      color: match.match_type === 'handle_match' ? 'var(--accent)' : '#4fc3f7',
                      fontWeight: 600,
                      whiteSpace: 'nowrap',
                    }}>
                      {match.match_type === 'handle_match' ? 'handle' : 'name'}
                    </span>

                    {/* Follow/Unfollow toggle (review mode) or hidden */}
                    {showReview && (
                      <button
                        className="btn"
                        onClick={() => isFollowed ? handleUnfollowOne(match) : handleFollowOne(match)}
                        style={{
                          padding: '4px 10px',
                          fontSize: 11,
                          background: isFollowed ? 'var(--accent)' : 'transparent',
                          color: isFollowed ? '#000' : 'var(--text-muted)',
                          border: isFollowed ? 'none' : '1px solid var(--text-muted)',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {isFollowed ? '✓' : '+ Follow'}
                      </button>
                    )}
                  </div>
                );
              })}
            </div>

            {/* Follow All button */}
            {followAllDone ? (
              <div style={{
                background: 'rgba(0,255,65,0.1)',
                borderRadius: 6,
                padding: '10px 14px',
                marginBottom: 8,
                textAlign: 'center',
              }}>
                <span style={{ color: '#4caf50', fontSize: 13, fontWeight: 600 }}>
                  ✅ Following {followAllCount} people
                </span>
              </div>
            ) : (
              <button
                className="btn"
                onClick={handleFollowAll}
                disabled={followAllLoading}
                style={{
                  background: 'var(--accent)',
                  color: '#000',
                  border: 'none',
                  fontWeight: 600,
                  width: '100%',
                  marginBottom: 8,
                }}
              >
                {followAllLoading ? 'Following...' : `Follow all (${socialGraph.matches.length})`}
              </button>
            )}

            {/* Review individually link */}
            {!showReview && !followAllDone && (
              <button
                className="btn-link"
                onClick={() => setShowReview(true)}
                style={{ color: 'var(--text-muted)', fontSize: 12, display: 'block', textAlign: 'center', width: '100%' }}
              >
                Review individually
              </button>
            )}
            {showReview && (
              <button
                className="btn-link"
                onClick={() => setShowReview(false)}
                style={{ color: 'var(--text-muted)', fontSize: 12, display: 'block', textAlign: 'center', width: '100%' }}
              >
                Hide per-user toggles
              </button>
            )}
          </div>
        )}

        {/* ── No matches state ── */}
        {socialGraph && socialGraph.matches.length === 0 && !error && (
          <div style={{ ...cardStyle, marginBottom: 16, textAlign: 'center', padding: '32px 20px' }}>
            <div style={{ fontSize: 36, marginBottom: 12 }}>🔍</div>
            <div style={headingStyle}>
              No Bytescape friends matched yet
            </div>
            <p style={{ color: 'var(--text-muted)', fontSize: 13, margin: 0, lineHeight: 1.5 }}>
              When someone you know imports their archive, we'll connect you!
            </p>
          </div>
        )}

        {/* ── RSS Candidates ── */}
        {socialGraph && socialGraph.rss_candidates && socialGraph.rss_candidates.length > 0 && (
          <div style={{ ...cardStyle, marginBottom: 16 }}>
            <div style={headingStyle}>
              {socialGraph.rss_candidates.length} more people have public feeds
            </div>
            <p style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 16 }}>
              They're not on the fediverse yet, but you can follow their public posts via RSS.
            </p>

            {/* RSS candidate list (scrollable, shows 2-3 at a time) */}
            <div style={{
              display: 'flex', flexDirection: 'column', gap: 10, marginBottom: 16,
              maxHeight: 200, overflowY: 'auto', paddingRight: 4,
            }}>
              {socialGraph.rss_candidates.map((candidate, i) => (
                <div key={i} style={{
                  display: 'flex', alignItems: 'center', gap: 12,
                  padding: '10px 12px', border: '1px solid var(--border-color)',
                  borderRadius: 6, background: 'rgba(0,0,0,0.2)',
                }}>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 13, color: 'var(--text-primary)', fontWeight: 600 }}>
                      {candidate.contact_name}
                    </div>
                    <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                      {candidate.source} · {candidate.contact_handle || candidate.feed_url}
                    </div>
                  </div>
                  <button
                    className="btn"
                    onClick={() => handleSubscribeRss(candidate)}
                    disabled={rssSubscribed.has(candidate.feed_url)}
                    style={{
                      padding: '4px 10px', fontSize: 11,
                      background: rssSubscribed.has(candidate.feed_url) ? 'var(--accent)' : 'transparent',
                      color: rssSubscribed.has(candidate.feed_url) ? '#000' : 'var(--text-muted)',
                      border: rssSubscribed.has(candidate.feed_url) ? 'none' : '1px solid var(--text-muted)',
                    }}
                  >
                    {rssSubscribed.has(candidate.feed_url) ? '✓' : '+ Subscribe'}
                  </button>
                </div>
              ))}
            </div>

            {/* Subscribe All button */}
            <button
              className="btn"
              onClick={handleSubscribeAllRss}
              disabled={rssSubscribing}
              style={{ background: 'var(--accent)', color: '#000', border: 'none', fontWeight: 600, width: '100%' }}
            >
              {rssSubscribing ? 'Subscribing...' : `Subscribe to all (${socialGraph.rss_candidates.length})`}
            </button>
          </div>
        )}

        {/* ── Next Step CTA ── */}
        <div style={{ ...cardStyle, textAlign: 'center', marginBottom: 16 }}>
          <p style={{ color: 'var(--text-muted)', fontSize: 12, marginBottom: 14 }}>
            Almost done! Pick a few interests to fill your feed even more.
          </p>
          <button
            className="btn"
            onClick={onContinue}
            style={{
              background: 'var(--accent)',
              color: '#000',
              border: 'none',
              fontWeight: 600,
              width: '100%',
              marginBottom: 8,
              padding: '10px 20px',
              fontSize: 13,
            }}
          >
            Continue to interests →
          </button>
          <button
            className="btn-link"
            style={{ color: 'var(--text-muted)', fontSize: 11 }}
            onClick={() => {
              const u = JSON.parse(atob((localStorage.getItem('bytescape_token') || '').split('.')[1]));
              window.location.href = `/app/profile/${u.username}`;
            }}
          >
            Skip to my feed
          </button>
        </div>
    </div>
  );
}
