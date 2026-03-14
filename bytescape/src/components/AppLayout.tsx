import React from 'react';
import { TopBar } from './TopBar';
import { FriendsPanel } from './FriendsPanel';
import { SidebarItem } from '../types';
import { Screen } from '../store';

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

/** Screens that already have their own buddy/contact list — don't double-up. */
const SCREENS_WITH_OWN_BUDDY_LIST: ReadonlySet<Screen> = new Set(['chat', 'friends']);

/** Application shell: top bar + friends panel (conditionally visible) + main content. */
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
    const showFriendsPanel = !SCREENS_WITH_OWN_BUDDY_LIST.has(screen);

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
        </div>
    );
}
