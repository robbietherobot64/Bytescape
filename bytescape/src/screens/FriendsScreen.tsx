import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { AuthStatus, ProfileView } from '../types';

interface FriendsScreenProps {
    auth: AuthStatus | null;
}

export function FriendsScreen({ auth }: FriendsScreenProps) {
    const [subview, setSubview] = useState<'followers' | 'following'>('following');
    const [people, setPeople] = useState<ProfileView[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (auth) {
            fetchPeople();
        }
    }, [auth, subview]);

    const fetchPeople = async () => {
        if (!auth) return;
        setLoading(true);
        setError(null);
        try {
            const result = subview === 'following'
                ? await invoke<ProfileView[]>('list_follows', { actor: auth.did })
                : await invoke<ProfileView[]>('list_followers', { actor: auth.did });
            setPeople(result);
        } catch (e: any) {
            console.error("Failed to fetch friends:", e);
            setError(typeof e === 'string' ? e : "Failed to load list");
        } finally {
            setLoading(false);
        }
    };

    return (
        <div className="friends-screen">
            <div className="friends-sidebar">
                <h3>Social Graph</h3>
                <button
                    className={`nav-item ${subview === 'following' ? 'active' : ''}`}
                    onClick={() => setSubview('following')}
                >
                    Following
                </button>
                <button
                    className={`nav-item ${subview === 'followers' ? 'active' : ''}`}
                    onClick={() => setSubview('followers')}
                >
                    Followers
                </button>
            </div>

            <div className="friends-content">
                <div className="friends-header">
                    <h2>{subview === 'following' ? 'Following' : 'Followers'}</h2>
                    <span className="count-badge">{people.length} people</span>
                </div>

                {loading && <div className="loading-state">Loading your network...</div>}
                {error && <div className="error-state">{error}</div>}

                <div className="friends-list">
                    {people.map(person => (
                        <div key={person.did} className="friend-card">
                            <div className="friend-avatar">
                                {person.avatar ? (
                                    <img src={person.avatar} alt={person.handle} />
                                ) : (
                                    <div className="avatar-placeholder">
                                        {person.handle.substring(0, 1).toUpperCase()}
                                    </div>
                                )}
                            </div>
                            <div className="friend-info">
                                <div className="friend-name">
                                    {person.displayName || person.handle}
                                </div>
                                <div className="friend-handle">@{person.handle}</div>
                                {person.description && (
                                    <div className="friend-bio">{person.description}</div>
                                )}
                            </div>
                        </div>
                    ))}

                    {!loading && people.length === 0 && (
                        <div className="empty-state">
                            No one found in this list yet.
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
