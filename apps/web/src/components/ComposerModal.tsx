import { useState, useEffect } from 'react';
import type { NewPostData, LinkedAccount } from '../utils/api';
import { accountApi } from '../utils/api';

const API = window.location.hostname === 'localhost' ? 'http://localhost:8080' : '';

interface ComposerModalProps {
  onClose: () => void;
  onPost: (post: NewPostData) => void;
}

export default function ComposerModal({ onClose, onPost }: ComposerModalProps) {
  const [content, setContent] = useState('');
  const [visibility, setVisibility] = useState<'public' | 'followers' | 'friends'>('public');
  const [selectedPlatforms, setSelectedPlatforms] = useState<string[]>(['bluesky']);
  const [mediaFiles, setMediaFiles] = useState<File[]>([]);
  const [charCount, setCharCount] = useState(0);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState('');
  const maxChars = 300;

  // Platform toggles reflect the user's LINKED accounts. A plain post should
  // reach every linked network (owner bug: Mastodon was missing entirely — only
  // a Bluesky toggle shipped — so posts never reached the home instance). The
  // toggles show + default-select each linked provider, so "regular posting"
  // reaches both Bluesky and the Mastodon home instance for an M-linked user.
  const [linkedProviders, setLinkedProviders] = useState<string[]>([]);
  useEffect(() => {
    let alive = true;
    accountApi.list()
      .then((accts: LinkedAccount[]) => {
        if (!alive) return;
        const provs: string[] = Array.from(new Set(
          (accts || []).map((a: LinkedAccount) => a.provider).filter((p: string) => p === 'bluesky' || p === 'mastodon'),
        ));
        setLinkedProviders(provs);
        setSelectedPlatforms(provs.length ? provs : ['bluesky']);
      })
      .catch(() => {});
    return () => { alive = false; };
  }, []);

  const handleSubmit = async () => {
    if (!content.trim() && mediaFiles.length === 0) return;
    if (charCount > maxChars) return;

    setSubmitting(true);
    try {
      const token = localStorage.getItem('bytescape_token');

        const mediaUrls: string[] = [];
        for (const file of mediaFiles) {
          const formData = new FormData();
          formData.append('file', file);
          const res = await fetch(`${API}/api/media/upload`, {
            method: 'POST',
            headers: { Authorization: `Bearer ${token}` },
            body: formData,
          });
          if (res.ok) {
            const data = await res.json();
            mediaUrls.push(data.url);
          } else {
            const text = await res.text();
            console.error('Media upload failed:', res.status, text);
            throw new Error(`Upload failed (${res.status}): ${text}`);
          }
        }

      const res = await fetch(`${API}/api/posts/create`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${token}`,
        },
        body: JSON.stringify({
          content: content.trim(),
          visibility,
          platforms: selectedPlatforms,
          media_urls: mediaUrls,
        }),
      });

      if (res.ok) {
        const created = await res.json();
        onClose();
        onPost(created);
      } else {
        const text = await res.text();
        console.error('Post creation failed:', res.status, text);
        setError(`Post failed (${res.status}): ${text}`);
      }
    } catch (e: any) {
      console.error('Submit error:', e);
      setError(typeof e === 'string' ? e : e.message || 'Post failed');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="composer-modal-overlay" onClick={onClose}>
      <div className="composer-modal" onClick={e => e.stopPropagation()}>
        <header className="composer-header">
          <h3>Compose Post</h3>
          <button onClick={onClose} className="composer-close">&times;</button>
        </header>

        <textarea
          className="composer-textarea"
          placeholder="What's on your mind?"
          value={content}
          onChange={e => {
            setContent(e.target.value);
            setCharCount(e.target.value.length);
          }}
          rows={5}
          disabled={submitting}
        />

        <div className="composer-meta">
          <span className={`char-count ${charCount > maxChars ? 'over-limit' : ''}`}>
            {charCount}/{maxChars}
          </span>
        </div>

        {mediaFiles.length > 0 && (
          <div className="composer-media-preview">
            {mediaFiles.map((file, i) => (
              <div key={i} className="media-thumb">
                {file.type.startsWith('image/') ? (
                  <img src={URL.createObjectURL(file)} alt="" />
                ) : (
                  <video src={URL.createObjectURL(file)} controls />
                )}
                <button
                  className="media-remove"
                  onClick={() => setMediaFiles(prev => prev.filter((_, idx) => idx !== i))}
                >
                  &times;
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="composer-platforms">
          <label>Post to:</label>
          <div className="platform-toggles">
            {[
              { id: 'bluesky', emoji: '\u{1F98B}', name: 'Bluesky' },
              { id: 'mastodon', emoji: '\u{1F42C}', name: 'Mastodon' },
            ].filter(p => linkedProviders.length === 0 || linkedProviders.includes(p.id))
              .map(({ id, emoji, name }) => (
              <button
                key={id}
                type="button"
                className={`platform-toggle ${selectedPlatforms.includes(id) ? 'selected' : ''}`}
                onClick={() => {
                  setSelectedPlatforms(prev =>
                    prev.includes(id) ? prev.filter(p => p !== id) : [...prev, id]
                  );
                }}
              >
                <span className="platform-toggle-emoji">{emoji}</span>
                <span className="platform-toggle-name">{name}</span>
              </button>
            ))}
          </div>
        </div>

        <div className="composer-visibility">
          <label>Visibility:</label>
          <select
            value={visibility}
            onChange={e => setVisibility(e.target.value as any)}
            disabled={submitting}
          >
            <option value="public">Public</option>
            <option value="followers">Followers only</option>
            <option value="friends">Friends (mutual follows)</option>
          </select>
        </div>

        <div className="composer-actions">
          <input
            type="file"
            accept="image/*,video/*"
            multiple
            onChange={e => {
              if (e.target.files) {
                setMediaFiles(prev => [...prev, ...Array.from(e.target.files!)]);
              }
            }}
            style={{ display: 'none' }}
            id="composer-media-input"
          />
          <label htmlFor="composer-media-input" className="btn btn-secondary">
            &#x1F4CE; Media
          </label>

          <button
            className="btn btn-primary"
            onClick={handleSubmit}
            disabled={submitting || (!content.trim() && mediaFiles.length === 0) || charCount > maxChars}
          >
            {submitting ? 'Posting...' : 'Post'}
          </button>
        </div>
      </div>
    </div>
  );
}
