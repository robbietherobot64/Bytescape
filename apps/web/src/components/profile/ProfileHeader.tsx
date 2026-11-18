import { useRef, type ChangeEvent } from 'react';
import type { ProfileData, UpdateProfileRequest } from '../../utils/api';

interface Props {
  profile: ProfileData;
  editing: boolean;
  editForm: UpdateProfileRequest;
  onEditFormChange: (form: UpdateProfileRequest) => void;
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

export default function ProfileHeader({ profile, editing, editForm, onEditFormChange }: Props) {
  const avatarInputRef = useRef<HTMLInputElement>(null);
  const headerInputRef = useRef<HTMLInputElement>(null);

  const displayName = editing ? (editForm.display_name || '') : profile.display_name;
  const bio = editing ? (editForm.bio || '') : profile.bio;
  const avatarUrl = editing ? (editForm.avatar_url || profile.avatar_url) : profile.avatar_url;
  const headerUrl = editing ? (editForm.header_url || profile.header_url) : profile.header_url;

  const handleAvatarUpload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const { profileApi } = await import('../../utils/api');
      const data = await profileApi.uploadAvatar(file);
      onEditFormChange({ ...editForm, avatar_url: data.avatar_url });
    } catch {}
  };

  const handleHeaderUpload = async (e: ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const { profileApi } = await import('../../utils/api');
      const data = await profileApi.uploadHeader(file);
      onEditFormChange({ ...editForm, header_url: data.header_url });
    } catch {}
  };

  const age = ageFromBirthday(profile.birthday);

  return (
    <div className="profile-header">
      <div
        className="profile-banner"
        style={headerUrl ? { backgroundImage: `url(${headerUrl})` } : undefined}
      >
        {profile.is_own_profile && editing && (
          <div className="profile-banner-edit" onClick={() => headerInputRef.current?.click()}>
            Change header
          </div>
        )}

        <div className="profile-banner-content">
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
              {editing ? (
                <input
                  className="profile-edit-input profile-name-input"
                  value={editForm.display_name || ''}
                  onChange={e => onEditFormChange({ ...editForm, display_name: e.target.value })}
                  placeholder="Display name"
                />
              ) : (
                <h1 className="profile-display-name">{displayName}</h1>
              )}
              <div className="profile-handle">@{profile.username}</div>

              {editing ? (
                <textarea
                  className="profile-edit-textarea profile-bio-input"
                  value={editForm.bio || ''}
                  onChange={e => onEditFormChange({ ...editForm, bio: e.target.value })}
                  placeholder="Bio"
                  rows={3}
                />
              ) : (
                <div className="profile-bio">{bio}</div>
              )}

              <div className="profile-details">
                {profile.location && !editing && (
                  <span className="profile-chip">📍 {profile.location}</span>
                )}
                {age !== null && !editing && (
                  <span className="profile-chip">🎂 {age} years old</span>
                )}
                {profile.occupation && !editing && (
                  <span className="profile-chip">💼 {profile.occupation}</span>
                )}
                {profile.relationship_status && !editing && (
                  <span className="profile-chip">❤️ {profile.relationship_status}</span>
                )}
                {profile.pronouns && !editing && (
                  <span className="profile-chip">🏷️ {profile.pronouns}</span>
                )}
                {profile.website && !editing && (
                  <a className="profile-chip profile-chip-link" href={profile.website} target="_blank" rel="noopener noreferrer">🔗 {profile.website.replace(/^https?:\/\//, '')}</a>
                )}
                {profile.interests.length > 0 && !editing && (
                  <span className="profile-chip">⭐ {profile.interests.join(', ')}</span>
                )}
              </div>
            </div>
          </div>

          <div className="profile-stats">
            <span className="profile-stat"><strong>{profile.posts_count}</strong> posts</span>
            <span className="profile-stat"><strong>{profile.followers_count}</strong> followers</span>
            <span className="profile-stat"><strong>{profile.following_count}</strong> following</span>
          </div>
        </div>
      </div>
      <input ref={headerInputRef} type="file" accept="image/*" style={{ display: 'none' }} onChange={handleHeaderUpload} />
    </div>
  );
}
