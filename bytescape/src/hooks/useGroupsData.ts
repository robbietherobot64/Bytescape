import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
    TopicNode,
    GroupSummary,
    LocalGroup,
    GroupMember,
    GroupVisibility,
    FeedEntry,
} from '../types';

/**
 * PURPOSE
 * -------
 * This module powers the Groups UI data layer. It exposes React hooks and helpers
 * for topic browsing, group details, membership actions, search, and create-group flow.
 *
 * HOW IT WORKS
 * ------------
 * Hooks call backend Tauri commands via `invoke(...)` for all operations.
 * On error, hooks return empty/default state and log to console.
 *
 * RELATED FILES
 * -------------
 * - src/screens/GroupsScreen.tsx (topic tree + discovery consumer)
 * - src/screens/GroupDetailScreen.tsx (group profile + membership consumer)
 * - src/types/index.ts (TopicNode/LocalGroup/GroupMember contracts)
 * - src-tauri/src/lib.rs (registered Tauri commands called via `invoke`)
 *
 * NOTE: Some exported helper functions (isGroupJoined, isGroupPending, etc.)
 * still use module-level Sets for quick synchronous membership checks in the
 * ForumScreen UI. These are updated by the hooks on successful backend calls.
 */

// ═══════════════════════════════════════════════════════════════════════════════
// Module-level membership tracking (synchronous helpers for ForumScreen UI)
// ═══════════════════════════════════════════════════════════════════════════════

// These Sets are populated by useGroupDetail's refresh() on successful backend
// calls and consumed by synchronous helper exports (isGroupJoined, etc.).
const _joinedGroups = new Set<string>();
const _pendingGroups = new Set<string>();

// ─── Seed Data (used only as empty-state fallback when backend returns nothing) ───

