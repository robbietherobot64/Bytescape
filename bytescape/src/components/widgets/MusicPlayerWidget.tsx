/**
 * Music Player Widget (B2.4)
 *
 * PURPOSE
 * - Render a profile-local music player with transport controls and playlist UI.
 * - Support both remote URL tracks and vault-backed blob tracks.
 *
 * HOW IT WORKS
 * - Initializes playlist from explicit props first, then falls back to persisted
 *   widget payload from the backend (`get_widget_data`).
 * - Resolves playable URLs from `url`/`src` directly, or from `blob_hash` via
 *   local media server port (`get_media_server_port`).
 * - Syncs playback state to a hidden `<audio>` element and auto-advances on `ended`.
 * - Persists playlist updates by merging into widget payload and saving with
 *   `save_widget_data`.
 *
 * RELATED FILES
 * - `src/types/index.ts` (`MusicTrack`, `WidgetData`)
 * - `src-tauri/src/lib.rs` (`get_widget_data`, `save_widget_data`, `vault_list_files`, `get_media_server_port` commands)
 * - `src/screens/ProfileScreen.tsx` (widget composition/host surface)
 *
 * Cross-system note (profile/widget data boundary):
 * - This widget owns only `WidgetData.music_playlist`. It must not overwrite
 *   sibling widget keys when persisting profile widget state.
 */

import { useState, useEffect, useRef, useCallback } from 'react';
import { MusicTrack, WidgetData } from '../../types';
import { invoke } from '@tauri-apps/api/core';
import { BsPlayFill, BsPauseFill, BsSkipForwardFill, BsSkipBackwardFill, BsTrash, BsMusicNoteBeamed, BsPlusCircle } from 'react-icons/bs';

interface MusicPlayerWidgetProps {
    tracks?: MusicTrack[];
    editable?: boolean;
    onUpdate?: (tracks: MusicTrack[]) => void;
}

/**
 * `MusicPlayerWidget`
 *
 * Exported profile widget component for local music playback and playlist editing.
 */
