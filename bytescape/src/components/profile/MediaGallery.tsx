import { VaultFile } from '../../types';

interface MediaGalleryProps {
    mediaFiles: VaultFile[];
    mediaThumbs: Record<string, string>;
    mediaLoading: boolean;
    onOpenLightbox: (file: VaultFile) => void;
}

export function MediaGallery({ mediaFiles, mediaThumbs, mediaLoading, onOpenLightbox }: MediaGalleryProps) {
    return (
        <div>
            <div className="profile-section-header">
                <span className="profile-section-header-emoji">🖼️</span> Media
            </div>

            {mediaLoading && <div className="profile-empty"><div className="profile-empty-icon">⏳</div><p className="profile-empty-text">Loading media...</p></div>}

            {!mediaLoading && mediaFiles.length === 0 && (
                <div className="profile-empty"><div className="profile-empty-icon">🖼️</div><p className="profile-empty-text">No media files yet.</p></div>
            )}

            <div className="profile-media-grid">
                {mediaFiles.map(file => (
                    <div key={file.id} className="profile-media-item" onClick={() => onOpenLightbox(file)}>
                        {mediaThumbs[file.id] ? (
                            <img
                                src={mediaThumbs[file.id]}
                                alt={file.name}
                                loading="lazy"
                            />
                        ) : (
                            <div className="profile-media-placeholder">
                                {file.kind === 'Video' ? '🎬' : '🖼️'}
                            </div>
                        )}
                    </div>
                ))}
            </div>
        </div>
    );
}
