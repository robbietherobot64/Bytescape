import { useState, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { invoke } from '@tauri-apps/api/core';
import { Win98Box } from './Win98Box';
import { FeedEntry, LinkedAccount } from '../types';
import { useAuthStore } from '../store';

/**
 * ═══════════════════════════════════════════════════════════════════════════════
 * PURPOSE
 * ═══════════════════════════════════════════════════════════════════════════════
 * Multi-platform repost modal — lets the user select which connected platforms
 * to repost/boost a post to, with an optional quote text field.
 *
 * HOW IT WORKS
 * - Fetches linked accounts via `list_linked_accounts` (with fallback).
 * - Displays platform checkboxes: pre-checks the original post's platform.
 * - Provides an optional quote textarea for platforms that support quoting.
 * - Dispatches platform-specific repost commands sequentially.
 *
 * RELATED FILES
 * - src/components/FeedEntryCard.tsx (parent — opens this modal)
 * - src/components/AudienceDrawer.tsx (similar platform-list pattern)
 * - src/components/QuoteRepostModal.tsx (Bluesky-only quote compose — retained
 *   for deep-quote flows; this modal handles the multi-platform selection)
 * - src/components/AudiencePills.tsx (PLATFORM_CONFIG visuals)
 *
 * CROSS-SYSTEM NOTES
 * - `invoke('repost_post', ...)` / `invoke('unrepost_post', ...)` — Bluesky
 * - `invoke('mastodon_reblog', ...)` / `invoke('mastodon_unreblog', ...)` — Mastodon
 * - `invoke('publish_post_v2', ...)` — for quote reposts on Bluesky
 */

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

interface RepostModalProps {
    /** The post to repost */
    entry: FeedEntry;
    /** Close the modal */
    onClose: () => void;
    /** Called after successful repost(s) with the repost URI (if available) */
    onReposted: (repostUri?: string) => void;
}

/** Platform target for the checkbox list */
interface PlatformTarget {
    id: string;
    label: string;
    emoji: string;
    handle: string;
    connected: boolean;
    supportsQuote: boolean;
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

const PLATFORM_META: Record<string, { emoji: string; label: string; supportsQuote: boolean }> = {
    bluesky: { emoji: '🦋', label: 'Bluesky', supportsQuote: true },
    mastodon: { emoji: '🐘', label: 'Mastodon', supportsQuote: false },
};

/**
 * Resolve the AT-URI for a feed entry, mirroring the logic in
 * src/components/feed/helpers.ts `resolveAtUri`.
 */
function resolveAtUri(entry: FeedEntry): string {
    if (entry.id.startsWith('at://')) return entry.id;
    if (entry.original_url) return entry.original_url;
    if (entry.syndicated_uri) return entry.syndicated_uri;
    const bskyEvent = entry.share_history?.find(e => e.platform === 'bluesky' && e.active);
    if (bskyEvent?.uri) return bskyEvent.uri;
    return entry.id;
}

function getCid(entry: FeedEntry): string {
    return entry.cid || '';
}

/** For Mastodon, the status ID is stored in entry.cid */
function getMastodonId(entry: FeedEntry): string {
    return entry.cid || '';
}

// ═══════════════════════════════════════════════════════════════════════════════
// Component
// ═══════════════════════════════════════════════════════════════════════════════

export function RepostModal({ entry, onClose, onReposted }: RepostModalProps) {
    const auth = useAuthStore((s) => s.auth);
    const [platforms, setPlatforms] = useState<PlatformTarget[]>([]);
    const [checked, setChecked] = useState<Set<string>>(new Set());
    const [quoteText, setQuoteText] = useState('');
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState('');
    const [results, setResults] = useState<string[]>([]);

    // ── Load linked accounts ─────────────────────────────────────────────────
    useEffect(() => {
        let cancelled = false;

        async function load() {
            let accounts: LinkedAccount[] = [];

            try {
                accounts = await invoke<LinkedAccount[]>('list_linked_accounts');
            } catch {
                // Fallback: build from auth state
                if (auth?.source === 'bluesky') {
                    accounts.push({
                        protocol: 'bluesky',
                        identifier: auth.did,
                        display_handle: `@${auth.handle}`,
                        connected_at: Date.now() / 1000,
                        active: true,
                    });
                }
                try {
                    const mastodonSession = await invoke<{ acct?: string }>('mastodon_get_session');
                    if (mastodonSession) {
                        accounts.push({
                            protocol: 'mastodon',
                            identifier: `acct:${mastodonSession.acct || 'unknown'}`,
                            display_handle: `@${mastodonSession.acct || 'unknown'}`,
                            connected_at: Date.now() / 1000,
                            active: true,
                        });
                    }
                } catch {
                    /* Mastodon not connected */
                }
            }

            if (cancelled) return;

            // Build platform targets
            const targets: PlatformTarget[] = [];
            const connectedProtocols = new Set(accounts.filter(a => a.active).map(a => a.protocol.toLowerCase()));

            for (const [id, meta] of Object.entries(PLATFORM_META)) {
                const account = accounts.find(a => a.protocol.toLowerCase() === id);
                targets.push({
                    id,
                    label: meta.label,
                    emoji: meta.emoji,
                    handle: account?.display_handle || '(not connected)',
                    connected: connectedProtocols.has(id),
                    supportsQuote: meta.supportsQuote,
                });
            }

            setPlatforms(targets);

            // Pre-check the source platform
            const sourceKey = (entry.source || '').toLowerCase();
            const initialChecked = new Set<string>();
            if (connectedProtocols.has(sourceKey)) {
                initialChecked.add(sourceKey);
            }
            // If source isn't connectable (e.g. archive), pre-check bluesky if connected
            if (initialChecked.size === 0 && connectedProtocols.has('bluesky')) {
                initialChecked.add('bluesky');
            }
            setChecked(initialChecked);
        }

        load();
        return () => { cancelled = true; };
    }, [auth, entry.source]);

    // ── Toggle a platform checkbox ────────────────────────────────────────────
    const toggle = useCallback((id: string) => {
        setChecked(prev => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
        });
    }, []);

    // ── Execute repost ────────────────────────────────────────────────────────
    const handleRepost = useCallback(async () => {
        if (checked.size === 0) return;
        setLoading(true);
        setError('');
        setResults([]);

        const outcomes: string[] = [];
        let firstUri: string | undefined;

        for (const platformId of checked) {
            try {
                if (platformId === 'bluesky') {
                    if (quoteText.trim()) {
                        // Quote repost on Bluesky
                        await invoke('publish_post_v2', {
                            text: quoteText.trim(),
                            audiences: ['Bluesky'],
                            quoteUri: resolveAtUri(entry),
                            quoteCid: getCid(entry),
                        });
                        outcomes.push('🦋 Quoted on Bluesky');
                    } else {
                        // Direct repost
                        const result = await invoke<{ uri: string; cid: string }>('repost_post', {
                            uri: resolveAtUri(entry),
                            cid: getCid(entry),
                        });
                        firstUri = result.uri;
                        outcomes.push('🦋 Reposted on Bluesky');
                    }
                } else if (platformId === 'mastodon') {
                    await invoke('mastodon_reblog', { statusId: getMastodonId(entry) });
                    outcomes.push('🐘 Boosted on Mastodon');
                }
            } catch (err: any) {
                const msg = typeof err === 'string' ? err : String(err);
                outcomes.push(`❌ ${PLATFORM_META[platformId]?.label || platformId}: ${msg}`);
            }
        }

        setResults(outcomes);
        setLoading(false);

        // If at least one succeeded (no ❌ prefix), notify parent
        if (outcomes.some(o => !o.startsWith('❌'))) {
            onReposted(firstUri);
        }
    }, [checked, quoteText, entry, onReposted]);

    // ── Has any selected platform that supports quoting? ──────────────────────
    const showQuoteField = [...checked].some(id =>
        platforms.find(p => p.id === id)?.supportsQuote
    );

    // ── Render ────────────────────────────────────────────────────────────────
    return createPortal(
        <div
            className="repost-modal-overlay"
            onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
        >
            <Win98Box title="Repost" onClose={onClose} style={{ width: 380, maxWidth: '90vw' } as React.CSSProperties}>
                <div className="repost-modal-body">
                    {/* Platform checkboxes */}
                    <div className="repost-modal-section-label">Share to:</div>
                    <div className="repost-modal-platforms">
                        {platforms.map(p => (
                            <label
                                key={p.id}
                                className={`repost-platform-row ${!p.connected ? 'disabled' : ''}`}
                            >
                                <input
                                    type="checkbox"
                                    checked={checked.has(p.id)}
                                    disabled={!p.connected || loading}
                                    onChange={() => toggle(p.id)}
                                />
                                <span className="repost-platform-emoji">{p.emoji}</span>
                                <span className="repost-platform-info">
                                    <span className="repost-platform-label">{p.label}</span>
                                    <span className="repost-platform-handle">{p.handle}</span>
                                </span>
                            </label>
                        ))}
                    </div>

                    {/* Optional quote text */}
                    {showQuoteField && (
                        <div className="repost-modal-quote">
                            <textarea
                                className="repost-modal-textarea"
                                placeholder="Add a quote (optional)"
                                value={quoteText}
                                onChange={e => setQuoteText(e.target.value)}
                                maxLength={300}
                                rows={3}
                                disabled={loading}
                            />
                            <div className="repost-modal-char-count">
                                {quoteText.length}/300
                            </div>
                        </div>
                    )}

                    {/* Error */}
                    {error && <div className="repost-modal-error">{error}</div>}

                    {/* Results */}
                    {results.length > 0 && (
                        <div className="repost-modal-results">
                            {results.map((r, i) => (
                                <div key={i} className="repost-modal-result">{r}</div>
                            ))}
                        </div>
                    )}

                    {/* Actions */}
                    <div className="repost-modal-actions">
                        <button
                            className="btn btn-sm"
                            onClick={onClose}
                            disabled={loading}
                        >
                            Cancel
                        </button>
                        {results.length > 0 ? (
                            <button
                                className="btn btn-sm btn-primary"
                                onClick={onClose}
                            >
                                Done
                            </button>
                        ) : (
                            <button
                                className="btn btn-sm btn-primary"
                                onClick={handleRepost}
                                disabled={loading || checked.size === 0}
                            >
                                {loading ? 'Sending…' : `Repost${checked.size > 1 ? ` to ${checked.size}` : ''}`}
                            </button>
                        )}
                    </div>
                </div>
            </Win98Box>
        </div>,
        document.body
    );
}
