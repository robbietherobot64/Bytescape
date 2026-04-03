import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { VaultFile } from '../types';
import { VideoPlayer } from './VideoPlayer';

/**
 * PURPOSE
 * Renders the full-screen modal viewer for a selected vault file, including:
 * - Media/document preview
 * - Modal navigation controls (previous/next)
 * - Delete action
 * - Optional expanded file metadata panel
 *
 * HOW IT WORKS
 * 1) Watches `selectedFile` and requests raw bytes for previewable kinds via Tauri.
 * 2) Builds an in-memory Blob URL for preview rendering.
 * 3) Cleans up Blob URLs when file changes/unmount occurs.
 * 4) Delegates destructive action + navigation back to parent callbacks.
 *
 * RELATED FILES
 * - src/types/index.ts (VaultFile shape)
 * - src/components/VideoPlayer.tsx (video rendering)
 * - Rust command: `vault_get_file_data` (Tauri invoke backend)
 * - Parent vault screen/container that owns `selectedFile`, filtering, and navigation state
 *
 * CROSS-SYSTEM NOTE
 * This component does not read from a local store/provider directly. It bridges UI ⇄ Engine
 * by invoking `vault_get_file_data` to hydrate preview bytes for the selected file ID.
 */

interface VaultFileModalProps {
    selectedFile: VaultFile;
    selectedIndex: number;
    filteredFilesLength: number;
    canNavigateModal: boolean;
    onClose: () => void;
    onPrevious: () => void;
    onNext: () => void;
    onDelete: (fileId: string) => void;
    KIND_EMOJI: Record<string, string>;
    sourceEmoji: (source: string) => string;
    formatFileSize: (bytes: number) => string;
    formatTimeFull: (ts: number) => string;
}

/**
 * Public props contract for `VaultFileModal`.
 *
 * Parent owns all modal state and orchestration; this component remains presentation-focused
 * except for preview byte retrieval via Tauri invoke.
 */

