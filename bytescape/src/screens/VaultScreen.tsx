import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { VaultFile, VaultFileKind, VaultFileStats } from '../types';
import { Win98Box } from '../components/Win98Box';
import { VaultFileModal } from '../components/VaultFileModal';
import { BsFolderFill, BsFolder2Open, BsSearch, BsGrid, BsListUl, BsUpload, BsChevronRight } from 'react-icons/bs';

/**
 * PURPOSE
 * - Present a local-first file explorer for imported and uploaded vault media/documents.
 *
 * HOW IT WORKS
 * - Loads metadata and aggregate stats from Rust commands, then composes a virtual explorer UI
 *   (tree filters, breadcrumbs, grid/list views, and detail modal navigation).
 * - Supports two upload paths:
 *   1) Native picker via filesystem paths for large files (`vault_upload_files_from_paths`)
 *   2) Drag-and-drop byte IPC for small files (`vault_upload_file`, capped to reduce UI freeze risk)
 * - Uses in-memory thumbnail URL caching with LRU eviction to balance responsiveness vs Blob memory growth.
 *
 * RELATED FILES
 * - src/components/VaultFileModal.tsx (detail preview / delete / next-prev navigation)
 * - src/types/index.ts (`VaultFile`, `VaultFileStats`, `VaultFileKind` contracts)
 * - src-tauri/src/lib.rs and vault backend modules (Tauri commands invoked here)
 */

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

/**
 * Cache of thumbnail object URLs keyed by file ID to avoid re-fetching.
 * Capped at 200 entries with LRU eviction — evicted blob URLs are revoked
 * to release the underlying Blob memory. Without this cap, hundreds of
 * blob URLs accumulate during long sessions (each holding 10-50KB).
 */
const THUMB_CACHE_MAX = 200;
const thumbCache = new Map<string, string>();

/** Set a thumbnail URL with LRU eviction + blob URL revocation. */
function cacheThumbUrl(id: string, url: string): void {
    // Delete first so re-insertion moves to end (most-recently-used)
    thumbCache.delete(id);
    if (thumbCache.size >= THUMB_CACHE_MAX) {
        const firstKey = thumbCache.keys().next().value;
        if (firstKey !== undefined) {
            const evictedUrl = thumbCache.get(firstKey);
            if (evictedUrl) URL.revokeObjectURL(evictedUrl);
            thumbCache.delete(firstKey);
        }
    }
    thumbCache.set(id, url);
}

/**
 * Thumbnail view with lazy Rust fetch + shared in-memory object URL cache.
 */
function FileThumbnail({ file }: { file: VaultFile }) {
    const [thumbUrl, setThumbUrl] = useState<string | null>(thumbCache.get(file.id) ?? null);

    useEffect(() => {
        if (thumbUrl) return; // already loaded from cache
        if (file.kind !== 'Image') return;

        let cancelled = false;
        (async () => {
            try {
                // Try dedicated thumbnail first
                if (file.thumbnail_hash) {
                    const bytes = await invoke<number[]>('vault_get_thumbnail', { fileId: file.id });
                    if (!cancelled && bytes) {
                        const blob = new Blob([new Uint8Array(bytes)], { type: 'image/webp' });
                        const url = URL.createObjectURL(blob);
                        cacheThumbUrl(file.id, url);
                        setThumbUrl(url);
                        return;
                    }
                }
                // Fall back to full image
                const bytes = await invoke<number[]>('vault_get_file_data', { fileId: file.id });
                if (!cancelled && bytes) {
                    const blob = new Blob([new Uint8Array(bytes)], { type: file.mime_type || 'image/jpeg' });
                    const url = URL.createObjectURL(blob);
                    cacheThumbUrl(file.id, url);
                    setThumbUrl(url);
                }
            } catch { /* silently fail */ }
        })();

        return () => { cancelled = true; };
    }, [file.id, file.kind, file.thumbnail_hash, file.mime_type, thumbUrl]);

    if (thumbUrl) {
        return <img src={thumbUrl} alt={file.name} className="vault-card-img" loading="lazy" />;
    }

    return (
        <div className="vault-card-placeholder">
            {KIND_EMOJI[file.kind]}
        </div>
    );
}

