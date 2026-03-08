import { useState, useEffect } from 'react';
import { invoke } from "@tauri-apps/api/core";
import { Win98Box } from './Win98Box';
import { AuthStatus } from '../types';

interface IdentitySetupModalProps {
    /** The current auth status (contains handle, source, etc.) */
    auth: AuthStatus;
    /** Called when the user confirms their username */
    onComplete: (updatedAuth: AuthStatus) => void;
}

/**
 * IdentitySetupModal — shown after login when identity_ready is false.
 *
 * Derives a suggested username from the user's handle (e.g. "lovingisliving"
 * from "lovingisliving.bsky.social") and lets the user confirm or change it.
 */
export function IdentitySetupModal({ auth, onComplete }: IdentitySetupModalProps) {
    const [username, setUsername] = useState('');
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState('');

    // Derive a suggested username from the handle on mount
    useEffect(() => {
        const suggested = deriveUsername(auth.handle, auth.source);
        setUsername(suggested);
    }, [auth.handle, auth.source]);

    function deriveUsername(handle: string, source: string): string {
        if (source === 'bluesky') {
            // "lovingisliving.bsky.social" → "lovingisliving"
            // "user.custom.domain" → "user"
            const parts = handle.split('.');
            return sanitize(parts[0] || '');
        }
        // For local or other sources, strip the "local:" prefix if present
        if (handle.startsWith('local:')) {
            return '';
        }
        return sanitize(handle);
    }

    function sanitize(raw: string): string {
        return raw
            .toLowerCase()
            .replace(/[^a-z0-9_-]/g, '')
            .slice(0, 32);
    }

    async function handleConfirm() {
        const trimmed = username.trim().toLowerCase();
        if (trimmed.length < 2) {
            setError('Username must be at least 2 characters.');
            return;
        }
        if (trimmed.length > 32) {
            setError('Username must be 32 characters or fewer.');
            return;
        }
        if (!/^[a-z0-9_-]+$/.test(trimmed)) {
            setError('Only letters, numbers, hyphens, and underscores allowed.');
            return;
        }

        setSaving(true);
        setError('');
        try {
            const updatedAuth = await invoke<AuthStatus>('set_local_username', { username: trimmed });
            onComplete(updatedAuth);
        } catch (e: any) {
            setError(typeof e === 'string' ? e : e.message || 'Failed to save username');
        } finally {
            setSaving(false);
        }
    }

    return (
        <div className="identity-setup-overlay">
            <Win98Box variant="window" title="Choose Your Identity" className="identity-setup-modal">
                <div className="identity-setup-content">
                    <div className="identity-setup-icon">🪪</div>
                    <h3 className="identity-setup-heading">
                        Pick your Bytescape username
                    </h3>
                    <p className="identity-setup-description">
                        This is your universal identity across Bytescape — it works across
                        all connected accounts and P2P features.
                    </p>

                    {auth.source === 'bluesky' && (
                        <p className="identity-setup-hint">
                            Suggested from your Bluesky handle: <strong>{auth.handle}</strong>
                        </p>
                    )}

                    <div className="identity-setup-input-group">
                        <span className="identity-setup-at">@</span>
                        <input
                            className="input-field identity-setup-input"
                            type="text"
                            placeholder="your-username"
                            value={username}
                            onChange={(e) => {
                                setUsername(e.target.value);
                                setError('');
                            }}
                            onKeyDown={(e) => e.key === 'Enter' && handleConfirm()}
                            maxLength={32}
                            autoFocus
                        />
                    </div>

                    {error && <p className="identity-setup-error">{error}</p>}

                    <button
                        className="btn btn-primary btn-block"
                        onClick={handleConfirm}
                        disabled={saving || username.trim().length < 2}
                    >
                        {saving ? 'Saving...' : 'Confirm Username'}
                    </button>

                    <p className="identity-setup-fine-print">
                        You can change this later in Settings.
                    </p>
                </div>
            </Win98Box>
        </div>
    );
}
