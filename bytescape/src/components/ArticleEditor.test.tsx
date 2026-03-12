import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { countWords, readingTime, markdownToHtml, htmlToMarkdown } from '../utils/markdown';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn().mockResolvedValue(0),
    convertFileSrc: vi.fn((path: string) => `asset://${path}`),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn().mockResolvedValue(null),
}));

describe('Markdown Utilities', () => {
    describe('countWords', () => {
        it('counts words in a simple sentence', () => {
            expect(countWords('hello world foo bar')).toBe(4);
        });

        it('returns 0 for empty string', () => {
            expect(countWords('')).toBe(0);
        });

        it('handles extra whitespace', () => {
            expect(countWords('  hello   world  ')).toBe(2);
        });

        it('handles newlines', () => {
            expect(countWords('hello\nworld\nfoo')).toBe(3);
        });
    });

    describe('readingTime', () => {
        it('returns 1 min for small word counts', () => {
            expect(readingTime(50)).toBe(1);
        });

        it('returns 1 min for exactly 200 words', () => {
            expect(readingTime(200)).toBe(1);
        });

        it('returns 2 min for 201 words', () => {
            expect(readingTime(201)).toBe(2);
        });

        it('returns 5 min for 1000 words', () => {
            expect(readingTime(1000)).toBe(5);
        });
    });

    describe('markdownToHtml', () => {
        it('converts headings', () => {
            expect(markdownToHtml('# Title')).toContain('<h1>Title</h1>');
            expect(markdownToHtml('## Subtitle')).toContain('<h2>Subtitle</h2>');
            expect(markdownToHtml('### Section')).toContain('<h3>Section</h3>');
        });

        it('converts bold', () => {
            expect(markdownToHtml('**bold text**')).toContain('<strong>bold text</strong>');
        });

        it('converts italic', () => {
            expect(markdownToHtml('*italic text*')).toContain('<em>italic text</em>');
        });

        it('converts inline code', () => {
            expect(markdownToHtml('use `const x`')).toContain('<code>const x</code>');
        });

        it('converts unordered lists', () => {
            const md = '- item 1\n- item 2';
            const html = markdownToHtml(md);
            expect(html).toContain('<ul>');
            expect(html).toContain('<li>item 1</li>');
            expect(html).toContain('<li>item 2</li>');
        });

        it('converts ordered lists', () => {
            const md = '1. first\n2. second';
            const html = markdownToHtml(md);
            expect(html).toContain('<ol>');
            expect(html).toContain('<li>first</li>');
        });

        it('converts blockquotes', () => {
            expect(markdownToHtml('> quoted text')).toContain('<blockquote>');
        });

        it('converts code blocks', () => {
            const md = '```\nconst x = 1;\n```';
            const html = markdownToHtml(md);
            expect(html).toContain('<pre><code>');
            expect(html).toContain('const x = 1;');
        });

        it('converts horizontal rules', () => {
            expect(markdownToHtml('---')).toContain('<hr>');
        });

        it('converts paragraphs', () => {
            expect(markdownToHtml('hello world')).toContain('<p>hello world</p>');
        });

        it('returns empty string for empty input', () => {
            expect(markdownToHtml('')).toBe('');
        });
    });

    describe('htmlToMarkdown', () => {
        it('converts headings back', () => {
            expect(htmlToMarkdown('<h1>Title</h1>')).toContain('# Title');
            expect(htmlToMarkdown('<h2>Sub</h2>')).toContain('## Sub');
        });

        it('converts bold back', () => {
            expect(htmlToMarkdown('<p><strong>bold</strong></p>')).toContain('**bold**');
        });

        it('converts italic back', () => {
            expect(htmlToMarkdown('<p><em>italic</em></p>')).toContain('*italic*');
        });

        it('converts lists back', () => {
            const md = htmlToMarkdown('<ul><li>a</li><li>b</li></ul>');
            expect(md).toContain('- a');
            expect(md).toContain('- b');
        });

        it('converts blockquotes back', () => {
            expect(htmlToMarkdown('<blockquote><p>quote</p></blockquote>')).toContain('> quote');
        });

        it('converts code blocks back', () => {
            const md = htmlToMarkdown('<pre><code>x = 1</code></pre>');
            expect(md).toContain('```');
            expect(md).toContain('x = 1');
        });

        it('converts horizontal rules back', () => {
            expect(htmlToMarkdown('<hr>')).toContain('---');
        });

        it('returns empty for empty/trivial input', () => {
            expect(htmlToMarkdown('')).toBe('');
            expect(htmlToMarkdown('<p></p>')).toBe('');
        });
    });
});

