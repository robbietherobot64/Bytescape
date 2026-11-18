import { useState } from 'react';

export interface NavItem {
  id: string;
  label: string;
  type: 'default' | 'custom';
  icon?: string;
}

interface ProfileNavProps {
  items: NavItem[];
  activeItem: string;
  onSelect: (id: string) => void;
  onAddPage: (name: string, type: string) => void;
  onRemovePage: (id: string) => void;
  isOwnProfile: boolean;
}

const PAGE_TYPES = ['Blog', 'Gallery', 'Custom Link'];

export default function ProfileNav({
  items,
  activeItem,
  onSelect,
  onAddPage,
  onRemovePage,
  isOwnProfile,
}: ProfileNavProps) {
  const [showForm, setShowForm] = useState(false);
  const [pageName, setPageName] = useState('');
  const [pageType, setPageType] = useState(PAGE_TYPES[0]);

  const handleCreate = () => {
    const name = pageName.trim();
    if (!name) return;
    onAddPage(name, pageType);
    setPageName('');
    setShowForm(false);
  };

  return (
    <nav
      style={{
        background: 'var(--surface)',
        border: '1px solid var(--border-color)',
        borderRadius: 6,
        padding: '12px 0',
        fontFamily: 'var(--font-mono)',
        fontSize: 13,
      }}
    >
      <style>{`
        .profile-nav-item:hover { background: rgba(255,255,255,0.03); }
        .profile-nav-item.active:hover { background: rgba(0, 255, 65, 0.06); }
        .profile-nav-remove:hover { opacity: 1 !important; }
      `}</style>
      <div style={{
        padding: '10px 16px',
        fontFamily: 'var(--font-retro)',
        fontSize: 15,
        color: 'var(--text-primary)',
        borderBottom: '1px solid var(--border-color)',
        fontWeight: 600,
      }}>
        Navigation
      </div>
      {items.map(item => (
        <div
          key={item.id}
          onClick={() => onSelect(item.id)}
          className={`profile-nav-item${activeItem === item.id ? ' active' : ''}`}
          style={{
            padding: '10px 16px',
            cursor: 'pointer',
            color: activeItem === item.id ? 'var(--accent)' : 'var(--text-secondary)',
            background: activeItem === item.id ? 'rgba(0, 255, 65, 0.06)' : 'transparent',
            borderLeft: activeItem === item.id ? '2px solid var(--accent)' : '2px solid transparent',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            transition: 'all 0.15s',
          }}
        >
          <span style={{ fontFamily: 'var(--font-retro)', fontSize: 15 }}>
            {item.icon && <span style={{ marginRight: 6 }}>{item.icon}</span>}
            {item.label}
          </span>
          {item.type === 'custom' && (
            <button
              onClick={e => {
                e.stopPropagation();
                onRemovePage(item.id);
              }}
              title="Remove page"
              className="profile-nav-remove"
              style={{
                background: 'none',
                border: 'none',
                color: 'var(--text-muted)',
                cursor: 'pointer',
                fontSize: 12,
                padding: '2px 4px',
                opacity: 0.5,
              }}
            >
              ✕
            </button>
          )}
        </div>
      ))}

      {isOwnProfile && (
        <div style={{ borderTop: '1px solid var(--border-color)', marginTop: 8, paddingTop: 8, padding: '8px 16px 0' }}>
          {!showForm ? (
            <button
              onClick={() => setShowForm(true)}
              style={{
                background: 'none',
                border: '1px dashed var(--border-color)',
                borderRadius: 4,
                color: 'var(--text-muted)',
                padding: '8px 12px',
                width: '100%',
                cursor: 'pointer',
                fontFamily: 'var(--font-mono)',
                fontSize: 12,
                textAlign: 'left',
              }}
            >
              + Add page
            </button>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <input
                type="text"
                placeholder="Page name..."
                value={pageName}
                onChange={e => setPageName(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleCreate()}
                autoFocus
                style={{
                  background: 'var(--bg)',
                  border: '1px solid var(--border-color)',
                  borderRadius: 4,
                  color: 'var(--text-primary)',
                  padding: '8px 10px',
                  fontFamily: 'var(--font-mono)',
                  fontSize: 12,
                  width: '100%',
                  boxSizing: 'border-box',
                }}
              />
              <select
                value={pageType}
                onChange={e => setPageType(e.target.value)}
                style={{
                  background: 'var(--bg)',
                  border: '1px solid var(--border-color)',
                  borderRadius: 4,
                  color: 'var(--text-primary)',
                  padding: '8px 10px',
                  fontFamily: 'var(--font-mono)',
                  fontSize: 12,
                  width: '100%',
                  boxSizing: 'border-box',
                }}
              >
                {PAGE_TYPES.map(t => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
              <div style={{ display: 'flex', gap: 6 }}>
                <button
                  className="btn"
                  onClick={handleCreate}
                  disabled={!pageName.trim()}
                  style={{
                    flex: 1,
                    background: pageName.trim() ? 'var(--accent)' : undefined,
                    color: pageName.trim() ? '#000' : undefined,
                    border: pageName.trim() ? 'none' : undefined,
                    fontSize: 12,
                    padding: '6px 10px',
                    fontWeight: 600,
                  }}
                >
                  Create
                </button>
                <button
                  onClick={() => { setShowForm(false); setPageName(''); }}
                  style={{
                    background: 'none',
                    border: '1px solid var(--border-color)',
                    color: 'var(--text-muted)',
                    borderRadius: 4,
                    padding: '6px 10px',
                    fontFamily: 'var(--font-mono)',
                    fontSize: 12,
                    cursor: 'pointer',
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </nav>
  );
}