const _SEED_TOPIC_TREE: TopicNode[] = [
    {
        id: 'topic-fediverse', name: 'Fediverse', slug: 'fediverse', parent_id: null,
        description: 'All things federation', icon: '🌐', sort_order: 0,
        created_by: 'system', created_at: Date.now(), subscribed: false,
        children: [
            {
                id: 'topic-bytescape', name: 'Bytescape', slug: 'bytescape', parent_id: 'topic-fediverse',
                description: 'Bytescape app community', icon: '🏠', sort_order: 0,
                created_by: 'system', created_at: Date.now(), subscribed: true,
                children: [
                    {
                        id: 'topic-announcements', name: 'Announcements', slug: 'announcements', parent_id: 'topic-bytescape',
                        description: 'Official announcements', icon: '📢', sort_order: 0,
                        created_by: 'system', created_at: Date.now(), subscribed: false,
                        children: [], groups: [], 
                    },
                    {
                        id: 'topic-feature-requests', name: 'Feature Requests', slug: 'feature-requests', parent_id: 'topic-bytescape',
                        description: 'Suggest new features', icon: '💡', sort_order: 1,
                        created_by: 'system', created_at: Date.now(), subscribed: false,
                        children: [], groups: [],
                    },
                    {
                        id: 'topic-bug-reports', name: 'Bug Reports', slug: 'bug-reports', parent_id: 'topic-bytescape',
                        description: 'Report bugs', icon: '🐛', sort_order: 2,
                        created_by: 'system', created_at: Date.now(), subscribed: false,
                        children: [], groups: [],
                    },
                ],
                groups: [
                    { group_id: 'grp-bytescape-general', name: 'General Discussion', slug: 'general', visibility: 'Public', member_count: 47, icon_blob: null },
                ],
            },
            {
                id: 'topic-lemmy', name: 'Lemmy', slug: 'lemmy', parent_id: 'topic-fediverse',
                description: 'Lemmy link aggregator', icon: '🐀', sort_order: 1,
                created_by: 'system', created_at: Date.now(), subscribed: false,
                children: [], groups: [],
            },
            {
                id: 'topic-mastodon', name: 'Mastodon', slug: 'mastodon', parent_id: 'topic-fediverse',
                description: 'Mastodon microblogging', icon: '🐘', sort_order: 2,
                created_by: 'system', created_at: Date.now(), subscribed: false,
                children: [], groups: [],
            },
        ],
        groups: [],
    },
    {
        id: 'topic-technology', name: 'Technology', slug: 'technology', parent_id: null,
        description: 'Tech discussions', icon: '💻', sort_order: 1,
        created_by: 'system', created_at: Date.now(), subscribed: false,
        children: [
            {
                id: 'topic-programming', name: 'Programming', slug: 'programming', parent_id: 'topic-technology',
                description: 'Software development', icon: '⌨️', sort_order: 0,
                created_by: 'system', created_at: Date.now(), subscribed: false,
                children: [], 
                groups: [
                    { group_id: 'grp-rust-enthusiasts', name: 'Rust Enthusiasts', slug: 'rust-enthusiasts', visibility: 'Public', member_count: 23, icon_blob: null },
                ],
            },
            {
                id: 'topic-linux', name: 'Linux & FOSS', slug: 'linux-foss', parent_id: 'topic-technology',
                description: 'Open source software', icon: '🐧', sort_order: 1,
                created_by: 'system', created_at: Date.now(), subscribed: false,
                children: [],
                groups: [
                    { group_id: 'grp-nixos-tips', name: 'NixOS Tips', slug: 'nixos-tips', visibility: 'Public', member_count: 12, icon_blob: null },
                ],
            },
            {
                id: 'topic-security', name: 'Security', slug: 'security', parent_id: 'topic-technology',
                description: 'Cybersecurity', icon: '🔐', sort_order: 2,
                created_by: 'system', created_at: Date.now(), subscribed: false,
                children: [],
                groups: [
                    { group_id: 'grp-opsec-club', name: 'OPSEC Club', slug: 'opsec-club', visibility: 'Closed', member_count: 8, icon_blob: null },
                ],
            },
        ],
        groups: [],
    },
    {
        id: 'topic-health', name: 'Health & Wellness', slug: 'health-wellness', parent_id: null,
        description: 'Physical and mental health', icon: '🏥', sort_order: 2,
        created_by: 'system', created_at: Date.now(), subscribed: false,
        children: [],
        groups: [
            { group_id: 'grp-mindfulness', name: 'Mindfulness Daily', slug: 'mindfulness-daily', visibility: 'Public', member_count: 42, icon_blob: null },
        ],
    },
    {
        id: 'topic-art', name: 'Art & Creative', slug: 'art-creative', parent_id: null,
        description: 'Art, design, and creativity', icon: '🎨', sort_order: 3,
        created_by: 'system', created_at: Date.now(), subscribed: false,
        children: [], groups: [],
    },
    {
        id: 'topic-gaming', name: 'Gaming', slug: 'gaming', parent_id: null,
        description: 'Video games', icon: '🎮', sort_order: 4,
        created_by: 'system', created_at: Date.now(), subscribed: false,
        children: [], groups: [],
    },
    {
        id: 'topic-science', name: 'Science', slug: 'science', parent_id: null,
        description: 'Scientific topics', icon: '🔬', sort_order: 5,
        created_by: 'system', created_at: Date.now(), subscribed: false,
        children: [], groups: [],
    },
    {
        id: 'topic-news', name: 'News & Politics', slug: 'news-politics', parent_id: null,
        description: 'News and political discussion', icon: '📰', sort_order: 6,
        created_by: 'system', created_at: Date.now(), subscribed: false,
        children: [], groups: [],
    },
];

// (Module-level membership tracking moved to top of file)

// ═══════════════════════════════════════════════════════════════════════════════
// Hooks
// ═══════════════════════════════════════════════════════════════════════════════

/**
 * Loads and manages the hierarchical topic tree used in Groups discovery.
 *
 * Cross-system note:
 * Uses Tauri command calls (`get_topic_tree`, `subscribe_topic`, `unsubscribe_topic`).
 * Falls back to seed topic tree if backend returns empty.
 */
