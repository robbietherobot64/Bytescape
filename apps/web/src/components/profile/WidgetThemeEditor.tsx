import { useState } from 'react';
import type { WidgetTheme, Widget } from '../../utils/api';

interface Props {
  widget: Widget;
  theme: WidgetTheme | undefined;
  themes: WidgetTheme[];
  onSave: (theme: WidgetTheme) => void;
  onClose: () => void;
}

const FONT_OPTIONS = [
  { value: '', label: 'System (inherit)' },
  { value: 'var(--font-mono)', label: 'Monospace' },
  { value: 'var(--font-retro)', label: 'Retro' },
  { value: "'Comic Sans MS', 'Chalkboard SE', sans-serif", label: 'Comic Sans' },
  { value: "'Segoe UI', Tahoma, sans-serif", label: 'Segoe UI' },
];

export default function WidgetThemeEditor({ widget, theme, themes, onSave, onClose }: Props) {
  const [name, setName] = useState(theme?.name || 'Custom');
  const [background, setBackground] = useState(theme?.background || '');
  const [borderColor, setBorderColor] = useState(theme?.border_color || '');
  const [borderRadius, setBorderRadius] = useState(theme?.border_radius || '4px');
  const [textColor, setTextColor] = useState(theme?.text_color || '');
  const [fontFamily, setFontFamily] = useState(theme?.font_family || '');
  const [padding, setPadding] = useState(theme?.padding || '16px');
  const [shadow, setShadow] = useState(theme?.shadow || '');

  const radiusVal = parseInt(borderRadius) || 4;
  const paddingVal = parseInt(padding) || 16;

  const handleSave = () => {
    const newTheme: WidgetTheme = {
      id: theme?.id || `theme_${Date.now()}`,
      name,
      background: background || undefined,
      border_color: borderColor || undefined,
      border_radius: borderRadius || undefined,
      text_color: textColor || undefined,
      font_family: fontFamily || undefined,
      padding: padding || undefined,
      shadow: shadow || undefined,
    };
    onSave(newTheme);
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-box" onClick={e => e.stopPropagation()} style={{ maxWidth: '550px' }}>
        <h2>🎨 Theme Editor</h2>
        <p style={{ marginBottom: '16px' }}>Customize the look of this widget</p>

        <div style={{ display: 'flex', flexDirection: 'column', gap: '14px' }}>
          <div className="profile-edit-field">
            <label className="profile-edit-label">Theme Name</label>
            <input className="profile-edit-input" value={name} onChange={e => setName(e.target.value)} placeholder="Theme name" />
          </div>

          <div className="profile-edit-field">
            <label className="profile-edit-label">Background Color</label>
            <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
              <input type="color" value={background || '#111111'} onChange={e => setBackground(e.target.value)} style={{ width: '48px', height: '36px', border: '1px solid var(--border-dim)', background: 'none', cursor: 'pointer' }} />
              <input className="profile-edit-input" style={{ flex: 1 }} value={background} onChange={e => setBackground(e.target.value)} placeholder="#111 or rgba(...)" />
            </div>
          </div>

          <div className="profile-edit-field">
            <label className="profile-edit-label">Border Color</label>
            <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
              <input type="color" value={borderColor || '#00ff41'} onChange={e => setBorderColor(e.target.value)} style={{ width: '48px', height: '36px', border: '1px solid var(--border-dim)', background: 'none', cursor: 'pointer' }} />
              <input className="profile-edit-input" style={{ flex: 1 }} value={borderColor} onChange={e => setBorderColor(e.target.value)} placeholder="#00ff41 or rgba(...)" />
            </div>
          </div>

          <div className="profile-edit-field">
            <label className="profile-edit-label">Border Radius: {radiusVal}px</label>
            <input type="range" min="0" max="24" value={radiusVal} onChange={e => setBorderRadius(e.target.value + 'px')} style={{ width: '100%', accentColor: 'var(--accent)' }} />
          </div>

          <div className="profile-edit-field">
            <label className="profile-edit-label">Text Color</label>
            <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
              <input type="color" value={textColor || '#00ff41'} onChange={e => setTextColor(e.target.value)} style={{ width: '48px', height: '36px', border: '1px solid var(--border-dim)', background: 'none', cursor: 'pointer' }} />
              <input className="profile-edit-input" style={{ flex: 1 }} value={textColor} onChange={e => setTextColor(e.target.value)} placeholder="#00ff41 or rgba(...)" />
            </div>
          </div>

          <div className="profile-edit-field">
            <label className="profile-edit-label">Font Family</label>
            <select className="profile-edit-input" value={fontFamily} onChange={e => setFontFamily(e.target.value)} style={{ cursor: 'pointer' }}>
              {FONT_OPTIONS.map(f => (
                <option key={f.value} value={f.value}>{f.label}</option>
              ))}
            </select>
          </div>

          <div className="profile-edit-field">
            <label className="profile-edit-label">Padding: {paddingVal}px</label>
            <input type="range" min="0" max="40" value={paddingVal} onChange={e => setPadding(e.target.value + 'px')} style={{ width: '100%', accentColor: 'var(--accent)' }} />
          </div>

          <div className="profile-edit-field">
            <label className="profile-edit-label">Shadow</label>
            <input className="profile-edit-input" value={shadow} onChange={e => setShadow(e.target.value)} placeholder="e.g. 0 4px 12px rgba(0,0,0,0.5)" />
          </div>
        </div>

        <div className="modal-actions">
          <button className="btn" onClick={onClose}>Cancel</button>
          <button className="btn" onClick={handleSave}>💾 Save Theme</button>
        </div>
      </div>
    </div>
  );
}
