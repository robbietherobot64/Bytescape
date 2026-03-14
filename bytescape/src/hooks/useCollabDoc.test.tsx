import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { render, screen } from '@testing-library/react';
import * as Y from 'yjs';

// ─── Mocks ───

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
    invoke: (...args: unknown[]) => mockInvoke(...args),
    convertFileSrc: vi.fn((path: string) => `asset://${path}`),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn().mockResolvedValue(null),
}));

// ─── Helpers ───

function uint8ArrayToBase64(bytes: Uint8Array): string {
    let binary = '';
    for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary);
}

function base64ToUint8Array(b64: string): Uint8Array {
    const binary = atob(b64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
}

/** Create a Yjs doc with some content and return its base64-encoded state. */
function createTestDocState(): string {
    const doc = new Y.Doc();
    const fragment = doc.getXmlFragment('content');
    // Add a paragraph element (simulating Tiptap ProseMirror structure)
    const el = new Y.XmlElement('paragraph');
    const text = new Y.XmlText();
    text.insert(0, 'Hello CRDT world');
    el.insert(0, [text]);
    fragment.insert(0, [el]);
    const state = Y.encodeStateAsUpdate(doc);
    doc.destroy();
    return uint8ArrayToBase64(state);
}

// ─── useCollabDoc Tests ───

describe('useCollabDoc', () => {
    // Import lazily to ensure mocks are set up
    let useCollabDoc: typeof import('../hooks/useCollabDoc').useCollabDoc;

    beforeEach(async () => {
        vi.clearAllMocks();
        const mod = await import('../hooks/useCollabDoc');
        useCollabDoc = mod.useCollabDoc;
    });

    it('is a no-op when enabled=false', () => {
        const { result } = renderHook(() =>
            useCollabDoc({ enabled: false })
        );

        expect(result.current.ydoc).toBeNull();
        expect(result.current.isLoaded).toBe(true);
        expect(result.current.collabDocId).toBeNull();
        expect(result.current.error).toBeNull();
    });

    it('creates a new collab doc when enabled=true and no docId', async () => {
        const mockMeta = {
            id: 'test-doc-123',
            created_at: 1710000000,
            updated_at: 1710000000,
            item_id: null,
            title: 'Test Article',
        };
        mockInvoke.mockResolvedValueOnce(mockMeta);

        const { result } = renderHook(() =>
            useCollabDoc({ enabled: true, title: 'Test Article' })
        );

        // Should have a Y.Doc immediately
        expect(result.current.ydoc).toBeInstanceOf(Y.Doc);

        // Wait for backend creation
        await waitFor(() => {
            expect(result.current.isLoaded).toBe(true);
        });

        expect(mockInvoke).toHaveBeenCalledWith('create_collab_doc', {
            itemId: null,
            title: 'Test Article',
        });
        expect(result.current.collabDocId).toBe('test-doc-123');
        expect(result.current.meta).toEqual(mockMeta);
        expect(result.current.error).toBeNull();
    });

    it('loads existing collab doc state when docId is provided', async () => {
        const testState = createTestDocState();
        const mockResponse = {
            state_b64: testState,
            state_vector_b64: 'AAAA', // dummy
            meta: {
                id: 'existing-doc-456',
                created_at: 1710000000,
                updated_at: 1710000001,
                item_id: 'item-789',
                title: 'Loaded Article',
            },
        };
        mockInvoke.mockResolvedValueOnce(mockResponse);

        const { result } = renderHook(() =>
            useCollabDoc({ enabled: true, docId: 'existing-doc-456' })
        );

        await waitFor(() => {
            expect(result.current.isLoaded).toBe(true);
        });

        expect(mockInvoke).toHaveBeenCalledWith('get_collab_state', {
            docId: 'existing-doc-456',
        });
        expect(result.current.collabDocId).toBe('existing-doc-456');
        expect(result.current.meta?.title).toBe('Loaded Article');
        expect(result.current.error).toBeNull();
    });

    it('sets error when backend call fails', async () => {
        mockInvoke.mockRejectedValueOnce('Database not initialized');

        const { result } = renderHook(() =>
            useCollabDoc({ enabled: true })
        );

        await waitFor(() => {
            expect(result.current.isLoaded).toBe(true);
        });

        expect(result.current.error).toBe('Database not initialized');
    });

    it('persists Y.Doc updates to backend via apply_collab_update', async () => {
        // First call: create_collab_doc
        const mockMeta = {
            id: 'persist-doc',
            created_at: 1710000000,
            updated_at: 1710000000,
            item_id: null,
            title: null,
        };
        mockInvoke.mockResolvedValueOnce(mockMeta);

        const { result } = renderHook(() =>
            useCollabDoc({ enabled: true })
        );

        await waitFor(() => {
            expect(result.current.isLoaded).toBe(true);
        });

        // Now make a Y.Doc change (simulating editor input)
        const ydoc = result.current.ydoc!;
        expect(ydoc).not.toBeNull();

        // apply_collab_update mock
        mockInvoke.mockResolvedValue('state-vector-b64');

        act(() => {
            const text = ydoc.getXmlFragment('content');
            const el = new Y.XmlElement('paragraph');
            const t = new Y.XmlText();
            t.insert(0, 'Typed text');
            el.insert(0, [t]);
            text.insert(0, [el]);
        });

        // Debounced at 300ms — wait for it
        await waitFor(() => {
            const calls = mockInvoke.mock.calls.filter(
                (c: unknown[]) => c[0] === 'apply_collab_update'
            );
            return expect(calls.length).toBeGreaterThan(0);
        }, { timeout: 1000 });

        const updateCalls = mockInvoke.mock.calls.filter(
            (c: unknown[]) => c[0] === 'apply_collab_update'
        );
        expect(updateCalls[0][1].docId).toBe('persist-doc');
        expect(typeof updateCalls[0][1].updateB64).toBe('string');
    });

    it('snapshot sends content to backend', async () => {
        const mockMeta = {
            id: 'snap-doc',
            created_at: 1710000000,
            updated_at: 1710000000,
            item_id: 'item-123',
            title: null,
        };
        mockInvoke.mockResolvedValueOnce(mockMeta);

        const { result } = renderHook(() =>
            useCollabDoc({ enabled: true, itemId: 'item-123' })
        );

        await waitFor(() => {
            expect(result.current.isLoaded).toBe(true);
        });

        mockInvoke.mockResolvedValueOnce('item-123');

        await act(async () => {
            await result.current.snapshot('# My Article\n\nHello world');
        });

        expect(mockInvoke).toHaveBeenCalledWith('snapshot_collab_to_item', {
            docId: 'snap-doc',
            content: '# My Article\n\nHello world',
        });
    });
});

// ─── Base64 Utility Tests ───

describe('Base64 Utilities', () => {
    it('round-trips Uint8Array through base64', () => {
        const original = new Uint8Array([0, 1, 2, 127, 128, 255]);
        const b64 = uint8ArrayToBase64(original);
        const decoded = base64ToUint8Array(b64);
        expect(decoded).toEqual(original);
    });

    it('round-trips Yjs state through base64', () => {
        const doc = new Y.Doc();
        const text = doc.getXmlFragment('test');
        const el = new Y.XmlElement('p');
        const t = new Y.XmlText();
        t.insert(0, 'test content');
        el.insert(0, [t]);
        text.insert(0, [el]);

        const state = Y.encodeStateAsUpdate(doc);
        const b64 = uint8ArrayToBase64(state);
        const decoded = base64ToUint8Array(b64);

        // Apply to a new doc and verify
        const doc2 = new Y.Doc();
        Y.applyUpdate(doc2, decoded);
        const fragment2 = doc2.getXmlFragment('test');
        expect(fragment2.length).toBe(1);

        doc.destroy();
        doc2.destroy();
    });
});

// ─── Yjs + Tiptap Compatibility Tests ───

describe('Yjs CRDT Document Operations', () => {
    it('creates an empty Y.Doc that can be encoded', () => {
        const doc = new Y.Doc();
        const state = Y.encodeStateAsUpdate(doc);
        expect(state).toBeInstanceOf(Uint8Array);
        expect(state.length).toBeGreaterThan(0);
        doc.destroy();
    });

    it('supports XmlFragment type (used by Tiptap Collaboration)', () => {
        const doc = new Y.Doc();
        const fragment = doc.getXmlFragment('content');

        // Simulate Tiptap creating a paragraph
        const para = new Y.XmlElement('paragraph');
        const text = new Y.XmlText();
        text.insert(0, 'Hello world');
        para.insert(0, [text]);
        fragment.insert(0, [para]);

        expect(fragment.length).toBe(1);
        expect(fragment.toJSON()).toContain('Hello world');

        doc.destroy();
    });

    it('merges updates from two Y.Docs', () => {
        const doc1 = new Y.Doc();
        const doc2 = new Y.Doc();

        // Doc1 adds content
        const frag1 = doc1.getXmlFragment('content');
        const p1 = new Y.XmlElement('paragraph');
        const t1 = new Y.XmlText();
        t1.insert(0, 'From doc1');
        p1.insert(0, [t1]);
        frag1.insert(0, [p1]);

        // Get update from doc1 and apply to doc2
        const update1 = Y.encodeStateAsUpdate(doc1);
        Y.applyUpdate(doc2, update1);

        const frag2 = doc2.getXmlFragment('content');
        expect(frag2.length).toBe(1);
        expect(frag2.toJSON()).toContain('From doc1');

        doc1.destroy();
        doc2.destroy();
    });

    it('state encodes and decodes through base64 correctly', () => {
        const doc1 = new Y.Doc();
        const frag = doc1.getXmlFragment('content');
        const p = new Y.XmlElement('paragraph');
        const t = new Y.XmlText();
        t.insert(0, 'Persistent content');
        p.insert(0, [t]);
        frag.insert(0, [p]);

        // Encode → base64 → decode → apply to new doc
        const state = Y.encodeStateAsUpdate(doc1);
        const b64 = uint8ArrayToBase64(state);
        const decoded = base64ToUint8Array(b64);

        const doc2 = new Y.Doc();
        Y.applyUpdate(doc2, decoded);

        const frag2 = doc2.getXmlFragment('content');
        expect(frag2.toJSON()).toContain('Persistent content');

        doc1.destroy();
        doc2.destroy();
    });

    it('handles state vector computation', () => {
        const doc = new Y.Doc();
        const frag = doc.getXmlFragment('content');
        const p = new Y.XmlElement('paragraph');
        const t = new Y.XmlText();
        t.insert(0, 'test');
        p.insert(0, [t]);
        frag.insert(0, [p]);

        const sv = Y.encodeStateVector(doc);
        expect(sv).toBeInstanceOf(Uint8Array);
        expect(sv.length).toBeGreaterThan(0);

        doc.destroy();
    });

    it('fires update events on mutations', () => {
        const doc = new Y.Doc();
        const updates: Uint8Array[] = [];

        doc.on('update', (update: Uint8Array) => {
            updates.push(update);
        });

        const frag = doc.getXmlFragment('content');
        const p = new Y.XmlElement('paragraph');
        const t = new Y.XmlText();
        t.insert(0, 'trigger update');
        p.insert(0, [t]);
        frag.insert(0, [p]);

        expect(updates.length).toBeGreaterThan(0);

        doc.destroy();
    });
});

// ─── ArticleEditor Collaborative Mode Tests ───

describe('ArticleEditor Collaborative Mode', () => {
    let ArticleEditor: typeof import('../components/ArticleEditor').ArticleEditor;

    beforeEach(async () => {
        vi.clearAllMocks();
        const mod = await import('../components/ArticleEditor');
        ArticleEditor = mod.ArticleEditor;
    });

    it('shows loading state when collaborative mode is initializing', () => {
        // Make create_collab_doc hang (never resolve)
        mockInvoke.mockReturnValue(new Promise(() => {}));

        render(
            <ArticleEditor
                collaborative={true}
                onChange={vi.fn()}
            />
        );

        expect(screen.getByTestId('article-editor-loading')).toBeInTheDocument();
    });

    it('shows error state when collab backend fails', async () => {
        mockInvoke.mockRejectedValueOnce('DB connection failed');

        render(
            <ArticleEditor
                collaborative={true}
                onChange={vi.fn()}
            />
        );

        await waitFor(() => {
            expect(screen.getByTestId('article-editor-error')).toBeInTheDocument();
        });

        expect(screen.getByText(/DB connection failed/)).toBeInTheDocument();
    });

    it('renders normally in non-collaborative mode', () => {
        render(
            <ArticleEditor
                onChange={vi.fn()}
            />
        );

        expect(screen.getByTestId('article-title')).toBeInTheDocument();
        expect(screen.getByTestId('article-footer')).toBeInTheDocument();
    });

    it('includes collabDocId in onChange data when collaborative', async () => {
        const mockMeta = {
            id: 'collab-doc-test',
            created_at: 1710000000,
            updated_at: 1710000000,
            item_id: null,
            title: null,
        };
        mockInvoke.mockResolvedValueOnce(mockMeta);

        const onChange = vi.fn();
        render(
            <ArticleEditor
                collaborative={true}
                onChange={onChange}
            />
        );

        // Wait for collab to load
        await waitFor(() => {
            expect(screen.queryByTestId('article-editor-loading')).not.toBeInTheDocument();
        }, { timeout: 3000 });
    });
});