describe('ArticleEditor', () => {
    // Dynamic import to avoid Tiptap SSR issues in test env
    let ArticleEditor: typeof import('./ArticleEditor').ArticleEditor;

    beforeEach(async () => {
        const mod = await import('./ArticleEditor');
        ArticleEditor = mod.ArticleEditor;
    });

    it('renders the title input', () => {
        const onChange = vi.fn();
        render(<ArticleEditor onChange={onChange} />);
        const titleInput = screen.getByTestId('article-title');
        expect(titleInput).toBeTruthy();
        expect(titleInput).toHaveAttribute('placeholder', 'Article title...');
    });

    it('renders with initial title', () => {
        const onChange = vi.fn();
        render(<ArticleEditor onChange={onChange} initialTitle="My Article" />);
        const titleInput = screen.getByTestId('article-title') as HTMLInputElement;
        expect(titleInput.value).toBe('My Article');
    });

    it('fires onChange when title changes', () => {
        const onChange = vi.fn();
        render(<ArticleEditor onChange={onChange} />);
        const titleInput = screen.getByTestId('article-title');
        fireEvent.change(titleInput, { target: { value: 'New Title' } });
        expect(onChange).toHaveBeenCalled();
        const lastCall = onChange.mock.calls[onChange.mock.calls.length - 1][0];
        expect(lastCall.title).toBe('New Title');
    });

    it('shows toolbar buttons', () => {
        const onChange = vi.fn();
        render(<ArticleEditor onChange={onChange} />);
        expect(screen.getByTitle('Bold')).toBeTruthy();
        expect(screen.getByTitle('Italic')).toBeTruthy();
        expect(screen.getByTitle('Heading 1')).toBeTruthy();
        expect(screen.getByTitle('Heading 2')).toBeTruthy();
        expect(screen.getByTitle('Heading 3')).toBeTruthy();
        expect(screen.getByTitle('Bullet List')).toBeTruthy();
        expect(screen.getByTitle('Ordered List')).toBeTruthy();
        expect(screen.getByTitle('Blockquote')).toBeTruthy();
        expect(screen.getByTitle('Code Block')).toBeTruthy();
        expect(screen.getByTitle('Horizontal Rule')).toBeTruthy();
    });

    it('hides toolbar in readOnly mode', () => {
        const onChange = vi.fn();
        render(<ArticleEditor onChange={onChange} readOnly />);
        expect(screen.queryByTitle('Bold')).toBeNull();
    });

    it('shows word count and reading time', () => {
        const onChange = vi.fn();
        render(<ArticleEditor onChange={onChange} />);
        const footer = screen.getByTestId('article-footer');
        expect(footer).toBeTruthy();
        // Initially 0 words
        expect(footer.textContent).toContain('0 words');
        expect(footer.textContent).toContain('1 min read');
    });

    it('onChange data has expected shape', () => {
        const onChange = vi.fn();
        render(<ArticleEditor onChange={onChange} />);
        const titleInput = screen.getByTestId('article-title');
        fireEvent.change(titleInput, { target: { value: 'Test' } });
        const data = onChange.mock.calls[onChange.mock.calls.length - 1][0];
        expect(data).toHaveProperty('title');
        expect(data).toHaveProperty('content');
        expect(data).toHaveProperty('contentJson');
        expect(data).toHaveProperty('wordCount');
        expect(typeof data.wordCount).toBe('number');
    });
});
