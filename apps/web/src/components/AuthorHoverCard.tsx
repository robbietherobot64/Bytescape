import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { ProfileData } from '../utils/api';
import { profileApi, interactionFailedToast } from '../utils/api';

const API = window.location.hostname === 'localhost' ? 'http://localhost:8080' : '';

const OPEN_DELAY = 250;
const CLOSE_DELAY = 300;

interface AuthorHoverCardProps {
  handle: string;
  source?: string;
  /** Author data already in hand from the post (works for external Bluesky
   *  handles, which the local profile endpoint cannot resolve). */
  displayName?: string;
  avatarUrl?: string;
  isFollowing?: boolean;
  children: ReactNode;
}

interface CardProfile {
  display_name: string;
  avatar_url: string | null;
  is_own_profile: boolean;
}

// Wraps an author handle: hovering (with a small delay) shows a floating card
// with avatar, display name and a Follow/Following toggle; clicking navigates
// to the profile via the app's bytescape:viewProfile event.
export default function AuthorHoverCard({ handle, source, displayName, avatarUrl, isFollowing, children }: AuthorHoverCardProps) {
  const [open, setOpen] = useState(false);
  const [profile, setProfile] = useState<CardProfile | null>(null);
  const [loading, setLoading] = useState(false);
  const [followState, setFollowState] = useState<'none' | 'pending' | 'following'>(
    isFollowing ? 'following' : 'none'
  );
  const [followPending, setFollowPending] = useState(false);
  const loadedRef = useRef(false);
  const touchedRef = useRef(false);
  const openTimer = useRef<number | null>(null);
  const closeTimer = useRef<number | null>(null);
  const wrapperRef = useRef<HTMLSpanElement>(null);
  const username = handle.replace(/^@+/, '').trim();

  useEffect(() => {
    loadedRef.current = false;
    touchedRef.current = false;
    setProfile(null);
    setFollowState(isFollowing ? 'following' : 'none');
  }, [username, isFollowing]);

  useEffect(() => () => {
    if (openTimer.current !== null) window.clearTimeout(openTimer.current);
    if (closeTimer.current !== null) window.clearTimeout(closeTimer.current);
  }, []);

  // Enrich from the local profile endpoint when the handle is a Bytescape
  // user; external Bluesky handles 404 and keep the post-provided baseline.
  useEffect(() => {
    if (!open || loadedRef.current || !username) return;
    loadedRef.current = true;
    setLoading(true);
    profileApi.get(username)
      .then((p: ProfileData) => {
        setProfile({
          display_name: p.display_name || username,
          avatar_url: p.avatar_url,
          is_own_profile: !!p.is_own_profile,
        });
        if (!touchedRef.current) setFollowState(p.is_following ? 'following' : 'none');
      })
      .catch(() => {
        // Failed attempt must not pin the card forever — allow a retry.
        loadedRef.current = false;
      })
      .finally(() => setLoading(false));
  }, [open, username]);

  const enter = () => {
    if (closeTimer.current !== null) {
      window.clearTimeout(closeTimer.current);
      closeTimer.current = null;
    }
    if (openTimer.current !== null) return;
    openTimer.current = window.setTimeout(() => {
      openTimer.current = null;
      setOpen(true);
    }, OPEN_DELAY);
  };

  const leave = () => {
    if (openTimer.current !== null) {
      window.clearTimeout(openTimer.current);
      openTimer.current = null;
    }
    if (closeTimer.current !== null) window.clearTimeout(closeTimer.current);
    closeTimer.current = window.setTimeout(() => {
      closeTimer.current = null;
      setOpen(false);
    }, CLOSE_DELAY);
  };

  // Follow through the feed routes (same contract as PostCard's FollowButton):
  // they resolve external Bluesky handles server-side; /api/profile/* only
  // knows local users and would fail for the dominant reshare case.
  const toggleFollow = async () => {
    if (followPending || followState === 'pending') return;
    const prev = followState;
    const next = followState === 'following' ? 'none' : 'following';
    touchedRef.current = true;
    setFollowPending(true);
    setFollowState(next);
    try {
      const token = localStorage.getItem('bytescape_token');
      const res = await fetch(`${API}/api/feed/${next === 'following' ? 'follow' : 'unfollow'}`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          handle: username,
          source: (source || '').split('|')[0] || source,
        }),
      });
      if (!res.ok) throw new Error(`follow failed (${res.status})`);
      const data = await res.json();
      if (data.status === 'pending' || data.status === 'following') setFollowState('following');
      else if (data.status === 'none') setFollowState('none');
    } catch {
      setFollowState(prev);
      interactionFailedToast();
    } finally {
      setFollowPending(false);
    }
  };

  if (!username || username === 'unknown') return <>{children}</>;

  const name = profile?.display_name || displayName || username;
  const avatar = profile?.avatar_url || avatarUrl || null;

  return (
    <span
      ref={wrapperRef}
      onMouseEnter={enter}
      onMouseLeave={leave}
      onClick={(e) => {
        e.stopPropagation();
        window.dispatchEvent(new CustomEvent('bytescape:viewProfile', {
          detail: { handle, source },
        }));
      }}
      style={{ position: 'relative', display: 'inline-flex', alignItems: 'center', cursor: 'pointer' }}
    >
      {children}
      {open && (
        <div
          onMouseEnter={enter}
          onMouseLeave={(e) => {
            // A card -> username move stays inside the wrapper but fires only
            // the card's mouseleave; without this guard the card would close
            // under the resting pointer (enter() never re-fires intra-span).
            const next = e.relatedTarget as Node | null;
            if (next && wrapperRef.current?.contains(next)) return;
            leave();
          }}
          onClick={e => e.stopPropagation()}
          style={{
            position: 'absolute', top: 'calc(100% + 6px)', left: 0, zIndex: 400,
            width: 240, padding: 12,
            background: 'var(--bg-secondary, #161616)',
            border: '1px solid var(--border-color, #333)', borderRadius: 10,
            boxShadow: '0 8px 24px rgba(0,0,0,0.5)',
            textAlign: 'left', cursor: 'default',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            {avatar ? (
              <img
                src={avatar}
                alt=""
                style={{ width: 40, height: 40, borderRadius: '50%', objectFit: 'cover', flexShrink: 0 }}
              />
            ) : (
              <span style={{
                width: 40, height: 40, borderRadius: '50%', flexShrink: 0,
                background: 'var(--surface-hover, #2a2a2a)',
                display: 'inline-flex', alignItems: 'center', justifyContent: 'center',
                fontSize: 16, fontWeight: 700, color: 'var(--accent, #00ff41)',
              }}>
                {name[0]?.toUpperCase() || '?'}
              </span>
            )}
            <div style={{ minWidth: 0 }}>
              <div style={{
                fontSize: 13, fontWeight: 600, color: 'var(--text-primary, #fff)',
                overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
              }}>
                {name}
              </div>
              <div style={{
                fontSize: 12, color: 'var(--text-secondary, #999)',
                overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
              }}>
                @{username}
              </div>
            </div>
          </div>
          {loading && (
            <div style={{ marginTop: 10, fontSize: 11, color: 'var(--text-muted, #666)' }}>Loading…</div>
          )}
          {!profile?.is_own_profile && (
            <button
              onClick={toggleFollow}
              disabled={followPending}
              style={{
                marginTop: 10, width: '100%', borderRadius: 4, padding: '4px 8px', fontSize: 12,
                cursor: followPending ? 'wait' : 'pointer',
                background: followState === 'following' ? 'var(--surface-hover, #333)' : 'transparent',
                color: followState === 'following' ? 'var(--text-secondary, #999)' : 'var(--accent-color, #4caf50)',
                border: followState === 'following' ? 'none' : '1px solid var(--accent-color, #4caf50)',
              }}
            >
              {followPending ? '...' : followState === 'following' || followState === 'pending' ? 'Following' : '+ Follow'}
            </button>
          )}
        </div>
      )}
    </span>
  );
}
