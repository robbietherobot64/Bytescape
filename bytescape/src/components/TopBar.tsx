import { useState, useRef, useEffect } from 'react';
import { SidebarItem } from '../types';
import {
    BsHouseFill, BsPersonFill, BsChatDotsFill,
    BsPeopleFill, BsBoxArrowInDown, BsFolderFill, BsGearFill, BsListUl
} from 'react-icons/bs';
import { Omnibar } from './Omnibar';

/**
 * TopBar
 *
 * Primary navigation and global search entry point.
 *
 * Cross-system note:
 * - Navigation drives lens/screen composition in app-level state.
 * - Search submits through `onSearchSubmit`, which may trigger frontend
 *   filtering and/or backend-backed query flows depending on the active screen.
 */

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

/**
 * Static menu model for dropdown navigation.
 * Kept local to avoid coupling display order to store definitions.
 */
const NAV_ITEMS: { id: SidebarItem; icon: React.ReactNode; label: string }[] = [
    { id: 'feed', icon: <BsHouseFill />, label: 'Feed' },
    { id: 'profile', icon: <BsPersonFill />, label: 'Profile' },
    { id: 'forums', icon: <BsListUl />, label: 'Forums' },
    { id: 'chat', icon: <BsChatDotsFill />, label: 'Chat' },
    { id: 'vault', icon: <BsFolderFill />, label: 'Vault' },
    { id: 'social', icon: <BsPeopleFill />, label: 'Social' },
    { id: 'import', icon: <BsBoxArrowInDown />, label: 'Import' },
    { id: 'settings', icon: <BsGearFill />, label: 'Settings' },
];

/**
 * Exported top navigation shell used across desktop and responsive layouts.
 */
export function TopBar({ activeSidebar, onNavigate, onSearchSubmit }: TopBarProps) {
    // ===== Section: local interaction state =====
    const [menuOpen, setMenuOpen] = useState(false);
    const menuRef = useRef<HTMLDivElement>(null);

    // ===== Section: side effects =====
    // Close menu on outside click.
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

    // ===== Section: event handlers =====
    const handleNavClick = (id: SidebarItem) => {
        setMenuOpen(false);
        onNavigate(id);
    };

    return (
        <div className="topbar">
            {/* ===== Section: center omnibar (global search/address) ===== */}
            <Omnibar onSearchSubmit={onSearchSubmit} />

            {/* ===== Section: right dropdown menu ===== */}
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
