/**
 * AccountSwitcher
 *
 * Profile avatar dropdown positioned in the tab bar strip.
 * Displays the active identity, connected platform accounts,
 * and a "Connect Account" action.
 *
 * CROSS-SYSTEM NOTES
 * - Fetches connected sources via `list_vault_sources` backend command.
 * - Navigation is driven through the parent `onNavigate` callback.
 */

import { useState, useRef, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { BsPersonFill, BsPlusCircleFill, BsPersonLinesFill } from 'react-icons/bs';
import { SidebarItem } from '../types';

interface AccountSwitcherProps {
    avatarInitial?: string;
    handle?: string | null;
    did?: string | null;
    onNavigate: (item: SidebarItem) => void;
}

const SOURCE_EMOJI: Record<string, string> = {
    bluesky: '🦋',
    facebook: '📘',
    mastodon: '🐘',
    instagram: '📸',
    local: '💾',
};

export function AccountSwitcher({ avatarInitial, handle, did, onNavigate }: AccountSwitcherProps) {
    const [open, setOpen] = useState(false);
    const [sources, setSources] = useState<string[]>([]);
    const dropdownRef = useRef<HTMLDivElement>(null);

    // Fetch connected sources when dropdown opens
    const fetchSources = useCallback(async () => {
        try {
            const s = await invoke<string[]>('list_vault_sources');
            setSources(s);
        } catch {
            setSources([]);
        }
    }, []);

    useEffect(() => {
        if (open) fetchSources();
    }, [open, fetchSources]);

    // Close on outside click
    useEffect(() => {
        if (!open) return;
        const handler = (e: MouseEvent) => {
            if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
                setOpen(false);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, [open]);

    const goToProfile = () => {
        setOpen(false);
        onNavigate('profile');
    };

    const goToSettings = () => {
        setOpen(false);
        onNavigate('settings');
    };

    return (
        <div className="account-switcher" ref={dropdownRef}>
            <button
                className="account-switcher-btn"
                onClick={() => setOpen(v => !v)}
                title="Account switcher"
            >
                <div className="account-switcher-avatar">
                    {avatarInitial || <BsPersonFill size={14} />}
                </div>
            </button>

            {open && (
                <div className="account-switcher-dropdown">
                    {/* Current identity header */}
                    <div className="account-switcher-header">
                        <div className="account-switcher-avatar account-switcher-avatar--large">
                            {avatarInitial || <BsPersonFill size={18} />}
                        </div>
                        <div className="account-switcher-identity">
                            <div className="account-switcher-handle">
                                {handle || 'Anonymous'}
                            </div>
                            {did && (
                                <div className="account-switcher-did">
                                    {did.length > 28 ? did.slice(0, 28) + '…' : did}
                                </div>
                            )}
                        </div>
                    </div>

                    {/* View Profile action */}
                    <button className="account-switcher-item" onClick={goToProfile}>
                        <BsPersonLinesFill size={14} />
                        <span>View Profile</span>
                    </button>

                    {/* Connected accounts */}
                    {sources.length > 0 && (
                        <>
                            <div className="account-switcher-divider" />
                            <div className="account-switcher-section-label">Connected Accounts</div>
                            {sources.map(s => (
                                <div key={s} className="account-switcher-account">
                                    <span className="account-switcher-account-emoji">
                                        {SOURCE_EMOJI[s.toLowerCase()] || '📝'}
                                    </span>
                                    <span className="account-switcher-account-name">{s}</span>
                                </div>
                            ))}
                        </>
                    )}

                    {/* Connect new account */}
                    <div className="account-switcher-divider" />
                    <button className="account-switcher-item account-switcher-item--connect" onClick={goToSettings}>
                        <BsPlusCircleFill size={14} />
                        <span>Connect Account</span>
                    </button>
                </div>
            )}
        </div>
    );
}
