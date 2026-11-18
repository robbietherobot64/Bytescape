import { useRef, type ChangeEvent } from 'react';
import type { ProfileData, Widget } from '../../../utils/api';

interface Props {
  widget: Widget;
  profile: ProfileData;
  editing: boolean;
  editForm?: Record<string, unknown>;
  onEditFormChange?: (form: Record<string, unknown>) => void;
}

export default function WidgetAvatarName({ widget, profile, editing, editForm, onEditFormChange }: Props) {
  const avatarInputRef = useRef<HTMLInputElement>(null);

  const displayName = editing && editForm ? (editForm.display_name as string || '') : profile.display_name;
  const bio = editing && editForm ? (editForm.bio as string || '') : profile.bio;
  const avatarUrl = editing && editForm ? (editForm.avatar_url as string || profile.avatar_url || '') : profile.avatar_url || '';

  const handleAvatarUpload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file || !onEditFormChange) return;
    try {
      const { profileApi } = await import('../../../utils/api');
      const data = await profileApi.uploadAvatar(file);
      onEditFormChange({ ...editForm, avatar_url: data.avatar_url });
    } catch {}
  };

  return (
    <div className="widget-avatar-name">
      <div className="profile-header-main">
        <div
          className="profile-avatar"
          onClick={() => { if (profile.is_own_profile && editing) avatarInputRef.current?.click(); }}
          style={{ cursor: profile.is_own_profile && editing ? 'pointer' : 'default' }}
        >
          {avatarUrl ? (
            <img src={avatarUrl} alt="" className="profile-avatar-img" />
          ) : (
            <span className="profile-avatar-fallback">{profile.username[0]?.toUpperCase() || '?'}</span>
          )}
          {profile.is_own_profile && editing && <div className="profile-avatar-edit-overlay">Edit</div>}
        </div>
        <input ref={avatarInputRef} type="file" accept="image/*" style={{ display: 'none' }} onChange={handleAvatarUpload} />

        <div className="profile-header-info">
          {editing && editForm ? (
            <input
              className="profile-edit-input profile-name-input"
              value={editForm.display_name as string || ''}
              onChange={e => onEditFormChange?.({ ...editForm, display_name: e.target.value })}
              placeholder="Display name"
            />
          ) : (
            <h1 className="profile-display-name">{displayName}</h1>
          )}
          <div className="profile-handle">@{profile.username}</div>

          {editing && editForm ? (
            <textarea
              className="profile-edit-textarea profile-bio-input"
              value={editForm.bio as string || ''}
              onChange={e => onEditFormChange?.({ ...editForm, bio: e.target.value })}
              placeholder="Bio"
              rows={3}
            />
          ) : (
            <div className="profile-bio">{bio}</div>
          )}
        </div>
      </div>
    </div>
  );
}
