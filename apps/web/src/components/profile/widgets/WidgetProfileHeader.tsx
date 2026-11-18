import { useEffect, useRef, useState, useMemo, type ChangeEvent } from 'react';
import { createPortal } from 'react-dom';
import type { ProfileData, Widget } from '../../../utils/api';
import { profileApi } from '../../../utils/api';

// ==== Types ====
interface CustomProfileField {
  field_key: string;
  label?: string;
  content?: string;
  visible: boolean;
}

interface Props {
  widget: Widget;
  profile: ProfileData;
  editing: boolean;
  editForm?: Record<string, unknown>;
  onEditFormChange?: (form: Record<string, unknown>) => void;
  onDataChange?: (data: Record<string, unknown>) => void;
}

// ==== Defaults ====
const DEFAULT_FIELDS: CustomProfileField[] = [
  { field_key: 'bio',               visible: true  },
  { field_key: 'location',          visible: false },
  { field_key: 'hometown',          visible: false },
  { field_key: 'birthday',          visible: true  },
  { field_key: 'age',               visible: false },
  { field_key: 'website',           visible: false },
  { field_key: 'pronouns',          visible: false },
  { field_key: 'relationship_status', visible: false },
  { field_key: 'education',         visible: false },
  { field_key: 'occupation',        visible: false },
  { field_key: 'interests',         visible: false },
  { field_key: 'stats',             visible: true  },
];

const RELATIONSHIP_OPTIONS = [
  'Single',
  'Married',
  'Dating',
  'Engaged',
  "It's Complicated",
  'Prefer not to say',
];

const FIELD_LABELS: Record<string, string> = {
  bio: 'About Me',
  location: 'Location',
  hometown: 'Hometown',
  birthday: 'Birthday',
  age: 'Age',
  website: 'Website',
  pronouns: 'Pronouns',
  relationship_status: 'Relationship',
  education: 'Education',
  occupation: 'Occupation',
  interests: 'Interests',
  stats: 'Stats',
};

// ==== Helpers ====
function ageFromBirthday(birthday: string | null | undefined): number | null {
  if (!birthday) return null;
  const birth = new Date(birthday);
  if (isNaN(birth.getTime())) return null;
  const now = new Date();
  let age = now.getFullYear() - birth.getFullYear();
  const m = now.getMonth() - birth.getMonth();
  if (m < 0 || (m === 0 && now.getDate() < birth.getDate())) age--;
  return age;
}

function fmtBirthday(birthday: string | null | undefined): string {
  if (!birthday) return '';
  const d = new Date(birthday);
  if (isNaN(d.getTime())) return '';
  return d.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
}

function buildActiveFields(saved: CustomProfileField[] | undefined): CustomProfileField[] {
  if (!saved || saved.length === 0) return DEFAULT_FIELDS;
  const map = new Map(saved.map(f => [f.field_key, f]));
  for (const def of DEFAULT_FIELDS) {
    if (!map.has(def.field_key)) {
      map.set(def.field_key, { field_key: def.field_key, visible: false });
    }
  }
  // Interests are no longer part of the header box (clutter) — hard-hide even
  // for profiles that saved it as visible before the change.
  const interests = map.get('interests');
  if (interests) map.set('interests', { ...interests, visible: false });
  return Array.from(map.values());
}

