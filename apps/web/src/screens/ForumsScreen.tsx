import { useState, useEffect } from 'react';
import { forumsApi } from '../utils/api';

interface AuthorInfo {
  handle: string;
  display_name: string;
  avatar?: string;
}

interface TopicData {
  id: string;
  title: string;
  content: string;
  content_html?: string;
  tags: string[];
  community?: string;
  pinned: boolean;
  locked: boolean;
  reply_count: number;
  view_count: number;
  last_reply_at?: string;
  created_at: string;
  author: AuthorInfo;
}

interface ReplyData {
  id: string;
  topic_id: string;
  content: string;
  content_html?: string;
  parent_reply_id?: string;
  created_at: string;
  author: AuthorInfo;
}

const COMMUNITIES = [
  { id: '', label: 'All Communities', icon: '🌐' },
  { id: 'tech', label: 'Tech', icon: '💻' },
  { id: 'music', label: 'Music', icon: '🎵' },
  { id: 'gaming', label: 'Gaming', icon: '🎮' },
  { id: 'general', label: 'General', icon: '💬' },
  { id: 'art', label: 'Art', icon: '🎨' },
  { id: 'science', label: 'Science', icon: '🔬' },
  { id: 'sports', label: 'Sports', icon: '⚽' },
];

export default function ForumsScreen() {
  const [topics, setTopics] = useState<TopicData[]>([]);
  const [replies, setReplies] = useState<ReplyData[]>([]);
  const [selectedTopic, setSelectedTopic] = useState<TopicData | null>(null);
  const [community, setCommunity] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [replyText, setReplyText] = useState('');
  const [showComposer, setShowComposer] = useState(false);
  const [newTitle, setNewTitle] = useState('');
  const [newContent, setNewContent] = useState('');
  const [newCommunity, setNewCommunity] = useState('');
  const [newTags, setNewTags] = useState('');

  const loadTopics = async () => {
    setLoading(true);
    setError(null);
    try {
      const params = new URLSearchParams();
      if (community) params.set('community', community);
      params.set('limit', '50');
      params.set('offset', '0');
      const data = await forumsApi.listTopics(params.toString());
      setTopics(data.items);
    } catch (e: any) {
      setError(e.message || 'Failed to load topics');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadTopics();
  }, [community]);

  const loadReplies = async (topicId: string) => {
    try {
      const data = await forumsApi.listReplies(topicId, 'limit=50&offset=0');
      setReplies(data);
    } catch {
      setReplies([]);
    }
  };

  const selectTopic = async (topic: TopicData) => {
    setSelectedTopic(topic);
    setReplies([]);
    await loadReplies(topic.id);
  };

  const handleCreateTopic = async () => {
    if (!newTitle.trim() || !newContent.trim()) return;
    try {
      const tags = newTags.split(',').map(t => t.trim()).filter(Boolean);
      await forumsApi.createTopic({
        title: newTitle,
        content: newContent,
        community: newCommunity || undefined,
        tags: tags.length > 0 ? tags : undefined,
      });
      setShowComposer(false);
      setNewTitle('');
      setNewContent('');
      setNewCommunity('');
      setNewTags('');
      await loadTopics();
    } catch (e: any) {
      setError(e.message || 'Failed to create topic');
    }
  };

  const handleAddReply = async () => {
    if (!replyText.trim() || !selectedTopic) return;
    try {
      await forumsApi.addReply(selectedTopic.id, { content: replyText });
      setReplyText('');
      await loadReplies(selectedTopic.id);
      await loadTopics();
    } catch (e: any) {
      setError(e.message || 'Failed to add reply');
    }
  };

  const formatTime = (ts: string) => {
    const d = new Date(ts);
    const now = new Date();
    const diff = now.getTime() - d.getTime();
    if (diff < 60000) return 'just now';
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`;
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`;
    return d.toLocaleDateString();
  };

  return (
    <div className="screen">
      <div className="forums-layout">
        {/* Left sidebar: communities */}
        <div className="forums-sidebar">
          <div className="sidebar-header">
            <span className="sidebar-title">Communities</span>
          </div>
          <div className="sidebar-categories">
            {COMMUNITIES.map(c => (
              <div
                key={c.id}
                className={`category-item ${community === c.id ? 'forums-cat-active' : ''}`}
                onClick={() => { setCommunity(c.id); setSelectedTopic(null); }}
              >
                <span className="category-icon">{c.icon}</span>
                <span className="category-item-label">{c.label}</span>
              </div>
            ))}
          </div>
        </div>

        {/* Center: topic list */}
        <div className="forums-center">
          <div className="feed-column-header">
            <div className="feed-header-row">
              <div>
                <h1>
                  {COMMUNITIES.find(c => c.id === community)?.label || 'All Communities'}
                </h1>
                <p>Forum discussions</p>
              </div>
              <button className="post-action-btn" onClick={() => setShowComposer(true)}>
                + New Topic
              </button>
            </div>
          </div>

          <div className="forums-topic-list">
            {loading && <div className="forums-loading">Loading topics...</div>}
            {error && <div className="forums-error">{error}</div>}
            {!loading && !error && topics.length === 0 && (
              <div className="forums-empty">
                <p>No topics yet. Start a discussion!</p>
              </div>
            )}
            {!loading && topics.map(topic => (
              <div
                key={topic.id}
                className={`forums-topic-card ${selectedTopic?.id === topic.id ? 'forums-topic-active' : ''} ${topic.pinned ? 'forums-topic-pinned' : ''}`}
                onClick={() => selectTopic(topic)}
              >
                <div className="forums-topic-header">
                  {topic.pinned && <span className="forums-pinned-badge">📌</span>}
                  <span className="forums-topic-title">{topic.title}</span>
                  {topic.locked && <span className="forums-locked-badge">🔒</span>}
                </div>
                <div className="forums-topic-meta">
                  <span className="forums-topic-author">by {topic.author.display_name}</span>
                  <span className="forums-topic-community">
                    {topic.community ? `#${topic.community}` : ''}
                  </span>
                  <span className="forums-topic-stats">
                    💬 {topic.reply_count} · 👁 {topic.view_count}
                  </span>
                  <span className="forums-topic-time">{formatTime(topic.created_at)}</span>
                </div>
                {topic.tags.length > 0 && (
                  <div className="forums-topic-tags">
                    {topic.tags.map(tag => (
                      <span key={tag} className="forums-tag">{tag}</span>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>

        {/* Right: topic detail / replies */}
        <div className="forums-detail">
          {selectedTopic ? (
            <div className="forums-detail-content">
              <div className="forums-detail-header">
                <h2>{selectedTopic.title}</h2>
                <div className="forums-topic-meta">
                  <span className="forums-topic-author">by {selectedTopic.author.display_name}</span>
                  <span className="forums-topic-time">{formatTime(selectedTopic.created_at)}</span>
                </div>
              </div>
              <div className="forums-detail-body">
                <p>{selectedTopic.content}</p>
              </div>

              {selectedTopic.tags.length > 0 && (
                <div className="forums-topic-tags">
                  {selectedTopic.tags.map(tag => (
                    <span key={tag} className="forums-tag">{tag}</span>
                  ))}
                </div>
              )}

              <div className="forums-replies-section">
                <h3>Replies ({replies.length})</h3>

                <div className="forums-composer">
                  <textarea
                    className="forums-reply-input"
                    placeholder="Write a reply..."
                    value={replyText}
                    onChange={e => setReplyText(e.target.value)}
                    rows={3}
                  />
                  <button
                    className="btn"
                    onClick={handleAddReply}
                    disabled={!replyText.trim()}
                  >
                    Reply
                  </button>
                </div>

                {replies.length === 0 && (
                  <div className="forums-empty">No replies yet.</div>
                )}

                {replies.map(reply => (
                  <div key={reply.id} className="forums-reply">
                    <div className="forums-reply-header">
                      <span className="forums-reply-author">{reply.author.display_name}</span>
                      <span className="forums-reply-time">{formatTime(reply.created_at)}</span>
                    </div>
                    <div className="forums-reply-body">{reply.content}</div>
                  </div>
                ))}
              </div>
            </div>
          ) : (
            <div className="forums-detail-empty">
              <p>Select a topic to view its replies</p>
            </div>
          )}
        </div>
      </div>

      {/* New Topic Composer Modal */}
      {showComposer && (
        <div className="modal-overlay" onClick={() => setShowComposer(false)}>
          <div className="modal-box" onClick={e => e.stopPropagation()}>
            <h2>New Topic</h2>
            <div className="form-group">
              <input
                className="input-field"
                placeholder="Title"
                value={newTitle}
                onChange={e => setNewTitle(e.target.value)}
              />
              <textarea
                className="input-field"
                placeholder="Content"
                value={newContent}
                onChange={e => setNewContent(e.target.value)}
                rows={5}
                style={{ resize: 'vertical' }}
              />
              <select
                className="input-field"
                value={newCommunity}
                onChange={e => setNewCommunity(e.target.value)}
              >
                <option value="">No community</option>
                {COMMUNITIES.filter(c => c.id).map(c => (
                  <option key={c.id} value={c.id}>{c.label}</option>
                ))}
              </select>
              <input
                className="input-field"
                placeholder="Tags (comma separated)"
                value={newTags}
                onChange={e => setNewTags(e.target.value)}
              />
              <button
                className="btn btn-block"
                onClick={handleCreateTopic}
                disabled={!newTitle.trim() || !newContent.trim()}
              >
                Create Topic
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
