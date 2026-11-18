import { useEffect, useRef, useState } from 'react';

interface Line { text: string; delay: number }

const LINES: Line[] = [
  { text: '> Booting bytescape kernel v0.1.0...', delay: 80 },
  { text: '> Initializing encrypted keychain...', delay: 60 },
  { text: '> Generating Ed25519 identity keypair...', delay: 100 },
  { text: '> Identity: did:key:z6Mk............. [OK]', delay: 50 },
  { text: '> Securing connection to bytescape.social...', delay: 80 },
  { text: '> Connecting to AT Protocol network...', delay: 60 },
  { text: '> Resolving DID identity... [OK]', delay: 80 },
  { text: '> Loading feed subscriptions... [OK]', delay: 60 },
  { text: '> Profile loaded. Welcome to bytescape.', delay: 60 },
];

export default function TerminalBoot({ onComplete }: { onComplete: () => void }) {
  const [lines, setLines] = useState<Line[]>([]);
  const [done, setDone] = useState(false);
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;

    let i = 0;
    const next = () => {
      if (i < LINES.length) {
        setLines(prev => [...prev, LINES[i]]);
        i++;
        setTimeout(next, LINES[i - 1].delay);
      } else {
        setDone(true);
      }
    };
    next();
  }, []);

  useEffect(() => {
    if (done) {
      const t = setTimeout(() => onComplete(), 1200);
      return () => clearTimeout(t);
    }
  }, [done, onComplete]);

  return (
    <div className="fullscreen">
      <div className="terminal-title">BYTESCAPE</div>
      {lines.filter(l => l).map((l, i) => (
        <div key={i} className="line">
          {l.text}{i === lines.length - 1 && !done && <span className="cursor">▌</span>}
        </div>
      ))}
      {done && (
        <div className="line" style={{ marginTop: 8 }}>
          <span className="cursor">▌</span>
        </div>
      )}
    </div>
  );
}
