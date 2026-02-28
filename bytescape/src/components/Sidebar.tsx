import { useState } from 'react';
import { SidebarItem } from '../types';

interface SidebarProps {
    activeSidebar: SidebarItem;
    onNavigate: (item: SidebarItem) => void;
}

/* Primary tabs shown in mobile bottom bar */
const PRIMARY_TABS: { id: SidebarItem; icon: string; label: string }[] = [
    { id: 'feed', icon: '🏠', label: 'Feed' },
    { id: 'chat', icon: '💬', label: 'Chat' },
    { id: 'vault', icon: '🗄️', label: 'Vault' },
    { id: 'search', icon: '🔍', label: 'Search' },
];

/* Overflow items behind hamburger menu on mobile */
const OVERFLOW_TABS: { id: SidebarItem; icon: string; label: string }[] = [
    { id: 'profile', icon: '👤', label: 'Profile' },
    { id: 'followers', icon: '👥', label: 'Social' },
    { id: 'import', icon: '📦', label: 'Import' },
    { id: 'settings', icon: '⚙️', label: 'Settings' },
];

export function Sidebar({ activeSidebar, onNavigate }: SidebarProps) {
    const [moreOpen, setMoreOpen] = useState(false);

    const handleOverflowNavigate = (id: SidebarItem) => {
        setMoreOpen(false);
        onNavigate(id);
    };

    const isOverflowActive = OVERFLOW_TABS.some(t => t.id === activeSidebar);

    return (
        <nav className="sidebar">
            {/* Desktop layout — full sidebar */}
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

            {/* Mobile bottom tab bar */}
            <div className="mobile-tab-bar">
                {PRIMARY_TABS.map(t => (
                    <SidebarIcon key={t.id} id={t.id} icon={t.icon} label={t.label} active={activeSidebar} onClick={onNavigate} />
                ))}
                {/* Hamburger / More button */}
                <button
                    className={`sidebar-icon mobile-more-btn ${isOverflowActive ? 'active' : ''} ${moreOpen ? 'open' : ''}`}
                    onClick={() => setMoreOpen(v => !v)}
                    title="More"
                >
                    <span className="sidebar-icon-emoji">{moreOpen ? '✕' : '☰'}</span>
                    <span className="sidebar-icon-label">More</span>
                </button>
            </div>

            {/* Overflow drawer (mobile only) */}
            {moreOpen && (
                <div className="mobile-overflow-drawer" onClick={() => setMoreOpen(false)}>
                    <div className="mobile-overflow-menu" onClick={e => e.stopPropagation()}>
                        <div className="mobile-overflow-header">BYTESCAPE</div>
                        {OVERFLOW_TABS.map(t => (
                            <button
                                key={t.id}
                                className={`mobile-overflow-item ${activeSidebar === t.id ? 'active' : ''}`}
                                onClick={() => handleOverflowNavigate(t.id)}
                            >
                                <span className="mobile-overflow-icon">{t.icon}</span>
                                <span className="mobile-overflow-label">{t.label}</span>
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
