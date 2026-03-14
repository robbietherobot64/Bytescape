import React from 'react';
import { TopBar } from './TopBar';
import { FriendsPanel } from './FriendsPanel';
import { SidebarItem } from '../types';
import { Screen } from '../store';
import {
    BsHouseFill, BsChatDotsFill, BsPeopleFill,
    BsFolderFill, BsPersonFill
} from 'react-icons/bs';

interface AppLayoutProps {
    /** Screen content rendered in the primary pane. */
    children: React.ReactNode;
    /** Sidebar item used for active navigation highlight state. */
    activeSidebar: SidebarItem;
    /** Called when a sidebar item is selected. */
    onNavigate: (item: SidebarItem) => void;
    /** Optional style override for full-bleed layouts. */
    mainStyle?: React.CSSProperties;
    /** Current active screen for context panel. */
    screen: Screen;
    /** Search bar state. */
    searchQuery: string;
    onSearchChange: (query: string) => void;
    onSearchSubmit: (query: string) => void;
    /** User avatar initial for top bar. */
    avatarInitial?: string;
    /** Friends panel callbacks. */
    onOpenChat?: (peerNodeId: string, peerName: string) => void;
    onViewProfile?: (peerNodeId: string, peerName?: string) => void;
}

/** Bottom navigation items for mobile. */
const BOTTOM_NAV_ITEMS: { id: SidebarItem; icon: React.ReactNode; label: string }[] = [
    { id: 'feed', icon: <BsHouseFill />, label: 'Feed' },
    { id: 'chat', icon: <BsChatDotsFill />, label: 'Chat' },
    { id: 'social', icon: <BsPeopleFill />, label: 'Social' },
    { id: 'vault', icon: <BsFolderFill />, label: 'Vault' },
    { id: 'profile', icon: <BsPersonFill />, label: 'Profile' },
];

/** Application shell: top bar + friends panel (feed only) + main content + mobile bottom nav. */
export function AppLayout({
    children,
    activeSidebar,
    onNavigate,
    mainStyle,
    screen,
    searchQuery,
    onSearchChange,
    onSearchSubmit,
    avatarInitial,
    onOpenChat,
    onViewProfile,
}: AppLayoutProps) {
    const showFriendsPanel = screen === 'feed';

    return (
        <div className="app-layout">
            <TopBar
                activeSidebar={activeSidebar}
                onNavigate={onNavigate}
                searchQuery={searchQuery}
                onSearchChange={onSearchChange}
                onSearchSubmit={onSearchSubmit}
                avatarInitial={avatarInitial}
            />
            <div className="app-body">
                <main className="main-content" style={mainStyle}>
                    {children}
                </main>
                {showFriendsPanel && (
                    <FriendsPanel
                        onOpenChat={onOpenChat}
                        onViewProfile={onViewProfile}
                    />
                )}
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
