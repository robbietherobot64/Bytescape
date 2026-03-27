import { useState, useCallback, useMemo } from 'react';
import { BsGlobe, BsLock, BsShieldLock, BsXLg } from 'react-icons/bs';
import { useTopicTree, createGroup, flattenTopics } from '../hooks/useGroupsData';
import { GroupVisibility } from '../types';
import './CreateGroupModal.css';

interface CreateGroupModalProps {
    onClose: () => void;
    onCreated: (groupId: string) => void;
}

function slugify(name: string): string {
    return name
        .toLowerCase()
        .replace(/[^a-z0-9\s-]/g, '')
        .replace(/\s+/g, '-')
        .replace(/-+/g, '-')
        .replace(/^-|-$/g, '');
}

export function CreateGroupModal({ onClose, onCreated }: CreateGroupModalProps) {
    const { topics } = useTopicTree();
    const [name, setName] = useState('');
    const [slug, setSlug] = useState('');
    const [slugManual, setSlugManual] = useState(false);
    const [description, setDescription] = useState('');
    const [visibility, setVisibility] = useState<GroupVisibility>('Public');
    const [topicId, setTopicId] = useState('');
    const [rules, setRules] = useState('');
    const [submitting, setSubmitting] = useState(false);
    const [error, setError] = useState('');

    const flatTopics = useMemo(() => flattenTopics(topics), [topics]);

    const handleNameChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
        const val = e.target.value;
        setName(val);
        if (!slugManual) {
            setSlug(slugify(val));
        }
    }, [slugManual]);

    const handleSlugChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
        setSlugManual(true);
        setSlug(slugify(e.target.value));
    }, []);

    const handleSubmit = useCallback(async (e: React.FormEvent) => {
        e.preventDefault();
        setError('');

        if (!name.trim()) { setError('Group name is required.'); return; }
        if (!slug.trim()) { setError('Group slug is required.'); return; }
        if (!description.trim()) { setError('Description is required.'); return; }
        if (!topicId) { setError('Please select a topic.'); return; }

        setSubmitting(true);
        try {
            const groupId = await createGroup({
                name: name.trim(),
                slug: slug.trim(),
                description: description.trim(),
                visibility,
                topicId,
                rules: rules.trim() || undefined,
            });
            onCreated(groupId);
        } catch (err: any) {
            setError(typeof err === 'string' ? err : err?.message || 'Failed to create group');
        }
        setSubmitting(false);
    }, [name, slug, description, visibility, topicId, rules, onCreated]);

    return (
        <div className="cgm-overlay" onClick={onClose}>
            <div className="cgm-card" onClick={e => e.stopPropagation()}>
                <div className="cgm-header">
                    <h2 className="cgm-title">Create Group</h2>
                    <button className="cgm-close-btn" onClick={onClose}>
                        <BsXLg />
                    </button>
                </div>

                <form className="cgm-form" onSubmit={handleSubmit}>
                    {/* Name */}
                    <div className="cgm-field">
                        <label className="cgm-label">Name *</label>
                        <input
                            className="cgm-input"
                            type="text"
                            placeholder="Mindfulness Daily"
                            value={name}
                            onChange={handleNameChange}
                            maxLength={100}
                        />
                    </div>

                    {/* Slug */}
                    <div className="cgm-field">
                        <label className="cgm-label">Slug</label>
                        <input
                            className="cgm-input cgm-slug-input"
                            type="text"
                            placeholder="mindfulness-daily"
                            value={slug}
                            onChange={handleSlugChange}
                            maxLength={100}
                        />
                    </div>

                    {/* Description */}
                    <div className="cgm-field">
                        <label className="cgm-label">Description *</label>
                        <textarea
                            className="cgm-textarea"
                            placeholder="What is this group about?"
                            value={description}
                            onChange={e => setDescription(e.target.value)}
                            maxLength={2000}
                            rows={3}
                        />
                    </div>

                    {/* Visibility */}
                    <div className="cgm-field">
                        <label className="cgm-label">Visibility *</label>
                        <div className="cgm-visibility-options">
                            {([
                                { value: 'Public' as const, icon: <BsGlobe />, desc: 'Anyone can find and join this group' },
                                { value: 'Closed' as const, icon: <BsLock />, desc: 'Anyone can find this group, but joining requires approval' },
                                { value: 'Private' as const, icon: <BsShieldLock />, desc: 'Only invited members can see and join this group' },
                            ]).map(opt => (
                                <label
                                    key={opt.value}
                                    className={`cgm-vis-option ${visibility === opt.value ? 'selected' : ''}`}
                                >
                                    <input
                                        type="radio"
                                        name="visibility"
                                        value={opt.value}
                                        checked={visibility === opt.value}
                                        onChange={() => setVisibility(opt.value)}
                                        className="cgm-vis-radio"
                                    />
                                    <span className="cgm-vis-icon">{opt.icon}</span>
                                    <span className="cgm-vis-details">
                                        <span className="cgm-vis-name">{opt.value}</span>
                                        <span className="cgm-vis-desc">{opt.desc}</span>
                                    </span>
                                </label>
                            ))}
                        </div>
                    </div>

                    {/* Topic */}
                    <div className="cgm-field">
                        <label className="cgm-label">Topic *</label>
                        <select
                            className="cgm-select"
                            value={topicId}
                            onChange={e => setTopicId(e.target.value)}
                        >
                            <option value="">Select a topic...</option>
                            {flatTopics.map(t => (
                                <option key={t.id} value={t.id}>
                                    {'  '.repeat(t.depth)}{t.name}
                                </option>
                            ))}
                        </select>
                    </div>

                    {/* Rules */}
                    <div className="cgm-field">
                        <label className="cgm-label">Rules (optional)</label>
                        <textarea
                            className="cgm-textarea"
                            placeholder="1. Be respectful&#10;2. No spam"
                            value={rules}
                            onChange={e => setRules(e.target.value)}
                            maxLength={5000}
                            rows={3}
                        />
                    </div>

                    {error && <div className="cgm-error">{error}</div>}

                    <div className="cgm-actions">
                        <button type="button" className="cgm-cancel-btn" onClick={onClose}>
                            Cancel
                        </button>
                        <button type="submit" className="cgm-submit-btn" disabled={submitting}>
                            {submitting ? 'Creating...' : 'Create Group'}
                        </button>
                    </div>
                </form>
            </div>
        </div>
    );
}
