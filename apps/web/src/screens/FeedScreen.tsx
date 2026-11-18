import { useState, useEffect } from 'react';
import FeedLayout from '../components/FeedLayout';
import ComposerModal from '../components/ComposerModal';

interface Props {
  onNavigate?: (tab: string) => void;
  onLogout?: () => void;
  onInterests?: () => void;
  activeTab?: string;
  interestsRequest?: number;
}

export default function FeedScreen({ onNavigate, onLogout, onInterests, activeTab, interestsRequest }: Props) {
  const [composerOpen, setComposerOpen] = useState(false);

  useEffect(() => {
    const handler = () => setComposerOpen(true);
    window.addEventListener('bytescape:openComposer', handler);
    return () => window.removeEventListener('bytescape:openComposer', handler);
  }, []);

  const handleNewPost = () => {
    window.dispatchEvent(new CustomEvent('bytescape:refreshFeed'));
  };

  return (
    <>
      <FeedLayout
        onNavigate={onNavigate}
        onLogout={onLogout}
        onInterests={onInterests}
        activeTab={activeTab}
        interestsRequest={interestsRequest}
      />
      {composerOpen && (
        <ComposerModal
          onClose={() => setComposerOpen(false)}
          onPost={handleNewPost}
        />
      )}
    </>
  );
}