export function MusicPlayerWidget({ tracks: initialTracks, editable = false, onUpdate }: MusicPlayerWidgetProps) {
    // ===== Section: Local component state =====
    const [tracks, setTracks] = useState<MusicTrack[]>(initialTracks || []);
    const [currentIndex, setCurrentIndex] = useState(0);
    const [isPlaying, setIsPlaying] = useState(false);
    const [mediaPort, setMediaPort] = useState<number | null>(null);
    const audioRef = useRef<HTMLAudioElement | null>(null);

    const currentTrack = tracks[currentIndex] || null;

    const getTrackUrl = useCallback((track: MusicTrack): string | null => {
        if (track.url) return track.url;
        if (track.src) return track.src;
        if (track.blob_hash && mediaPort) {
            return `http://127.0.0.1:${mediaPort}/media/${track.blob_hash}`;
        }
        return null;
    }, [mediaPort]);

    // ===== Section: Data bootstrap and playback effects =====
    // Get media server port on mount
    useEffect(() => {
        invoke<number>('get_media_server_port').then(setMediaPort).catch(() => {});
    }, []);

    // Load saved widget data on mount
    useEffect(() => {
        if (initialTracks && initialTracks.length > 0) return;
        invoke('get_widget_data').then((raw) => {
            if (!raw) return;
            const data: WidgetData = JSON.parse(raw as string);
            if (data.music_playlist && data.music_playlist.length > 0) {
                setTracks(data.music_playlist);
            }
        }).catch(() => {});
    }, [initialTracks]);

    // Play/pause audio when state changes
    useEffect(() => {
        if (!audioRef.current || !currentTrack) return;
        const url = getTrackUrl(currentTrack);
        if (!url) return;

        if (audioRef.current.src !== url) {
            audioRef.current.src = url;
        }
        if (isPlaying) {
            audioRef.current.play().catch(() => setIsPlaying(false));
        } else {
            audioRef.current.pause();
        }
    }, [isPlaying, currentTrack, getTrackUrl]);

    // Auto-advance on track end
    useEffect(() => {
        const audio = audioRef.current;
        if (!audio) return;
        const onEnded = () => {
            if (currentIndex < tracks.length - 1) {
                setCurrentIndex(i => i + 1);
                setIsPlaying(true);
            } else {
                setIsPlaying(false);
            }
        };
        audio.addEventListener('ended', onEnded);
        return () => audio.removeEventListener('ended', onEnded);
    }, [currentIndex, tracks.length]);

    // ===== Section: User actions =====
    const handlePlayPause = () => {
        if (tracks.length === 0) return;
        setIsPlaying(p => !p);
    };

    const handleNext = () => {
        if (currentIndex < tracks.length - 1) {
            setCurrentIndex(i => i + 1);
            setIsPlaying(true);
        }
    };

    const handlePrev = () => {
        if (currentIndex > 0) {
            setCurrentIndex(i => i - 1);
            setIsPlaying(true);
        }
    };

    const handleAddFromVault = async () => {
        try {
            const files = await invoke<Array<{ name: string; mime: string | null; blob_hash: string }>>('vault_list_files');
            const audioFiles = files.filter(f =>
                f.mime?.startsWith('audio/') ||
                f.name.match(/\.(mp3|wav|ogg|flac|m4a|aac|opus)$/i)
            );
            if (audioFiles.length === 0) {
                alert('No audio files found in your vault. Upload some first!');
                return;
            }
            const newTracks: MusicTrack[] = audioFiles.map(f => ({
                id: crypto.randomUUID(),
                title: f.name.replace(/\.[^.]+$/, ''),
                artist: 'Unknown Artist',
                blob_hash: f.blob_hash,
            }));
            const merged = [...tracks, ...newTracks];
            setTracks(merged);
            persistTracks(merged);
        } catch (err) {
            console.error('Failed to load vault audio:', err);
        }
    };

    const handleRemoveTrack = (id: string) => {
        const updated = tracks.filter(t => t.id !== id);
        setTracks(updated);
        if (currentIndex >= updated.length) {
            setCurrentIndex(Math.max(0, updated.length - 1));
        }
        persistTracks(updated);
    };

    const persistTracks = useCallback((newTracks: MusicTrack[]) => {
        // Cross-system boundary: only mutate `music_playlist` while preserving
        // all other widget-owned keys in `WidgetData`.
        invoke('get_widget_data').then((raw) => {
            const data: WidgetData = raw ? JSON.parse(raw as string) : {};
            data.music_playlist = newTracks;
            invoke('save_widget_data', { widgetData: JSON.stringify(data) }).catch(console.error);
        }).catch(console.error);
        onUpdate?.(newTracks);
    }, [onUpdate]);

    // ===== Section: Render =====
    return (
        <div style={{
            background: 'var(--bg-secondary)',
            border: '2px solid var(--border-color)',
            borderRadius: 'var(--border-radius)',
            padding: 16,
            overflow: 'hidden',
        } as React.CSSProperties}>
            <audio ref={audioRef} preload="none" />

            {/* Now Playing Display */}
            <div style={{
                display: 'flex',
                alignItems: 'center',
                gap: 12,
                marginBottom: 12,
            } as React.CSSProperties}>
                <div
                    className={isPlaying ? 'music-disc spinning' : 'music-disc'}
                    style={{
                        width: 48,
                        height: 48,
                        borderRadius: '50%',
                        background: 'linear-gradient(135deg, var(--accent-purple), var(--accent-blue))',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        flexShrink: 0,
                    } as React.CSSProperties}
                >
                    <BsMusicNoteBeamed size={20} color="#fff" />
                </div>
                <div style={{ minWidth: 0, flex: 1 }}>
                    <div style={{
                        color: 'var(--text-primary)',
                        fontWeight: 600,
                        fontSize: 14,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                    } as React.CSSProperties}>
                        {currentTrack?.title || 'No tracks'}
                    </div>
                    <div style={{ color: 'var(--text-muted)', fontSize: 12 }}>
                        {currentTrack?.artist || 'Add songs to your playlist'}
                    </div>
                </div>
            </div>

            {/* Transport Controls */}
            <div style={{
                display: 'flex',
                justifyContent: 'center',
                gap: 12,
                marginBottom: 12,
            } as React.CSSProperties}>
                <button onClick={handlePrev} disabled={currentIndex <= 0}
                    style={{ background: 'none', border: 'none', color: 'var(--text-primary)', cursor: 'pointer', opacity: currentIndex <= 0 ? 0.3 : 1, fontSize: 20 } as React.CSSProperties}>
                    <BsSkipBackwardFill />
                </button>
                <button onClick={handlePlayPause}
                    style={{
                        background: 'var(--accent-purple)',
                        border: 'none',
                        borderRadius: '50%',
                        width: 40,
                        height: 40,
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        cursor: 'pointer',
                        color: '#fff',
                        fontSize: 18,
                    } as React.CSSProperties}>
                    {isPlaying ? <BsPauseFill /> : <BsPlayFill />}
                </button>
                <button onClick={handleNext} disabled={currentIndex >= tracks.length - 1}
                    style={{ background: 'none', border: 'none', color: 'var(--text-primary)', cursor: 'pointer', opacity: currentIndex >= tracks.length - 1 ? 0.3 : 1, fontSize: 20 } as React.CSSProperties}>
                    <BsSkipForwardFill />
                </button>
            </div>

            {/* Playlist */}
            {tracks.length > 0 && (
                <div style={{
                    maxHeight: 150,
                    overflowY: 'auto',
                    borderTop: '1px solid var(--border-color)',
                    paddingTop: 8,
                } as React.CSSProperties}>
                    {tracks.map((t, i) => (
                        <div key={t.id} style={{
                            display: 'flex',
                            alignItems: 'center',
                            gap: 8,
                            padding: '4px 0',
                            cursor: 'pointer',
                            opacity: i === currentIndex ? 1 : 0.7,
                            background: i === currentIndex ? 'var(--bg-tertiary)' : 'transparent',
                            borderRadius: 4,
                            paddingLeft: 8,
                            paddingRight: 8,
                        } as React.CSSProperties}
                            onClick={() => { setCurrentIndex(i); setIsPlaying(true); }}
                        >
                            <span style={{ fontSize: 11, color: 'var(--text-muted)', width: 20 } as React.CSSProperties}>
                                {i === currentIndex && isPlaying ? '▶' : `${i + 1}.`}
                            </span>
                            <span style={{ flex: 1, fontSize: 13, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' } as React.CSSProperties}>
                                {t.title} — <span style={{ color: 'var(--text-muted)' }}>{t.artist || 'Unknown'}</span>
                            </span>
                            {editable && (
                                <button
                                    onClick={(e) => { e.stopPropagation(); handleRemoveTrack(t.id); }}
                                    style={{ background: 'none', border: 'none', color: '#f44336', cursor: 'pointer', fontSize: 14, padding: 2 } as React.CSSProperties}
                                    title="Remove track"
                                >
                                    <BsTrash />
                                </button>
                            )}
                        </div>
                    ))}
                </div>
            )}

            {/* Add From Vault */}
            {editable && (
                <button
                    onClick={handleAddFromVault}
                    style={{
                        width: '100%',
                        marginTop: 8,
                        background: 'var(--bg-tertiary)',
                        border: '1px dashed var(--border-color)',
                        borderRadius: 4,
                        padding: '8px 0',
                        cursor: 'pointer',
                        color: 'var(--text-secondary)',
                        fontSize: 13,
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'center',
                        gap: 6,
                    } as React.CSSProperties}
                >
                    <BsPlusCircle /> Add Audio from Vault
                </button>
            )}
        </div>
    );
}
