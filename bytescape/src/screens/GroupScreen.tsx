import { useState, useMemo, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { BsArrowLeft, BsLock, BsShieldLock, BsGlobe, BsPencilSquare, BsPersonFill, BsShieldCheck, BsStar, BsThreeDotsVertical, BsPin } from 'react-icons/bs';
import { useGroupDetail } from '../hooks/useGroupsData';
import { FeedItemRenderer } from '../components/lenses/FeedItemRenderer';
import { GroupMember, MemberRole } from '../types';
import './GroupScreen.css';

interface GroupScreenProps {
    groupId: string;
    onBack: () => void;
}

type GroupTab = 'posts' | 'about' | 'members';

export function GroupScreen({ groupId, onBack }: GroupScreenProps) {
    const { group, members, posts, loading, membership, myRole, joinGroup, leaveGroup, refresh } = useGroupDetail(groupId);
    const [activeTab, setActiveTab] = useState<GroupTab>('posts');

    const visibilityIcon = useMemo(() => {
        if (!group) return null;
        switch (group.visibility) {
            case 'Public': return <BsGlobe className="group-vis-icon" />;
            case 'Closed': return <BsLock className="group-vis-icon" />;
            case 'Private': return <BsShieldLock className="group-vis-icon" />;
        }
    }, [group]);

    const isModerator = myRole === 'Owner' || myRole === 'Moderator';
    const pendingCount = members.filter(m => m.status === 'Pending').length;

    if (loading || !group) {
        return (
            <div className="group-screen">
                <div className="group-back-bar">
                    <button className="group-back-btn" onClick={onBack}>
                        <BsArrowLeft /> Back to Forums
                    </button>
                </div>
                <div className="group-loading">
                    {loading ? (
                        <><span className="group-spinner" /> Loading group...</>
                    ) : 'Group not found'}
                </div>
            </div>
        );
    }

    return (
        <div className="group-screen">
            <div className="group-back-bar">
                <button className="group-back-btn" onClick={onBack} aria-label="Back to Forums">
                    <BsArrowLeft /> Back to Forums
                </button>
            </div>

            {/* Header */}
            <div className="group-header">
                {group.banner_blob && (
                    <div className="group-banner" style={{ backgroundImage: `url(${group.banner_blob})` }} />
                )}
                <div className="group-header-info">
                    <h2 className="group-name">{group.name}</h2>
                    <div className="group-meta">
                        {visibilityIcon}
                        <span className="group-visibility-label">{group.visibility}</span>
                        <span className="group-meta-sep">·</span>
                        <span className="group-member-count">{group.member_count} members</span>
                        {myRole && (
                            <>
                                <span className="group-meta-sep">·</span>
                                <span className="group-my-role">
                                    {myRole === 'Owner' ? '👑' : myRole === 'Moderator' ? '🛡️' : ''} {myRole}
                                </span>
                            </>
                        )}
                    </div>
                    <div className="group-header-actions">
                        {membership === 'member' ? (
                            <button className="group-action-btn group-leave-btn" onClick={leaveGroup} aria-label={`Leave ${group.name}`}>
                                Joined ✓
                            </button>
                        ) : membership === 'pending' ? (
                            <button className="group-action-btn group-pending-btn" disabled aria-label="Join request pending">
                                Pending ⏳
                            </button>
                        ) : (
                            <button className="group-action-btn group-join-btn" onClick={joinGroup} aria-label={`Join ${group.name}`}>
                                Join
                            </button>
                        )}
                    </div>
                </div>
            </div>

            {/* Tabs */}
            <div className="group-tabs">
                {(['posts', 'about', 'members'] as GroupTab[]).map(tab => (
                    <button
                        key={tab}
                        className={`group-tab ${activeTab === tab ? 'active' : ''}`}
                        onClick={() => setActiveTab(tab)}
                    >
                        {tab === 'posts' && 'Posts'}
                        {tab === 'about' && 'About'}
                        {tab === 'members' && `Members (${members.length})`}
                        {tab === 'members' && isModerator && pendingCount > 0 && (
                            <span className="pending-badge">{pendingCount}</span>
                        )}
                    </button>
                ))}
            </div>

            {/* Tab content */}
            <div className="group-tab-content">
                {activeTab === 'posts' && (
                    <GroupPostsTab
                        groupId={groupId}
                        posts={posts}
                        membership={membership}
                        isModerator={isModerator}
                        onRefresh={refresh}
                    />
                )}
                {activeTab === 'about' && (
                    <GroupAboutTab description={group.description} rules={group.rules} />
                )}
                {activeTab === 'members' && (
                    <GroupMembersTab
                        groupId={groupId}
                        members={members}
                        isModerator={isModerator}
                        isOwner={myRole === 'Owner'}
                        visibility={group.visibility}
                        onRefresh={refresh}
                    />
                )}
            </div>
        </div>
    );
}

// ─── Posts Tab ───

interface GroupPostsTabProps {
    groupId: string;
    posts: any[];
    membership: string;
    isModerator: boolean;
    onRefresh: () => void;
}

function GroupPostsTab({ groupId, posts, membership, isModerator, onRefresh }: GroupPostsTabProps) {
    const [actionMenuOpen, setActionMenuOpen] = useState<string | null>(null);

    const handlePin = async (itemId: string) => {
        try {
            await invoke('pin_group_post', { groupId, itemId });
            setActionMenuOpen(null);
            onRefresh();
        } catch (e) { console.error('Failed to pin:', e); }
    };

    const handleUnpin = async (itemId: string) => {
        try {
            await invoke('unpin_group_post', { groupId, itemId });
            setActionMenuOpen(null);
            onRefresh();
        } catch (e) { console.error('Failed to unpin:', e); }
    };

    const handleRemovePost = async (itemId: string) => {
        if (!confirm('Remove this post from the group?')) return;
        try {
            await invoke('remove_group_post', { groupId, itemId });
            setActionMenuOpen(null);
            onRefresh();
        } catch (e) { console.error('Failed to remove post:', e); }
    };

    return (
        <div className="group-posts-tab">
            {membership === 'member' && (
                <div className="group-new-post-bar">
                    <button className="group-new-post-btn">
                        <BsPencilSquare /> New Post
                    </button>
                </div>
            )}
            {posts.length === 0 ? (
                <div className="group-empty-state">
                    <p>No posts yet. Be the first to share something!</p>
                </div>
            ) : (
                <div className="group-posts-list">
                    {posts.map((entry: any) => {
                        const itemId = entry.id || entry.item_id;
                        return (
                            <div key={itemId} className="group-post-wrapper">
                                {entry.pinned && (
                                    <div className="group-pinned-badge"><BsPin /> Pinned</div>
                                )}
                                <FeedItemRenderer entry={entry} />
                                {isModerator && (
                                    <div className="group-post-mod-actions">
                                        <button
                                            className="group-mod-menu-btn"
                                            onClick={() => setActionMenuOpen(actionMenuOpen === itemId ? null : itemId)}
                                            aria-label="Post moderation"
                                        >
                                            <BsThreeDotsVertical />
                                        </button>
                                        {actionMenuOpen === itemId && (
                                            <div className="group-mod-menu">
                                                {entry.pinned ? (
                                                    <button onClick={() => handleUnpin(itemId)}>
                                                        <BsPin /> Unpin
                                                    </button>
                                                ) : (
                                                    <button onClick={() => handlePin(itemId)}>
                                                        <BsPin /> Pin
                                                    </button>
                                                )}
                                                <button className="group-mod-danger" onClick={() => handleRemovePost(itemId)}>
                                                    🗑️ Remove
                                                </button>
                                            </div>
                                        )}
                                    </div>
                                )}
                            </div>
                        );
                    })}
                </div>
            )}
        </div>
    );
}

// ─── About Tab ───

function GroupAboutTab({ description, rules }: { description: string; rules: string | null }) {
    return (
        <div className="group-about-tab">
            <div className="group-about-section">
                <h3 className="group-about-heading">Description</h3>
                <p className="group-about-text">{description}</p>
            </div>
            {rules && (
                <div className="group-about-section">
                    <h3 className="group-about-heading">Rules</h3>
                    <div className="group-rules-list">
                        {rules.split('\n').filter(Boolean).map((rule, i) => (
                            <div key={i} className="group-rule">{rule}</div>
                        ))}
                    </div>
                </div>
            )}
        </div>
    );
}

// ─── Members Tab ───

interface GroupMembersTabProps {
    groupId: string;
    members: GroupMember[];
    isModerator: boolean;
    isOwner: boolean;
    visibility: string;
    onRefresh: () => void;
}

function GroupMembersTab({ groupId, members, isModerator, isOwner, visibility, onRefresh }: GroupMembersTabProps) {
    const [actionMenuOpen, setActionMenuOpen] = useState<string | null>(null);
    const [inviteInput, setInviteInput] = useState('');
    const [inviteStatus, setInviteStatus] = useState<string | null>(null);

    const activeMembers = members.filter(m => m.status === 'Active');
    const pendingMembers = members.filter(m => m.status === 'Pending');
    const bannedMembers = members.filter(m => m.status === 'Banned');

    const handleApprove = useCallback(async (memberDid: string) => {
        try {
            await invoke('approve_member', { groupId, memberDid });
            setActionMenuOpen(null);
            onRefresh();
        } catch (e) { console.error('Failed to approve:', e); }
    }, [groupId, onRefresh]);

    const handleReject = useCallback(async (memberDid: string) => {
        try {
            await invoke('reject_member', { groupId, memberDid });
            setActionMenuOpen(null);
            onRefresh();
        } catch (e) { console.error('Failed to reject:', e); }
    }, [groupId, onRefresh]);

    const handleBan = useCallback(async (memberDid: string) => {
        if (!confirm('Ban this member? They will not be able to rejoin.')) return;
        try {
            await invoke('ban_member', { groupId, memberDid });
            setActionMenuOpen(null);
            onRefresh();
        } catch (e) { console.error('Failed to ban:', e); }
    }, [groupId, onRefresh]);

    const handleSetRole = useCallback(async (memberDid: string, role: MemberRole) => {
        try {
            await invoke('set_member_role', { groupId, memberDid, role });
            setActionMenuOpen(null);
            onRefresh();
        } catch (e) { console.error('Failed to set role:', e); }
    }, [groupId, onRefresh]);

    const handleInvite = useCallback(async () => {
        const did = inviteInput.trim();
        if (!did) return;
        setInviteStatus(null);
        try {
            await invoke('invite_member', { groupId, memberDid: did });
            setInviteInput('');
            setInviteStatus('Invitation sent!');
            onRefresh();
        } catch (e) {
            setInviteStatus(typeof e === 'string' ? e : 'Failed to send invitation');
        }
    }, [groupId, inviteInput, onRefresh]);

    const roleIcon = (role: MemberRole) => {
        switch (role) {
            case 'Owner': return <BsStar className="member-role-icon owner" />;
            case 'Moderator': return <BsShieldCheck className="member-role-icon mod" />;
            case 'Member': return <BsPersonFill className="member-role-icon" />;
        }
    };

    return (
        <div className="group-members-tab">
            {/* Invite (Private groups, Owner/Mod only) */}
            {isModerator && visibility === 'Private' && (
                <div className="group-invite-section">
                    <h4 className="group-section-heading">Invite Member</h4>
                    <div className="group-invite-row">
                        <input
                            type="text"
                            placeholder="Enter DID or handle..."
                            value={inviteInput}
                            onChange={e => setInviteInput(e.target.value)}
                            onKeyDown={e => e.key === 'Enter' && handleInvite()}
                            className="group-invite-input"
                        />
                        <button className="group-invite-btn" onClick={handleInvite} disabled={!inviteInput.trim()}>
                            Invite
                        </button>
                    </div>
                    {inviteStatus && <div className="group-invite-status">{inviteStatus}</div>}
                </div>
            )}

            {/* Pending (Owner/Mod only) */}
            {isModerator && pendingMembers.length > 0 && (
                <div className="group-members-section">
                    <h4 className="group-section-heading">Pending ({pendingMembers.length})</h4>
                    {pendingMembers.map(member => (
                        <div key={member.member_did} className="group-member-row pending">
                            {roleIcon(member.role)}
                            <span className="member-did">{member.member_did}</span>
                            <div className="member-pending-actions">
                                <button className="member-approve-btn" onClick={() => handleApprove(member.member_did)} aria-label="Approve">✅</button>
                                <button className="member-reject-btn" onClick={() => handleReject(member.member_did)} aria-label="Reject">❌</button>
                            </div>
                        </div>
                    ))}
                </div>
            )}

            {/* Active members */}
            <div className="group-members-section">
                <h4 className="group-section-heading">Active ({activeMembers.length})</h4>
                {activeMembers.length === 0 ? (
                    <div className="group-empty-state"><p>No members yet.</p></div>
                ) : (
                    <div className="group-members-list">
                        {activeMembers.map(member => (
                            <div key={member.member_did} className="group-member-row">
                                {roleIcon(member.role)}
                                <span className="member-did">{member.member_did}</span>
                                <span className="member-role-label">{member.role}</span>
                                {isModerator && member.role !== 'Owner' && (
                                    <div className="member-action-menu-container">
                                        <button
                                            className="member-action-btn"
                                            onClick={() => setActionMenuOpen(actionMenuOpen === member.member_did ? null : member.member_did)}
                                            aria-label="Member actions"
                                        >
                                            <BsThreeDotsVertical />
                                        </button>
                                        {actionMenuOpen === member.member_did && (
                                            <div className="member-action-menu">
                                                {isOwner && member.role === 'Member' && (
                                                    <button onClick={() => handleSetRole(member.member_did, 'Moderator')}>🛡️ Promote to Mod</button>
                                                )}
                                                {isOwner && member.role === 'Moderator' && (
                                                    <button onClick={() => handleSetRole(member.member_did, 'Member')}>👤 Demote to Member</button>
                                                )}
                                                <button className="member-action-danger" onClick={() => handleBan(member.member_did)}>🚫 Ban</button>
                                            </div>
                                        )}
                                    </div>
                                )}
                            </div>
                        ))}
                    </div>
                )}
            </div>

            {/* Banned (Owner/Mod only) */}
            {isModerator && bannedMembers.length > 0 && (
                <div className="group-members-section">
                    <h4 className="group-section-heading">Banned ({bannedMembers.length})</h4>
                    {bannedMembers.map(member => (
                        <div key={member.member_did} className="group-member-row banned">
                            <span className="member-did">{member.member_did}</span>
                            <span className="member-role-label banned-label">Banned</span>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}
