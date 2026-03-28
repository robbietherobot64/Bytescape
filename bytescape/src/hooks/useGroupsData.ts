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

// ─── Backend availability flag ───
// Set to true once backend sprint v12 commands are registered in lib.rs.
// Until then, all functions return mock/seeded data so the UI can be developed.
const BACKEND_READY = true;

// ─── Mock / Seed Data ───

const MOCK_TOPIC_TREE: TopicNode[] = [
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

const MOCK_GROUPS: Record<string, LocalGroup> = {
    'grp-bytescape-general': {
        group_id: 'grp-bytescape-general', name: 'General Discussion', slug: 'general',
        description: 'The main Bytescape community group. Discuss anything related to the app, share tips, and connect with other users.',
        visibility: 'Public', topic_id: 'topic-bytescape', owner_did: 'did:plc:system',
        icon_blob: null, banner_blob: null,
        rules: '1. Be respectful\n2. No spam\n3. Stay on topic\n4. No self-promotion without context',
        posting_restricted_to_mods: false, member_count: 47,
        created_at: Date.now() - 86400000 * 30, updated_at: Date.now(),
    },
    'grp-rust-enthusiasts': {
        group_id: 'grp-rust-enthusiasts', name: 'Rust Enthusiasts', slug: 'rust-enthusiasts',
        description: 'A community for Rust programmers. Share crates, discuss patterns, and help newcomers learn the language.',
        visibility: 'Public', topic_id: 'topic-programming', owner_did: 'did:plc:system',
        icon_blob: null, banner_blob: null,
        rules: '1. Be helpful\n2. No flamewars about languages\n3. Share minimal reproducible examples when asking questions',
        posting_restricted_to_mods: false, member_count: 23,
        created_at: Date.now() - 86400000 * 20, updated_at: Date.now(),
    },
    'grp-nixos-tips': {
        group_id: 'grp-nixos-tips', name: 'NixOS Tips', slug: 'nixos-tips',
        description: 'Share NixOS configurations, flakes, and tips for declarative system management.',
        visibility: 'Public', topic_id: 'topic-linux', owner_did: 'did:plc:system',
        icon_blob: null, banner_blob: null, rules: null,
        posting_restricted_to_mods: false, member_count: 12,
        created_at: Date.now() - 86400000 * 15, updated_at: Date.now(),
    },
    'grp-opsec-club': {
        group_id: 'grp-opsec-club', name: 'OPSEC Club', slug: 'opsec-club',
        description: 'Operational security discussions. Closed group — request to join.',
        visibility: 'Closed', topic_id: 'topic-security', owner_did: 'did:plc:system',
        icon_blob: null, banner_blob: null,
        rules: '1. No sharing private info\n2. Verify claims with sources\n3. Chatham House Rules apply',
        posting_restricted_to_mods: false, member_count: 8,
        created_at: Date.now() - 86400000 * 10, updated_at: Date.now(),
    },
    'grp-mindfulness': {
        group_id: 'grp-mindfulness', name: 'Mindfulness Daily', slug: 'mindfulness-daily',
        description: 'A supportive space for mindfulness practice, meditation, and mental wellness.',
        visibility: 'Public', topic_id: 'topic-health', owner_did: 'did:plc:system',
        icon_blob: null, banner_blob: null,
        rules: '1. Be kind and supportive\n2. No medical advice\n3. Share resources freely',
        posting_restricted_to_mods: false, member_count: 42,
        created_at: Date.now() - 86400000 * 25, updated_at: Date.now(),
    },
};

// Track which groups the user has joined (mock state)
const MOCK_JOINED_GROUPS = new Set<string>(['grp-bytescape-general', 'grp-rust-enthusiasts']);
const MOCK_PENDING_GROUPS = new Set<string>();

const MOCK_MEMBERS: Record<string, GroupMember[]> = {
    'grp-bytescape-general': [
        { group_id: 'grp-bytescape-general', member_did: 'did:plc:system', role: 'Owner', status: 'Active', joined_at: Date.now() - 86400000 * 30 },
        { group_id: 'grp-bytescape-general', member_did: 'did:plc:alice', role: 'Moderator', status: 'Active', joined_at: Date.now() - 86400000 * 25 },
        { group_id: 'grp-bytescape-general', member_did: 'did:plc:bob', role: 'Member', status: 'Active', joined_at: Date.now() - 86400000 * 10 },
    ],
};

// ─── Hook: useGroupsData ───

/** Topic tree data + operations */
export function useTopicTree() {
    const [topics, setTopics] = useState<TopicNode[]>([]);
    const [loading, setLoading] = useState(true);

    const refresh = useCallback(async () => {
        setLoading(true);
        try {
            if (BACKEND_READY) {
                const tree = await invoke<TopicNode[]>('get_topic_tree');
                setTopics(tree);
            } else {
                setTopics(MOCK_TOPIC_TREE);
            }
        } catch (err) {
            console.error('Failed to load topic tree:', err);
            setTopics(MOCK_TOPIC_TREE);
        }
        setLoading(false);
    }, []);

    useEffect(() => { refresh(); }, [refresh]);

    const subscribeTopic = useCallback(async (topicId: string) => {
        if (BACKEND_READY) {
            await invoke('subscribe_topic', { topicId });
        }
        setTopics(prev => toggleSubscription(prev, topicId, true));
    }, []);

    const unsubscribeTopic = useCallback(async (topicId: string) => {
        if (BACKEND_READY) {
            await invoke('unsubscribe_topic', { topicId });
        }
        setTopics(prev => toggleSubscription(prev, topicId, false));
    }, []);

    return { topics, loading, refresh, subscribeTopic, unsubscribeTopic };
}

/** Group detail data + operations */
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
            if (BACKEND_READY) {
                const [g, m, p] = await Promise.all([
                    invoke<LocalGroup>('get_group_cmd', { groupId }),
                    invoke<GroupMember[]>('get_group_members_cmd', { groupId }),
                    invoke<FeedEntry[]>('get_group_feed', { groupId, limit: 50 }),
                ]);
                setGroup(g);
                setMembers(m);
                setPosts(p);

                // Determine current user's membership and role from the members list.
                // The backend returns the current user's DID — we match by checking
                // if any member's status is Active/Pending and try to find ourselves.
                // Since we don't have the DID here directly, we rely on the backend
                // to include us in the members list or use a separate call.
                try {
                    const myInfo = await invoke<{ did: string }>('check_auth_status');
                    const myMember = m.find(member => member.member_did === myInfo.did);
                    if (myMember) {
                        if (myMember.status === 'Active') {
                            setMembership('member');
                            setMyRole(myMember.role);
                        } else if (myMember.status === 'Pending') {
                            setMembership('pending');
                            setMyRole(null);
                        } else {
                            setMembership('none');
                            setMyRole(null);
                        }
                    } else {
                        setMembership('none');
                        setMyRole(null);
                    }
                } catch {
                    // Auth check failed — treat as non-member
                    setMembership('none');
                    setMyRole(null);
                }
            } else {
                setGroup(MOCK_GROUPS[groupId] || null);
                setMembers(MOCK_MEMBERS[groupId] || []);
                setPosts([]);
                if (MOCK_JOINED_GROUPS.has(groupId)) setMembership('member');
                else if (MOCK_PENDING_GROUPS.has(groupId)) setMembership('pending');
                else setMembership('none');
            }
        } catch (err) {
            console.error('Failed to load group:', err);
            setGroup(MOCK_GROUPS[groupId] || null);
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
        if (BACKEND_READY) {
            await invoke('join_group', { groupId });
        }
        if (group.visibility === 'Closed') {
            MOCK_PENDING_GROUPS.add(groupId);
            setMembership('pending');
        } else {
            MOCK_JOINED_GROUPS.add(groupId);
            setMembership('member');
        }
        await refresh();
    }, [groupId, group, refresh]);

    const leaveGroup = useCallback(async () => {
        if (!groupId) return;
        if (BACKEND_READY) {
            await invoke('leave_group', { groupId });
        }
        MOCK_JOINED_GROUPS.delete(groupId);
        MOCK_PENDING_GROUPS.delete(groupId);
        setMembership('none');
        setMyRole(null);
    }, [groupId]);

    return { group, members, posts, loading, membership, myRole, joinGroup, leaveGroup, refresh };
}

