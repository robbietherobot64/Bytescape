interface Props {
  activeTab: 'wall' | 'posts' | 'edit';
  onChange: (tab: 'wall' | 'posts' | 'edit') => void;
  isOwn: boolean;
}

export default function ProfileTabs({ activeTab, onChange, isOwn }: Props) {
  const tabs: { id: 'wall' | 'posts' | 'edit'; label: string }[] = [
    { id: 'wall', label: 'Wall' },
    { id: 'posts', label: 'Posts' },
  ];
  if (isOwn) {
    tabs.push({ id: 'edit', label: 'Edit' });
  }

  return (
    <div className="profile-tabs">
      {tabs.map(tab => (
        <button
          key={tab.id}
          className={`profile-tab ${activeTab === tab.id ? 'active' : ''}`}
          onClick={() => onChange(tab.id)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}
