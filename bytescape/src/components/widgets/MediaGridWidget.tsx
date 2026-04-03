/**
 * Media Grid Widget (B2.4)
 *
 * Displays a grid of images from the user's vault.
 * Fetches image files and renders thumbnails via the media server.
 *
 * Cross-system note:
 * - Data and media are resolved through Tauri command bridges into Rust Engine services.
 * - `get_media_server_port` provides the local media HTTP endpoint.
 * - `vault_list_files` provides vault metadata used for image filtering.
 */

import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface VaultFile {
    name: string;
    mime: string | null;
    blob_hash: string;
    size: number;
}

/**
 * Renders a compact image grid for profile/widget surfaces.
 *
 * Exported for widget hydration by the profile rendering pipeline.
 */
export function MediaGridWidget() {
    // --- Local widget state ---
    const [images, setImages] = useState<VaultFile[]>([]);
    const [mediaPort, setMediaPort] = useState<number | null>(null);

    // --- Initial data hydration ---
    useEffect(() => {
        invoke<number>('get_media_server_port').then(setMediaPort).catch(() => {});
        invoke<VaultFile[]>('vault_list_files')
            .then(files => {
                const imgFiles = files.filter(f =>
                    f.mime?.startsWith('image/') ||
                    f.name.match(/\.(jpg|jpeg|png|gif|webp|bmp|svg)$/i)
                );
                setImages(imgFiles.slice(0, 12));
            })
            .catch(() => {});
    }, []);

    // --- Empty state ---
    if (images.length === 0) {
        return (
            <div style={{
                background: 'var(--bg-secondary)',
                border: '2px solid var(--border-color)',
                borderRadius: 'var(--border-radius)',
                padding: 16,
                textAlign: 'center',
                color: 'var(--text-muted)',
                fontSize: 12,
            } as React.CSSProperties}>
                📷 No images in vault yet
            </div>
        );
    }

    // --- Populated state ---
    return (
        <div style={{
            background: 'var(--bg-secondary)',
            border: '2px solid var(--border-color)',
            borderRadius: 'var(--border-radius)',
            padding: 8,
        } as React.CSSProperties}>
            <div style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fill, minmax(80px, 1fr))',
                gap: 4,
            } as React.CSSProperties}>
                {images.map(img => (
                    <div key={img.blob_hash} style={{
                        aspectRatio: '1',
                        borderRadius: 4,
                        overflow: 'hidden',
                        background: 'var(--bg-tertiary)',
                    } as React.CSSProperties}>
                        {mediaPort ? (
                            <img
                                src={`http://127.0.0.1:${mediaPort}/media/${img.blob_hash}`}
                                alt={img.name}
                                loading="lazy"
                                style={{
                                    width: '100%',
                                    height: '100%',
                                    objectFit: 'cover',
                                } as React.CSSProperties}
                                onError={e => {
                                    (e.target as HTMLImageElement).style.display = 'none';
                                }}
                            />
                        ) : (
                            <div style={{
                                width: '100%',
                                height: '100%',
                                display: 'flex',
                                alignItems: 'center',
                                justifyContent: 'center',
                                color: 'var(--text-muted)',
                                fontSize: 10,
                            } as React.CSSProperties}>
                                Loading...
                            </div>
                        )}
                    </div>
                ))}
            </div>
        </div>
    );
}
