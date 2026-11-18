import { useState, useEffect, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { profileApi, type FeedPost, type FeedResponse } from '../../utils/api';
import ComposerModal from '../ComposerModal';
import ImportStep from '../ImportStep';

interface ProfileFeedProps {
  username: string;
  isOwnProfile: boolean;
}

export default function ProfileFeed({ username, isOwnProfile }: ProfileFeedProps) {
  const [posts, setPosts] = useState<FeedPost[]>([]);
  const [loading, setLoading] = useState(true);
  const [hasMore, setHasMore] = useState(false);
  const [cursor, setCursor] = useState<string | undefined>();
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [showComposer, setShowComposer] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  const loadPosts = useCallback(async () => {
    setLoading(true);
    try {
      const data: FeedResponse = await profileApi.feed(username, 20, 0);
      setPosts(data.items);
      setHasMore(data.has_more);
      setCursor(data.cursor);
    } catch {
      // silent
    } finally {
      setLoading(false);
    }
  }, [username]);

  useEffect(() => {
    loadPosts();
  }, [loadPosts]);

  // Close dropdown on outside click or Escape (but not during a tour)
  useEffect(() => {
    if (!dropdownOpen) return;
    const isTourActive = localStorage.getItem('onboarding_complete') !== 'true';
    const handleClick = (e: MouseEvent) => {
      if (isTourActive) return; // don't dismiss during tour
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setDropdownOpen(false);
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !isTourActive) setDropdownOpen(false);
    };
    document.addEventListener('mousedown', handleClick);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('mousedown', handleClick);
      document.removeEventListener('keydown', handleKey);
    };
  }, [dropdownOpen]);

  // Listen for tour-driven dropdown open
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail?.action === 'openDropdown') setDropdownOpen(true);
      if (detail?.action === 'openImport') { setDropdownOpen(false); setShowImport(true); }
    };
    window.addEventListener('bytescape:tourAction', handler);
    return () => window.removeEventListener('bytescape:tourAction', handler);
  }, []);

  // Listen for closeAllModals event (from tour advance)
  useEffect(() => {
    const handler = () => {
      setDropdownOpen(false);
      setShowImport(false);
      setShowComposer(false);
    };
    window.addEventListener('bytescape:closeAllModals', handler);
    return () => window.removeEventListener('bytescape:closeAllModals', handler);
  }, []);

  return (
    <div
      style={{
        background: 'var(--surface)',
        border: '1px solid var(--border-color)',
        borderRadius: 6,
        padding: 16,
        position: 'relative',
      }}
    >
      {/* [+] button — own profile only */}
      {isOwnProfile && (
        <div ref={dropdownRef} style={{ position: 'absolute', top: 12, right: 12 }}>
          <button
            data-tour="import-button"
            onClick={() => setDropdownOpen(prev => !prev)}
            style={{
              width: 32,
              height: 32,
              borderRadius: 4,
              background: dropdownOpen ? 'var(--accent)' : 'var(--surface-secondary)',
              border: '1px solid var(--border-color)',
              color: dropdownOpen ? '#000' : 'var(--accent)',
              fontSize: 18,
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              fontFamily: 'var(--font-retro)',
              fontWeight: 700,
              transition: 'all 0.15s',
            }}
          >
            +
          </button>

          {/* Dropdown */}
          {dropdownOpen && (
            <div
              style={{
                position: 'absolute',
                top: 38,
                right: 0,
                background: 'var(--bg-secondary, #1a1a2e)',
                border: '1px solid var(--border-color)',
                borderRadius: 6,
                minWidth: 160,
                zIndex: 50,
                boxShadow: '0 4px 16px rgba(0,0,0,0.4)',
                overflow: 'hidden',
              }}
            >
              <button
                data-tour="post-option"
                onClick={() => { setDropdownOpen(false); setShowComposer(true); }}
                style={{
                  display: 'block',
                  width: '100%',
                  padding: '10px 14px',
                  background: 'none',
                  border: 'none',
                  color: 'var(--text-secondary)',
                  textAlign: 'left',
                  cursor: 'pointer',
                  fontFamily: 'var(--font-mono)',
                  fontSize: 13,
                }}
                onMouseEnter={e => (e.currentTarget.style.background = 'rgba(255,255,255,0.05)')}
                onMouseLeave={e => (e.currentTarget.style.background = 'none')}
              >
                ✏️ Post
              </button>
              <button
                data-tour="import-option"
                onClick={() => { setDropdownOpen(false); setShowImport(true); }}
                style={{
                  display: 'block',
                  width: '100%',
                  padding: '10px 14px',
                  background: 'none',
                  border: 'none',
                  borderTop: '1px solid var(--border-color)',
                  color: 'var(--text-secondary)',
                  textAlign: 'left',
                  cursor: 'pointer',
                  fontFamily: 'var(--font-mono)',
                  fontSize: 13,
                }}
                onMouseEnter={e => (e.currentTarget.style.background = 'rgba(255,255,255,0.05)')}
                onMouseLeave={e => (e.currentTarget.style.background = 'none')}
              >
                📦 Import
              </button>
            </div>
          )}
        </div>
      )}

      <div style={{ fontFamily: 'var(--font-retro)', fontSize: 15, color: 'var(--text-primary)', marginBottom: 12 }}>
        Posts
      </div>

      {/* Feed */}
      {loading && posts.length === 0 ? (
        <div style={{ color: 'var(--text-muted)', fontSize: 13, padding: '20px 0', textAlign: 'center' }}>
          Loading...
        </div>
      ) : posts.length === 0 ? (
        <div style={{ color: 'var(--text-muted)', fontSize: 13, padding: '20px 0', textAlign: 'center', lineHeight: 1.6 }}>
          No posts yet. Click <span style={{ color: 'var(--accent)' }}>[+]</span> to make your first post.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {posts.map(post => (
            <div
              key={post.id}
              style={{
                padding: '12px 14px',
                background: 'var(--surface-secondary)',
                borderRadius: 4,
                border: '1px solid var(--border-color)',
              }}
            >
              <div style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--text-muted)', marginBottom: 6 }}>
                {post.author?.display_name || post.author?.handle || 'Unknown'}
                <span style={{ marginLeft: 6, opacity: 0.6 }}>
                  {new Date(post.created_at).toLocaleDateString()}
                </span>
              </div>
              <div style={{ color: 'var(--text-secondary)', fontSize: 14, lineHeight: 1.5, whiteSpace: 'pre-wrap' }}>
                {post.content}
              </div>
              {post.media && post.media.length > 0 && (
                <div style={{ display: 'flex', gap: 8, marginTop: 8, flexWrap: 'wrap' }}>
                  {post.media.slice(0, 4).map((m, i) => (
                    <img
                      key={i}
                      src={m.thumbnail_url || m.url}
                      alt={m.alt_text || ''}
                      style={{ width: 80, height: 80, objectFit: 'cover', borderRadius: 4 }}
                    />
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {hasMore && !loading && (
        <button
          onClick={() => loadPosts()}
          style={{
            display: 'block',
            width: '100%',
            marginTop: 12,
            padding: '8px',
            background: 'none',
            border: '1px solid var(--border-color)',
            borderRadius: 4,
            color: 'var(--text-muted)',
            cursor: 'pointer',
            fontFamily: 'var(--font-mono)',
            fontSize: 12,
          }}
        >
          Load more
        </button>
      )}

      {/* Modals — portaled to body so position:fixed works despite grid transforms */}
      {showComposer && createPortal(
        <ComposerModal
          onClose={() => setShowComposer(false)}
          onPost={() => { setShowComposer(false); loadPosts(); }}
        />,
        document.body
      )}
      {showImport && createPortal(
        <ImportStep
          onComplete={() => { setShowImport(false); loadPosts(); }}
          onSkip={() => setShowImport(false)}
        />,
        document.body
      )}
    </div>
  );
}
