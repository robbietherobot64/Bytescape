import { useState, useEffect, useCallback } from 'react';
import StoryArchiveList from '../components/stories/StoryArchiveList';
import { vaultApi, MediaItem } from '../utils/api';

export default function VaultScreen() {
  const [items, setItems] = useState<MediaItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [lightboxIndex, setLightboxIndex] = useState<number | null>(null);

  useEffect(() => {
    loadVault();
  }, []);

  const loadVault = async () => {
    try {
      const d = await vaultApi.list();
      setItems(d.items);
    } catch {
      // silent
    } finally {
      setLoading(false);
    }
  };

  const openLightbox = useCallback((index: number) => {
    setLightboxIndex(index);
  }, []);

  const closeLightbox = useCallback(() => {
    setLightboxIndex(null);
  }, []);

  const prevLightbox = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    setLightboxIndex(prev => prev !== null ? (prev - 1 + items.length) % items.length : null);
  }, [items.length]);

  const nextLightbox = useCallback((e: React.MouseEvent) => {
    e.stopPropagation();
    setLightboxIndex(prev => prev !== null ? (prev + 1) % items.length : null);
  }, [items.length]);

  if (loading) {
    return <div className="screen vault-screen"><p className="vault-loading">Loading vault...</p></div>;
  }

  const isImage = (mime: string) => mime.startsWith('image/');
  const isVideo = (mime: string) => mime.startsWith('video/');

  return (
    <div className="screen vault-screen">
      <div className="vault-header">
        <h1 className="vault-title">Vault</h1>
        <span className="vault-count">{items.length} item{items.length !== 1 ? 's' : ''}</span>
      </div>
      <div className="vault-stories-section">
        <h2 className="stories-explorer-subtitle">Your story archive</h2>
        <StoryArchiveList />
      </div>


      {items.length === 0 ? (
        <div className="vault-empty">
          <p>No media yet.</p>
        </div>
      ) : (
        <div className="vault-grid">
          {items.map((item, i) => (
            <div
              key={item.id}
              className="vault-item"
              onClick={() => openLightbox(i)}
            >
              {isImage(item.mime_type) ? (
                <img
                  src={item.thumbnail_url || item.storage_url}
                  alt={item.alt_text || ''}
                  className="vault-item-img"
                  loading="lazy"
                />
              ) : isVideo(item.mime_type) ? (
                <video
                  src={item.storage_url}
                  className="vault-item-img"
                  muted
                  playsInline
                  preload="metadata"
                />
              ) : (
                <div className="vault-item-icon">
                  <span className="vault-mime-badge">{item.mime_type.split('/')[1]?.toUpperCase() || 'FILE'}</span>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {lightboxIndex !== null && items[lightboxIndex] && (
        <div className="vault-lightbox" onClick={closeLightbox}>
          <button className="vault-lightbox-close" onClick={closeLightbox}>×</button>
          {items.length > 1 && (
            <div className="vault-lightbox-nav">
              <button className="vault-lightbox-nav-btn" onClick={prevLightbox}>‹</button>
              <span className="vault-lightbox-counter">{lightboxIndex + 1} / {items.length}</span>
              <button className="vault-lightbox-nav-btn" onClick={nextLightbox}>›</button>
            </div>
          )}
          {isImage(items[lightboxIndex].mime_type) ? (
            <img
              src={items[lightboxIndex].storage_url}
              alt={items[lightboxIndex].alt_text || ''}
              className="vault-lightbox-img"
            />
          ) : isVideo(items[lightboxIndex].mime_type) ? (
            <video
              src={items[lightboxIndex].storage_url}
              className="vault-lightbox-img"
              controls
              autoPlay
              playsInline
            />
          ) : (
            <div className="vault-lightbox-file">
              <div className="vault-lightbox-file-icon">📄</div>
              <p className="vault-lightbox-file-mime">{items[lightboxIndex].mime_type}</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
