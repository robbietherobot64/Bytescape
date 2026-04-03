/**
 * AppLayout
 *
 * Root shell composition for lens-side navigation chrome.
 *
 * Cross-system note:
 * - This component is UI-only and does not invoke engine commands directly.
 * - Effective content is still driven by engine-backed state (tabs/routes) via stores/providers.
 */
import React from 'react';
import { TopBar } from './TopBar';
import { TabBar } from './TabBar';
import { SidebarItem } from '../types';
import { useNavStore } from '../store';
import {
    BsHouseFill, BsChatDotsFill, BsPeopleFill,
    BsFolderFill, BsGearFill, BsBoxArrowInDown, BsListUl
} from 'react-icons/bs';

interface AppLayoutProps {
    /** Screen content rendered in the primary pane. */
    children: React.ReactNode;
    /** Sidebar item used for active navigation highlight state. */
    activeSidebar: SidebarItem;
    /** Called when a sidebar item is selected. */
    onNavigate: (item: SidebarItem) => void;
    /** Search bar state. */
    searchQuery: string;
    onSearchChange: (query: string) => void;
    onSearchSubmit: (query: string) => void;
    /** User avatar initial for top bar / tab bar. */
    avatarInitial?: string;
    /** User handle for account switcher. */
    handle?: string | null;
    /** User DID for account switcher. */
    did?: string | null;
}

// ============================================================
// Mobile Navigation Composition
// ============================================================

/** Bottom navigation items for mobile. */
const BOTTOM_NAV_ITEMS: { id: SidebarItem; icon: React.ReactNode; label: string }[] = [
    { id: 'feed', icon: <BsHouseFill />, label: 'Feed' },
    { id: 'forums', icon: <BsListUl />, label: 'Forums' },
    { id: 'chat', icon: <BsChatDotsFill />, label: 'Chat' },
    { id: 'social', icon: <BsPeopleFill />, label: 'Social' },
    { id: 'import', icon: <BsBoxArrowInDown />, label: 'Import' },
    { id: 'vault', icon: <BsFolderFill />, label: 'Vault' },
    { id: 'settings', icon: <BsGearFill />, label: 'Settings' },
];

/**
 * Exported shell component that composes:
 * - `TabBar` for multi-tab workspace state
 * - `TopBar` for search + global actions
 * - Main content viewport
 * - Mobile bottom navigation
 */
/** Application shell: tab bar + top bar + main content + mobile bottom nav. */
export function AppLayout({
    children,
    activeSidebar,
    onNavigate,
    searchQuery,
    onSearchChange,
    onSearchSubmit,
    avatarInitial,
    handle,
    did,
}: AppLayoutProps) {
    const { tabs } = useNavStore();

    return (
        <div className="app-layout">
            {/* Desktop/Web top tab strip + account switcher */}
            <TabBar
                avatarInitial={avatarInitial}
                handle={handle}
                did={did}
                onNavigate={onNavigate}
            />

            {/* Global top chrome: nav trigger, search, profile affordances */}
            <TopBar
                activeSidebar={activeSidebar}
                onNavigate={onNavigate}
                searchQuery={searchQuery}
                onSearchChange={onSearchChange}
                onSearchSubmit={onSearchSubmit}
                avatarInitial={avatarInitial}
            />

            {/* Main viewport: rendered child route/screen or empty-tab placeholder */}
            <div className="app-body">
                <main className="main-content">
                    {tabs.length === 0 ? (
                        <div className="no-tabs-state">
                            <div className="no-tabs-box">
                                <div style={{ fontSize: '2rem', marginBottom: 8 }}>🌐</div>
                                <h2>No Tabs Open</h2>
                                <p>Use the navigation menu to open a page.</p>
                            </div>
                        </div>
                    ) : children}
                </main>
            </div>

            {/* Mobile bottom navigation bar — visible only on small screens via CSS */}
            <nav className="mobile-bottom-nav" aria-label="Main navigation">
                {BOTTOM_NAV_ITEMS.map(item => (
                    <button
                        key={item.id}
                        className={`mobile-bottom-nav-item ${activeSidebar === item.id ? 'active' : ''}`}
                        onClick={() => onNavigate(item.id)}
                        aria-label={item.label}
                        aria-current={activeSidebar === item.id ? 'page' : undefined}
                    >
                        <span className="mobile-bottom-nav-icon">{item.icon}</span>
                        <span className="mobile-bottom-nav-label">{item.label}</span>
                    </button>
                ))}
            </nav>
        </div>
    );
}
