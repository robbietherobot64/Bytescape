import { Screen } from '../store';
import { FeedTab, FEED_TAB_LABELS } from '../hooks/useUnifiedFeed';
import {
    BsHouseFill, BsChatDotsFill, BsFolderFill,
    BsPeopleFill, BsBoxArrowInDown, BsGearFill,
    BsPersonFill
} from 'react-icons/bs';

interface ContextPanelProps {
    screen: Screen;
    /** Feed tab state — only relevant when screen === 'feed' */
    feedActiveTab?: FeedTab;
    setFeedActiveTab?: (tab: FeedTab) => void;
    /** Feed lens state */
    activeLens?: string;
    setActiveLens?: (lens: string) => void;
    /** Context content rendered by the active screen (e.g. FeedRightPanel) */
    contextContent?: React.ReactNode;
}

/** Screen-label mapping for the context panel header */
const SCREEN_LABELS: Partial<Record<Screen, { icon: React.ReactNode; label: string }>> = {
    feed: { icon: <BsHouseFill />, label: 'Feed' },
    chat: { icon: <BsChatDotsFill />, label: 'Chat' },
    vault: { icon: <BsFolderFill />, label: 'Vault' },
    friends: { icon: <BsPeopleFill />, label: 'Social' },
    import: { icon: <BsBoxArrowInDown />, label: 'Import' },
    settings: { icon: <BsGearFill />, label: 'Settings' },
    profile: { icon: <BsPersonFill />, label: 'Profile' },
};

export function ContextPanel({ screen, feedActiveTab, setFeedActiveTab, activeLens, setActiveLens, contextContent }: ContextPanelProps) {
    const screenInfo = SCREEN_LABELS[screen];

    return (
        <div className="context-panel">
            {/* Panel Header */}
            <div className="context-panel-header">
                {screenInfo && (
                    <>
                        <span className="context-panel-icon">{screenInfo.icon}</span>
                        <span className="context-panel-title">{screenInfo.label}</span>
                    </>
                )}
            </div>

            {/* Feed-specific: Tab navigation + Lens picker */}
            {screen === 'feed' && feedActiveTab && setFeedActiveTab && (
                <div className="context-panel-section">
                    <div className="context-panel-section-label">View</div>
                    <div className="context-panel-nav">
                        {(['all', 'following', 'subscribed', 'you'] as FeedTab[]).map(tab => (
                            <button
                                key={tab}
                                className={`context-nav-item ${feedActiveTab === tab ? 'active' : ''}`}
                                onClick={() => setFeedActiveTab(tab)}
                            >
                                {FEED_TAB_LABELS[tab]}
                            </button>
                        ))}
                    </div>

                    {/* Lens toggle */}
                    {activeLens !== undefined && setActiveLens && (
                        <div className="context-panel-lenses">
                            <div className="context-panel-section-label">Lens</div>
                            <div className="context-lens-row">
                                <button
                                    className={`context-lens-btn ${activeLens === 'list' ? 'active' : ''}`}
                                    onClick={() => setActiveLens('list')}
                                    title="List View"
                                >
                                    ☰ List
                                </button>
                                <button
                                    className={`context-lens-btn ${activeLens === 'gallery' ? 'active' : ''}`}
                                    onClick={() => setActiveLens('gallery')}
                                    title="Gallery View"
                                >
                                    ▦ Grid
                                </button>
                                <button
                                    className={`context-lens-btn ${activeLens === 'reader' ? 'active' : ''}`}
                                    onClick={() => setActiveLens('reader')}
                                    title="Reader View"
                                >
                                    📖 Read
                                </button>
                            </div>
                        </div>
                    )}
                </div>
            )}

            {/* Screen-specific context content */}
            {contextContent && (
                <div className="context-panel-content">
                    {contextContent}
                </div>
            )}
        </div>
    );
}
