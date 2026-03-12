import { useEditor, EditorContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import { useState, useEffect, useRef, useCallback } from 'react';
import { markdownToHtml, htmlToMarkdown, countWords, readingTime } from '../utils/markdown';

export interface ArticleEditorData {
    title: string;
    content: string;       // Markdown
    contentJson: object;   // Tiptap JSON (for future Yjs backing)
    wordCount: number;
}

export interface ArticleEditorProps {
    initialTitle?: string;
    initialContent?: string;  // Markdown string
    onChange: (data: ArticleEditorData) => void;
    readOnly?: boolean;
}

export function ArticleEditor({
    initialTitle = '',
    initialContent = '',
    onChange,
    readOnly = false,
}: ArticleEditorProps) {
    const [title, setTitle] = useState(initialTitle);
    const [wordCount, setWordCount] = useState(0);

    // Use refs for values accessed inside Tiptap's onUpdate closure
    const titleRef = useRef(initialTitle);
    const onChangeRef = useRef(onChange);
    onChangeRef.current = onChange;

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
        });
    }, []);

    const editor = useEditor({
        extensions: [
            StarterKit,
            Placeholder.configure({
                placeholder: 'Write your article...',
            }),
        ],
        content: initialContent ? markdownToHtml(initialContent) : '',
        editable: !readOnly,
        onUpdate: ({ editor: ed }) => {
            emitChange(ed.getHTML(), ed.getJSON(), ed.getText());
        },
    });

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
            </div>
        </div>
    );
}
