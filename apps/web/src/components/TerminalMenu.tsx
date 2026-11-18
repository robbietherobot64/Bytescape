import { useState, useEffect, useCallback } from 'react';

export interface MenuOption {
  id: string;
  label: string;
  hint?: string;
}

interface Props {
  title: string;
  options: MenuOption[];
  onSelect: (id: string) => void;
  onBack?: () => void;
}

export default function TerminalMenu({ title, options, onSelect, onBack }: Props) {
  const [highlighted, setHighlighted] = useState(0);

  const handleKey = useCallback((e: KeyboardEvent) => {
    try {
      if (e.key === 'ArrowDown' || e.key === 'j') {
        e.preventDefault();
        setHighlighted(prev => Math.min(prev + 1, options.length - 1));
      } else if (e.key === 'ArrowUp' || e.key === 'k') {
        e.preventDefault();
        setHighlighted(prev => Math.max(prev - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        onSelect(options[highlighted].id);
      } else if (e.key === 'Escape' && onBack) {
        e.preventDefault();
        onBack();
      }
    } catch (err) {
      console.error('[TerminalMenu] key handler error:', err);
    }
  }, [highlighted, options, onSelect, onBack]);

  useEffect(() => {
    window.addEventListener('keydown', handleKey);
    return () => window.removeEventListener('keydown', handleKey);
  }, [handleKey]);

  return (
    <div className="fullscreen">
      <div className="terminal-title">BYTESCAPE</div>
      {title && <div style={{ color: 'var(--text-muted)', marginBottom: 16, fontSize: 13 }}>{title}</div>}
      <div className="terminal-menu">
        {options.map((opt, i) => (
          <div
            key={opt.id}
            className={`terminal-option ${i === highlighted ? 'highlighted' : ''}`}
            onClick={() => { setHighlighted(i); onSelect(opt.id); }}
            style={{ cursor: 'pointer' }}
          >
            <span className="arrow">{i === highlighted ? '▸' : ' '}</span>
            <span className="label">{opt.label}</span>
            {opt.hint && <span className="hint">{opt.hint}</span>}
          </div>
        ))}
      </div>
      <div style={{ marginTop: 24, fontSize: 12, color: 'var(--text-muted)' }}>
        ↑↓ to navigate · Enter to select{onBack ? ' · Esc to go back' : ''}
      </div>
    </div>
  );
}

