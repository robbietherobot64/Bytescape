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

/**
 * Screens that get the left ContextPanel.
 * Feed: shows tab navigation + lens picker.
 * All other screens either have their own built-in side panels
 * (Chat, Vault, Friends) or are simple single-column layouts.
 */
const SCREENS_WITH_CONTEXT_PANEL: Set<Screen> = new Set(['feed']);

/**
 * Screens that get the right FriendsPanel.
 * Currently none — Feed has FeedRightPanel, Chat has its own buddy list,
 * Vault/Friends/Profile all manage their own layouts.
 * This set exists so we can opt individual screens in later.
 */
const SCREENS_WITH_FRIENDS_PANEL: Set<Screen> = new Set<Screen>();

/** Application shell: top bar + conditional side panels + main content. */
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
    const showContext = SCREENS_WITH_CONTEXT_PANEL.has(screen);
    const showFriends = SCREENS_WITH_FRIENDS_PANEL.has(screen);

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
                {showContext && (
                    <ContextPanel
                        screen={screen}
                        feedActiveTab={feedActiveTab}
                        setFeedActiveTab={setFeedActiveTab}
                        activeLens={activeLens}
                        setActiveLens={setActiveLens}
                        contextContent={contextContent}
                    />
                )}
                <main className="main-content" style={mainStyle}>
                    {children}
                </main>
                {showFriends && (
                    <FriendsPanel
                        onOpenChat={onOpenChat}
                        onViewProfile={onViewProfile}
                    />
                )}
            </div>
        </div>
    );
}