export function VaultFileModal({
    selectedFile,
    selectedIndex,
    filteredFilesLength,
    canNavigateModal,
    onClose,
    onPrevious,
    onNext,
    onDelete,
    KIND_EMOJI,
    sourceEmoji,
    formatFileSize,
    formatTimeFull,
}: VaultFileModalProps) {
    // ═══════════════════════════════════════════════════════════════════════════
    // Local UI state
    // ═══════════════════════════════════════════════════════════════════════════
    const [showFileData, setShowFileData] = useState(false);
    const [previewUrl, setPreviewUrl] = useState<string | null>(null);
    const [previewLoading, setPreviewLoading] = useState(false);

    // ═══════════════════════════════════════════════════════════════════════════
    // Preview lifecycle (UI ⇄ Tauri command `vault_get_file_data`)
    // ═══════════════════════════════════════════════════════════════════════════
    useEffect(() => {
        if (!selectedFile) {
            if (previewUrl && previewUrl.startsWith('blob:')) {
                URL.revokeObjectURL(previewUrl);
            }
            setPreviewUrl(null);
            setShowFileData(false);
            return;
        }

        if (selectedFile.kind === 'Image' || selectedFile.kind === 'Video' || selectedFile.kind === 'Audio' || selectedFile.kind === 'Document') {
            setPreviewLoading(true);
            invoke<number[]>('vault_get_file_data', { fileId: selectedFile.id })
                .then(bytes => {
                    const blob = new Blob([new Uint8Array(bytes)], { type: selectedFile.mime_type });
                    const url = URL.createObjectURL(blob);
                    setPreviewUrl(url);
                })
                .catch(e => {
                    console.error('Failed to load preview:', e);
                })
                .finally(() => setPreviewLoading(false));
        }

        return () => {
            if (previewUrl && previewUrl.startsWith('blob:')) {
                URL.revokeObjectURL(previewUrl);
            }
        };
    }, [selectedFile?.id]);

    return (
        <div className="vault-modal-backdrop" onClick={onClose}>
            <div className="vault-modal" onClick={e => e.stopPropagation()}>
                <div className="vault-viewer-topbar">
                    <div className="vault-viewer-title" title={selectedFile.name}>{selectedFile.name}</div>
                    <div className="vault-viewer-actions">
                        {selectedIndex >= 0 && (
                            <span className="vault-viewer-position">{selectedIndex + 1} / {filteredFilesLength}</span>
                        )}
                        <button
                            className="vault-modal-action-btn"
                            onClick={() => setShowFileData(v => !v)}
                        >
                            {showFileData ? 'Hide File Data' : 'File Data'}
                        </button>
                        <button
                            className="vault-modal-action-btn vault-action-delete"
                            onClick={() => {
                                if (confirm(`Delete "${selectedFile.name}"?`)) {
                                    onDelete(selectedFile.id);
                                }
                            }}
                        >
                            🗑️ Delete
                        </button>
                        <button className="vault-modal-close" onClick={onClose}>✕</button>
                    </div>
                </div>

                {/* ═══ Preview surface + modal navigation ═══ */}
                <div className="vault-modal-preview">
                    {canNavigateModal && (
                        <>
                            <button
                                className="vault-viewer-nav vault-viewer-nav-prev"
                                onClick={onPrevious}
                                aria-label="Previous file"
                                title="Previous (←)"
                            >
                                ‹
                            </button>
                            <button
                                className="vault-viewer-nav vault-viewer-nav-next"
                                onClick={onNext}
                                aria-label="Next file"
                                title="Next (→)"
                            >
                                ›
                            </button>
                        </>
                    )}

                    {previewLoading ? (
                        <div className="vault-modal-preview-loading">⏳ Loading preview...</div>
                    ) : previewUrl ? (
                        selectedFile.kind === 'Image' ? (
                            <img src={previewUrl} alt={selectedFile.name} className="vault-modal-preview-img" />
                        ) : selectedFile.kind === 'Video' ? (
                            <VideoPlayer src={previewUrl} className="vault-modal-preview-video" />
                        ) : selectedFile.kind === 'Audio' ? (
                            <audio src={previewUrl} controls className="vault-modal-preview-audio" />
                        ) : selectedFile.kind === 'Document' ? (
                            selectedFile.mime_type.toLowerCase().includes('pdf') ? (
                                <iframe
                                    title={selectedFile.name}
                                    src={previewUrl}
                                    style={{ width: '100%', height: '100%', border: 'none', borderRadius: 8 }}
                                />
                            ) : (
                                <div className="vault-modal-preview-placeholder">
                                    <span>📄</span>
                                    <a
                                        href={previewUrl}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        className="btn btn-ghost"
                                        style={{ marginTop: 10 }}
                                    >
                                        Open Document
                                    </a>
                                </div>
                            )
                        ) : null
                    ) : (
                        <div className="vault-modal-preview-placeholder">
                            <span>{KIND_EMOJI[selectedFile.kind]}</span>
                        </div>
                    )}
                </div>

                {/* ═══ Optional metadata/detail panel ═══ */}
                {showFileData && (
                    <div className="vault-file-data-panel">
                        <div className="vault-modal-details">
                            <div className="vault-detail-row">
                                <span className="vault-detail-label">Type</span>
                                <span className="vault-detail-value">{KIND_EMOJI[selectedFile.kind]} {selectedFile.kind} — {selectedFile.mime_type}</span>
                            </div>
                            <div className="vault-detail-row">
                                <span className="vault-detail-label">Size</span>
                                <span className="vault-detail-value">{formatFileSize(selectedFile.size)}</span>
                            </div>
                            <div className="vault-detail-row">
                                <span className="vault-detail-label">Source</span>
                                <span className="vault-detail-value">{sourceEmoji(selectedFile.source)} {selectedFile.source}</span>
                            </div>
                            <div className="vault-detail-row">
                                <span className="vault-detail-label">Added</span>
                                <span className="vault-detail-value">{formatTimeFull(selectedFile.added_at)}</span>
                            </div>
                            {selectedFile.created_at && (
                                <div className="vault-detail-row">
                                    <span className="vault-detail-label">Created</span>
                                    <span className="vault-detail-value">{formatTimeFull(selectedFile.created_at)}</span>
                                </div>
                            )}
                            {selectedFile.tags.length > 0 && (
                                <div className="vault-detail-row">
                                    <span className="vault-detail-label">Tags</span>
                                    <span className="vault-detail-value vault-detail-tags">
                                        {selectedFile.tags.map(t => (
                                            <span key={t} className="vault-tag">#{t}</span>
                                        ))}
                                    </span>
                                </div>
                            )}
                        </div>

                        <details className="vault-modal-debug">
                            <summary>Raw metadata</summary>
                            <pre>{JSON.stringify(selectedFile, null, 2)}</pre>
                        </details>
                    </div>
                )}
            </div>
        </div>
    );
}
