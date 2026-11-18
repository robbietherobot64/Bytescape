import { useState } from 'react';
import type { WidgetType } from './widgetTypes';

interface Props {
  isOwn: boolean;
  editMode: boolean;
  previewMode: boolean;
  saving: boolean;
  onEdit: () => void;
  onPreview: () => void;
  onSave: () => void;
  onCancel: () => void;
  onAddWidget: (type: WidgetType) => void;
  onResetLayout: () => void;
  theme: string;
  onThemeChange: (theme: string) => void;
}

const THEME_PRESETS: { id: string; label: string }[] = [
  { id: 'default', label: 'Default' },
  { id: 'terminal', label: 'Terminal' },
  { id: '1999', label: '1999' },
  { id: 'xp', label: 'XP' },
];

const WIDGET_PALETTE: { type: WidgetType; label: string }[] = [
  { type: 'profile_header', label: 'Profile Header' },
  { type: 'navigation', label: 'Navigation' },
  { type: 'friends', label: 'Friends' },
  { type: 'following', label: 'Following' },
  { type: 'music_player', label: 'Music Player' },
  { type: 'photo_gallery', label: 'Photo Gallery' },
  { type: 'custom_links', label: 'Custom Links' },
  { type: 'status', label: 'Status' },
];

export default function ProfileToolbar({
  isOwn,
  editMode,
  previewMode,
  saving,
  onEdit,
  onPreview,
  onSave,
  onCancel,
  onAddWidget,
  onResetLayout,
  theme,
  onThemeChange,
}: Props) {
  const [showPalette, setShowPalette] = useState(false);

  if (!isOwn) return null;

  if (!editMode) {
    return (
      <div className="profile-toolbar">
        <button className="btn profile-toolbar-btn" onClick={onEdit} data-tour="edit-profile">
          ✏️ Edit Profile
        </button>
        <button
          className="btn profile-toolbar-btn"
          data-tour="import-button"
          onClick={() => window.dispatchEvent(new Event('bytescape:openImport'))}
        >
          📦 Import archive
        </button>
        <div className="profile-theme-picker" style={{ display: 'flex', gap: 6, alignItems: 'center', marginLeft: 'auto' }}>
          <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Theme:</span>
          {THEME_PRESETS.map(t => (
            <button
              key={t.id}
              className={`btn profile-toolbar-btn${(theme || 'default') === t.id ? ' active' : ''}`}
              style={{ fontSize: 11, padding: '2px 8px' }}
              onClick={() => onThemeChange(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="profile-toolbar profile-toolbar-editing">
      <div className="profile-toolbar-left">
        <button
          className={`btn profile-toolbar-btn ${previewMode ? '' : 'active'}`}
          onClick={onPreview}
        >
          {previewMode ? '👁 Edit' : '👁 Preview'}
        </button>
      </div>
      <div className="profile-toolbar-right">
        <div className="profile-toolbar-add">
          <button className="btn profile-toolbar-btn" onClick={() => setShowPalette(!showPalette)}>
            + Add Widget
          </button>
          {showPalette && (
            <div className="profile-toolbar-palette">
              {WIDGET_PALETTE.map(w => (
                <button
                  key={w.type}
                  className="profile-toolbar-palette-item"
                  onClick={() => { onAddWidget(w.type); setShowPalette(false); }}
                >
                  {w.label}
                </button>
              ))}
            </div>
          )}
        </div>
        <button className="btn profile-toolbar-btn" onClick={onResetLayout} disabled={saving || previewMode}>
          🔄 Reset
        </button>
        <button className="btn profile-toolbar-btn" onClick={onSave} disabled={saving || previewMode}>
          {saving ? 'Saving...' : '💾 Save'}
        </button>
        <button className="btn profile-toolbar-btn profile-toolbar-cancel" onClick={onCancel} disabled={saving}>
          Cancel
        </button>
      </div>
    </div>
  );
}
