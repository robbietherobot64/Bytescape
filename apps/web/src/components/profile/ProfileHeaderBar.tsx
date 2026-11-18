import type { ProfileData } from '../../utils/api';

interface ProfileHeaderBarProps {
  profile: ProfileData;
  isOwnProfile: boolean;
  onEditProfile: () => void;
}

export default function ProfileHeaderBar({ profile, isOwnProfile, onEditProfile }: ProfileHeaderBarProps) {
  const initials = (profile.display_name || profile.username || '?')[0].toUpperCase();

  return (
    <div
      style={{
        background: 'var(--surface)',
        border: '1px solid var(--border-color)',
        borderRadius: 6,
        overflow: 'hidden',
        marginBottom: 16,
      }}
    >
      {/* Banner */}
      <div
        style={{
          height: 120,
          background: profile.header_url
            ? `url(${profile.header_url}) center/cover`
            : 'var(--surface-secondary)',
          position: 'relative',
        }}
      >
        {/* Avatar positioned overlapping banner bottom */}
        <div
          style={{
            position: 'absolute',
            bottom: -40,
            left: 20,
            width: 80,
            height: 80,
            borderRadius: '50%',
            border: '3px solid var(--bg)',
            background: 'var(--surface-secondary)',
            overflow: 'hidden',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          {profile.avatar_url ? (
            <img
              src={profile.avatar_url}
              alt={profile.display_name}
              style={{ width: '100%', height: '100%', objectFit: 'cover' }}
            />
          ) : (
            <span
              style={{
                fontFamily: 'var(--font-retro)',
                fontSize: 32,
                color: 'var(--accent)',
              }}
            >
              {initials}
            </span>
          )}
        </div>

        {/* Edit button */}
        {isOwnProfile && (
          <button
            data-tour="edit-profile"
            onClick={onEditProfile}
            style={{
              position: 'absolute',
              top: 10,
              right: 10,
              background: 'rgba(0,0,0,0.6)',
              border: '1px solid var(--border-color)',
              borderRadius: 4,
              color: 'var(--text-muted)',
              padding: '6px 10px',
              cursor: 'pointer',
              fontFamily: 'var(--font-mono)',
              fontSize: 12,
              display: 'flex',
              alignItems: 'center',
              gap: 4,
            }}
          >
            ✏️ Edit
          </button>
        )}
      </div>

      {/* Info */}
      <div style={{ padding: '48px 20px 16px' }}>
        <div style={{ fontFamily: 'var(--font-retro)', fontSize: 22, color: 'var(--text-primary)', lineHeight: 1.2 }}>
          {profile.display_name || profile.username}
        </div>
        <div style={{ fontFamily: 'var(--font-mono)', fontSize: 13, color: 'var(--text-muted)', marginTop: 2 }}>
          @{profile.username}
        </div>
        {profile.bio && (
          <p style={{ color: 'var(--text-secondary)', fontSize: 14, marginTop: 10, lineHeight: 1.5, margin: '10px 0 0' }}>
            {profile.bio}
          </p>
        )}

        {/* Stats */}
        <div
          style={{
            display: 'flex',
            gap: 20,
            marginTop: 14,
            paddingTop: 12,
            borderTop: '1px solid var(--border-color)',
            fontFamily: 'var(--font-mono)',
            fontSize: 13,
          }}
        >
          <span>
            <strong style={{ color: 'var(--text-primary)' }}>{profile.posts_count}</strong>{' '}
            <span style={{ color: 'var(--text-muted)' }}>posts</span>
          </span>
          <span>
            <strong style={{ color: 'var(--text-primary)' }}>{profile.followers_count}</strong>{' '}
            <span style={{ color: 'var(--text-muted)' }}>followers</span>
          </span>
          <span>
            <strong style={{ color: 'var(--text-primary)' }}>{profile.following_count}</strong>{' '}
            <span style={{ color: 'var(--text-muted)' }}>following</span>
          </span>
        </div>
      </div>
    </div>
  );
}