// ==== Component ====
export default function WidgetProfileHeader({ widget, profile, editing, editForm, onEditFormChange }: Props) {
  const avatarInputRef = useRef<HTMLInputElement>(null);
  const headerInputRef = useRef<HTMLInputElement>(null);
  const [showConfig, setShowConfig] = useState(false);
  const [panelPos, setPanelPos] = useState({ top: 0, left: 0 });

  // --- Data sources ---
  const displayName = editing && editForm ? (editForm.display_name as string || '') : profile.display_name;
  const avatarUrl = editing && editForm ? (editForm.avatar_url as string || profile.avatar_url || '') : profile.avatar_url || '';
  const headerUrl = editing && editForm ? (editForm.header_url as string || profile.header_url || '') : profile.header_url || '';

  // Source of truth: custom_profile_fields from editForm (edit mode) or profile (view mode)
  const sourceFields = editing && editForm && editForm.custom_profile_fields
    ? (editForm.custom_profile_fields as CustomProfileField[])
    : profile.custom_profile_fields;

  const activeFields = useMemo(() => buildActiveFields(sourceFields), [sourceFields]);
  const visibleFields = activeFields.filter(f => f.visible);
  const isOwn = profile.is_own_profile;
  const canEdit = editing && isOwn;

  // Sort fields: DEFAULT order first, then custom_* fields at the end
  const sortedVisible = useMemo(() => {
    const builtIn = DEFAULT_FIELDS
      .filter(df => visibleFields.some(vf => vf.field_key === df.field_key))
      .map(df => visibleFields.find(vf => vf.field_key === df.field_key)!);
    const custom = visibleFields.filter(f => f.field_key.startsWith('custom_'));
    return [...builtIn, ...custom];
  }, [visibleFields]);

  // --- Upload handlers ---
  const handleAvatarUpload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !onEditFormChange) return;
    try {
      const data = await profileApi.uploadAvatar(file);
      onEditFormChange({ ...editForm, avatar_url: data.avatar_url });
    } catch {}
  };

  const handleHeaderUpload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !onEditFormChange) return;
    try {
      const data = await profileApi.uploadHeader(file);
      onEditFormChange({ ...editForm, header_url: data.header_url });
    } catch {}
  };

  // --- Toggle field visibility ---
  function toggleField(key: string) {
    const current = editing && editForm && editForm.custom_profile_fields
      ? (editForm.custom_profile_fields as CustomProfileField[])
      : profile.custom_profile_fields;
    const updated = buildActiveFields(current).map(f =>
      f.field_key === key ? { ...f, visible: !f.visible } : f
    );
    onEditFormChange?.({ ...editForm, custom_profile_fields: updated });
  }

  // --- Custom field CRUD ---
  function addCustom() {
    const current = (editForm?.custom_profile_fields as CustomProfileField[]) || [];
    const idx = current.filter(f => f.field_key.startsWith('custom_')).length + 1;
    const newField: CustomProfileField = {
      field_key: `custom_${Date.now()}`,
      label: `Custom ${idx}`,
      content: '',
      visible: true,
    };
    onEditFormChange?.({ ...editForm, custom_profile_fields: [...current, newField] });
  }

  function updateCustom(key: string, patch: Partial<CustomProfileField>) {
    const current = (editForm?.custom_profile_fields as CustomProfileField[]) || [];
    const updated = current.map(f => (f.field_key === key ? { ...f, ...patch } : f));
    onEditFormChange?.({ ...editForm, custom_profile_fields: updated });
  }

  function removeCustom(key: string) {
    const current = (editForm?.custom_profile_fields as CustomProfileField[]) || [];
    const updated = current.filter(f => f.field_key !== key);
    onEditFormChange?.({ ...editForm, custom_profile_fields: updated });
  }

  // --- Field content helper ---
  function getFieldContent(fkey: string): string | null {
    switch (fkey) {
      case 'location':
        return (editing && editForm ? (editForm.location as string) : profile.location) || null;
      case 'birthday': {
        const bd = editing && editForm ? (editForm.birthday as string) : profile.birthday;
        return bd ? fmtBirthday(bd) : null;
      }
      case 'age': {
        const bd = editing && editForm ? (editForm.birthday as string) : profile.birthday;
        const a = ageFromBirthday(bd);
        return a !== null ? `${a} years old` : null;
      }
      case 'occupation':
        return editing && editForm ? ((editForm.occupation as string) || null) : (profile.occupation || null);
      case 'relationship_status':
        return editing && editForm ? ((editForm.relationship_status as string) || null) : (profile.relationship_status || null);
      case 'pronouns':
        return editing && editForm ? ((editForm.pronouns as string) || null) : (profile.pronouns || null);
      case 'website':
        return editing && editForm ? ((editForm.website as string) || null) : (profile.website || null);
      case 'interests': {
        const interests = editing && editForm ? (editForm.interests as string[]) : profile.interests;
        return interests && interests.length > 0 ? interests.join(', ') : null;
      }
      default:
        return null;
    }
  }

  // --- Render a single field ---
  function renderFieldContent(f: CustomProfileField): React.ReactNode {
    const fkey = f.field_key;

    // Bio — content from profile.bio / editForm.bio, NOT from f.content
    if (fkey === 'bio') {
      const bioText = editing && editForm ? (editForm.bio as string || '') : profile.bio || '';
      if (canEdit) {
        return (
          <textarea
            className="profile-edit-textarea profile-bio-input"
            value={bioText}
            onChange={e => onEditFormChange?.({ ...editForm, bio: e.target.value })}
            placeholder="Bio"
            rows={3}
            style={{ color:'#fff', background:'rgba(0,0,0,0.4)', width:'100%' }}
          />
        );
      }
      return bioText ? <p style={{ margin:'4px 0', lineHeight:'1.5' }}>{bioText}</p> : null;
    }

    // Hometown & Education — no DB column, stored in f.content
    if (fkey === 'hometown' || fkey === 'education') {
      if (canEdit) {
        return (
          <textarea
            className="profile-edit-textarea"
            value={f.content || ''}
            onChange={e => updateCustom(fkey, { content: e.target.value })}
            placeholder={FIELD_LABELS[fkey] || fkey}
            rows={2}
            style={{ color:'#fff', background:'rgba(0,0,0,0.4)', width:'100%' }}
          />
        );
      }
      if (!f.content) return null;
      return <p style={{ margin:'4px 0' }}>{f.content}</p>;
    }

    // Custom fields
    if (fkey.startsWith('custom_')) {
      if (canEdit) {
        return (
          <div style={{ display:'flex', flexDirection:'column', gap:'4px' }}>
            <input
              className="profile-edit-input"
              value={f.label || ''}
              onChange={e => updateCustom(fkey, { label: e.target.value })}
              placeholder="Section title"
              style={{ color:'#fff', background:'rgba(0,0,0,0.4)' }}
            />
            <textarea
              className="profile-edit-textarea"
              value={f.content || ''}
              onChange={e => updateCustom(fkey, { content: e.target.value })}
              placeholder="Content"
              rows={3}
              style={{ color:'#fff', background:'rgba(0,0,0,0.4)' }}
            />
          </div>
        );
      }
      return (
        <div style={{ margin:'4px 0' }}>
          {f.label && <strong>{f.label}: </strong>}
          {f.content && <span style={{ whiteSpace:'pre-wrap' }}>{f.content}</span>}
        </div>
      );
    }

    // Stats — always rendered from profile counts
    if (fkey === 'stats') {
      return (
        <div className="profile-stats" style={{ display:'flex', justifyContent:'center', gap:'24px', padding:'10px 0', borderTop:'1px solid rgba(255,255,255,0.2)', borderBottom:'1px solid rgba(255,255,255,0.2)', margin:'8px 0' }}>
          <span className="profile-stat" style={{ color:'rgba(255,255,255,0.7)' }}>
            <strong style={{ color:'#fff', fontFamily:'var(--font-retro)', fontSize:'16px' }}>{profile.posts_count}</strong> posts
          </span>
          <span className="profile-stat" style={{ color:'rgba(255,255,255,0.7)' }}>
            <strong style={{ color:'#fff', fontFamily:'var(--font-retro)', fontSize:'16px' }}>{profile.followers_count}</strong> followers
          </span>
          <span className="profile-stat" style={{ color:'rgba(255,255,255,0.7)' }}>
            <strong style={{ color:'#fff', fontFamily:'var(--font-retro)', fontSize:'16px' }}>{profile.following_count}</strong> following
          </span>
        </div>
      );
    }

    // Standard built-in fields — per-field edit/view handling
    const chipStyle: Record<string, string> = {
      display:'inline-block',
      padding:'3px 12px',
      borderRadius:'12px',
      fontSize:'13px',
      background:'rgba(255,255,255,0.15)',
      border:'1px solid rgba(255,255,255,0.2)',
      color:'#fff',
      margin:'2px 0',
    };

    const emoji: Record<string, string> = {
      location: '📍',
      birthday: '🎂',
      age: '🎂',
      occupation: '💼',
      relationship_status: '❤️',
      pronouns: '🏷️',
      interests: '⭐',
    };

    const inputStyle: Record<string, string> = {
      color:'#fff',
      background:'rgba(0,0,0,0.4)',
      width:'100%',
    };

    // --- location ---
    if (fkey === 'location') {
      const raw = editing && editForm ? ((editForm.location as string) || '') : (profile.location || '');
      if (canEdit) {
        return (
          <input
            className="profile-edit-input"
            value={raw}
            onChange={e => onEditFormChange?.({ ...editForm, location: e.target.value || null })}
            placeholder="Location"
            style={inputStyle}
          />
        );
      }
      if (!raw) return null;
      return <span className="profile-chip" style={chipStyle}>{emoji.location} {raw}</span>;
    }

    // --- website ---
    if (fkey === 'website') {
      const raw = editing && editForm ? ((editForm.website as string) || '') : (profile.website || '');
      if (canEdit) {
        return (
          <input
            className="profile-edit-input"
            value={raw}
            onChange={e => onEditFormChange?.({ ...editForm, website: e.target.value || null })}
            placeholder="Website"
            style={inputStyle}
          />
        );
      }
      if (!raw) return null;
      return (
        <a className="profile-chip profile-chip-link" href={raw} target="_blank" rel="noopener noreferrer" style={{ ...chipStyle, textDecoration:'none' }}>
          🔗 {raw.replace(/^https?:\/\//, '')}
        </a>
      );
    }

    // --- pronouns ---
    if (fkey === 'pronouns') {
      const raw = editing && editForm ? ((editForm.pronouns as string) || '') : (profile.pronouns || '');
      if (canEdit) {
        return (
          <input
            className="profile-edit-input"
            value={raw}
            onChange={e => onEditFormChange?.({ ...editForm, pronouns: e.target.value || null })}
            placeholder="Pronouns"
            style={inputStyle}
          />
        );
      }
      if (!raw) return null;
      return <span className="profile-chip" style={chipStyle}>{emoji.pronouns} {raw}</span>;
    }

    // --- birthday ---
    if (fkey === 'birthday') {
      const rawBd = editing && editForm ? ((editForm.birthday as string) || '') : (profile.birthday || '');
      if (canEdit) {
        return (
          <input
            className="profile-edit-input"
            type="date"
            value={rawBd}
            onChange={e => onEditFormChange?.({ ...editForm, birthday: e.target.value || null })}
            style={inputStyle}
          />
        );
      }
      const formatted = rawBd ? fmtBirthday(rawBd) : null;
      if (!formatted) return null;
      return <span className="profile-chip" style={chipStyle}>{emoji.birthday} {formatted}</span>;
    }

    // --- age (always static, never editable) ---
    if (fkey === 'age') {
      const bd = editing && editForm ? ((editForm.birthday as string) || '') : (profile.birthday || '');
      const a = ageFromBirthday(bd);
      if (a === null) return null;
      return <span className="profile-chip" style={chipStyle}>{emoji.age} {a} years old</span>;
    }

    // --- occupation ---
    if (fkey === 'occupation') {
      const raw = editing && editForm ? ((editForm.occupation as string) || '') : (profile.occupation || '');
      if (canEdit) {
        return (
          <input
            className="profile-edit-input"
            value={raw}
            onChange={e => onEditFormChange?.({ ...editForm, occupation: e.target.value || null })}
            placeholder="Occupation"
            style={inputStyle}
          />
        );
      }
      if (!raw) return null;
      return <span className="profile-chip" style={chipStyle}>{emoji.occupation} {raw}</span>;
    }

    // --- relationship_status ---
    if (fkey === 'relationship_status') {
      const raw = editing && editForm ? ((editForm.relationship_status as string) || '') : (profile.relationship_status || '');
      if (canEdit) {
        return (
          <select
            className="profile-edit-input"
            value={raw}
            onChange={e => onEditFormChange?.({ ...editForm, relationship_status: e.target.value || null })}
            style={inputStyle}
          >
            <option value="">-- Select --</option>
            {RELATIONSHIP_OPTIONS.map(opt => (
              <option key={opt} value={opt}>{opt}</option>
            ))}
          </select>
        );
      }
      if (!raw) return null;
      return <span className="profile-chip" style={chipStyle}>{emoji.relationship_status} {raw}</span>;
    }

    // --- interests ---
    if (fkey === 'interests') {
      const interestsArr: string[] = editing && editForm ? ((editForm.interests as string[]) || []) : (profile.interests || []);
      if (canEdit) {
        return (
          <div>
            <div style={{ display:'flex', flexWrap:'wrap', gap:'4px', marginBottom:'4px' }}>
              {interestsArr.map((interest, idx) => (
                <span key={idx} className="profile-interest-tag" style={{ display:'inline-flex', alignItems:'center', gap:'4px', padding:'2px 8px', borderRadius:'12px', background:'rgba(255,255,255,0.15)', fontSize:'12px', color:'#fff' }}>
                  {interest}
                  <button
                    type="button"
                    style={{ background:'none', border:'none', color:'#ff6b6b', cursor:'pointer', fontSize:'14px', padding:0, lineHeight:1 }}
                    onClick={() => {
                      const updated = interestsArr.filter((_, i) => i !== idx);
                      onEditFormChange?.({ ...editForm, interests: updated });
                    }}
                  >
                    ×
                  </button>
                </span>
              ))}
            </div>
            <input
              className="profile-edit-input"
              placeholder="Type and press Enter to add"
              style={inputStyle}
              onKeyDown={e => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  const val = (e.target as HTMLInputElement).value.trim();
                  if (val && !interestsArr.includes(val)) {
                    onEditFormChange?.({ ...editForm, interests: [...interestsArr, val] });
                  }
                  (e.target as HTMLInputElement).value = '';
                }
              }}
            />
          </div>
        );
      }
      if (interestsArr.length === 0) return null;
      return (
        <div style={{ display:'flex', flexWrap:'wrap', gap:'4px' }}>
          {interestsArr.map((interest, idx) => (
            <span key={idx} className="profile-chip" style={chipStyle}>
              {emoji.interests} {interest}
            </span>
          ))}
        </div>
      );
    }

    return null;
  }

  // --- Event listener for config button ---
  useEffect(() => {
    const handler = (e: Event) => {
      const evt = e as CustomEvent;
      if (evt.detail?.widgetId === widget.id) {
        if (evt.detail?.btnRect) {
          const r = evt.detail.btnRect as DOMRect;
          setPanelPos({
            top: r.bottom + window.scrollY + 4,
            left: r.left + window.scrollX,
          });
        }
        setShowConfig(prev => !prev);
      }
    };
    window.addEventListener('profile:toggle-config', handler);
    return () => window.removeEventListener('profile:toggle-config', handler);
  }, [widget.id]);

  // === RENDER ===
  return (
    <div className="widget-profile-header" style={{ position:'relative', width:'100%', height:'100%', display:'flex', flexDirection:'column' }}>
      {/* Banner image */}
      <div
        className="profile-header-banner"
        style={{
          position:'absolute', inset:0, zIndex:0,
          backgroundImage: `url(${headerUrl})`,
          backgroundSize:'cover',
          backgroundPosition:'center',
          backgroundColor:'var(--bg-tertiary)',
        }}
      >
        {canEdit && (
          <div
            className="profile-banner-edit"
            onClick={() => headerInputRef.current?.click()}
            style={{
              position:'absolute', inset:0,
              display:'flex', alignItems:'center', justifyContent:'center',
              background:'rgba(0,0,0,0.5)', color:'#fff',
              fontFamily:'var(--font-mono)', fontSize:'13px',
              cursor:'pointer', opacity:0, transition:'opacity 0.15s',
            }}
            onMouseOver={e => (e.currentTarget.style.opacity = '1')}
            onMouseOut={e => (e.currentTarget.style.opacity = '0')}
          >
            Change header
          </div>
        )}
      </div>
      <input ref={headerInputRef} type="file" accept="image/*" style={{ display:'none' }} onChange={handleHeaderUpload} />

      {/* Content overlaid on bottom of banner */}
      <div
        className="profile-header-content"
        style={{
          position:'relative', zIndex:1, marginTop:'auto',
          padding:'60px 16px 16px',
          background:'linear-gradient(to top, rgba(0,0,0,0.85) 0%, rgba(0,0,0,0.4) 60%, transparent 100%)',
          color:'#fff',
        }}
      >
        <div style={{ display:'flex', gap:'20px', alignItems:'flex-start' }}>
          {/* Avatar */}
          <div
            className="profile-avatar"
            onClick={() => { if (canEdit) avatarInputRef.current?.click(); }}
            style={{ cursor:canEdit ? 'pointer':'default', width:'120px', height:'120px', flexShrink:0 }}
          >
            {avatarUrl ? (
              <img src={avatarUrl} alt="" className="profile-avatar-img" />
            ) : (
              <span className="profile-avatar-fallback">{profile.username[0]?.toUpperCase() || '?'}</span>
            )}
            {canEdit && <div className="profile-avatar-edit-overlay">Edit</div>}
          </div>
          <input ref={avatarInputRef} type="file" accept="image/*" style={{ display:'none' }} onChange={handleAvatarUpload} />

          {/* Info + fields */}
          <div style={{ flex:1, minWidth:0, display:'flex', flexDirection:'column', gap:'6px' }}>
            {/* Display name */}
            {canEdit ? (
              <input
                className="profile-edit-input profile-name-input"
                value={displayName}
                onChange={e => onEditFormChange?.({ ...editForm, display_name: e.target.value })}
                placeholder="Display name"
                style={{ color:'#fff', background:'rgba(0,0,0,0.4)' }}
              />
            ) : (
              <h1 className="profile-display-name" style={{ margin:0, color:'#fff', textShadow:'0 1px 3px rgba(0,0,0,0.8)' }}>
                {displayName}
              </h1>
            )}
            <div className="profile-handle" style={{ color:'rgba(255,255,255,0.7)', marginBottom:'4px' }}>
              @{profile.username}
            </div>

            {/* Visible fields rendered in DEFAULT order, then custom fields */}
            {sortedVisible.map(f => (
              <div key={f.field_key} style={{ margin:'1px 0' }}>
                {renderFieldContent(f)}
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Floating config panel via portal */}
      {showConfig && createPortal(
        <div style={{ position:'fixed', inset:0, zIndex:9998 }} onClick={() => setShowConfig(false)}>
          <div
            onClick={e => e.stopPropagation()}
            style={{
              position:'absolute',
              top: panelPos.top,
              left: panelPos.left,
              zIndex: 9999,
              background: 'rgba(0,0,0,0.9)',
              border: '1px solid rgba(255,255,255,0.3)',
              borderRadius: '10px',
              padding: '16px',
              minWidth: '220px',
              maxHeight: '400px',
              overflowY: 'auto',
              backdropFilter: 'blur(10px)',
              boxShadow: '0 8px 32px rgba(0,0,0,0.6)',
              fontFamily: 'var(--font-mono)',
              fontSize: '13px',
              color: '#fff',
            }}
          >
            <div style={{ display:'flex', justifyContent:'space-between', alignItems:'center', marginBottom:'10px' }}>
              <span style={{ fontWeight:'bold', fontSize:'14px' }}>Configure Sections</span>
              <button onClick={() => setShowConfig(false)} style={{ background:'none', border:'none', color:'#fff', fontSize:'18px', cursor:'pointer' }}>×</button>
            </div>

            {/* Built-in field checkboxes */}
            {activeFields.filter(f => !f.field_key.startsWith('custom_')).map(f => (
              <label key={f.field_key} style={{ display:'flex', alignItems:'center', gap:'8px', marginBottom:'6px', cursor:'pointer' }}>
                <input
                  type="checkbox"
                  checked={f.visible}
                  onChange={() => toggleField(f.field_key)}
                />
                {FIELD_LABELS[f.field_key] || f.field_key}
              </label>
            ))}

            {/* Custom field checkboxes */}
            {activeFields.some(f => f.field_key.startsWith('custom_')) && (
              <>
                <div style={{ fontSize:'11px', textTransform:'uppercase', margin:'10px 0 6px', color:'rgba(255,255,255,0.5)' }}>Custom</div>
                {activeFields.filter(f => f.field_key.startsWith('custom_')).map(f => (
                  <div key={f.field_key} style={{ display:'flex', alignItems:'center', gap:'8px', marginBottom:'6px' }}>
                    <label style={{ display:'flex', alignItems:'center', gap:'6px', flex:1, cursor:'pointer' }}>
                      <input type="checkbox" checked={f.visible} onChange={() => toggleField(f.field_key)} />
                      <span>{f.label || 'Untitled'}</span>
                    </label>
                    <button
                      onClick={() => removeCustom(f.field_key)}
                      style={{
                        background:'none', border:'1px solid rgba(255,255,255,0.3)', color:'#ff6b6b',
                        borderRadius:'6px', cursor:'pointer', padding:'2px 8px', fontSize:'11px',
                      }}
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </>
            )}

            {/* Add custom section button */}
            <button
              onClick={addCustom}
              style={{
                marginTop:'10px', background:'rgba(255,255,255,0.1)', border:'1px dashed rgba(255,255,255,0.3)',
                color:'#fff', borderRadius:'8px', padding:'6px 12px', cursor:'pointer', width:'100%', fontSize:'12px',
              }}
            >
              + Add Custom Section
            </button>
          </div>
        </div>,
        document.body
      )}
    </div>
  );
}
