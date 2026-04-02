import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Win98Box } from '../components/Win98Box';
import { FeedEntryCard } from '../components/FeedEntryCard';
import { FeedEntry } from '../types';

/**
 * PURPOSE
 * Resolves and renders a remote/local social profile plus recent posts,
 * with protocol-aware fallback behavior.
 *
 * HOW IT WORKS
 * - Attempts unified backend resolution via `resolve_profile`.
 * - Falls back to protocol-specific commands/cache heuristics when needed.
 * - Loads recent posts once profile identity is available.
 *
 * RELATED FILES
 * - src-tauri/src/lib.rs (Tauri command registration for invoked methods)
 * - src/components/FeedEntryCard.tsx (post rendering)
 * - src/types/index.ts (FeedEntry contract)
 *
 * CROSS-SYSTEM NOTES
 * - This screen directly performs backend `invoke(...)` calls instead of routing
 *   through a dedicated provider/store hook, coupling UI lifecycle to transport.
 */

interface ProfileViewProps {
    handle: string;
    protocol?: 'bluesky' | 'activitypub' | 'mastodon';
}

interface ResolvedProfile {
    did: string;
    handle: string;
    display_name?: string;
    avatar_url?: string;
    description?: string;
    metrics?: {
        followers: number;
        following: number;
        posts: number;
    };
    source: string;
}

/**
 * Protocol-agnostic profile view with best-effort identity/post hydration.
 */
