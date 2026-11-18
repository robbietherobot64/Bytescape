import { useState, useEffect } from 'react';
import type { FollowingView } from '../../../utils/api';
import { profileApi } from '../../../utils/api';
import ProfileSidebarRight from '../ProfileSidebarRight';

interface Props {
  username: string;
  onViewProfile?: (username: string) => void;
}

export default function WidgetFollowing({ username, onViewProfile }: Props) {
  const [following, setFollowing] = useState<FollowingView[]>([]);

  useEffect(() => {
    profileApi
      .following(username)
      .then((r) => setFollowing(r.rows))
      .catch(() => setFollowing([]));
  }, [username]);

  return (
    <ProfileSidebarRight following={following} onViewProfile={onViewProfile} />
  );
}
