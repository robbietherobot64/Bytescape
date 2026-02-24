import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { VaultFile, VaultFileKind, VaultFileStats } from '../types';

import { VaultFileModal } from '../components/VaultFileModal';

type ViewMode = 'grid' | 'list';
type KindFilter = 'all' | VaultFileKind;

interface VaultScreenProps {
    auth?: { handle: string; did: string } | null;
}

const KIND_EMOJI: Record<VaultFileKind | 'all', string> = {
    all: '🗂️',
    Image: '🖼️',
    Video: '🎬',
    Audio: '🎵',
    Document: '📄',
    Archive: '📦',
    Other: '📁',
};

const SOURCE_EMOJI: Record<string, string> = {
    bluesky: '🦋',
    mastodon: '🐘',
    facebook: '📘',
    upload: '⬆️',
    manual: '✏️',
};

function sourceEmoji(source: string): string {
    return SOURCE_EMOJI[source.toLowerCase()] ?? '📄';
}

function formatFileSize(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

function formatDate(ts: number): string {
    if (ts <= 0) return '—';
    const d = new Date(ts * 1000);
    return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

function formatTimeFull(ts: number): string {
    if (ts <= 0) return '—';
    const d = new Date(ts * 1000);
    return d.toLocaleString(undefined, {
        year: 'numeric', month: 'short', day: 'numeric',
        hour: '2-digit', minute: '2-digit',
    });
}

/** Cache of thumbnail object URLs keyed by file ID to avoid re-fetching. */
const thumbCache = new Map<string, string>();

function FileThumbnail({ file }: { file: VaultFile }) {
    const [thumbUrl, setThumbUrl] = useState<string | null>(thumbCache.get(file.id) ?? null);

    useEffect(() => {
        if (thumbUrl) return; // already loaded from cache
        if (file.kind !== 'Image' || !file.thumbnail_hash) return;

        let cancelled = false;
        invoke<number[]>('vault_get_thumbnail', { fileId: file.id })
            .then(bytes => {
                if (cancelled || !bytes) return;
                const blob = new Blob([new Uint8Array(bytes)], { type: 'image/webp' });
                const url = URL.createObjectURL(blob);
                thumbCache.set(file.id, url);
                setThumbUrl(url);
            })
            .catch(() => { /* silently fail */ });

        return () => { cancelled = true; };
    }, [file.id, file.kind, file.thumbnail_hash, thumbUrl]);

    if (thumbUrl) {
        return <img src={thumbUrl} alt={file.name} className="vault-card-img" loading="lazy" />;
    }

    return (
        <div className="vault-card-placeholder">
            {KIND_EMOJI[file.kind]}
        </div>
    );
}

export function VaultScreen({ auth: _auth }: VaultScreenProps) {
    const [files, setFiles] = useState<VaultFile[]>([]);
    const [stats, setStats] = useState<VaultFileStats | null>(null);
    const [loading, setLoading] = useState(false);
    const [uploading, setUploading] = useState(false);
    const [dragOver, setDragOver] = useState(false);
    const [uploadError, setUploadError] = useState<string | null>(null);

    // Filters
    const [kindFilter, setKindFilter] = useState<KindFilter>('all');
    const [sourceFilter, setSourceFilter] = useState<string | null>(null);
    const [searchQuery, setSearchQuery] = useState('');
    const [viewMode, setViewMode] = useState<ViewMode>('grid');

    // Detail modal
    const [selectedFile, setSelectedFile] = useState<VaultFile | null>(null);
    const dedupeCheckedRef = useRef(false);

    // Load on mount
    useEffect(() => {
        loadFiles();
        loadStats();
    }, []);

    async function loadFiles() {
        setLoading(true);
        try {
            if (!dedupeCheckedRef.current) {
                dedupeCheckedRef.current = true;
                const removed = await invoke<number>('vault_deduplicate_files');
                if (removed > 0) {
                    console.info(`Vault: removed ${removed} duplicate file entr${removed === 1 ? 'y' : 'ies'}`);
                }
            }
            const result = await invoke<VaultFile[]>('vault_list_files');
            setFiles(result);
        } catch (e) {
            console.error('Failed to load vault files:', e);
        } finally {
            setLoading(false);
        }
    }

    async function loadStats() {
        try {
            const s = await invoke<VaultFileStats>('vault_get_stats');
            setStats(s);
        } catch (e) {
            console.error('Failed to load vault stats:', e);
        }
    }

    // Upload files via native file picker → Rust reads files from disk (no IPC bloat)
    const pickAndUploadFiles = useCallback(async () => {
        try {
            const selected = await open({
                multiple: true,
                title: 'Select files to upload to Vault',
            });
            if (!selected) return; // User cancelled
            const paths = Array.isArray(selected) ? selected : [selected];
            if (paths.length === 0) return;

            setUploading(true);
            setUploadError(null);
            try {
                await invoke<VaultFile[]>('vault_upload_files_from_paths', {
                    paths,
                    tags: [],
                });
            } catch (e: any) {
                const msg = typeof e === 'string' ? e : (e?.message ?? 'Unknown error');
                console.error('Failed to upload files:', msg);
                setUploadError(msg);
            }
            setUploading(false);
            loadFiles();
            loadStats();
        } catch (e: any) {
            console.error('File picker error:', e);
        }
    }, []);

    // Small-file upload for drag-and-drop (capped at 10MB per file to avoid freeze)
    const MAX_DROP_FILE_SIZE = 10 * 1024 * 1024; // 10MB

    const uploadDroppedFiles = useCallback(async (fileList: FileList | File[]) => {
        setUploading(true);
        setUploadError(null);
        const arr = Array.from(fileList);
        const tooLarge = arr.filter(f => f.size > MAX_DROP_FILE_SIZE);
        if (tooLarge.length > 0) {
            setUploadError(
                `${tooLarge.length} file(s) too large for drag-and-drop (max 10MB). Use the Upload button instead:\n` +
                tooLarge.map(f => `  ${f.name} (${(f.size / 1024 / 1024).toFixed(1)}MB)`).join('\n')
            );
            const arr2 = arr.filter(f => f.size <= MAX_DROP_FILE_SIZE);
            if (arr2.length === 0) {
                setUploading(false);
                return;
            }
        }
        const errors: string[] = [];
        for (const file of arr.filter(f => f.size <= MAX_DROP_FILE_SIZE)) {
            try {
                const buffer = await file.arrayBuffer();
                const data = Array.from(new Uint8Array(buffer));
                await invoke<VaultFile>('vault_upload_file', {
                    name: file.name,
                    mimeType: file.type || 'application/octet-stream',
                    data,
                    source: 'upload',
                    itemId: null,
                    createdAt: Math.floor(file.lastModified / 1000) || null,
                    tags: [],
                });
            } catch (e: any) {
                const msg = typeof e === 'string' ? e : (e?.message ?? 'Unknown error');
                console.error('Failed to upload file:', file.name, msg);
                errors.push(`${file.name}: ${msg}`);
            }
        }
        if (errors.length > 0) {
            setUploadError(prev => (prev ? prev + '\n' : '') + errors.join('\n'));
        }
        setUploading(false);
        loadFiles();
        loadStats();
    }, []);

    // Drag and drop
    function handleDragOver(e: React.DragEvent) {
        e.preventDefault();
        e.stopPropagation();
        setDragOver(true);
    }

    function handleDragLeave(e: React.DragEvent) {
        e.preventDefault();
        e.stopPropagation();
        setDragOver(false);
    }

    function handleDrop(e: React.DragEvent) {
        e.preventDefault();
        e.stopPropagation();
        setDragOver(false);
        if (e.dataTransfer.files.length > 0) {
            uploadDroppedFiles(e.dataTransfer.files);
        }
    }

    // Delete file
    async function deleteFile(fileId: string) {
        try {
            await invoke('vault_delete_file', { fileId });
            setFiles(prev => prev.filter(f => f.id !== fileId));
            setSelectedFile(null);
            loadStats();
        } catch (e) {
            console.error('Failed to delete file:', e);
        }
    }

    // Filtered files
    const filteredFiles = files.filter(f => {
        if (kindFilter !== 'all' && f.kind !== kindFilter) return false;
        if (sourceFilter && f.source !== sourceFilter) return false;
        if (searchQuery) {
            const q = searchQuery.toLowerCase();
            return f.name.toLowerCase().includes(q) ||
                   f.tags.some(t => t.toLowerCase().includes(q));
        }
        return true;
    });

    // Unique sources for tabs
    const allSources = [...new Set(files.map(f => f.source))];

    const selectedIndex = selectedFile
        ? filteredFiles.findIndex(f => f.id === selectedFile.id)
        : -1;

    const canNavigateModal = selectedIndex >= 0 && filteredFiles.length > 1;

    const showPreviousFile = useCallback(() => {
        if (!selectedFile || filteredFiles.length < 2 || selectedIndex < 0) return;
        const prevIndex = (selectedIndex - 1 + filteredFiles.length) % filteredFiles.length;
        setSelectedFile(filteredFiles[prevIndex]);
    }, [selectedFile, filteredFiles, selectedIndex]);

    const showNextFile = useCallback(() => {
        if (!selectedFile || filteredFiles.length < 2 || selectedIndex < 0) return;
        const nextIndex = (selectedIndex + 1) % filteredFiles.length;
        setSelectedFile(filteredFiles[nextIndex]);
    }, [selectedFile, filteredFiles, selectedIndex]);

    useEffect(() => {
        if (!selectedFile) return;

        function onKeyDown(e: KeyboardEvent) {
            if (e.key === 'ArrowLeft') {
                e.preventDefault();
                showPreviousFile();
            } else if (e.key === 'ArrowRight') {
                e.preventDefault();
                showNextFile();
            } else if (e.key === 'Escape') {
                e.preventDefault();
                setSelectedFile(null);
            }
        }

        window.addEventListener('keydown', onKeyDown);
        return () => window.removeEventListener('keydown', onKeyDown);
    }, [selectedFile, showPreviousFile, showNextFile]);

    return (
        <div
            className={`vault-screen ${dragOver ? 'vault-drag-over' : ''}`}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
        >
            {/* Drag overlay */}
            {dragOver && (
                <div className="vault-drop-overlay">
                    <div className="vault-drop-icon">📥</div>
                    <div className="vault-drop-text">Drop files to upload</div>
                </div>
            )}

            {/* Header */}
            <div className="vault-header">
                <div className="vault-title-row">
                    <h1 className="vault-title">📁 Vault</h1>
                    <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                        <button
                            className="vault-upload-btn"
                            onClick={pickAndUploadFiles}
                            disabled={uploading}
                        >
                            {uploading ? '⏳ Uploading...' : '⬆️ Upload'}
                        </button>
                        <div className="vault-view-toggle">
                            <button
                                className={`vault-view-btn ${viewMode === 'grid' ? 'active' : ''}`}
                                onClick={() => setViewMode('grid')}
                                title="Grid view"
                            >⊞</button>
                            <button
                                className={`vault-view-btn ${viewMode === 'list' ? 'active' : ''}`}
                                onClick={() => setViewMode('list')}
                                title="List view"
                            >☰</button>
                        </div>
                    </div>
                </div>

                {stats && (
                    <div className="vault-stats">
                        <div className="vault-stat">
                            <span className="vault-stat-number">{stats.total_files}</span>
                            <span className="vault-stat-label">Files</span>
                        </div>
                        <div className="vault-stat">
                            <span className="vault-stat-number">{formatFileSize(stats.total_size)}</span>
                            <span className="vault-stat-label">Storage</span>
                        </div>
                        {Object.entries(stats.by_kind).map(([kind, count]) => (
                            <div key={kind} className="vault-stat">
                                <span className="vault-stat-number">{count}</span>
                                <span className="vault-stat-label">{KIND_EMOJI[kind as VaultFileKind] ?? '📁'} {kind}</span>
                            </div>
                        ))}
                    </div>
                )}
            </div>

            {/* Filter Bar */}
            <div className="vault-filters">
                <div className="vault-filter-row">
                    {/* Source tabs */}
                    <div className="vault-source-tabs">
                        <button
                            className={`vault-source-tab ${sourceFilter === null ? 'active' : ''}`}
                            onClick={() => setSourceFilter(null)}
                        >
                            All
                        </button>
                        {allSources.map(s => (
                            <button
                                key={s}
                                className={`vault-source-tab ${sourceFilter === s ? 'active' : ''}`}
                                onClick={() => setSourceFilter(s)}
                            >
                                {sourceEmoji(s)} {s}
                            </button>
                        ))}
                    </div>

                    {/* Search */}
                    <input
                        className="vault-search"
                        type="text"
                        placeholder="Search files..."
                        value={searchQuery}
                        onChange={e => setSearchQuery(e.target.value)}
                    />
                </div>

                {/* Kind filters */}
                <div className="vault-type-filters">
                    {(['all', 'Image', 'Video', 'Audio', 'Document', 'Archive', 'Other'] as KindFilter[]).map(k => (
                        <button
                            key={k}
                            className={`vault-type-btn ${kindFilter === k ? 'active' : ''}`}
                            onClick={() => setKindFilter(k)}
                        >
                            {KIND_EMOJI[k]} {k === 'all' ? 'All' : k}
                        </button>
                    ))}
                </div>
            </div>

            {/* Content */}
            <div className="vault-content">
                {uploadError && (
                    <div className="vault-error">
                        <span>⚠️ Upload failed:</span>
                        <pre>{uploadError}</pre>
                        <button onClick={() => setUploadError(null)}>Dismiss</button>
                    </div>
                )}

                {loading && (
                    <div className="vault-loading">
                        <span className="vault-spinner">⏳</span> Loading vault...
                    </div>
                )}

                {!loading && filteredFiles.length === 0 && (
                    <div className="vault-empty">
                        <div className="vault-empty-icon">📭</div>
                        <p>No files in vault yet.</p>
                        <p className="text-muted">
                            Drag &amp; drop files here, or click Upload to add media.
                        </p>
                    </div>
                )}

                {!loading && filteredFiles.length > 0 && (
                    <>
                        <div className="vault-result-count">
                            {filteredFiles.length} file{filteredFiles.length !== 1 ? 's' : ''}
                            {filteredFiles.length !== files.length && ` (of ${files.length})`}
                        </div>

                        {viewMode === 'grid' ? (
                            <div className="vault-grid">
                                {filteredFiles.map(file => (
                                    <div
                                        key={file.id}
                                        className="vault-card"
                                        onClick={() => setSelectedFile(file)}
                                    >
                                        <div className="vault-card-thumb">
                                            <FileThumbnail file={file} />
                                        </div>
                                        <div className="vault-card-info">
                                            <div className="vault-card-meta">
                                                <span className="vault-card-source">
                                                    {sourceEmoji(file.source)}
                                                </span>
                                                <span className="vault-card-size">
                                                    {formatFileSize(file.size)}
                                                </span>
                                            </div>
                                            <div className="vault-card-text" title={file.name}>
                                                {file.name}
                                            </div>
                                            {file.tags.length > 0 && (
                                                <div className="vault-card-tags">
                                                    {file.tags.slice(0, 3).map(t => (
                                                        <span key={t} className="vault-tag">#{t}</span>
                                                    ))}
                                                </div>
                                            )}
                                        </div>
                                    </div>
                                ))}
                            </div>
                        ) : (
                            <div className="vault-list">
                                {filteredFiles.map(file => (
                                    <div
                                        key={file.id}
                                        className="vault-list-row"
                                        onClick={() => setSelectedFile(file)}
                                    >
                                        <span className="vault-list-icon">
                                            {KIND_EMOJI[file.kind]}
                                        </span>
                                        <span className="vault-list-name">
                                            {file.name}
                                        </span>
                                        <span className="vault-list-kind">
                                            {file.kind}
                                        </span>
                                        <span className="vault-list-size">
                                            {formatFileSize(file.size)}
                                        </span>
                                        <span className="vault-list-source">
                                            {sourceEmoji(file.source)}
                                        </span>
                                        <span className="vault-list-date">
                                            {formatDate(file.added_at)}
                                        </span>
                                    </div>
                                ))}
                            </div>
                        )}
                    </>
                )}
            </div>

            {/* Detail Modal */}
            {selectedFile && (
                <VaultFileModal
                    selectedFile={selectedFile}
                    selectedIndex={selectedIndex}
                    filteredFilesLength={filteredFiles.length}
                    canNavigateModal={canNavigateModal}
                    onClose={() => setSelectedFile(null)}
                    onPrevious={showPreviousFile}
                    onNext={showNextFile}
                    onDelete={deleteFile}
                    KIND_EMOJI={KIND_EMOJI}
                    sourceEmoji={sourceEmoji}
                    formatFileSize={formatFileSize}
                    formatTimeFull={formatTimeFull}
                />
            )}
        </div>
    );
}
