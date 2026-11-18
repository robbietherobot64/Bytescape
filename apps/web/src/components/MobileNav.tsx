import type { TabName } from './tabs';

interface Props {
  activeTab: TabName;
  onSelectTab: (tab: TabName) => void;
  onOpenMore: () => void;
}

export default function MobileNav({ activeTab, onSelectTab, onOpenMore }: Props) {
  const openComposer = () => {
    window.dispatchEvent(new CustomEvent('bytescape:openComposer'));
  };

  return (
    <div className="mobile-bottom-nav">
      <button
        className={`mobile-nav-item ${activeTab === 'Feed' ? 'active' : ''}`}
        onClick={() => onSelectTab('Feed')}
      >
        <span className="mobile-nav-icon">📰</span>
        <span className="mobile-nav-label">Feed</span>
      </button>
      <button
        className={`mobile-nav-item ${activeTab === 'Profile' ? 'active' : ''}`}
        onClick={() => onSelectTab('Profile')}
      >
        <span className="mobile-nav-icon">👤</span>
        <span className="mobile-nav-label">Profile</span>
      </button>
      <button
        className="mobile-nav-item mobile-nav-composer"
        onClick={openComposer}
      >
        <span className="mobile-nav-icon">+</span>
      </button>
      <button
        className={`mobile-nav-item ${activeTab === 'Chat' ? 'active' : ''}`}
        onClick={() => onSelectTab('Chat')}
      >
        <span className="mobile-nav-icon">💬</span>
        <span className="mobile-nav-label">Chat</span>
      </button>
      <button className="mobile-nav-item" onClick={onOpenMore}>
        <span className="mobile-nav-icon">☰</span>
        <span className="mobile-nav-label">More</span>
      </button>
    </div>
  );
}
