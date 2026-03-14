/**
 * useCollabDoc — React hook for Yjs CRDT document lifecycle (C6.1)
 *
 * Manages creating, loading, and syncing a Yjs document with the Rust backend.
 * Used by ArticleEditor in collaborative mode.
 */
import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import * as Y from 'yjs';
import type { CollabDocMeta, CollabStateResponse } from '../types';

/** Base64 ↔ Uint8Array helpers */
function base64ToUint8Array(b64: string): Uint8Array {
    const binary = atob(b64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
}

function uint8ArrayToBase64(bytes: Uint8Array): string {
    let binary = '';
    for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary);
}

export interface UseCollabDocOptions {
    /** Existing collab doc ID to load. If null, a new doc is created on mount. */
    docId?: string | null;
    /** BytescapeItem ID to link the doc to (used when creating a new doc). */
    itemId?: string | null;
    /** Title for the collab doc (used when creating a new doc). */
    title?: string | null;
    /** Whether collaborative mode is enabled. When false, hook is a no-op. */
    enabled: boolean;
}

export interface UseCollabDocResult {
    /** The Yjs document instance. Null until initialized. */
    ydoc: Y.Doc | null;
    /** Whether the initial state has been loaded from the backend. */
    isLoaded: boolean;
    /** The collab doc ID (may differ from input if a new doc was created). */
    collabDocId: string | null;
    /** Metadata for the collab doc. */
    meta: CollabDocMeta | null;
    /** Error message if loading/creation failed. */
    error: string | null;
    /** Snapshot the current editor content to the linked BytescapeItem. */
    snapshot: (content: string) => Promise<string>;
}

/**
 * Hook that manages a Yjs document backed by the Rust CRDT storage layer.
 *
 * Lifecycle:
 * 1. Creates a Y.Doc instance
 * 2. If docId is provided, loads existing state from backend
 * 3. If no docId, creates a new collab doc in the backend
 * 4. Subscribes to Y.Doc updates and persists them to backend
 * 5. Cleans up on unmount
 */
export function useCollabDoc({
    docId,
    itemId,
    title,
    enabled,
}: UseCollabDocOptions): UseCollabDocResult {
    const ydocRef = useRef<Y.Doc | null>(null);
    const [isLoaded, setIsLoaded] = useState(false);
    const [collabDocId, setCollabDocId] = useState<string | null>(docId ?? null);
    const [meta, setMeta] = useState<CollabDocMeta | null>(null);
    const [error, setError] = useState<string | null>(null);

    // Stable ref to avoid re-subscribing on every render
    const collabDocIdRef = useRef(collabDocId);
    collabDocIdRef.current = collabDocId;

    // Create Y.Doc once
    if (enabled && !ydocRef.current) {
        ydocRef.current = new Y.Doc();
    }

    // Initialize: load existing doc or create new one
    useEffect(() => {
        if (!enabled || !ydocRef.current) {
            setIsLoaded(true);
            return;
        }

        const ydoc = ydocRef.current;
        let cancelled = false;

        const init = async () => {
            try {
                if (docId) {
                    // Load existing collab doc state
                    const response = await invoke<CollabStateResponse>('get_collab_state', {
                        docId,
                    });
                    if (cancelled) return;

                    const stateBytes = base64ToUint8Array(response.state_b64);
                    Y.applyUpdate(ydoc, stateBytes);
                    setMeta(response.meta);
                    setCollabDocId(docId);
                } else {
                    // Create a new collab doc
                    const newMeta = await invoke<CollabDocMeta>('create_collab_doc', {
                        itemId: itemId ?? null,
                        title: title ?? null,
                    });
                    if (cancelled) return;

                    setMeta(newMeta);
                    setCollabDocId(newMeta.id);
                    collabDocIdRef.current = newMeta.id;
                }
            } catch (err) {
                if (!cancelled) {
                    setError(typeof err === 'string' ? err : String(err));
                }
            } finally {
                if (!cancelled) {
                    setIsLoaded(true);
                }
            }
        };

        init();

        return () => {
            cancelled = true;
        };
    }, [enabled, docId, itemId, title]);

    // Subscribe to Y.Doc updates and persist to backend
    useEffect(() => {
        if (!enabled || !ydocRef.current || !isLoaded) return;

        const ydoc = ydocRef.current;
        let debounceTimer: ReturnType<typeof setTimeout> | null = null;
        let pendingUpdates: Uint8Array[] = [];

        const flushUpdates = () => {
            const currentDocId = collabDocIdRef.current;
            if (!currentDocId || pendingUpdates.length === 0) return;

            // Merge all pending updates into one
            const merged = Y.mergeUpdates(pendingUpdates);
            pendingUpdates = [];

            const updateB64 = uint8ArrayToBase64(merged);
            invoke('apply_collab_update', {
                docId: currentDocId,
                updateB64,
            }).catch((err) => {
                console.error('[collab] Failed to persist update:', err);
            });
        };

        const handleUpdate = (update: Uint8Array, origin: unknown) => {
            // Skip updates that originated from loading remote state
            if (origin === 'remote') return;

            pendingUpdates.push(update);

            // Debounce: batch updates every 300ms
            if (debounceTimer) clearTimeout(debounceTimer);
            debounceTimer = setTimeout(flushUpdates, 300);
        };

        ydoc.on('update', handleUpdate);

        return () => {
            ydoc.off('update', handleUpdate);
            // Flush any remaining updates on cleanup
            if (debounceTimer) clearTimeout(debounceTimer);
            flushUpdates();
        };
    }, [enabled, isLoaded]);

    // Snapshot content to linked BytescapeItem
    const snapshot = useCallback(async (content: string): Promise<string> => {
        const currentDocId = collabDocIdRef.current;
        if (!currentDocId) {
            throw new Error('No collab doc ID — cannot snapshot');
        }
        return invoke<string>('snapshot_collab_to_item', {
            docId: currentDocId,
            content,
        });
    }, []);

    // Cleanup Y.Doc on unmount
    useEffect(() => {
        return () => {
            if (ydocRef.current) {
                ydocRef.current.destroy();
                ydocRef.current = null;
            }
        };
    }, []);

    return {
        ydoc: enabled ? ydocRef.current : null,
        isLoaded: enabled ? isLoaded : true,
        collabDocId,
        meta,
        error,
        snapshot,
    };
}
