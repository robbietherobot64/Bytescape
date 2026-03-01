import { useState } from 'react';
import { SidebarItem } from '../types';

interface SidebarProps {
    activeSidebar: SidebarItem;
    onNavigate: (item: SidebarItem) => void;
    /** Optional: feed tab buttons rendered inside the top bar on mobile */
    feedTabSlot?: React.ReactNode;
}

/* All nav items — shown in hamburger menu on mobile */
const ALL_NAV_ITEMS: { id: SidebarItem; icon: string; label: string }[] = [
    { id: 'feed', icon: '🏠', label: 'Feed' },
    { id: 'chat', icon: '💬', label: 'Chat' },
    { id: 'vault', icon: '🗄️', label: 'Vault' },
    { id: 'search', icon: '🔍', label: 'Search' },
    { id: 'profile', icon: '👤', label: 'Profile' },
    { id: 'followers', icon: '👥', label: 'Social' },
    { id: 'import', icon: '📦', label: 'Import' },
    { id: 'settings', icon: '⚙️', label: 'Settings' },
];

export function Sidebar({ activeSidebar, onNavigate, feedTabSlot }: SidebarProps) {
    const [menuOpen, setMenuOpen] = useState(false);

    const handleMenuNavigate = (id: SidebarItem) => {
        setMenuOpen(false);
        onNavigate(id);
    };

    return (
        <nav className="sidebar">
            {/* Desktop layout — full sidebar (hidden on mobile via CSS) */}
            <div className="sidebar-top">
                <div className="sidebar-logo">BYTESCAPE</div>
                <SidebarIcon id="feed" icon="🏠" label="Feed" active={activeSidebar} onClick={onNavigate} />
                <SidebarIcon id="search" icon="🔍" label="Search" active={activeSidebar} onClick={onNavigate} />
                <SidebarIcon id="profile" icon="👤" label="Profile" active={activeSidebar} onClick={onNavigate} />
                <SidebarIcon id="chat" icon="💬" label="Chat" active={activeSidebar} onClick={onNavigate} />
                <SidebarIcon id="followers" icon="👥" label="Social" active={activeSidebar} onClick={onNavigate} />
                <SidebarIcon id="import" icon="📦" label="Import" active={activeSidebar} onClick={onNavigate} />
                <SidebarIcon id="vault" icon="🗄️" label="Vault" active={activeSidebar} onClick={onNavigate} />
            </div>
            <div className="sidebar-bottom">
                <SidebarIcon id="settings" icon="⚙️" label="Settings" active={activeSidebar} onClick={onNavigate} />
            </div>

            {/* Mobile top bar: hamburger + feed tabs (hidden on desktop via CSS) */}
            <div className="mobile-top-bar">
                <button
                    className={`mobile-hamburger ${menuOpen ? 'open' : ''}`}
                    onClick={() => setMenuOpen(v => !v)}
                    title="Menu"
                >
                    {menuOpen ? '✕' : '☰'}
                </button>
                {/* Feed tabs slot — rendered inline when on feed screen */}
                {feedTabSlot && (
                    <div className="mobile-feed-tabs">
                        {feedTabSlot}
                    </div>
                )}
                {!feedTabSlot && (
                    <div className="mobile-screen-title">
                        {ALL_NAV_ITEMS.find(t => t.id === activeSidebar)?.label || 'Bytescape'}
                    </div>
                )}
            </div>

            {/* Dropdown menu (mobile only) */}
            {menuOpen && (
                <div className="mobile-menu-backdrop" onClick={() => setMenuOpen(false)}>
                    <div className="mobile-menu-dropdown" onClick={e => e.stopPropagation()}>
                        {ALL_NAV_ITEMS.map(t => (
                            <button
                                key={t.id}
                                className={`mobile-menu-item ${activeSidebar === t.id ? 'active' : ''}`}
                                onClick={() => handleMenuNavigate(t.id)}
                            >
                                <span className="mobile-menu-icon">{t.icon}</span>
                                <span className="mobile-menu-label">{t.label}</span>
                            </button>
                        ))}
                    </div>
                </div>
            )}
        </nav>
    );
}

interface SidebarIconProps {
    id: SidebarItem;
    icon: string;
    label: string;
    active: SidebarItem;
    onClick: (id: SidebarItem) => void;
}

function SidebarIcon({ id, icon, label, active, onClick }: SidebarIconProps) {
    return (
        <button
            className={`sidebar-icon ${active === id ? "active" : ""}`}
            onClick={() => onClick(id)}
            title={label}
        >
            <span className="sidebar-icon-emoji">{icon}</span>
            <span className="sidebar-icon-label">{label}</span>
        </button>
    );
}
