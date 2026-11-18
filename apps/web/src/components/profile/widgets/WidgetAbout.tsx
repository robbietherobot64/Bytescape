import type { ProfileData, Widget } from '../../../utils/api';

interface Props {
  widget: Widget;
  profile: ProfileData;
  editing: boolean;
  editForm?: Record<string, unknown>;
  onEditFormChange?: (form: Record<string, unknown>) => void;
}

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

export default function WidgetAbout({ widget, profile, editing, editForm, onEditFormChange }: Props) {
  const age = ageFromBirthday(profile.birthday);

  const fields: { key: string; icon: string; label: string; value: string | null }[] = [];
  if (profile.location) fields.push({ key: 'location', icon: '📍', label: 'Location', value: profile.location });
  if (profile.birthday) fields.push({ key: 'birthday', icon: '🎂', label: 'Birthday', value: fmtBirthday(profile.birthday) + (age !== null ? ` (${age})` : '') });
  if (profile.occupation) fields.push({ key: 'occupation', icon: '💼', label: 'Occupation', value: profile.occupation });
  if (profile.relationship_status) fields.push({ key: 'relationship_status', icon: '❤️', label: 'Status', value: profile.relationship_status });
  if (profile.pronouns) fields.push({ key: 'pronouns', icon: '🏷️', label: 'Pronouns', value: profile.pronouns });
  if (profile.website) fields.push({ key: 'website', icon: '🔗', label: 'Website', value: profile.website });
  if (profile.interests.length > 0) fields.push({ key: 'interests', icon: '⭐', label: 'Interests', value: profile.interests.join(', ') });

  if (editing && editForm) {
    return (
      <div className="profile-details" style={{ flexDirection: 'column', gap: '12px' }}>
        <div className="profile-edit-field">
          <label className="profile-edit-label">Location</label>
          <input className="profile-edit-input" value={editForm.location as string || ''} onChange={e => onEditFormChange?.({ ...editForm, location: e.target.value })} placeholder="Location" />
        </div>
        <div className="profile-edit-field">
          <label className="profile-edit-label">Birthday</label>
          <input className="profile-edit-input" type="date" value={editForm.birthday as string || ''} onChange={e => onEditFormChange?.({ ...editForm, birthday: e.target.value })} />
        </div>
        <div className="profile-edit-field">
          <label className="profile-edit-label">Occupation</label>
          <input className="profile-edit-input" value={editForm.occupation as string || ''} onChange={e => onEditFormChange?.({ ...editForm, occupation: e.target.value })} placeholder="Occupation" />
        </div>
        <div className="profile-edit-field">
          <label className="profile-edit-label">Relationship Status</label>
          <input className="profile-edit-input" value={editForm.relationship_status as string || ''} onChange={e => onEditFormChange?.({ ...editForm, relationship_status: e.target.value })} placeholder="Relationship status" />
        </div>
        <div className="profile-edit-field">
          <label className="profile-edit-label">Pronouns</label>
          <input className="profile-edit-input" value={editForm.pronouns as string || ''} onChange={e => onEditFormChange?.({ ...editForm, pronouns: e.target.value })} placeholder="Pronouns" />
        </div>
        <div className="profile-edit-field">
          <label className="profile-edit-label">Website</label>
          <input className="profile-edit-input" value={editForm.website as string || ''} onChange={e => onEditFormChange?.({ ...editForm, website: e.target.value })} placeholder="https://" />
        </div>
        <div className="profile-edit-field">
          <label className="profile-edit-label">Interests (comma separated)</label>
          <input className="profile-edit-input" value={(editForm.interests as string[])?.join(', ') || ''} onChange={e => onEditFormChange?.({ ...editForm, interests: e.target.value.split(',').map(s => s.trim()).filter(Boolean) })} placeholder="interests, separated, by commas" />
        </div>
      </div>
    );
  }

  if (fields.length === 0) {
    return <div className="profile-details"><span className="profile-chip">No details added yet</span></div>;
  }

  return (
    <div className="profile-details">
      {fields.map(f => {
        if (f.key === 'website') {
          return (
            <a key={f.key} className="profile-chip profile-chip-link" href={f.value!} target="_blank" rel="noopener noreferrer">
              {f.icon} {f.value!.replace(/^https?:\/\//, '')}
            </a>
          );
        }
        return <span key={f.key} className="profile-chip">{f.icon} {f.value}</span>;
      })}
    </div>
  );
}