export function useTopicTree() {
    const [topics, setTopics] = useState<TopicNode[]>([]);
    const [loading, setLoading] = useState(true);

    const refresh = useCallback(async () => {
        setLoading(true);
        try {
            const tree = await invoke<TopicNode[]>('get_topic_tree');
            setTopics(tree.length > 0 ? tree : _SEED_TOPIC_TREE);
        } catch (err) {
            console.error('Failed to load topic tree:', err);
            setTopics([]);
        }
        setLoading(false);
    }, []);

    useEffect(() => { refresh(); }, [refresh]);

    const subscribeTopic = useCallback(async (topicId: string) => {
        try {
            await invoke('subscribe_topic', { topicId });
        } catch (err) {
            console.error('subscribe_topic failed:', err);
        }
        setTopics(prev => toggleSubscription(prev, topicId, true));
    }, []);

    const unsubscribeTopic = useCallback(async (topicId: string) => {
        try {
            await invoke('unsubscribe_topic', { topicId });
        } catch (err) {
            console.error('unsubscribe_topic failed:', err);
        }
        setTopics(prev => toggleSubscription(prev, topicId, false));
    }, []);

    return { topics, loading, refresh, subscribeTopic, unsubscribeTopic };
}

/**
 * Loads one group's full detail model, members, feed posts, and membership state.
 *
 * Cross-system note:
 * Calls backend commands (`get_group_cmd`, `get_group_members_cmd`, `get_group_feed`,
 * `check_auth_status`, `join_group`, `leave_group`) through Tauri IPC via `invoke`.
 * Also updates module-level `_joinedGroups` / `_pendingGroups` sets for synchronous
 * membership checks used by ForumScreen.
 */
export function useGroupDetail(groupId: string | null) {
    const [group, setGroup] = useState<LocalGroup | null>(null);
    const [members, setMembers] = useState<GroupMember[]>([]);
    const [posts, setPosts] = useState<FeedEntry[]>([]);
    const [loading, setLoading] = useState(true);
    const [membership, setMembership] = useState<'none' | 'member' | 'pending'>('none');
    const [myRole, setMyRole] = useState<'Owner' | 'Moderator' | 'Member' | null>(null);

    const refresh = useCallback(async () => {
        if (!groupId) return;
        setLoading(true);
        try {
            const [g, m, p] = await Promise.all([
                invoke<LocalGroup>('get_group_cmd', { groupId }),
                invoke<GroupMember[]>('get_group_members_cmd', { groupId }),
                invoke<FeedEntry[]>('get_group_feed', { groupId, limit: 50 }),
            ]);
            setGroup(g);
            setMembers(m);
            setPosts(p);

            // Determine current user's membership and role from the members list.
            try {
                const myInfo = await invoke<{ did: string }>('check_auth_status');
                const myMember = m.find(member => member.member_did === myInfo.did);
                if (myMember) {
                    if (myMember.status === 'Active') {
                        setMembership('member');
                        setMyRole(myMember.role);
                        _joinedGroups.add(groupId);
                        _pendingGroups.delete(groupId);
                    } else if (myMember.status === 'Pending') {
                        setMembership('pending');
                        setMyRole(null);
                        _pendingGroups.add(groupId);
                        _joinedGroups.delete(groupId);
                    } else {
                        setMembership('none');
                        setMyRole(null);
                        _joinedGroups.delete(groupId);
                        _pendingGroups.delete(groupId);
                    }
                } else {
                    setMembership('none');
                    setMyRole(null);
                    _joinedGroups.delete(groupId);
                    _pendingGroups.delete(groupId);
                }
            } catch {
                // Auth check failed — treat as non-member
                setMembership('none');
                setMyRole(null);
            }
        } catch (err) {
            console.error('Failed to load group:', err);
            setGroup(null);
            setMembers([]);
            setPosts([]);
            setMembership('none');
            setMyRole(null);
        }
        setLoading(false);
    }, [groupId]);

    useEffect(() => { refresh(); }, [refresh]);

    const joinGroup = useCallback(async () => {
        if (!groupId || !group) return;
        try {
            await invoke('join_group', { groupId });
            if (group.visibility === 'Closed') {
                _pendingGroups.add(groupId);
                setMembership('pending');
            } else {
                _joinedGroups.add(groupId);
                setMembership('member');
            }
            await refresh();
        } catch (err) {
            console.error('join_group failed:', err);
        }
    }, [groupId, group, refresh]);

    const leaveGroup = useCallback(async () => {
        if (!groupId) return;
        try {
            await invoke('leave_group', { groupId });
            _joinedGroups.delete(groupId);
            _pendingGroups.delete(groupId);
            setMembership('none');
            setMyRole(null);
        } catch (err) {
            console.error('leave_group failed:', err);
        }
    }, [groupId]);

    return { group, members, posts, loading, membership, myRole, joinGroup, leaveGroup, refresh };
}

