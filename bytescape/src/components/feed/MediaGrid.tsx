import React from 'react';
import { BlobImage } from '../BlobImage';
import { VideoPlayer } from '../VideoPlayer';
import { ExpandIcon } from './icons';

interface MediaGridProps {
    mediaUrls: string[];
    mediaKinds?: string[];
    mediaSources: string[];
    thumbnailSources: string[];
    mediaDir?: string;
    openGallery: (index: number) => void;
}

export function MediaGrid({ mediaUrls, mediaKinds, mediaSources, thumbnailSources, mediaDir, openGallery }: MediaGridProps) {
    if (mediaUrls.length === 0) return null;

    return (
        <div className={`post-media-grid items-${Math.min(mediaUrls.length, 4)}`}>
            {mediaUrls.slice(0, 4).map((_, i) => {
                const kind = (mediaKinds && mediaKinds[i]) || 'image';
                const thumb = thumbnailSources[i] || mediaSources[i];

                return (
                    <div key={i} className="media-item">
                        {kind === 'video' ? (
                            /* Inline video player — plays directly in the feed */
                            <div onClick={(e) => e.stopPropagation()}>
                                <VideoPlayer
                                    src={mediaSources[i]}
                                    poster={thumb}
                                    className="inline-video-container"
                                    style={{ width: '100%', height: '100%' }}
                                />
                                <button
                                    className="media-expand-btn"
                                    onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                                    title="Open fullscreen"
                                >
                                    <ExpandIcon />
                                </button>
                            </div>
                        ) : (
                            /* Image */
                            <div onClick={(e) => { e.stopPropagation(); openGallery(i); }}>
                                <BlobImage
                                    src={mediaSources[i]}
                                    alt=""
                                    mediaDir={mediaDir}
                                    style={{ width: '100%', height: '100%', display: 'block' } as React.CSSProperties}
                                />
                                <button
                                    className="media-expand-btn"
                                    onClick={(e) => { e.stopPropagation(); openGallery(i); }}
                                    title="Open fullscreen"
                                >
                                    <ExpandIcon />
                                </button>
                            </div>
                        )}
                    </div>
                );
            })}
        </div>
    );
}
