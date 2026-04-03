/**
 * TabBar
 *
 * Workspace tab strip for lens navigation state.
 * Includes account switcher avatar on the right side.
 *
 * Cross-system note:
 * - This component renders store-driven tab metadata and does not call backend commands.
 * - Tab identity/state can still reflect engine-backed screens opened by higher-level flows.
 * - Account switcher fetches connected sources from the backend.
 */
import { useNavStore } from '../store';
import { BsX } from 'react-icons/bs';
import { AccountSwitcher } from './AccountSwitcher';
import { SidebarItem } from '../types';

// ============================================================
// Tab Strip Component
// ============================================================

interface TabBarProps {
    avatarInitial?: string;
    handle?: string | null;
    did?: string | null;
    onNavigate?: (item: SidebarItem) => void;
}

/**
 * Exported tab strip component.
 *
 * Chrome-style tab bar — sits ABOVE the address bar / TopBar.
 * Account switcher avatar positioned on the right side of the strip.
 *
 * Always shows the current active tab. Additional tabs appear when
 * the user opens profiles or secondary screens. Closing the last tab
 * shows the "no tabs open" empty state.
 */
export function TabBar({ avatarInitial, handle, did, onNavigate }: TabBarProps) {
    const { tabs, activeTabId, switchTab, closeTab } = useNavStore();

    // Always render the tab bar (even with 0 tabs for the visual bar)
    return (
        <div className="tab-bar">
            <div className="tab-bar-tabs">
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

            {/* Account switcher — right side of tab strip */}
            {onNavigate && (
                <AccountSwitcher
                    avatarInitial={avatarInitial}
                    handle={handle}
                    did={did}
                    onNavigate={onNavigate}
                />
            )}
        </div>
    );
}
