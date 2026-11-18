import { useState, useEffect, useCallback } from 'react';
import type { FollowingView } from '../../../utils/api';
import { profileApi, api } from '../../../utils/api';

const PAGE_SIZE = 20;

const NETWORK_BADGES: Record<string, { label: string; color: string }> = {
  local: { label: 'local', color: '#2ecc71' },
  activitypub: { label: 'fediverse', color: '#e67e22' },
  atproto: { label: 'bluesky', color: '#3498db' },
};

interface Props {
  username: string;
  isOwn: boolean;
  onViewProfile?: (username: string) => void;
}

/**
 * Followers tab (identity-graph Phase 3): the merged follower list —
 * network badges per row (provenance), follow-state actions through the
 * unified follow API, remove-follower where the network permits (the AP case
 * retracts on-network via an Undo), and paging through the list.
 */
export default function WidgetFollowers({ username, isOwn, onViewProfile }: Props) {
  const [rows, setRows] = useState<FollowingView[]>([]);
  const [shown, setShown] = useState(PAGE_SIZE);
  const [busy, setBusy] = useState<string | null>(null);
  const [hidden, setHidden] = useState(false);
  const [homeHidden, setHomeHidden] = useState(false);
  const [homeInstance, setHomeInstance] = useState<string | null>(null);

  useEffect(() => {
    setShown(PAGE_SIZE);
    profileApi
      .followers(username)
      .then((r) => {
        setRows(r.rows);
        setHidden(r.hidden);
        setHomeHidden(r.home_graph_hidden);
        setHomeInstance(r.home_instance);
      })
      .catch(() => setRows([]));
  }, [username]);

  const toggleFollow = useCallback(
    async (row: FollowingView) => {
      setBusy(row.handle);
      const wasFollowing = row.following;
      try {
        setRows(rs =>
          rs.map(r =>
            r.handle === row.handle
              ? { ...r, following: !wasFollowing, follow_pending: !wasFollowing }
              : r,
          ),
        );
        const source =
          row.network === 'atproto' ? 'bluesky' : row.network === 'activitypub' ? 'mastodon' : 'local';
        // Unfollowing a pending request is the cancel — the undo withdraws
        // the Follow on-network.
        await api(wasFollowing ? '/api/feed/unfollow' : '/api/feed/follow', {
          method: 'POST',
          body: JSON.stringify({ handle: row.username, source }),
        });
      } catch {
        setRows(rs =>
          rs.map(r =>
            r.handle === row.handle
              ? { ...r, following: wasFollowing, follow_pending: row.follow_pending }
              : r,
          ),
        );
      } finally {
        setBusy(null);
      }
    },
    [],
  );

  const removeFollower = useCallback(
    async (row: FollowingView) => {
      if (!window.confirm(`Remove ${row.username} from your followers?`)) return;
      setBusy(row.handle);
      try {
        await profileApi.removeFollower(username, row.handle);
        setRows(rs => rs.filter(r => r.handle !== row.handle));
      } catch {
        // Keep the row: removal failed and nothing changed.
      } finally {
        setBusy(null);
      }
    },
    [username],
  );

  return (
    <div className="widget-followers" style={{ padding: 'var(--widget-padding, 12px)' }}>
      <h3 style={{ margin: '0 0 10px', color: 'var(--widget-text, inherit)', fontFamily: 'var(--font-retro)' }}>
        Followers ({rows.length})
      </h3>
      {hidden && (
        <div style={{ color: 'var(--text-muted)', fontSize: 13, marginBottom: 8 }}>
          This user's follow lists are hidden.
        </div>
      )}
      {!hidden && homeHidden && (
        <div style={{ color: 'var(--text-muted)', fontSize: 12, marginBottom: 8 }}>
          Follow lists are hidden on {homeInstance ?? 'the home instance'} — only the
          Bytescape-side graph is shown.
        </div>
      )}
      {!hidden && rows.length === 0 && (
        <div style={{ color: 'var(--text-muted)', fontSize: 13 }}>No followers yet.</div>
      )}
      {rows.slice(0, shown).map(row => {
        const badge = NETWORK_BADGES[row.network] ?? { label: row.network, color: '#888' };
        return (
          <div
            key={`${row.network}:${row.handle}`}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              padding: '6px 0',
              borderBottom: '1px solid var(--border-color)',
            }}
          >
            {row.avatar ? (
              <img
                src={row.avatar}
                alt=""
                style={{ width: 32, height: 32, borderRadius: '50%', objectFit: 'cover' }}
              />
            ) : (
              <div
                style={{
                  width: 32,
                  height: 32,
                  borderRadius: '50%',
                  background: 'var(--surface)',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  fontSize: 14,
                }}
              >
                👤
              </div>
            )}
            <div style={{ flex: 1, minWidth: 0 }}>
              <button
                onClick={() => onViewProfile?.(row.username)}
                style={{
                  background: 'none',
                  border: 'none',
                  color: 'var(--accent)',
                  cursor: 'pointer',
                  padding: 0,
                  fontFamily: 'var(--font-retro)',
                  fontSize: 13,
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  whiteSpace: 'nowrap',
                  maxWidth: '100%',
                  textAlign: 'left',
                }}
                title={row.handle}
              >
                {row.display_name || row.username}
              </button>
              <div style={{ display: 'flex', gap: 6, alignItems: 'center', marginTop: 2 }}>
                <span
                  style={{
                    fontSize: 10,
                    padding: '1px 6px',
                    borderRadius: 8,
                    background: badge.color,
                    color: '#fff',
                  }}
                >
                  {badge.label}
                </span>
                <span style={{ fontSize: 11, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  @{row.username}
                </span>
              </div>
            </div>
            {!isOwn && (
              <button
                onClick={() => toggleFollow(row)}
                disabled={busy === row.handle}
                style={{
                  borderRadius: 4,
                  padding: '3px 10px',
                  fontSize: 11,
                  cursor: busy === row.handle ? 'wait' : 'pointer',
                  border: row.following ? '1px solid var(--border-color)' : 'none',
                  background: row.following ? 'transparent' : 'var(--accent)',
                  color: row.following ? 'var(--text-muted)' : '#000',
                  whiteSpace: 'nowrap',
                }}
              >
                {row.following ? (row.follow_pending ? 'Requested' : '✓ Following') : '+ Follow'}
              </button>
            )}
            {isOwn && row.can_remove && (
              <button
                onClick={() => removeFollower(row)}
                disabled={busy === row.handle}
                style={{
                  borderRadius: 4,
                  padding: '3px 10px',
                  fontSize: 11,
                  cursor: busy === row.handle ? 'wait' : 'pointer',
                  border: '1px solid var(--border-color)',
                  background: 'transparent',
                  color: '#e74c3c',
                  whiteSpace: 'nowrap',
                }}
              >
                Remove
              </button>
            )}
          </div>
        );
      })}
      {shown < rows.length && (
        <button
          onClick={() => setShown(s => s + PAGE_SIZE)}
          style={{
            marginTop: 8,
            width: '100%',
            padding: '6px 0',
            borderRadius: 4,
            border: '1px solid var(--border-color)',
            background: 'var(--surface)',
            color: 'var(--text-secondary)',
            cursor: 'pointer',
            fontSize: 12,
          }}
        >
          Load more ({rows.length - shown} more)
        </button>
      )}
    </div>
  );
}
