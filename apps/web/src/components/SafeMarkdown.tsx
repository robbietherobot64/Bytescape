import type React from 'react';
import { parseMarkdown, type Seg } from '../utils/markdown';

/**
 * Safe markdown body renderer for fediverse posts. Renders parsed segments as
 * React elements — text is auto-escaped by React (never
 * `dangerouslySetInnerHTML`), links use scheme-sanitized hrefs with
 * `rel="noopener noreferrer"` and `target="_blank"`. Raw HTML in the source is
 * stripped by `parseMarkdown` before we ever see it.
 */
export default function SafeMarkdown({ text }: { text: string }) {
  const segs = parseMarkdown(text);
  return <>{segs.map((s, i) => renderSeg(s, i))}</>;
}

function renderSeg(s: Seg, i: number): React.ReactNode {
  switch (s.t) {
    case 'text':
      // React escapes this — no raw HTML execution.
      return <span key={i}>{s.v}</span>;
    case 'link':
      return (
        <a
          key={i}
          href={s.href}
          target="_blank"
          rel="noopener noreferrer"
          onClick={(e) => e.stopPropagation()}
          style={{ color: 'var(--accent)', textDecoration: 'underline', textUnderlineOffset: 2 }}
        >
          {s.text}
        </a>
      );
    case 'strong':
      return <strong key={i}>{s.v}</strong>;
    case 'em':
      return <em key={i}>{s.v}</em>;
    case 'code':
      return (
        <code
          key={i}
          style={{
            background: 'rgba(255,255,255,0.08)',
            padding: '1px 5px',
            borderRadius: 3,
            fontFamily: 'var(--font-mono, monospace)',
          }}
        >
          {s.v}
        </code>
      );
    case 'heading': {
      const size = s.level === 1 ? 18 : s.level === 2 ? 16 : 14;
      return (
        <div key={i} style={{ fontWeight: 700, fontSize: size, margin: '6px 0 2px' }}>
          {s.v}
        </div>
      );
    }
    default:
      return null;
  }
}
