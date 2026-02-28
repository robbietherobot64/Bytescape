import React from 'react';
import { Sidebar } from './Sidebar';
import { SidebarItem } from '../types';

interface AppLayoutProps {
    /** Screen content rendered in the primary pane. */
    children: React.ReactNode;
    /** Sidebar item used for active navigation highlight state. */
    activeSidebar: SidebarItem;
    /** Called when a sidebar item is selected. */
    onNavigate: (item: SidebarItem) => void;
    /** Optional style override for full-bleed layouts. */
    mainStyle?: React.CSSProperties;
}

/** Application shell: persistent sidebar + main content region. */
export function AppLayout({ children, activeSidebar, onNavigate, mainStyle }: AppLayoutProps) {
    return (
        <div className="app-layout">
            <Sidebar activeSidebar={activeSidebar} onNavigate={onNavigate} />
            <main className="main-content" style={mainStyle}>
                {children}
            </main>
        </div>
    );
}
