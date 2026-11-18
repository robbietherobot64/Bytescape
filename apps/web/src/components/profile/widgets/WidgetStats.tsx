import type { ProfileData, Widget } from '../../../utils/api';

interface Props {
  widget: Widget;
  profile: ProfileData;
}

export default function WidgetStats({ widget, profile }: Props) {
  return (
    <div className="profile-stats">
      <span className="profile-stat"><strong>{profile.posts_count}</strong> posts</span>
      <span className="profile-stat"><strong>{profile.followers_count}</strong> followers</span>
      <span className="profile-stat"><strong>{profile.following_count}</strong> following</span>
    </div>
  );
}