/**
 * Group search hook for query-driven discovery.
 *
 * Cross-system note:
 * Uses backend command `search_groups_cmd` via Tauri IPC.
 */
export function useGroupSearch() {
    const [results, setResults] = useState<GroupSummary[]>([]);
    const [searching, setSearching] = useState(false);

    const search = useCallback(async (query: string) => {
        if (!query.trim()) { setResults([]); return; }
        setSearching(true);
        try {
            const r = await invoke<GroupSummary[]>('search_groups_cmd', { query });
            setResults(r);
        } catch (err) {
            console.error('search_groups_cmd failed:', err);
            setResults([]);
        }
        setSearching(false);
    }, []);

    return { results, searching, search };
}

// ═══════════════════════════════════════════════════════════════════════════════
// Exported non-hook helpers
// ═══════════════════════════════════════════════════════════════════════════════

/**
 * Creates a group and returns its `group_id`.
 *
 * Cross-system note:
 * Uses backend command `create_group` via Tauri IPC.
 */
export async function createGroup(params: {
    name: string;
    slug: string;
    description: string;
    visibility: GroupVisibility;
    topicId: string;
    rules?: string;
}): Promise<string> {
    const groupId = await invoke<string>('create_group', params);
    _joinedGroups.add(groupId);
    return groupId;
}

/** Returns whether the current user is an active member of `groupId`. */
export function isGroupJoined(groupId: string): boolean {
    return _joinedGroups.has(groupId);
}

/** Returns whether `groupId` currently has a pending join request. */
export function isGroupPending(groupId: string): boolean {
    return _pendingGroups.has(groupId);
}

/** Returns all joined group IDs for quick membership checks. */
export function getJoinedGroupIds(): string[] {
    return Array.from(_joinedGroups);
}

/** Returns joined group IDs. Use with backend queries for full group records. */
export function getJoinedGroups(): LocalGroup[] {
    // Note: this returns an empty array since we no longer maintain a local
    // group record cache. Use invoke('get_group_cmd') for full records.
    return [];
}

/**
 * Flattens a nested topic tree into an indented list for dropdown/select controls.
 */
export function flattenTopics(nodes: TopicNode[], depth = 0): { id: string; name: string; depth: number }[] {
    const result: { id: string; name: string; depth: number }[] = [];
    for (const node of nodes) {
        result.push({ id: node.id, name: node.name, depth });
        if (node.children.length > 0) {
            result.push(...flattenTopics(node.children, depth + 1));
        }
    }
    return result;
}

// ═══════════════════════════════════════════════════════════════════════════════
// Internal helpers
// ═══════════════════════════════════════════════════════════════════════════════

function toggleSubscription(nodes: TopicNode[], topicId: string, subscribed: boolean): TopicNode[] {
    return nodes.map(node => {
        if (node.id === topicId) {
            return { ...node, subscribed };
        }
        if (node.children.length > 0) {
            return { ...node, children: toggleSubscription(node.children, topicId, subscribed) };
        }
        return node;
    });
}
