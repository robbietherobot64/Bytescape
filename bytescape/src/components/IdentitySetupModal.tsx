/**
 * ════════════════════════════════════════════════════════════════════════════
 * PURPOSE
 * ════════════════════════════════════════════════════════════════════════════
 * Gate first-run identity completion after authentication when
 * `identity_ready === false`.
 *
 * HOW IT WORKS
 * - Seeds a username suggestion from the authenticated handle/source.
 * - Applies client-side normalization and validation constraints.
 * - Persists username via Tauri command invoke (`set_local_username`).
 * - Returns updated auth payload upstream through `onComplete`.
 *
 * RELATED FILES
 * - `src/providers/*` / auth bootstrap flow (controls modal visibility)
 * - `src/types/index.ts` (`AuthStatus` contract)
 * - `src-tauri/src/lib.rs` (registers `set_local_username` command)
 *
 * CROSS-SYSTEM NOTES
 * - This component bridges UI ↔ backend command invocation directly.
 * - It does not own global auth state; parent provider/store reconciles the
 *   `AuthStatus` returned from invoke and propagates app-wide updates.
 */
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
 * ════════════════════════════════════════════════════════════════════════════
 * IdentitySetupModal
 * ════════════════════════════════════════════════════════════════════════════
 * Shown after login when `identity_ready` is false.
 *
 * Derives a suggested username from the user's handle (e.g. "lovingisliving"
 * from "lovingisliving.bsky.social") and lets the user confirm or change it.
 */
export function IdentitySetupModal({ auth, onComplete }: IdentitySetupModalProps) {
    const [username, setUsername] = useState('');
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState('');

    // ═══ Seed default username from authenticated handle/source ═══
    useEffect(() => {
        const suggested = deriveUsername(auth.handle, auth.source);
        setUsername(suggested);
    }, [auth.handle, auth.source]);

    function deriveUsername(handle: string, _source: string): string {
        // Strip any leading '@'
        let raw = handle.replace(/^@+/, '');

        // For local placeholder handles, clear entirely
        if (raw.startsWith('local:')) return '';

        // Universal: extract username before first '.' or '@'
        // Handles: "user.bsky.social" → "user"
        //          "user@instance.social" → "user"
        //          "Name (@user@instance.social)" → "user" (Mastodon display format)
        // First try to extract from Mastodon display format: "Name (@user@domain)"
        const mastodonMatch = raw.match(/@?([\w.-]+)@[\w.-]+\.[a-z]+/i);
        if (mastodonMatch) {
            return sanitize(mastodonMatch[1]);
        }

        // Split on '.' or '@' and take the first segment
        const firstSegment = raw.split(/[.@]/)[0] || '';
        return sanitize(firstSegment);
    }

    function sanitize(raw: string): string {
        return raw
            .toLowerCase()
            .replace(/[^a-z0-9_-]/g, '')
            .slice(0, 32);
    }

    async function handleConfirm() {
        // ═══ Local validation boundary before backend persistence ═══
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
            // Backend call updates canonical auth identity in engine state.
            const updatedAuth = await invoke<AuthStatus>('set_local_username', { username: trimmed });
            // Parent/provider receives updated auth and syncs app-wide state.
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
