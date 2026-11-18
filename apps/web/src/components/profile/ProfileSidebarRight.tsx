import type { FollowingView } from '../../utils/api';

interface Props {
  following: Pick<FollowingView, 'username' | 'handle' | 'display_name' | 'avatar' | 'is_local'>[];
  onViewProfile?: (username: string) => void;
}

export default function ProfileSidebarRight({ following, onViewProfile }: Props) {
  return (
    <div className="profile-sidebar-panel">
      <div className="profile-sidebar-title">Following</div>
      <div className="profile-sidebar-subtitle">{following.length} accounts</div>
      <div className="profile-following-list">
        {following.length === 0 && (
          <div className="profile-sidebar-empty">Not following anyone yet</div>
        )}
        {following.map(f => (
          <div
            key={f.handle}
            className="profile-following-item"
            onClick={() => onViewProfile?.(f.username)}
            role="button"
            tabIndex={0}
          >
            {f.avatar ? (
              <img src={f.avatar} alt="" className="profile-following-avatar" />
            ) : (
              <div className="profile-following-avatar-fallback">{f.username[0]?.toUpperCase() || '?'}</div>
            )}
            <div className="profile-following-info">
              <div className="profile-following-name">{f.display_name || f.username}</div>
              <div className="profile-following-handle">
                @{f.username}
                {!f.is_local && <span className="profile-following-external"> 🌐</span>}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
