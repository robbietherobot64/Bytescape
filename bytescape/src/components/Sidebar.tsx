import { SidebarItem } from '../types';

interface SidebarProps {
    activeSidebar: SidebarItem;
    onNavigate: (item: SidebarItem) => void;
}

export function Sidebar({ activeSidebar, onNavigate }: SidebarProps) {
    return (
        <nav className="sidebar">
            <div className="sidebar-top">
                <div className="sidebar-logo">🔱</div>
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
