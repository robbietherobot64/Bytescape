import React, { useMemo } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useLazyBlobUrl } from '../../hooks/useBlobUrl';
import { VideoPlayer } from '../VideoPlayer';
import './FeedMedia.css';

// ── URL resolution ──

function useResolvedUrl(src: string, mediaDir?: string): [React.RefCallback<HTMLElement>, string | null] {
    const effectiveSrc = useMemo(() => {
        if (src?.startsWith('blob:')) {
            const after = src.slice(5);
            if (!after.startsWith('http://') && !after.startsWith('https://') && !after.startsWith('tauri://')) {
                return `blob-display:${after}`;
            }
        }
        return src;
    }, [src]);

    const [containerRef, blobUrl, , error] = useLazyBlobUrl(
        effectiveSrc?.startsWith('blob:') || effectiveSrc?.startsWith('blob-display:') ? effectiveSrc : undefined
    );

    const url = useMemo(() => {
        if (!src) return null;
        if (effectiveSrc?.startsWith('blob:') || effectiveSrc?.startsWith('blob-display:')) return blobUrl;
        if (src.startsWith('http://') || src.startsWith('https://') || src.startsWith('asset://') || src.startsWith('tauri://')) return src;
        if (mediaDir) {
            if (src.startsWith('media/')) return convertFileSrc(`${mediaDir}/${src.replace(/^media\//, '')}`);
            if (src.startsWith('/') || src.match(/^[A-Za-z]:\\/)) return convertFileSrc(src);
            return convertFileSrc(`${mediaDir}/${src}`);
        }
        return convertFileSrc(src);
    }, [src, effectiveSrc, blobUrl, mediaDir]);

    return [containerRef, url && !error ? url : null];
}

// ── Single image: just an <img> ──

function FeedImg({ src, mediaDir, cover }: { src: string; mediaDir?: string; cover?: boolean }) {
    const [ref, url] = useResolvedUrl(src, mediaDir);

    return (
        <img
            ref={ref as React.RefCallback<HTMLImageElement>}
            src={url || undefined}
            alt=""
            style={cover
                ? { display: 'block', width: '100%', height: '100%', objectFit: 'cover' as const }
                : { display: 'block', width: '100%' }
            }
        />
    );
}

// ── Main component ──

interface FeedMediaProps {
    mediaSources: string[];
    thumbnailSources: string[];
    mediaKinds?: string[];
    mediaDir?: string;
}

export function FeedMedia({ mediaSources, thumbnailSources, mediaKinds, mediaDir }: FeedMediaProps) {
    if (mediaSources.length === 0) return null;

    const count = Math.min(mediaSources.length, 4);

    // Single image — just an <img> filling the card width.
    if (count === 1) {
        const kind = mediaKinds?.[0] || 'image';
        const isVideo = kind === 'video';
        return (
            <div className="fm-cell" style={{ position: 'relative', overflow: 'hidden' } as React.CSSProperties}>
                {isVideo ? (
                    <VideoPlayer
                        src={mediaSources[0]}
                        poster={thumbnailSources[0] || undefined}
                        autoPlay
                        controls
                        muted
                        style={{ display: 'block', width: '100%' } as React.CSSProperties}
                    />
                ) : (
                    <FeedImg src={mediaSources[0]} mediaDir={mediaDir} />
                )}
            </div>
        );
    }

    // Multi-image grid
    const cols = count === 3 ? '2fr 1fr' : '1fr 1fr';
    const rows = count >= 3 ? '1fr 1fr' : undefined;

    return (
        <div className="fm-grid" style={{ display: 'grid', gridTemplateColumns: cols, gridTemplateRows: rows, gap: 2, width: '100%', overflow: 'hidden' } as React.CSSProperties}>
            {mediaSources.slice(0, 4).map((src, i) => {
                const kind = mediaKinds?.[i] || 'image';
                const isVideo = kind === 'video';
                const cellStyle: React.CSSProperties = { position: 'relative', overflow: 'hidden', aspectRatio: '16/9' };
                if (count === 3 && i === 0) (cellStyle as Record<string, unknown>).gridRow = '1 / 3';
                return (
                    <div key={i} className="fm-cell" style={cellStyle}>
                        {isVideo ? (
                            <VideoPlayer
                                src={src}
                                poster={thumbnailSources[i] || undefined}
                                autoPlay
                                controls
                                muted
                                style={{ display: 'block', width: '100%', height: '100%', objectFit: 'cover' } as React.CSSProperties}
                            />
                        ) : (
                            <FeedImg src={src} mediaDir={mediaDir} cover />
                        )}
                    </div>
                );
            })}
        </div>
    );
}
