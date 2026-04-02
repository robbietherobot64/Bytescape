import { useState, useEffect, useCallback } from 'react';
import { BsSearch, BsPlusCircle, BsChevronRight, BsChevronDown, BsLock } from 'react-icons/bs';
import { useTopicTree, useGroupSearch, isGroupJoined, isGroupPending } from '../hooks/useGroupsData';
import { TopicNode, GroupSummary } from '../types';
import './ForumScreen.css';

/**
 * PURPOSE
 * Provides a navigable forum directory with topic hierarchy, group discovery,
 * and topic subscription controls.
 *
 * HOW IT WORKS
 * - Reads topic tree and topic subscription actions from `useTopicTree()`.
 * - Runs query-driven group search via `useGroupSearch()`.
 * - Persists expanded topic IDs in localStorage for stable navigation state.
 * - Renders recursive topic nodes and group rows with membership badges.
 *
 * RELATED FILES
 * - src/hooks/useGroupsData.ts (topic tree + search data layer)
 * - src/types/index.ts (TopicNode / GroupSummary contracts)
 * - src/screens/GroupScreen.tsx (group detail view opened from this screen)
 *
 * CROSS-SYSTEM NOTES
 * - Provider/store/invoke interactions are encapsulated inside `useGroupsData`.
 *   This screen delegates network/state effects to hooks and remains a view layer.
 */

interface ForumScreenProps {
    onOpenGroup: (groupId: string) => void;
    onCreateGroup: () => void;
}

/**
 * Top-level forum browser entry screen.
 */
export function ForumScreen({ onOpenGroup, onCreateGroup }: ForumScreenProps) {
    // ═══════════════════════════════════════════════════════════════════════
    // Data hooks and persisted UI state
    // ═══════════════════════════════════════════════════════════════════════
    const { topics, loading, subscribeTopic, unsubscribeTopic } = useTopicTree();
    const { results: searchResults, searching, search } = useGroupSearch();
    const [searchQuery, setSearchQuery] = useState('');
    const [expandedTopics, setExpandedTopics] = useState<Set<string>>(() => {
        try {
            const stored = localStorage.getItem('bytescape:forum-expanded');
            return stored ? new Set(JSON.parse(stored)) : new Set<string>();
        } catch { return new Set<string>(); }
    });

    // Persist expansion state
    useEffect(() => {
        localStorage.setItem('bytescape:forum-expanded', JSON.stringify([...expandedTopics]));
    }, [expandedTopics]);

    const toggleExpanded = useCallback((topicId: string) => {
        setExpandedTopics(prev => {
            const next = new Set(prev);
            if (next.has(topicId)) next.delete(topicId);
            else next.add(topicId);
            return next;
        });
    }, []);

    const handleSearch = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
        const q = e.target.value;
        setSearchQuery(q);
        search(q);
    }, [search]);

    const handleSubscribeToggle = useCallback(async (topicId: string, currentlySubscribed: boolean) => {
        if (currentlySubscribed) {
            await unsubscribeTopic(topicId);
        } else {
            await subscribeTopic(topicId);
        }
    }, [subscribeTopic, unsubscribeTopic]);

    // ═══════════════════════════════════════════════════════════════════════
    // Render
    // ═══════════════════════════════════════════════════════════════════════
    return (
        <div className="forum-screen">
            <div className="forum-header">
                <h2 className="forum-title">📋 Forums</h2>
                <div className="forum-header-actions">
                    <div className="forum-search-bar">
                        <BsSearch className="forum-search-icon" />
                        <input
                            type="text"
                            className="forum-search-input"
                            placeholder="Search groups..."
                            value={searchQuery}
                            onChange={handleSearch}
                            aria-label="Search groups"
                            onKeyDown={e => { if (e.key === 'Escape') { setSearchQuery(''); search(''); } }}
                        />
                    </div>
                    <button className="forum-create-btn" onClick={onCreateGroup} title="Create Group" aria-label="Create a new group">
                        <BsPlusCircle />
                    </button>
                </div>
            </div>

            {/* Search results overlay */}
            {searchQuery.trim() && (
                <div className="forum-search-results">
                    {searching && <div className="forum-search-loading">Searching...</div>}
                    {!searching && searchResults.length === 0 && (
                        <div className="forum-search-empty">No groups found for &ldquo;{searchQuery}&rdquo;</div>
                    )}
                    {searchResults.map(group => (
                        <GroupListItem
                            key={group.group_id}
                            group={group}
                            onOpen={onOpenGroup}
                        />
                    ))}
                </div>
            )}

            {/* Topic tree */}
            <div className="forum-tree" role="tree" aria-label="Forum topics">
                {loading ? (
                    <div className="forum-loading">
                        <span className="forum-spinner" />
                        Loading forums...
                    </div>
                ) : (
                    topics.map(topic => (
                        <TopicTreeNode
                            key={topic.id}
                            topic={topic}
                            depth={0}
                            expanded={expandedTopics}
                            onToggleExpand={toggleExpanded}
                            onSubscribeToggle={handleSubscribeToggle}
                            onOpenGroup={onOpenGroup}
                        />
                    ))
                )}
            </div>

            {/* Create group footer */}
            <div className="forum-footer">
                <button className="forum-create-group-btn" onClick={onCreateGroup}>
                    <BsPlusCircle /> Create Group
                </button>
            </div>
        </div>
    );
}

