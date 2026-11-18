import { useEffect, useState } from 'react';

/**
 * Terminal-style loading scroll shown while the personalized feed is being
 * computed. Instead of leaking unpersonalized posts, we animate a monospaced
 * log of what is being loaded — the user's interests/sources mixed with light
 * system jargon — so there is never a silent "random posts" window.
 *
 * End state (owner QA 2026-11-17): if the reveal animation outlasts the data,
 * the log HOLDS on a "loading…" line with a blinking block cursor. It must
 * never finish on a readiness claim the data hasn't earned, and never go
 * blank/dead-air while the fetch is still in flight.
 */
export default function TerminalLoader({ sources }: { sources: string[] }) {
  const lines = buildLines(sources);
  const [visible, setVisible] = useState(1);

  useEffect(() => {
    setVisible(1);
    const id = setInterval(() => {
      setVisible(v => (v >= lines.length ? lines.length : v + 1));
    }, 260);
    return () => clearInterval(id);
  }, [lines.length]);

  // Reveal finished but data hasn't unmounted us yet → hold the tail line.
  const finished = visible >= lines.length;

  return (
    <div className="terminal-loader" aria-busy="true" aria-live="polite">
      <div className="terminal-loader-inner">
        {lines.slice(0, visible).map((l, i) => (
          <div key={i} className={l.kind === 'sys' ? 'tl-sys' : 'tl-item'}>
            <span className="tl-glyph">{l.kind === 'sys' ? '›' : '+'}</span>{' '}
            {l.text}
          </div>
        ))}
        <div className="tl-cursor">
          {finished ? (
            <>loading… <span className="tl-blink">█</span></>
          ) : (
            <><span className="tl-glyph">›</span> <span className="tl-blink">█</span></>
          )}
        </div>
      </div>
    </div>
  );
}

type Line = { text: string; kind: 'sys' | 'item' };

function buildLines(sources: string[]): Line[] {
  const jargon = [
    'resolving atproto handles',
    'hydrating pds index',
    'merging firehose deltas',
    'reconciling graph follows',
    'querying appview cursor',
    'normalizing provenance tags',
    'deduplicating record cids',
  ];
  const src = sources.filter(Boolean).slice(0, 6);
  const lines: Line[] = [{ text: 'bytescape feed :: assembling personalized stream', kind: 'sys' }];
  src.forEach(s => lines.push({ text: `loading ${s}`, kind: 'item' }));
  jargon.forEach(j => lines.push({ text: j, kind: 'sys' }));
  return lines;
}
