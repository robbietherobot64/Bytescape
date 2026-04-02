/**
 * PURPOSE
 * - Rich article authoring surface for Bytescape items, supporting local editing and optional CRDT collaboration.
 *
 * HOW IT WORKS
 * - Initializes Tiptap with local-history extensions or Yjs-backed collaboration extensions.
 * - Converts between Markdown <-> HTML for persistence while exposing Tiptap JSON for structured consumers.
 * - Emits normalized editor state (`ArticleEditorData`) on content/title changes.
 * - Shows collaboration loading/error UI while `useCollabDoc` initializes or fails.
 *
 * RELATED FILES
 * - `src/hooks/useCollabDoc.ts` (CRDT lifecycle, doc resolution, snapshot API)
 * - `src/utils/markdown.ts` (Markdown/HTML transforms + word/reading metrics)
 * - Components/screens that persist article data through store/provider flows
 */
import { useEditor, EditorContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import Collaboration from '@tiptap/extension-collaboration';
import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { markdownToHtml, htmlToMarkdown, countWords, readingTime } from '../utils/markdown';
import { useCollabDoc } from '../hooks/useCollabDoc';

/**
 * Serialized payload emitted to parent components for persistence and preview.
 */
export interface ArticleEditorData {
    title: string;
    content: string;       // Markdown
    contentJson: object;   // Tiptap JSON
    collabDocId?: string | null;  // Set when collaborative mode is active
    wordCount: number;
}

/**
 * Public configuration contract for `ArticleEditor`.
 */
export interface ArticleEditorProps {
    initialTitle?: string;
    initialContent?: string;  // Markdown string
    onChange: (data: ArticleEditorData) => void;
    readOnly?: boolean;
    /** Enable CRDT-backed collaborative editing. */
    collaborative?: boolean;
    /** Existing collab doc ID to resume editing. If omitted + collaborative=true, a new doc is created. */
    collabDocId?: string | null;
    /** BytescapeItem ID to link the collab doc to. */
    itemId?: string | null;
}

/**
 * `ArticleEditor`
 *
 * Cross-system notes:
 * - Hook integration: consumes `useCollabDoc()` to attach editor state to shared Yjs docs.
 * - Provider/store integration: parent layers own persistence and pass `onChange`; this component is UI + transform focused.
 * - Backend/invoke integration: no direct `invoke()` calls here; Tauri interaction is intentionally delegated to hooks/services.
 */
export function ArticleEditor({
    initialTitle = '',
    initialContent = '',
    onChange,
    readOnly = false,
    collaborative = false,
    collabDocId: inputCollabDocId,
    itemId,
}: ArticleEditorProps) {
    // ═══════════════════════════════════════════════════════════════════════
    // Local UI + callback state
    // ═══════════════════════════════════════════════════════════════════════
    const [title, setTitle] = useState(initialTitle);
    const [wordCount, setWordCount] = useState(0);

    // Use refs for values accessed inside Tiptap's onUpdate closure
    const titleRef = useRef(initialTitle);
    const onChangeRef = useRef(onChange);
    onChangeRef.current = onChange;

    // ═══════════════════════════════════════════════════════════════════════
    // Collaboration bridge (Yjs/CRDT)
    // ═══════════════════════════════════════════════════════════════════════
    // CRDT collaborative editing hook
    const {
        ydoc,
        isLoaded: collabLoaded,
        collabDocId: activeCollabDocId,
        error: collabError,
        snapshot,
    } = useCollabDoc({
        enabled: collaborative,
        docId: inputCollabDocId,
        itemId,
        title: initialTitle || null,
    });

    // Store snapshot function on a ref for external access
    const snapshotRef = useRef(snapshot);
    snapshotRef.current = snapshot;

    useEffect(() => {
        titleRef.current = title;
    }, [title]);

    const emitChange = useCallback((editorHtml: string, editorJson: object, editorText: string) => {
        const wc = countWords(editorText);
        setWordCount(wc);
        onChangeRef.current({
            title: titleRef.current,
            content: htmlToMarkdown(editorHtml),
            contentJson: editorJson,
            wordCount: wc,
            collabDocId: activeCollabDocId,
        });
    }, [activeCollabDocId]);

    // ═══════════════════════════════════════════════════════════════════════
    // Tiptap composition and lifecycle
    // ═══════════════════════════════════════════════════════════════════════
    // Build Tiptap extensions based on mode
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const extensions = useMemo((): any[] => {
        if (collaborative && ydoc) {
            // Collaborative mode: Yjs manages undo/redo
            return [
                StarterKit.configure({ undoRedo: false }),
                Placeholder.configure({ placeholder: 'Write your article...' }),
                Collaboration.configure({ document: ydoc, field: 'content' }),
            ];
        }
        // Standard mode: local history
        return [
            StarterKit,
            Placeholder.configure({ placeholder: 'Write your article...' }),
        ];
    }, [collaborative, ydoc]);

    // Wait for collab state to load before creating the editor
    const shouldCreateEditor = !collaborative || (collabLoaded && ydoc !== null);

    const editor = useEditor({
        extensions,
        // In collaborative mode, content comes from Yjs doc — do not set `content`
        content: collaborative ? undefined : (initialContent ? markdownToHtml(initialContent) : ''),
        editable: !readOnly,
        onUpdate: ({ editor: ed }) => {
            emitChange(ed.getHTML(), ed.getJSON(), ed.getText());
        },
    }, [shouldCreateEditor, extensions]);

    // In collaborative mode, pre-populate with initial content after editor + Yjs are ready
    const hasSetInitialContent = useRef(false);
    useEffect(() => {
        if (
            collaborative &&
            editor &&
            collabLoaded &&
            !inputCollabDocId &&
            initialContent &&
            !hasSetInitialContent.current
        ) {
            // New collab doc with initial content — insert into empty Yjs doc
            const currentText = editor.getText();
            if (!currentText || currentText.trim().length === 0) {
                editor.commands.setContent(markdownToHtml(initialContent));
                hasSetInitialContent.current = true;
            }
        }
    }, [collaborative, editor, collabLoaded, inputCollabDocId, initialContent]);

    // Compute initial word count once editor is ready
    useEffect(() => {
        if (editor) {
            const text = editor.getText();
            setWordCount(countWords(text));
        }
    }, [editor]);

    const handleTitleChange = useCallback((newTitle: string) => {
        setTitle(newTitle);
        titleRef.current = newTitle;
        if (editor) {
            emitChange(editor.getHTML(), editor.getJSON(), editor.getText());
        }
    }, [editor, emitChange]);

    const rt = readingTime(wordCount);

    // ═══════════════════════════════════════════════════════════════════════
    // Render states
    // ═══════════════════════════════════════════════════════════════════════
    // Show loading state while collaborative doc state loads
    if (collaborative && !collabLoaded) {
        return (
            <div className="article-editor-container" data-testid="article-editor-loading">
                <div className="article-editor-footer">Loading collaborative document…</div>
            </div>
        );
    }

    // Show error state
    if (collabError) {
        return (
            <div className="article-editor-container" data-testid="article-editor-error">
                <div className="article-editor-footer" style={{ color: 'var(--error-red, #ff4444)' } as React.CSSProperties}>
                    Collab error: {collabError}
                </div>
            </div>
        );
    }

    if (!editor) return null;

    return (
        <div className="article-editor-container">
            <input
                type="text"
                className="article-title-input"
                value={title}
                onChange={e => handleTitleChange(e.target.value)}
                placeholder="Article title..."
                readOnly={readOnly}
                data-testid="article-title"
            />

            {!readOnly && (
                <div className="article-toolbar">
                    <button
                        onClick={() => editor.chain().focus().toggleBold().run()}
                        className={editor.isActive('bold') ? 'is-active' : ''}
                        title="Bold"
                        type="button"
                    >
                        <strong>B</strong>
                    </button>
                    <button
                        onClick={() => editor.chain().focus().toggleItalic().run()}
                        className={editor.isActive('italic') ? 'is-active' : ''}
                        title="Italic"
                        type="button"
                    >
                        <em>I</em>
                    </button>
                    <button
                        onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
                        className={editor.isActive('heading', { level: 1 }) ? 'is-active' : ''}
                        title="Heading 1"
                        type="button"
                    >
                        H1
                    </button>
                    <button
                        onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
                        className={editor.isActive('heading', { level: 2 }) ? 'is-active' : ''}
                        title="Heading 2"
                        type="button"
                    >
                        H2
                    </button>
                    <button
                        onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
                        className={editor.isActive('heading', { level: 3 }) ? 'is-active' : ''}
                        title="Heading 3"
                        type="button"
                    >
                        H3
                    </button>
                    <button
                        onClick={() => editor.chain().focus().toggleBulletList().run()}
                        className={editor.isActive('bulletList') ? 'is-active' : ''}
                        title="Bullet List"
                        type="button"
                    >
                        • List
                    </button>
                    <button
                        onClick={() => editor.chain().focus().toggleOrderedList().run()}
                        className={editor.isActive('orderedList') ? 'is-active' : ''}
                        title="Ordered List"
                        type="button"
                    >
                        1. List
                    </button>
                    <button
                        onClick={() => editor.chain().focus().toggleBlockquote().run()}
                        className={editor.isActive('blockquote') ? 'is-active' : ''}
                        title="Blockquote"
                        type="button"
                    >
                        ❝ Quote
                    </button>
                    <button
                        onClick={() => editor.chain().focus().toggleCodeBlock().run()}
                        className={editor.isActive('codeBlock') ? 'is-active' : ''}
                        title="Code Block"
                        type="button"
                    >
                        {'</>'}
                    </button>
                    <button
                        onClick={() => editor.chain().focus().setHorizontalRule().run()}
                        title="Horizontal Rule"
                        type="button"
                    >
                        ─
                    </button>
                </div>
            )}

            <EditorContent editor={editor} />

            <div className="article-editor-footer" data-testid="article-footer">
                {wordCount} {wordCount === 1 ? 'word' : 'words'} · {rt} min read
                {collaborative && activeCollabDocId && (
                    <span style={{ marginLeft: '8px', opacity: 0.6 } as React.CSSProperties}>
                        · CRDT synced
                    </span>
                )}
            </div>
        </div>
    );
}
