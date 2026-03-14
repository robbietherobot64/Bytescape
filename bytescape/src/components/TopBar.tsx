import { useState, useRef, useEffect } from 'react';
import { SidebarItem } from '../types';
import {
    BsHouseFill, BsPersonFill, BsChatDotsFill,
    BsPeopleFill, BsBoxArrowInDown, BsFolderFill, BsGearFill
} from 'react-icons/bs';
import { Omnibar } from './Omnibar';

interface TopBarProps {
    activeSidebar: SidebarItem;
    onNavigate: (item: SidebarItem) => void;
    /** @deprecated — now handled internally by Omnibar. Kept for backwards compat. */
    searchQuery?: string;
    /** @deprecated — now handled internally by Omnibar. Kept for backwards compat. */
    onSearchChange?: (query: string) => void;
    onSearchSubmit: (query: string) => void;
    avatarInitial?: string;
}

const NAV_ITEMS: { id: SidebarItem; icon: React.ReactNode; label: string }[] = [
    { id: 'feed', icon: <BsHouseFill />, label: 'Feed' },
    { id: 'chat', icon: <BsChatDotsFill />, label: 'Chat' },
    { id: 'vault', icon: <BsFolderFill />, label: 'Vault' },
    { id: 'social', icon: <BsPeopleFill />, label: 'Social' },
    { id: 'import', icon: <BsBoxArrowInDown />, label: 'Import' },
    { id: 'settings', icon: <BsGearFill />, label: 'Settings' },
];

export function TopBar({ activeSidebar, onNavigate, onSearchSubmit, avatarInitial }: TopBarProps) {
    const [menuOpen, setMenuOpen] = useState(false);
    const menuRef = useRef<HTMLDivElement>(null);

    // Close menu on outside click
    useEffect(() => {
        if (!menuOpen) return;
        const handler = (e: MouseEvent) => {
            if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
                setMenuOpen(false);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, [menuOpen]);

    const handleNavClick = (id: SidebarItem) => {
        setMenuOpen(false);
        onNavigate(id);
    };

    return (
        <div className="topbar">
            {/* Left: Profile Icon */}
            <button
                className="topbar-profile-btn"
                onClick={() => handleNavClick('profile')}
                title="My Profile"
            >
                <div className="topbar-avatar">
                    {avatarInitial || <BsPersonFill />}
                </div>
            </button>

            {/* Center: Omnibar (B10.3) — browser-style address bar + search */}
            <Omnibar onSearchSubmit={onSearchSubmit} />

            {/* Right: Hamburger Menu */}
            <div className="topbar-menu-container" ref={menuRef}>
                <button
                    className={`topbar-hamburger ${menuOpen ? 'open' : ''}`}
                    onClick={() => setMenuOpen(v => !v)}
                    title="Navigation"
                >
                    <span className="topbar-hamburger-line" />
                    <span className="topbar-hamburger-line" />
                    <span className="topbar-hamburger-line" />
                </button>

                {menuOpen && (
                    <div className="topbar-dropdown">
                        <div className="topbar-dropdown-header">
                            <span className="topbar-dropdown-title">BYTESCAPE</span>
                        </div>
                        {NAV_ITEMS.map(item => (
                            <button
                                key={item.id}
                                className={`topbar-dropdown-item ${activeSidebar === item.id ? 'active' : ''}`}
                                onClick={() => handleNavClick(item.id)}
                            >
                                <span className="topbar-dropdown-icon">{item.icon}</span>
                                <span className="topbar-dropdown-label">{item.label}</span>
                            </button>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}
