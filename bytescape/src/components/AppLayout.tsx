import React from 'react';
import { TopBar } from './TopBar';
import { FriendsPanel } from './FriendsPanel';
import { ContextPanel } from './ContextPanel';
import { SidebarItem } from '../types';
import { Screen } from '../store';
import { FeedTab } from '../hooks/useUnifiedFeed';

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
    /** Feed-specific context props. */
    feedActiveTab?: FeedTab;
    setFeedActiveTab?: (tab: FeedTab) => void;
    activeLens?: string;
    setActiveLens?: (lens: string) => void;
    /** Context panel slot — screen-specific content rendered in left panel. */
    contextContent?: React.ReactNode;
    /** Friends panel callbacks. */
    onOpenChat?: (peerNodeId: string, peerName: string) => void;
    onViewProfile?: (peerNodeId: string, peerName?: string) => void;
}

/** Application shell: top bar, context panel, main content, friends panel. */
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
    feedActiveTab,
    setFeedActiveTab,
    activeLens,
    setActiveLens,
    contextContent,
    onOpenChat,
    onViewProfile,
}: AppLayoutProps) {
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
                <ContextPanel
                    screen={screen}
                    feedActiveTab={feedActiveTab}
                    setFeedActiveTab={setFeedActiveTab}
                    activeLens={activeLens}
                    setActiveLens={setActiveLens}
                    contextContent={contextContent}
                />
                <main className="main-content" style={mainStyle}>
                    {children}
                </main>
                <FriendsPanel
                    onOpenChat={onOpenChat}
                    onViewProfile={onViewProfile}
                />
            </div>
        </div>
    );
}