// ─── TopicTreeNode ───

interface TopicTreeNodeProps {
    topic: TopicNode;
    depth: number;
    expanded: Set<string>;
    onToggleExpand: (topicId: string) => void;
    onSubscribeToggle: (topicId: string, subscribed: boolean) => void;
    onOpenGroup: (groupId: string) => void;
}

/**
 * Recursive topic node renderer for hierarchical forum taxonomy.
 */
function TopicTreeNode({ topic, depth, expanded, onToggleExpand, onSubscribeToggle, onOpenGroup }: TopicTreeNodeProps) {
    const hasChildren = topic.children.length > 0 || topic.groups.length > 0;
    const isExpanded = expanded.has(topic.id);

    const nodeStyle: React.CSSProperties = {
        paddingLeft: `${20 * depth + 12}px`,
    };

    return (
        <div className="topic-node" role="treeitem" aria-expanded={hasChildren ? isExpanded : undefined}>
            <div
                className={`topic-row ${depth % 2 === 1 ? 'topic-row-alt' : ''}`}
                style={nodeStyle}
            >
                {hasChildren ? (
                    <button
                        className="topic-expand-btn"
                        onClick={() => onToggleExpand(topic.id)}
                        aria-expanded={isExpanded}
                        aria-label={`${isExpanded ? 'Collapse' : 'Expand'} ${topic.name}`}
                    >
                        {isExpanded ? <BsChevronDown /> : <BsChevronRight />}
                    </button>
                ) : (
                    <span className="topic-expand-spacer" />
                )}
                <span className="topic-icon">{topic.icon || '📁'}</span>
                <span className="topic-name">{topic.name}</span>
                <button
                    className={`topic-subscribe-btn ${topic.subscribed ? 'subscribed' : ''}`}
                    onClick={() => onSubscribeToggle(topic.id, topic.subscribed)}
                    aria-label={`${topic.subscribed ? 'Unsubscribe from' : 'Subscribe to'} ${topic.name}`}
                >
                    {topic.subscribed ? 'Subscribed ✓' : 'Subscribe'}
                </button>
            </div>

            {isExpanded && (
                <div className="topic-children" role="group">
                    {topic.children.map(child => (
                        <TopicTreeNode
                            key={child.id}
                            topic={child}
                            depth={depth + 1}
                            expanded={expanded}
                            onToggleExpand={onToggleExpand}
                            onSubscribeToggle={onSubscribeToggle}
                            onOpenGroup={onOpenGroup}
                        />
                    ))}
                    {topic.groups.map(group => (
                        <GroupListItem
                            key={group.group_id}
                            group={group}
                            depth={depth + 1}
                            onOpen={onOpenGroup}
                        />
                    ))}
                </div>
            )}
        </div>
    );
}

// ─── GroupListItem ───

interface GroupListItemProps {
    group: GroupSummary;
    depth?: number;
    onOpen: (groupId: string) => void;
}

/**
 * Group row renderer used by both search results and expanded topic branches.
 */
function GroupListItem({ group, depth = 0, onOpen }: GroupListItemProps) {
    const joined = isGroupJoined(group.group_id);
    const pending = isGroupPending(group.group_id);

    const itemStyle: React.CSSProperties = {
        paddingLeft: `${20 * (depth) + 32}px`,
    };

    return (
        <div className="group-list-item" style={itemStyle}>
            <button className="group-name-btn" onClick={() => onOpen(group.group_id)} aria-label={`Open group ${group.name}`}>
                {group.visibility === 'Closed' && <BsLock className="group-lock-icon" />}
                {group.name}
                <span className="group-member-count">({group.member_count})</span>
            </button>
            <span className="group-join-status">
                {joined ? (
                    <span className="group-joined-badge">Joined ✓</span>
                ) : pending ? (
                    <span className="group-pending-badge">Pending ⏳</span>
                ) : (
                    <span className="group-join-badge">Join</span>
                )}
            </span>
        </div>
    );
}
