import { useState, useEffect } from 'react';
import type { FriendView } from '../../../utils/api';
import { profileApi } from '../../../utils/api';
import ProfileSidebarLeft from '../ProfileSidebarLeft';

interface Props {
  username: string;
  onViewProfile?: (username: string) => void;
}

export default function WidgetFriends({ username, onViewProfile }: Props) {
  const [friends, setFriends] = useState<FriendView[]>([]);

  useEffect(() => {
    profileApi.friends(username).then(setFriends).catch(() => setFriends([]));
  }, [username]);

  return (
    <ProfileSidebarLeft friends={friends} onViewProfile={onViewProfile} />
  );
}
