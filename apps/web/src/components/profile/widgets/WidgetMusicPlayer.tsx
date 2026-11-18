import type { Widget } from '../../../utils/api';

interface Props {
  widget: Widget;
  editing: boolean;
  onDataChange?: (data: Record<string, unknown>) => void;
}

function extractSpotifyId(url: string): string | null {
  const m = url.match(/(?:open\.spotify\.com\/(?:track|album|playlist|episode)\/)([a-zA-Z0-9]+)/);
  return m ? m[1] : null;
}

function extractBandcampId(url: string): { track: string; size?: string } | null {
  const m = url.match(/bandcamp\.com\/track\/([a-zA-Z0-9_-]+)/);
  return m ? { track: m[1] } : null;
}

function extractSoundCloudId(url: string): boolean {
  return url.includes('soundcloud.com');
}

export default function WidgetMusicPlayer({ widget, editing, onDataChange }: Props) {
  const url = (widget.data?.url as string) || '';
  const service = (widget.data?.service as string) || '';

  if (editing) {
    return (
      <div className="widget-music-editor">
        <div className="profile-edit-field">
          <label className="profile-edit-label">Music URL</label>
          <input
            className="profile-edit-input"
            placeholder="Spotify / Bandcamp / SoundCloud URL"
            value={url}
            onChange={e => onDataChange?.({ ...widget.data, url: e.target.value })}
          />
        </div>
      </div>
    );
  }

  if (!url) {
    return <div className="widget-music-empty" style={{ color: 'var(--text-muted)', fontStyle: 'italic', fontSize: '13px', padding: '8px 0' }}>No music player configured</div>;
  }

  const spotifyId = extractSpotifyId(url);
  if (spotifyId) {
    return (
      <iframe
        src={`https://open.spotify.com/embed/track/${spotifyId}`}
        width="100%"
        height="80"
        frameBorder="0"
        allow="encrypted-media"
        style={{ borderRadius: '4px' }}
      />
    );
  }

  const bandcamp = extractBandcampId(url);
  if (bandcamp) {
    return (
      <iframe
        src={`https://bandcamp.com/EmbeddedPlayer/track=${bandcamp.track}/size=large/`}
        width="100%"
        height="120"
        frameBorder="0"
        seamless
        style={{ borderRadius: '4px' }}
      />
    );
  }

  if (extractSoundCloudId(url)) {
    return (
      <iframe
        src={`https://w.soundcloud.com/player/?url=${encodeURIComponent(url)}&color=%2300ff41&auto_play=false`}
        width="100%"
        height="140"
        frameBorder="0"
        style={{ borderRadius: '4px' }}
      />
    );
  }

  return (
    <a href={url} target="_blank" rel="noopener noreferrer" className="profile-chip-link" style={{ display: 'inline-block', padding: '8px 0' }}>
      🎵 {url}
    </a>
  );
}
