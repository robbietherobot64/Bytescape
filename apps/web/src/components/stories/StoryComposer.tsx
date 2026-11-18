import { useRef, useState } from 'react';
import { apiRaw, storiesApi } from '../../utils/api';

const PROMPT_CHIPS = ['Listening to…', 'Reading…', 'Working on…', 'Thinking about…', 'Watching…', 'Playing…'];
const BG_COLORS = ['#0a2a0a', '#1a1a2e', '#2d1b3d', '#3d1b1b', '#1b2d3d', '#000000'];
const BG_GRADIENTS = [
  'linear-gradient(135deg, #0a2a0a, #004411)',
  'linear-gradient(135deg, #1a1a2e, #4a2d8a)',
  'linear-gradient(135deg, #3d1b1b, #8a2d2d)',
];

interface PendingMedia {
  file: File;
  previewUrl: string;
  alt: string;
  duration: string;
}

export default function StoryComposer({ onClose, onPublished }: { onClose: () => void; onPublished: () => void }) {
  const [mode, setMode] = useState<'media' | 'text'>('media');
  const [pending, setPending] = useState<PendingMedia[]>([]);
  const [caption, setCaption] = useState('');
  const [lifetime, setLifetime] = useState<'24h' | '7d' | 'until' | 'never'>('24h');
  const [untilDate, setUntilDate] = useState('');
  const [scheduled, setScheduled] = useState(false);
  const [startDate, setStartDate] = useState('');
  const [pinned, setPinned] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);

  const [statusText, setStatusText] = useState('');
  const [statusMinutes, setStatusMinutes] = useState(60);
  const [embedUri, setEmbedUri] = useState('');
  const [bg, setBg] = useState<{ color?: string; image?: string }>({ color: BG_COLORS[0] });

  const addFiles = (files: FileList | null) => {
    if (!files) return;
    const next: PendingMedia[] = [...pending];
    for (const file of Array.from(files)) {
      if (next.length >= 6) break;
      next.push({ file, previewUrl: URL.createObjectURL(file), alt: '', duration: '' });
    }
    setPending(next);
  };

  const uploadOne = async (pm: PendingMedia): Promise<string> => {
    const form = new FormData();
    form.append('file', pm.file);
    const res = await apiRaw('/api/media/upload', { method: 'POST', body: form });
    if (!res.ok) throw new Error('Upload failed');
    const data: { url: string } = await res.json();
    return data.url;
  };

  const publishMedia = async () => {
    setBusy(true);
    setError('');
    try {
      const media_urls: string[] = [];
      const alts: string[] = [];
      const durations: (number | null)[] = [];
      for (const pm of pending) {
        media_urls.push(await uploadOne(pm));
        alts.push(pm.alt);
        durations.push(pm.duration ? Math.min(300, Math.max(1, parseInt(pm.duration, 10) || 5)) : null);
      }
      let expires_at: string | null = null;
      if (lifetime === '24h') expires_at = new Date(Date.now() + 24 * 3600e3).toISOString();
      else if (lifetime === '7d') expires_at = new Date(Date.now() + 7 * 24 * 3600e3).toISOString();
      else if (lifetime === 'until' && untilDate) expires_at = new Date(untilDate).toISOString();
      await storiesApi.create({
        media_urls,
        alts,
        durations,
        text: caption || null,
        expires_at,
        active_from: scheduled && startDate ? new Date(startDate).toISOString() : null,
        pinned,
      });
      onPublished();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Publish failed');
    } finally {
      setBusy(false);
    }
  };

  const publishStatus = async () => {
    setBusy(true);
    setError('');
    try {
      await storiesApi.setStatus({
        text: statusText,
        duration_minutes: statusMinutes,
        embed_uri: embedUri || null,
        style: { bg },
      });
      onPublished();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Publish failed');
    } finally {
      setBusy(false);
    }
  };

  const uploadBg = async (file: File | undefined) => {
    if (!file) return;
    try {
      const url = await uploadOne({ file, previewUrl: '', alt: '', duration: '' });
      setBg({ image: url });
    } catch {
      setError('Background upload failed');
    }
  };

  return (
    <div className="composer-modal-overlay" onClick={onClose}>
      <div className="composer-modal story-composer" onClick={(e) => e.stopPropagation()}>
        <div className="story-composer-tabs">
          <button className={`btn${mode === 'media' ? ' active' : ''}`} onClick={() => setMode('media')}>
            📷 Story
          </button>
          <button className={`btn${mode === 'text' ? ' active' : ''}`} onClick={() => setMode('text')}>
            ✏️ Status
          </button>
          <button className="btn story-composer-close" onClick={onClose}>×</button>
        </div>

        {mode === 'media' ? (
          <div className="story-composer-body">
            <input
              ref={fileRef}
              type="file"
              accept="image/*,video/*"
              multiple
              hidden
              onChange={(e) => addFiles(e.target.files)}
            />
            <button className="btn" onClick={() => fileRef.current?.click()}>
              + Add photos or video (1–6)
            </button>
            <div className="story-composer-previews">
              {pending.map((pm, i) => (
                <div key={pm.previewUrl} className="story-composer-preview">
                  {pm.file.type.startsWith('video') ? (
                    <video src={pm.previewUrl} className="story-composer-thumb" />
                  ) : (
                    <img src={pm.previewUrl} alt="" className="story-composer-thumb" />
                  )}
                  <input
                    className="story-composer-alt"
                    placeholder="alt text"
                    value={pm.alt}
                    onChange={(e) => {
                      const next = [...pending];
                      next[i] = { ...pm, alt: e.target.value };
                      setPending(next);
                    }}
                  />
                  <input
                    className="story-composer-alt"
                    placeholder="seconds (≤300)"
                    value={pm.duration}
                    onChange={(e) => {
                      const next = [...pending];
                      next[i] = { ...pm, duration: e.target.value };
                      setPending(next);
                    }}
                  />
                  <button
                    className="btn story-composer-remove"
                    onClick={() => setPending(pending.filter((_, idx) => idx !== i))}
                  >
                    ×
                  </button>
                </div>
              ))}
            </div>
            <textarea
              className="story-composer-caption"
              placeholder="Add a caption (also mirrors to your status)…"
              value={caption}
              maxLength={500}
              onChange={(e) => setCaption(e.target.value)}
            />
            <div className="story-composer-lifetime">
              <span>Lifetime:</span>
              {(['24h', '7d', 'until', 'never'] as const).map((l) => (
                <button
                  key={l}
                  className={`btn${lifetime === l ? ' active' : ''}`}
                  onClick={() => setLifetime(l)}
                >
                  {l === 'until' ? 'Until date…' : l === 'never' ? 'Never' : l}
                </button>
              ))}
              {lifetime === 'until' && (
                <input
                  type="datetime-local"
                  className="story-composer-date"
                  value={untilDate}
                  onChange={(e) => setUntilDate(e.target.value)}
                />
              )}
            </div>
            <div className="story-composer-lifetime">
              <label>
                <input type="checkbox" checked={scheduled} onChange={(e) => setScheduled(e.target.checked)} /> Goes live at…
              </label>
              {scheduled && (
                <input
                  type="datetime-local"
                  className="story-composer-date"
                  value={startDate}
                  onChange={(e) => setStartDate(e.target.value)}
                />
              )}
              <label>
                <input type="checkbox" checked={pinned} onChange={(e) => setPinned(e.target.checked)} /> 📌 Pin to front
              </label>
            </div>
            {error && <div className="story-composer-error">{error}</div>}
            <button
              className="btn story-composer-publish"
              disabled={busy || pending.length === 0}
              onClick={publishMedia}
            >
              {busy ? 'Publishing…' : 'Share story'}
            </button>
          </div>
        ) : (
          <div className="story-composer-body">
            <div className="story-composer-chips">
              {PROMPT_CHIPS.map((chip) => (
                <button
                  key={chip}
                  className="btn story-composer-chip"
                  onClick={() => setStatusText(chip + ' ')}
                >
                  {chip}
                </button>
              ))}
            </div>
            <textarea
              className="story-composer-caption"
              placeholder="What are you up to?"
              value={statusText}
              maxLength={500}
              onChange={(e) => setStatusText(e.target.value)}
            />
            <div className="story-composer-lifetime">
              <span>Duration:</span>
              {[
                [60, '1h'],
                [240, '4h'],
                [1440, '24h'],
              ].map(([mins, label]) => (
                <button
                  key={label}
                  className={`btn${statusMinutes === mins ? ' active' : ''}`}
                  onClick={() => setStatusMinutes(mins as number)}
                >
                  {label}
                </button>
              ))}
            </div>
            <input
              className="story-composer-caption"
              placeholder="Music link (SoundCloud / Bandcamp)…"
              value={embedUri}
              onChange={(e) => setEmbedUri(e.target.value)}
            />
            <div className="story-composer-bg-row">
              <span>Background:</span>
              {BG_COLORS.map((c) => (
                <button
                  key={c}
                  className={`story-bg-swatch${bg.color === c && !bg.image ? ' active' : ''}`}
                  style={{ background: c }}
                  onClick={() => setBg({ color: c })}
                  aria-label={`Color ${c}`}
                />
              ))}
              {BG_GRADIENTS.map((g) => (
                <button
                  key={g}
                  className={`story-bg-swatch${bg.color === g && !bg.image ? ' active' : ''}`}
                  style={{ background: g }}
                  onClick={() => setBg({ color: g })}
                  aria-label="Gradient"
                />
              ))}
              <label className="btn story-bg-upload">
                🖼
                <input type="file" accept="image/*" hidden onChange={(e) => uploadBg(e.target.files?.[0])} />
              </label>
            </div>
            {bg.image && <img src={bg.image} alt="" className="story-bg-preview" />}
            {error && <div className="story-composer-error">{error}</div>}
            <button
              className="btn story-composer-publish"
              disabled={busy || !statusText.trim()}
              onClick={publishStatus}
            >
              {busy ? 'Publishing…' : 'Set status'}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
