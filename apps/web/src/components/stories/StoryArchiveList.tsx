import { useEffect, useState } from 'react';
import { api, storiesApi, storyMediaUrl } from '../../utils/api';
import type { StoryItem } from '../../utils/api';

/**
 * Owner-only story archive listing: active/scheduled/never/archived items
 * with lifetime metadata, reshare-from-archive (as story or as post), and
 * permanent deletion. Shared by the Stories explorer and the Vault.
 */
export default function StoryArchiveList() {
  const [items, setItems] = useState<StoryItem[] | null>(null);
  const [busyUri, setBusyUri] = useState('');

  useEffect(() => {
    storiesApi.mine().then((r) => setItems(r.items)).catch(() => setItems([]));
  }, []);

  const deleteStory = async (uri: string, archived: boolean) => {
    const ok = window.confirm(
      archived
        ? 'Delete this story permanently? Its media will be removed from the archive.'
        : 'Delete this story? It will be removed from your story and your Bluesky repo.'
    );
    if (!ok) return;
    setBusyUri(uri);
    try {
      if (archived) await storiesApi.archiveDelete(uri);
      else await storiesApi.remove(uri);
      setItems((list) => (list || []).filter((i) => i.uri !== uri));
    } catch {
      /* keep listing */
    } finally {
      setBusyUri('');
    }
  };

  const reshareAsStory = async (item: StoryItem) => {
    setBusyUri(item.uri);
    try {
      const media_urls = (item.media || []).map((m) =>
        item.state === 'archived' ? storiesApi.archiveMediaUrl(item.uri, m.cid) : storyMediaUrl(item.author_did, m)
      );
      await storiesApi.create({
        media_urls,
        text: item.text,
        expires_at: new Date(Date.now() + 24 * 3600e3).toISOString(),
      });
    } catch {
      /* keep listing */
    } finally {
      setBusyUri('');
    }
  };

  const reshareAsPost = async (item: StoryItem) => {
    setBusyUri(item.uri);
    try {
      const media_urls = (item.media || []).map((m) =>
        item.state === 'archived' ? storiesApi.archiveMediaUrl(item.uri, m.cid) : storyMediaUrl(item.author_did, m)
      );
      await api('/api/posts/create', {
        method: 'POST',
        body: JSON.stringify({
          content: item.text || 'From my story archive',
          visibility: 'public',
          platforms: [],
          media_urls,
        }),
      });
    } catch {
      /* keep listing */
    } finally {
      setBusyUri('');
    }
  };

  if (items === null) return <div className="story-comments-empty">Loading stories…</div>;
  if (items.length === 0) return <div className="story-comments-empty">No stories yet.</div>;

  return (
    <div className="stories-explorer-grid">
      {items.map((item) => (
        <div key={item.uri} className="story-mine-card">
          {item.kind === 'status' ? (
            <div
              className="story-mine-thumb story-mine-thumb-text"
              style={{ background: item.style?.bg?.image || item.style?.bg?.color || '#0a2a0a' }}
            >
              {(item.text || '…').slice(0, 40)}
            </div>
          ) : item.media?.[0] ? (
            item.state === 'archived' ? (
              <img
                src={storiesApi.archiveMediaUrl(item.uri, item.media[0].cid)}
                alt=""
                className="story-mine-thumb"
              />
            ) : item.media[0].type === 'video' ? (
              <video
                src={storyMediaUrl(item.author_did, item.media[0])}
                className="story-mine-thumb"
                autoPlay
                muted
                loop
                playsInline
                preload="metadata"
              />
            ) : (
              <img src={storyMediaUrl(item.author_did, item.media[0])} alt="" className="story-mine-thumb" />
            )
          ) : (
            <div className="story-mine-thumb" />
          )}
          <div className="story-mine-meta">
            <span className={`story-state-badge story-state-${item.state}`}>{item.state}</span>
            {item.pinned && <span title="Pinned">📌</span>}
            <span className="story-mine-age">
              {new Date(item.created_at).toLocaleDateString()}
              {item.expires_at
                ? ` → ${new Date(item.expires_at).toLocaleDateString()}`
                : ' → never'}
            </span>
          </div>
          <div className="story-mine-actions">
            <button
              className="btn"
              disabled={busyUri === item.uri}
              onClick={() => reshareAsStory(item)}
              title="Reshare as a new story"
            >
              🔁 Story
            </button>
            <button
              className="btn"
              disabled={busyUri === item.uri}
              onClick={() => reshareAsPost(item)}
              title="Share as a post"
            >
              📤 Post
            </button>
            <button
              className="btn story-mine-delete"
              disabled={busyUri === item.uri}
              onClick={() => deleteStory(item.uri, item.state === 'archived')}
              title={item.state === 'archived' ? 'Delete permanently' : 'Delete story'}
            >
              🗑
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
