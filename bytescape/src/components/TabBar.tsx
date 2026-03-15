import { useNavStore } from '../store';
import { BsX } from 'react-icons/bs';

/** Browser-style tab bar — renders open tabs between TopBar and content area. */
export function TabBar() {
    const { tabs, activeTabId, switchTab, closeTab } = useNavStore();

    // Hide tab bar when there are fewer than 2 tabs
    if (tabs.length < 2) return null;

    return (
        <div className="tab-bar">
            {tabs.map((tab) => (
                <button
                    key={tab.id}
                    className={`tab-bar-item ${tab.id === activeTabId ? 'active' : ''}`}
                    onClick={() => switchTab(tab.id)}
                    title={tab.label}
                >
                    <span className="tab-bar-label">{tab.label}</span>
                    <span
                        className="tab-bar-close"
                        onClick={(e) => {
                            e.stopPropagation();
                            closeTab(tab.id);
                        }}
                        role="button"
                        aria-label={`Close ${tab.label}`}
                    >
                        <BsX size={14} />
                    </span>
                </button>
            ))}
        </div>
    );
}
