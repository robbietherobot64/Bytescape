import { useState } from 'react';
import type { Widget } from '../../../utils/api';

interface Props {
  widget: Widget;
  editing: boolean;
  onDataChange?: (data: Record<string, unknown>) => void;
}

export default function WidgetPhotoGallery({ widget, editing, onDataChange }: Props) {
  const photos = (widget.data?.photos as string[]) || [];
  const [lightboxIdx, setLightboxIdx] = useState<number | null>(null);

  const handleAddPhoto = () => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/*';
    input.onchange = (e) => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) return;
      const reader = new FileReader();
      reader.onload = (ev) => {
        const dataUrl = ev.target?.result as string;
        onDataChange?.({ ...widget.data, photos: [...photos, dataUrl] });
      };
      reader.readAsDataURL(file);
    };
    input.click();
  };

  const handleRemovePhoto = (idx: number) => {
    const updated = photos.filter((_, i) => i !== idx);
    onDataChange?.({ ...widget.data, photos: updated });
  };

  if (editing) {
    return (
      <div className="widget-gallery">
        <div className="widget-gallery-grid" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(100px, 1fr))', gap: '8px' }}>
          {photos.map((photo, idx) => (
            <div key={idx} style={{ position: 'relative' }}>
              <img src={photo} alt="" style={{ width: '100%', height: '100px', objectFit: 'cover', border: '1px solid var(--border-dim)' }} />
              <button
                onClick={() => handleRemovePhoto(idx)}
                style={{ position: 'absolute', top: '2px', right: '2px', background: 'rgba(0,0,0,0.7)', color: '#fff', border: 'none', cursor: 'pointer', fontSize: '14px', padding: '2px 6px', borderRadius: '2px' }}
              >
                ×
              </button>
            </div>
          ))}
        </div>
        <button className="btn" style={{ marginTop: '8px', fontSize: '12px', padding: '6px 12px' }} onClick={handleAddPhoto}>
          + Add Photo
        </button>
      </div>
    );
  }

  if (photos.length === 0) {
    return <div style={{ color: 'var(--text-muted)', fontStyle: 'italic', fontSize: '13px', padding: '8px 0' }}>No photos yet</div>;
  }

  return (
    <div className="widget-gallery">
      <div className="widget-gallery-grid" style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(120px, 1fr))', gap: '8px' }}>
        {photos.map((photo, idx) => (
          <img
            key={idx}
            src={photo}
            alt=""
            onClick={() => setLightboxIdx(idx)}
            style={{ width: '100%', height: '120px', objectFit: 'cover', cursor: 'pointer', border: '1px solid var(--border-dim)', transition: 'opacity 0.15s' }}
          />
        ))}
      </div>

      {lightboxIdx !== null && (
        <div className="lightbox" onClick={() => setLightboxIdx(null)}>
          <button className="lightbox-close" onClick={() => setLightboxIdx(null)}>×</button>
          <img src={photos[lightboxIdx]} alt="" className="lightbox-img" onClick={e => e.stopPropagation()} />
          <div className="lightbox-nav">
            <button onClick={(e) => { e.stopPropagation(); setLightboxIdx(lightboxIdx > 0 ? lightboxIdx - 1 : photos.length - 1); }}>←</button>
            <span>{lightboxIdx + 1} / {photos.length}</span>
            <button onClick={(e) => { e.stopPropagation(); setLightboxIdx(lightboxIdx < photos.length - 1 ? lightboxIdx + 1 : 0); }}>→</button>
          </div>
        </div>
      )}
    </div>
  );
}
