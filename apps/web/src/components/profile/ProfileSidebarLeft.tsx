import type { FriendView } from '../../utils/api';

interface Props {
  friends: FriendView[];
  onViewProfile?: (username: string) => void;
}

export default function ProfileSidebarLeft({ friends, onViewProfile }: Props) {
  return (
    <div className="profile-sidebar-panel">
      <div className="profile-sidebar-title">Friends</div>
      <div className="profile-sidebar-subtitle">{friends.length} mutual</div>
      <div className="profile-friends-list">
        {friends.length === 0 && (
          <div className="profile-sidebar-empty">No mutual friends yet</div>
        )}
        {friends.map(f => (
          <div
            key={f.handle}
            className="profile-friend-item"
            onClick={() => onViewProfile?.(f.username)}
            role="button"
            tabIndex={0}
          >
            {f.avatar ? (
              <img src={f.avatar} alt="" className="profile-friend-avatar" />
            ) : (
              <div className="profile-friend-avatar-fallback">{f.username[0]?.toUpperCase() || '?'}</div>
            )}
            <div className="profile-friend-info">
              <div className="profile-friend-name">{f.display_name || f.username}</div>
              <div className="profile-friend-handle">@{f.username}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
