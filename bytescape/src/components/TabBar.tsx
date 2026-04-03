/**
 * TabBar
 *
 * Workspace tab strip for lens navigation state.
 *
 * Cross-system note:
 * - This component renders store-driven tab metadata and does not call backend commands.
 * - Tab identity/state can still reflect engine-backed screens opened by higher-level flows.
 */
import { useNavStore } from '../store';
import { BsX } from 'react-icons/bs';

// ============================================================
// Tab Strip Component
// ============================================================

/**
 * Exported tab strip component.
 *
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
                // Entire tab surface activates/switches the tab.
                <button
                    key={tab.id}
                    className={`tab-bar-item ${tab.id === activeTabId ? 'active' : ''}`}
                    onClick={() => switchTab(tab.id)}
                    title={tab.label}
                >
                    <span className="tab-bar-label">{tab.label}</span>
                    {/* Close affordance uses event stopPropagation to avoid tab activation conflicts. */}
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