/**
 * Vault explorer screen.
 *
 * Cross-system notes:
 * - Engine interaction: all file CRUD/stats/thumbnail operations are Tauri `invoke()` calls.
 * - Native integration: file picker comes from Tauri dialog plugin to avoid browser sandbox limits.
 * - Composition boundary: modal preview/deletion flow is delegated to `VaultFileModal`.
 */
export function VaultScreen({ auth: _auth }: VaultScreenProps) {
    const [files, setFiles] = useState<VaultFile[]>([]);
    const [stats, setStats] = useState<VaultFileStats | null>(null);
    const [loading, setLoading] = useState(false);
    const [uploading, setUploading] = useState(false);
    const [dragOver, setDragOver] = useState(false);
    const [uploadError, setUploadError] = useState<string | null>(null);

    // Filters
    const [kindFilter, setKindFilter] = useState<KindFilter>('all');
    const [searchQuery, setSearchQuery] = useState('');
    const [viewMode, setViewMode] = useState<ViewMode>('grid');

    // Detail modal
    const [selectedFile, setSelectedFile] = useState<VaultFile | null>(null);
    const dedupeCheckedRef = useRef(false);

    // ═══ Lifecycle + Engine Fetch ═══

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

    // ═══ Upload Flows (Path-based + Byte-based) ═══

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

    // ═══ Drag-and-Drop Interaction ═══

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

    // ═══ Mutations + Local Refresh ═══

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

    // ═══ Explorer Model (Folders, Filters, Breadcrumbs) ═══

    // Virtual folder tree
    type FolderPath = { type: 'root' } | { type: 'source'; source: string } | { type: 'kind'; kind: VaultFileKind } | { type: 'tag'; tag: string };
    const [currentFolder, setCurrentFolder] = useState<FolderPath>({ type: 'root' });
    const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['sources', 'types']));

    const toggleFolder = (key: string) => {
        setExpandedFolders(prev => {
            const next = new Set(prev);
            next.has(key) ? next.delete(key) : next.add(key);
            return next;
        });
    };

    // All unique tags
    const allTags = useMemo(() => {
        const tags = new Set<string>();
        files.forEach(f => f.tags.forEach(t => tags.add(t)));
        return [...tags].sort();
    }, [files]);

    // Apply folder-based filtering on top of search
    const filteredFiles = useMemo(() => {
        let result = files;

        // Folder filter
        if (currentFolder.type === 'source') {
            result = result.filter(f => f.source === currentFolder.source);
        } else if (currentFolder.type === 'kind') {
            result = result.filter(f => f.kind === currentFolder.kind);
        } else if (currentFolder.type === 'tag') {
            result = result.filter(f => f.tags.includes(currentFolder.tag));
        }

        // Additional kind filter (from toolbar)
        if (kindFilter !== 'all') {
            result = result.filter(f => f.kind === kindFilter);
        }

        // Search
        if (searchQuery) {
            const q = searchQuery.toLowerCase();
            result = result.filter(f =>
                f.name.toLowerCase().includes(q) ||
                f.tags.some(t => t.toLowerCase().includes(q))
            );
        }

        return result;
    }, [files, currentFolder, kindFilter, searchQuery]);

    // Breadcrumb
    const breadcrumbs = useMemo(() => {
        const crumbs: { label: string; folder: FolderPath }[] = [
            { label: 'Vault', folder: { type: 'root' } },
        ];
        if (currentFolder.type === 'source') {
            crumbs.push({ label: 'Sources', folder: { type: 'root' } });
            crumbs.push({ label: currentFolder.source, folder: currentFolder });
        } else if (currentFolder.type === 'kind') {
            crumbs.push({ label: 'Types', folder: { type: 'root' } });
            crumbs.push({ label: currentFolder.kind, folder: currentFolder });
        } else if (currentFolder.type === 'tag') {
            crumbs.push({ label: 'Tags', folder: { type: 'root' } });
            crumbs.push({ label: `#${currentFolder.tag}`, folder: currentFolder });
        }
        return crumbs;
    }, [currentFolder]);

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

    // ═══ Keyboard Navigation + Render Helpers ═══

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

    const folderActive = (f: FolderPath) => {
        if (f.type === currentFolder.type) {
            if (f.type === 'root') return true;
            if (f.type === 'source' && currentFolder.type === 'source') return f.source === currentFolder.source;
            if (f.type === 'kind' && currentFolder.type === 'kind') return f.kind === currentFolder.kind;
            if (f.type === 'tag' && currentFolder.type === 'tag') return f.tag === currentFolder.tag;
        }
        return false;
    };

    return (
        <div
            className={`vault-explorer ${dragOver ? 'vault-drag-over' : ''}`}
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

            {/* ═══ Toolbar ═══ */}
            <div className="vault-toolbar">
                <div className="vault-toolbar-left">
                    <button
                        className="vault-toolbar-btn"
                        onClick={pickAndUploadFiles}
                        disabled={uploading}
                        title="Upload files"
                    >
                        <BsUpload size={11} />
                        <span>{uploading ? 'Uploading...' : 'Upload'}</span>
                    </button>

                    <span className="vault-toolbar-sep" />

                    {/* Kind filter pills */}
                    {(['all', 'Image', 'Video', 'Audio', 'Document'] as KindFilter[]).map(k => (
                        <button
                            key={k}
                            className={`vault-toolbar-pill ${kindFilter === k ? 'active' : ''}`}
                            onClick={() => setKindFilter(k)}
                        >
                            {KIND_EMOJI[k]} {k === 'all' ? 'All' : k}
                        </button>
                    ))}
                </div>

                <div className="vault-toolbar-right">
                    <div className="vault-search-box">
                        <BsSearch size={10} />
                        <input
                            type="text"
                            placeholder="Search files..."
                            value={searchQuery}
                            onChange={e => setSearchQuery(e.target.value)}
                        />
                    </div>

                    <div className="vault-view-toggle">
                        <button
                            className={`vault-view-btn ${viewMode === 'grid' ? 'active' : ''}`}
                            onClick={() => setViewMode('grid')}
                            title="Grid view"
                        ><BsGrid size={11} /></button>
                        <button
                            className={`vault-view-btn ${viewMode === 'list' ? 'active' : ''}`}
                            onClick={() => setViewMode('list')}
                            title="List view"
                        ><BsListUl size={11} /></button>
                    </div>
                </div>
            </div>

            {/* ═══ Address bar / Breadcrumbs ═══ */}
            <div className="vault-address-bar">
                <span className="vault-address-icon">📁</span>
                {breadcrumbs.map((crumb, i) => (
                    <span key={i} className="vault-breadcrumb-item">
                        {i > 0 && <BsChevronRight size={8} className="vault-breadcrumb-sep" />}
                        <button
                            className={`vault-breadcrumb-btn ${i === breadcrumbs.length - 1 ? 'current' : ''}`}
                            onClick={() => setCurrentFolder(crumb.folder)}
                        >
                            {crumb.label}
                        </button>
                    </span>
                ))}
                <span className="vault-address-count">
                    {filteredFiles.length} item{filteredFiles.length !== 1 ? 's' : ''}
                </span>
            </div>

            {/* ═══ Main Explorer Layout ═══ */}
            <div className="vault-explorer-body">
                {/* Left: Folder Tree */}
                <Win98Box variant="sunken" className="vault-tree-panel">
                    {/* All Files */}
                    <div
                        className={`vault-tree-item ${folderActive({ type: 'root' }) ? 'active' : ''}`}
                        onClick={() => setCurrentFolder({ type: 'root' })}
                    >
                        <BsFolder2Open size={12} className="vault-tree-icon" />
                        <span>All Files</span>
                        <span className="vault-tree-count">{files.length}</span>
                    </div>

                    {/* By Source */}
                    <div className="vault-tree-group">
                        <div
                            className="vault-tree-header"
                            onClick={() => toggleFolder('sources')}
                        >
                            <span className={`vault-tree-arrow ${expandedFolders.has('sources') ? 'open' : ''}`}>▸</span>
                            <BsFolderFill size={12} className="vault-tree-icon" style={{ color: '#e8a926' }} />
                            <span>Sources</span>
                        </div>
                        {expandedFolders.has('sources') && allSources.map(s => (
                            <div
                                key={s}
                                className={`vault-tree-item indent ${folderActive({ type: 'source', source: s }) ? 'active' : ''}`}
                                onClick={() => setCurrentFolder({ type: 'source', source: s })}
                            >
                                <span className="vault-tree-source-icon">{sourceEmoji(s)}</span>
                                <span>{s}</span>
                                <span className="vault-tree-count">
                                    {files.filter(f => f.source === s).length}
                                </span>
                            </div>
                        ))}
                    </div>

                    {/* By Type */}
                    <div className="vault-tree-group">
                        <div
                            className="vault-tree-header"
                            onClick={() => toggleFolder('types')}
                        >
                            <span className={`vault-tree-arrow ${expandedFolders.has('types') ? 'open' : ''}`}>▸</span>
                            <BsFolderFill size={12} className="vault-tree-icon" style={{ color: '#e8a926' }} />
                            <span>Types</span>
                        </div>
                        {expandedFolders.has('types') && (['Image', 'Video', 'Audio', 'Document', 'Archive', 'Other'] as VaultFileKind[]).map(k => {
                            const count = files.filter(f => f.kind === k).length;
                            if (count === 0) return null;
                            return (
                                <div
                                    key={k}
                                    className={`vault-tree-item indent ${folderActive({ type: 'kind', kind: k }) ? 'active' : ''}`}
                                    onClick={() => setCurrentFolder({ type: 'kind', kind: k })}
                                >
                                    <span>{KIND_EMOJI[k]}</span>
                                    <span>{k}</span>
                                    <span className="vault-tree-count">{count}</span>
                                </div>
                            );
                        })}
                    </div>

                    {/* Tags */}
                    {allTags.length > 0 && (
                        <div className="vault-tree-group">
                            <div
                                className="vault-tree-header"
                                onClick={() => toggleFolder('tags')}
                            >
                                <span className={`vault-tree-arrow ${expandedFolders.has('tags') ? 'open' : ''}`}>▸</span>
                                <BsFolderFill size={12} className="vault-tree-icon" style={{ color: '#e8a926' }} />
                                <span>Tags</span>
                            </div>
                            {expandedFolders.has('tags') && allTags.map(tag => (
                                <div
                                    key={tag}
                                    className={`vault-tree-item indent ${folderActive({ type: 'tag', tag }) ? 'active' : ''}`}
                                    onClick={() => setCurrentFolder({ type: 'tag', tag })}
                                >
                                    <span>🏷️</span>
                                    <span>#{tag}</span>
                                    <span className="vault-tree-count">
                                        {files.filter(f => f.tags.includes(tag)).length}
                                    </span>
                                </div>
                            ))}
                        </div>
                    )}

                    {/* Stats Footer */}
                    {stats && (
                        <div className="vault-tree-stats">
                            <div>{stats.total_files} files</div>
                            <div>{formatFileSize(stats.total_size)}</div>
                        </div>
                    )}
                </Win98Box>

                {/* Right: File Content Area */}
                <div className="vault-content-area">
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
                            <p>No files found.</p>
                            <p className="text-muted">
                                Drag &amp; drop files here, or click Upload to add media.
                            </p>
                        </div>
                    )}

                    {!loading && filteredFiles.length > 0 && viewMode === 'grid' && (
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
                    )}

                    {!loading && filteredFiles.length > 0 && viewMode === 'list' && (
                        <div className="vault-list">
                            <div className="vault-list-header">
                                <span className="vault-list-col-icon"></span>
                                <span className="vault-list-col-name">Name</span>
                                <span className="vault-list-col-kind">Type</span>
                                <span className="vault-list-col-size">Size</span>
                                <span className="vault-list-col-source">Source</span>
                                <span className="vault-list-col-date">Added</span>
                            </div>
                            {filteredFiles.map(file => (
                                <div
                                    key={file.id}
                                    className="vault-list-row"
                                    onClick={() => setSelectedFile(file)}
                                >
                                    <span className="vault-list-col-icon">
                                        {KIND_EMOJI[file.kind]}
                                    </span>
                                    <span className="vault-list-col-name">
                                        {file.name}
                                    </span>
                                    <span className="vault-list-col-kind">
                                        {file.kind}
                                    </span>
                                    <span className="vault-list-col-size">
                                        {formatFileSize(file.size)}
                                    </span>
                                    <span className="vault-list-col-source">
                                        {sourceEmoji(file.source)}
                                    </span>
                                    <span className="vault-list-col-date">
                                        {formatDate(file.added_at)}
                                    </span>
                                </div>
                            ))}
                        </div>
                    )}
                </div>
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
