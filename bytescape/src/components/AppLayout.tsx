import React from 'react';
import { Sidebar } from './Sidebar';
import { SidebarItem } from '../types';

interface AppLayoutProps {
    children: React.ReactNode;
    activeSidebar: SidebarItem;
    onNavigate: (item: SidebarItem) => void;
    mainStyle?: React.CSSProperties;
}

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
