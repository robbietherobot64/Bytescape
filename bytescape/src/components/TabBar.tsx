import { useNavStore } from '../store';
import { BsX } from 'react-icons/bs';

/**
 * Chrome-style tab bar — sits ABOVE the address bar / TopBar.
 *
 * Always shows the current active tab. Additional tabs appear when
 * the user opens profiles or secondary screens. Closing the last tab
 * shows the "no tabs open" empty state.
 */
export function TabBar() {
    const { tabs, activeTabId, switchTab, closeTab } = useNavStore();

    // Always render the tab bar (even with 0 tabs for the visual bar)
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
