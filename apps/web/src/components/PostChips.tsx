import { useState } from 'react';
import { isStarred, sourceKey, ChipSource } from '../utils/feedPrefs';

interface Props {
  sources: ChipSource[];
  onOpenSource: (m: ChipSource) => void;
  maxInline?: number;
}

// Provenance chips: the user's subscribed sources that matched this post,
// most specific first. Truthful by construction — fed by matched_sources.
export default function PostChips({ sources, onOpenSource, maxInline = 3 }: Props) {
  const [expanded, setExpanded] = useState(false);
  if (!sources || sources.length === 0) return null;

  const sorted = [...sources].sort((a, b) => a.rank - b.rank);
  const shown = expanded ? sorted : sorted.slice(0, maxInline);
  const overflow = sorted.length - shown.length;

  return (
    <div className="post-chips" style={{ display: 'flex', gap: 6, flexWrap: 'wrap', margin: '6px 0 2px' }}>
          {shown.map((m, i) => (
        <button
          key={`${m.kind}:${m.label}:${i}`}
          className="post-chip"
          title={m.kind === 'discover' ? 'From discovery — not one of your sources' : `Open everything from ${m.label}`}
          style={{
            background: 'var(--surface-secondary, rgba(255,255,255,0.05))',
            border: m.kind === 'discover' ? '1px dashed var(--border-dim, var(--border-color))' : '1px solid var(--border-color)',
            color: 'var(--text-muted)',
            borderRadius: 999,
            padding: '2px 10px',
            fontSize: 11,
            cursor: 'pointer',
            lineHeight: '16px',
            opacity: m.kind === 'discover' ? 0.75 : 1,
          }}
          onClick={(e) => {
            e.stopPropagation();
            if (m.kind !== 'discover') onOpenSource(m);
          }}
        >
          {isStarred(sourceKey(m)) ? '★ ' : ''}
          {m.kind === 'following' ? 'Following' : m.kind === 'hashtag' ? `#${m.label}` : m.kind === 'discover' ? 'Discover' : m.label}
        </button>
      ))}
      {overflow > 0 && !expanded && (
        <button
          className="post-chip post-chip-more"
          title="Show all matched sources"
          style={{
            background: 'transparent',
            border: '1px dashed var(--border-color)',
            color: 'var(--text-muted)',
            borderRadius: 999,
            padding: '2px 8px',
            fontSize: 11,
            cursor: 'pointer',
          }}
          onClick={(e) => {
            e.stopPropagation();
            setExpanded(true);
          }}
        >
          +{overflow}
        </button>
      )}
    </div>
  );
}
