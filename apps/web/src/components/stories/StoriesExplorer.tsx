import { useCallback, useEffect, useState } from 'react';
import { storiesApi } from '../../utils/api';
import type { StoryGroup } from '../../utils/api';
import StoryArchiveList from './StoryArchiveList';

export default function StoriesExplorer({
  onOpenGroup,
  onClose,
}: {
  onOpenGroup: (groups: StoryGroup[], index: number) => void;
  onClose: () => void;
}) {
  const [groups, setGroups] = useState<StoryGroup[]>([]);
  const [total, setTotal] = useState(0);
  const [sort, setSort] = useState('recent');
  const [person, setPerson] = useState('');
  const [kind, setKind] = useState('both');

  const load = useCallback(() => {
    storiesApi.explorer({ sort, person, kind, limit: 50 }).then((r) => {
      setGroups(r.groups);
      setTotal(r.total);
    }).catch(() => {});
  }, [sort, person, kind]);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="composer-modal-overlay" onClick={onClose}>
      <div className="composer-modal stories-explorer" onClick={(e) => e.stopPropagation()}>
        <div className="story-composer-tabs">
          <span className="stories-explorer-title">✦ Stories</span>
          <button className="btn story-composer-close" onClick={onClose}>×</button>
        </div>

        <div className="stories-explorer-controls">
          <select className="btn" value={sort} onChange={(e) => setSort(e.target.value)}>
            <option value="recent">Most recent</option>
            <option value="author">By author</option>
          </select>
          <input
            className="stories-explorer-person"
            placeholder="Search people…"
            value={person}
            onChange={(e) => setPerson(e.target.value)}
          />
          <select className="btn" value={kind} onChange={(e) => setKind(e.target.value)}>
            <option value="both">All</option>
            <option value="media">Media</option>
            <option value="status">Text</option>
          </select>
          <span className="stories-explorer-total">{total} groups</span>
        </div>

        <div className="stories-explorer-grid">
          {groups.map((g, idx) => {
            const name = g.author.display_name || g.author.handle;
            return (
              <button
                key={g.author.did}
                className={`story-ring${g.seen ? ' story-ring-seen' : ''}`}
                onClick={() => {
                  onOpenGroup(groups, idx);
                  onClose();
                }}
              >
                {g.author.avatar ? (
                  <img src={g.author.avatar} alt="" className="story-ring-avatar" />
                ) : (
                  <span className="story-ring-avatar story-ring-avatar-fallback">
                    {name[1]?.toUpperCase() || '?'}
                  </span>
                )}
                <span className="story-ring-name">{name}</span>
                <span className="story-ring-meta">{g.items.length} item{g.items.length === 1 ? '' : 's'}</span>
              </button>
            );
          })}
        </div>

        <div className="stories-explorer-mine">
          <h3 className="stories-explorer-subtitle">Your stories</h3>
          <StoryArchiveList />
        </div>
      </div>
    </div>
  );
}
