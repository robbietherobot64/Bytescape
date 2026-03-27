import { useState, useMemo } from 'react';
import { BsArrowLeft, BsLock, BsShieldLock, BsGlobe, BsPencilSquare, BsPersonFill, BsShieldCheck, BsStar } from 'react-icons/bs';
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
    const { group, members, posts, loading, membership, joinGroup, leaveGroup } = useGroupDetail(groupId);
    const [activeTab, setActiveTab] = useState<GroupTab>('posts');

    const visibilityIcon = useMemo(() => {
        if (!group) return null;
        switch (group.visibility) {
            case 'Public': return <BsGlobe className="group-vis-icon" />;
            case 'Closed': return <BsLock className="group-vis-icon" />;
            case 'Private': return <BsShieldLock className="group-vis-icon" />;
        }
    }, [group]);

    if (loading || !group) {
        return (
            <div className="group-screen">
                <div className="group-back-bar">
                    <button className="group-back-btn" onClick={onBack}>
                        <BsArrowLeft /> Back to Forums
                    </button>
                </div>
                <div className="group-loading">{loading ? 'Loading group...' : 'Group not found'}</div>
            </div>
        );
    }

    return (
        <div className="group-screen">
            <div className="group-back-bar">
                <button className="group-back-btn" onClick={onBack}>
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
                    </div>
                    <div className="group-header-actions">
                        {membership === 'member' ? (
                            <button className="group-action-btn group-leave-btn" onClick={leaveGroup}>
                                Joined ✓
                            </button>
                        ) : membership === 'pending' ? (
                            <button className="group-action-btn group-pending-btn" disabled>
                                Pending ⏳
                            </button>
                        ) : (
                            <button className="group-action-btn group-join-btn" onClick={joinGroup}>
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
                    </button>
                ))}
            </div>

            {/* Tab content */}
            <div className="group-tab-content">
                {activeTab === 'posts' && (
                    <GroupPostsTab posts={posts} membership={membership} />
                )}
                {activeTab === 'about' && (
                    <GroupAboutTab description={group.description} rules={group.rules} />
                )}
                {activeTab === 'members' && (
                    <GroupMembersTab members={members} />
                )}
            </div>
        </div>
    );
}

// ─── Posts Tab ───

function GroupPostsTab({ posts, membership }: { posts: any[]; membership: string }) {
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
                    {posts.map(entry => (
                        <FeedItemRenderer key={entry.id} entry={entry} />
                    ))}
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

function GroupMembersTab({ members }: { members: GroupMember[] }) {
    const roleIcon = (role: MemberRole) => {
        switch (role) {
            case 'Owner': return <BsStar className="member-role-icon owner" />;
            case 'Moderator': return <BsShieldCheck className="member-role-icon mod" />;
            case 'Member': return <BsPersonFill className="member-role-icon" />;
        }
    };

    return (
        <div className="group-members-tab">
            {members.length === 0 ? (
                <div className="group-empty-state">
                    <p>No members yet.</p>
                </div>
            ) : (
                <div className="group-members-list">
                    {members.map(member => (
                        <div key={member.member_did} className="group-member-row">
                            {roleIcon(member.role)}
                            <span className="member-did">{member.member_did}</span>
                            <span className="member-role-label">{member.role}</span>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
}
