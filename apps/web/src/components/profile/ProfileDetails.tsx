import type { ProfileData, UpdateProfileRequest } from '../../utils/api';

interface Props {
  profile: ProfileData;
  editing: boolean;
  editForm: UpdateProfileRequest;
  onEditFormChange: (form: UpdateProfileRequest) => void;
}

const RELATIONSHIP_OPTIONS = [
  'Single',
  'Married',
  'Dating',
  'Engaged',
  "It's Complicated",
  'Prefer not to say',
];

export default function ProfileDetails({ profile, editing, editForm, onEditFormChange }: Props) {
  if (!editing) return null;

  const interests = editForm.interests ?? profile.interests ?? [];

  const addInterest = (value: string) => {
    const trimmed = value.trim();
    if (!trimmed) return;
    if (interests.includes(trimmed)) return;
    onEditFormChange({ ...editForm, interests: [...interests, trimmed] });
  };

  const removeInterest = (idx: number) => {
    onEditFormChange({ ...editForm, interests: interests.filter((_, i) => i !== idx) });
  };

  return (
    <div className="profile-details-edit">
      <div className="profile-edit-field">
        <label className="profile-edit-label">Location</label>
        <input
          className="profile-edit-input"
          value={editForm.location ?? profile.location ?? ''}
          onChange={e => onEditFormChange({ ...editForm, location: e.target.value || null })}
          placeholder="City, Country"
        />
      </div>

      <div className="profile-edit-field">
        <label className="profile-edit-label">Website</label>
        <input
          className="profile-edit-input"
          value={editForm.website ?? profile.website ?? ''}
          onChange={e => onEditFormChange({ ...editForm, website: e.target.value || null })}
          placeholder="https://..."
        />
      </div>

      <div className="profile-edit-field">
        <label className="profile-edit-label">Pronouns</label>
        <input
          className="profile-edit-input"
          value={editForm.pronouns ?? profile.pronouns ?? ''}
          onChange={e => onEditFormChange({ ...editForm, pronouns: e.target.value || null })}
          placeholder="they/them"
        />
      </div>

      <div className="profile-edit-field">
        <label className="profile-edit-label">Relationship Status</label>
        <select
          className="profile-edit-input"
          value={editForm.relationship_status ?? profile.relationship_status ?? ''}
          onChange={e => onEditFormChange({ ...editForm, relationship_status: e.target.value || null })}
        >
          <option value="">-- Select --</option>
          {RELATIONSHIP_OPTIONS.map(opt => (
            <option key={opt} value={opt}>{opt}</option>
          ))}
        </select>
      </div>

      <div className="profile-edit-field">
        <label className="profile-edit-label">Birthday</label>
        <input
          className="profile-edit-input"
          type="date"
          value={editForm.birthday ?? profile.birthday ?? ''}
          onChange={e => onEditFormChange({ ...editForm, birthday: e.target.value || null })}
        />
      </div>

      <div className="profile-edit-field">
        <label className="profile-edit-label">Occupation</label>
        <input
          className="profile-edit-input"
          value={editForm.occupation ?? profile.occupation ?? ''}
          onChange={e => onEditFormChange({ ...editForm, occupation: e.target.value || null })}
          placeholder="What do you do?"
        />
      </div>

      <div className="profile-edit-field">
        <label className="profile-edit-label">Interests</label>
        <div className="profile-interests-tags">
          {interests.map((interest, idx) => (
            <span key={idx} className="profile-interest-tag">
              {interest}
              <button type="button" className="profile-interest-remove" onClick={() => removeInterest(idx)}>×</button>
            </span>
          ))}
        </div>
        <input
          className="profile-edit-input"
          placeholder="Type and press Enter to add"
          onKeyDown={e => {
            if (e.key === 'Enter') {
              e.preventDefault();
              addInterest((e.target as HTMLInputElement).value);
              (e.target as HTMLInputElement).value = '';
            }
          }}
        />
      </div>
    </div>
  );
}
