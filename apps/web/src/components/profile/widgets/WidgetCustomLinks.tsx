import { useState } from 'react';
import type { Widget } from '../../../utils/api';

interface Props {
  widget: Widget;
  editing: boolean;
  onDataChange?: (data: Record<string, unknown>) => void;
}

interface LinkItem {
  label: string;
  url: string;
}

export default function WidgetCustomLinks({ widget, editing, onDataChange }: Props) {
  const links = (widget.data?.links as LinkItem[]) || [];
  const [newLabel, setNewLabel] = useState('');
  const [newUrl, setNewUrl] = useState('');

  const addLink = () => {
    if (!newLabel.trim() || !newUrl.trim()) return;
    const updated = [...links, { label: newLabel.trim(), url: newUrl.trim() }];
    onDataChange?.({ ...widget.data, links: updated });
    setNewLabel('');
    setNewUrl('');
  };

  const removeLink = (idx: number) => {
    const updated = links.filter((_, i) => i !== idx);
    onDataChange?.({ ...widget.data, links: updated });
  };

  const updateLink = (idx: number, field: 'label' | 'url', value: string) => {
    const updated = links.map((l, i) => i === idx ? { ...l, [field]: value } : l);
    onDataChange?.({ ...widget.data, links: updated });
  };

  if (editing) {
    return (
      <div className="widget-links">
        {links.map((link, idx) => (
          <div key={idx} style={{ display: 'flex', gap: '8px', marginBottom: '8px', alignItems: 'center' }}>
            <input
              className="profile-edit-input"
              style={{ flex: 1, fontSize: '12px', padding: '6px 10px' }}
              value={link.label}
              onChange={e => updateLink(idx, 'label', e.target.value)}
              placeholder="Label"
            />
            <input
              className="profile-edit-input"
              style={{ flex: 1, fontSize: '12px', padding: '6px 10px' }}
              value={link.url}
              onChange={e => updateLink(idx, 'url', e.target.value)}
              placeholder="https://"
            />
            <button onClick={() => removeLink(idx)} style={{ background: 'none', border: 'none', color: 'var(--danger)', cursor: 'pointer', fontSize: '16px', padding: '4px' }}>×</button>
          </div>
        ))}
        <div style={{ display: 'flex', gap: '8px', marginTop: '8px' }}>
          <input
            className="profile-edit-input"
            style={{ flex: 1, fontSize: '12px', padding: '6px 10px' }}
            placeholder="Link label"
            value={newLabel}
            onChange={e => setNewLabel(e.target.value)}
          />
          <input
            className="profile-edit-input"
            style={{ flex: 1, fontSize: '12px', padding: '6px 10px' }}
            placeholder="https://"
            value={newUrl}
            onChange={e => setNewUrl(e.target.value)}
          />
          <button className="btn" style={{ fontSize: '12px', padding: '6px 12px' }} onClick={addLink}>Add</button>
        </div>
      </div>
    );
  }

  if (links.length === 0) {
    return <div style={{ color: 'var(--text-muted)', fontStyle: 'italic', fontSize: '13px', padding: '8px 0' }}>No links configured</div>;
  }

  return (
    <div className="widget-links" style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
      {links.map((link, idx) => (
        <a
          key={idx}
          href={link.url}
          target="_blank"
          rel="noopener noreferrer"
          className="profile-chip-link"
          style={{ display: 'flex', alignItems: 'center', gap: '6px', padding: '4px 0' }}
        >
          🔗 {link.label}
        </a>
      ))}
    </div>
  );
}