/** Search groups */
export function useGroupSearch() {
    const [results, setResults] = useState<GroupSummary[]>([]);
    const [searching, setSearching] = useState(false);

    const search = useCallback(async (query: string) => {
        if (!query.trim()) { setResults([]); return; }
        setSearching(true);
        try {
            if (BACKEND_READY) {
                const r = await invoke<GroupSummary[]>('search_groups_cmd', { query });
                setResults(r);
            } else {
                const q = query.toLowerCase();
                const allGroups = Object.values(MOCK_GROUPS);
                setResults(
                    allGroups
                        .filter(g => g.name.toLowerCase().includes(q) || g.slug.includes(q))
                        .map(g => ({ group_id: g.group_id, name: g.name, slug: g.slug, visibility: g.visibility, member_count: g.member_count, icon_blob: g.icon_blob }))
                );
            }
        } catch {
            setResults([]);
        }
        setSearching(false);
    }, []);

    return { results, searching, search };
}

/** Create group */
export async function createGroup(params: {
    name: string;
    slug: string;
    description: string;
    visibility: GroupVisibility;
    topicId: string;
    rules?: string;
}): Promise<string> {
    if (BACKEND_READY) {
        return invoke<string>('create_group', params);
    }
    // Mock: create a local group entry
    const groupId = `grp-${params.slug}`;
    MOCK_GROUPS[groupId] = {
        group_id: groupId,
        name: params.name,
        slug: params.slug,
        description: params.description,
        visibility: params.visibility,
        topic_id: params.topicId,
        owner_did: 'did:plc:self',
        icon_blob: null,
        banner_blob: null,
        rules: params.rules || null,
        posting_restricted_to_mods: false,
        member_count: 1,
        created_at: Date.now(),
        updated_at: Date.now(),
    };
    MOCK_JOINED_GROUPS.add(groupId);
    return groupId;
}

/** Check if user has joined a group */
export function isGroupJoined(groupId: string): boolean {
    return MOCK_JOINED_GROUPS.has(groupId);
}

/** Check if join request is pending */
export function isGroupPending(groupId: string): boolean {
    return MOCK_PENDING_GROUPS.has(groupId);
}

/** Get list of all groups the user has joined */
export function getJoinedGroupIds(): string[] {
    return Array.from(MOCK_JOINED_GROUPS);
}

/** Get all mock groups (for share-to-group picker) */
export function getJoinedGroups(): LocalGroup[] {
    return Array.from(MOCK_JOINED_GROUPS)
        .map(id => MOCK_GROUPS[id])
        .filter((g): g is LocalGroup => !!g);
}

/** Flatten all topics for a dropdown picker */
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

// ─── Helpers ───

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
