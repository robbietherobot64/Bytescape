import ImportStep from '../components/ImportStep';
import type { TabName } from '../components/tabs';

interface Props {
  onNavigate: (tab: TabName) => void;
}

/**
 * Dedicated Import screen — a full-screen tab on the left nav. Renders the
 * import flow (equal source picker, per-source guide, upload, live parse
 * feedback, results) embedded rather than as a modal. "Skip" leaves the import
 * for later and returns to the Feed.
 */
export default function ImportScreen({ onNavigate }: Props) {
  const finish = (tab: TabName) => {
    // Completing or skipping the import finishes new-user onboarding.
    localStorage.setItem('onboarding_complete', 'true');
    onNavigate(tab);
  };
  return (
    <div className="import-screen" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column', overflow: 'hidden', background: 'var(--bg-primary)' }}>
      <ImportStep
        embedded
        onComplete={() => finish('Feed')}
        onSkip={() => finish('Feed')}
      />
    </div>
  );
}