export function ProfileViewScreen({ handle, protocol = 'bluesky' }: ProfileViewProps) {
    // ═══════════════════════════════════════════════════════════════════════
    // Local state
    // ═══════════════════════════════════════════════════════════════════════
    const [profile, setProfile] = useState<ResolvedProfile | null>(null);
    const [posts, setPosts] = useState<FeedEntry[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    // ═══════════════════════════════════════════════════════════════════════
    // Profile resolution effect (with protocol-specific fallback paths)
    // ═══════════════════════════════════════════════════════════════════════
    useEffect(() => {
        let isMounted = true;
        
        async function fetchProfile() {
            setLoading(true);
            setError(null);
            
            try {
                // Try unified command
                const res = await invoke<ResolvedProfile>('resolve_profile', { handle, protocol });
                if (isMounted) setProfile(res);
            } catch (err: any) {
                // Fallback approaches
                if (protocol === 'activitypub' || protocol === 'mastodon') {
                    try {
                        const actorUrl = handle.includes('@') ? handle : `https://${handle}`; // very crude map
                        const apRes = await invoke<any>('fetch_remote_actor', { actorUrl });
                        if (isMounted) {
                            setProfile({
                                did: apRes.id || handle,
                                handle: handle,
                                display_name: apRes.name,
                                description: apRes.summary,
                                source: 'activitypub',
                            });
                        }
                    } catch (apErr) {
                        if (isMounted) setError(String(apErr));
                    }
                } else {
                    // Fallback for bluesky: scan list_contacts
                    try {
                        const contacts = await invoke<any[]>('list_contacts');
                        const match = contacts.find(c => c.handle === handle || c.did === handle);
                        if (match && isMounted) {
                            setProfile({
                                did: match.did,
                                handle: match.handle,
                                display_name: match.display_name,
                                avatar_url: match.avatar_url,
                                description: match.description,
                                source: 'bluesky',
                            });
                        } else if (isMounted) {
                            setError('Profile not found in local cache. Unified resolve_profile failed.');
                        }
                    } catch (bskyErr) {
                        if (isMounted) setError(String(err));
                    }
                }
            } finally {
                if (isMounted) setLoading(false);
            }
        }
        
        fetchProfile();
        return () => { isMounted = false; };
    }, [handle, protocol]);

    // Fetch posts once profile is resolved
    // ═══════════════════════════════════════════════════════════════════════
    // Posts fetch effect
    // ═══════════════════════════════════════════════════════════════════════
    useEffect(() => {
        if (!profile?.did) return;
        let isMounted = true;

        async function fetchPosts() {
            try {
                if (profile?.source === 'bluesky') {
                    const res = await invoke<FeedEntry[]>('get_author_feed', { actor: profile.did, limit: 20 });
                    if (isMounted) setPosts(res || []);
                } else if (profile?.source === 'activitypub' || profile?.source === 'mastodon') {
                    const res = await invoke<FeedEntry[]>('mastodon_get_timeline', { actor: profile.did, limit: 20 });
                    if (isMounted) setPosts(res || []);
                }
            } catch (e) {
                console.error("Failed to load posts", e);
            }
        }

        fetchPosts();
        return () => { isMounted = false; };
    }, [profile?.did, profile?.source]);

    const isBluesky = protocol === 'bluesky';

    // ═══════════════════════════════════════════════════════════════════════
    // Render
    // ═══════════════════════════════════════════════════════════════════════
    return (
        <div style={{ padding: '20px', maxWidth: '800px', margin: '0 auto' }}>
            <Win98Box title={`Profile: ${handle}`}>
                {loading ? (
                    <div style={{ padding: '40px', textAlign: 'center' }}>Loading user profile...</div>
                ) : error ? (
                    <div style={{ padding: '40px', color: 'red' }}>{error}</div>
                ) : profile ? (
                    <div style={{ padding: '20px' }}>
                        <div style={{ display: 'flex', gap: '20px', marginBottom: '20px' }}>
                            <div style={{ width: 80, height: 80, borderRadius: '50%', backgroundColor: 'var(--bg-tertiary)', overflow: 'hidden' }}>
                                {profile.avatar_url ? (
                                    <img src={profile.avatar_url} alt="Avatar" style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
                                ) : (
                                    <div style={{ width: '100%', height: '100%', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: '2rem' }}>
                                        {profile.handle.charAt(0).toUpperCase()}
                                    </div>
                                )}
                            </div>
                            <div style={{ flex: 1 }}>
                                <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                                    <h2 style={{ margin: 0 }}>{profile.display_name || profile.handle}</h2>
                                    <span style={{ 
                                        padding: '2px 8px', 
                                        borderRadius: '12px', 
                                        fontSize: '0.8rem', 
                                        background: isBluesky ? '#1185fe' : '#10b981', 
                                        color: '#fff' 
                                    }}>
                                        {isBluesky ? '🦋 Bluesky' : '🌐 Fediverse'}
                                    </span>
                                </div>
                                <div style={{ color: 'var(--text-muted)', marginBottom: "10px" }}>@{profile.handle}</div>
                                {profile.description && (
                                    <div style={{ marginBottom: "10px", lineHeight: "1.4" }} dangerouslySetInnerHTML={{ __html: profile.description }}></div>
                                )}
                                <div style={{ display: 'flex', gap: '15px', color: 'var(--text-secondary)' }}>
                                    {profile.metrics && (
                                        <>
                                            <div><strong>{profile.metrics.followers}</strong> Followers</div>
                                            <div><strong>{profile.metrics.following}</strong> Following</div>
                                            <div><strong>{profile.metrics.posts}</strong> Posts</div>
                                        </>
                                    )}
                                </div>
                            </div>
                        </div>
                        
                        <hr style={{ border: 'none', borderTop: '1px solid var(--border-color)', margin: '20px 0' }} />
                        
                        <h3 style={{ marginTop: 0 }}>Recent Posts</h3>
                        {posts.length > 0 ? (
                            <div style={{ display: 'flex', flexDirection: 'column', gap: '20px' }}>
                                {posts.map(post => (
                                    <FeedEntryCard 
                                        key={post.id} 
                                        entry={post} 
                                    />
                                ))}
                            </div>
                        ) : (
                            <div style={{ padding: '20px', textAlign: 'center', color: 'var(--text-muted)' }}>
                                No recent posts.
                            </div>
                        )}
                    </div>
                ) : null}
            </Win98Box>
        </div>
    );
}

