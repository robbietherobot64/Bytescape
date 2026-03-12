/**
 * Lightweight Markdown ↔ HTML converters.
 *
 * These handle the subset of Markdown that Tiptap's StarterKit supports:
 * headings, bold, italic, lists, blockquotes, code blocks, inline code,
 * horizontal rules, links, and paragraphs.
 *
 * NOT a full CommonMark implementation — intentionally minimal.
 */

/**
 * Convert a Markdown string to HTML suitable for Tiptap's StarterKit parser.
 */
export function markdownToHtml(md: string): string {
    if (!md) return '';

    let html = '';
    const lines = md.split('\n');
    let i = 0;
    let inList: 'ul' | 'ol' | null = null;

    const closeList = () => {
        if (inList) {
            html += `</${inList}>`;
            inList = null;
        }
    };

    while (i < lines.length) {
        const line = lines[i];

        // Fenced code block
        if (line.startsWith('```')) {
            closeList();
            const codeLines: string[] = [];
            i++;
            while (i < lines.length && !lines[i].startsWith('```')) {
                codeLines.push(escapeHtml(lines[i]));
                i++;
            }
            i++; // skip closing ```
            html += `<pre><code>${codeLines.join('\n')}</code></pre>`;
            continue;
        }

        // Horizontal rule
        if (/^-{3,}$|^\*{3,}$|^_{3,}$/.test(line.trim())) {
            closeList();
            html += '<hr>';
            i++;
            continue;
        }

        // Headings
        const headingMatch = line.match(/^(#{1,3})\s+(.+)$/);
        if (headingMatch) {
            closeList();
            const level = headingMatch[1].length;
            html += `<h${level}>${inlineMarkdown(headingMatch[2])}</h${level}>`;
            i++;
            continue;
        }

        // Blockquote
        if (line.startsWith('> ')) {
            closeList();
            const quoteLines: string[] = [];
            while (i < lines.length && lines[i].startsWith('> ')) {
                quoteLines.push(lines[i].slice(2));
                i++;
            }
            html += `<blockquote><p>${inlineMarkdown(quoteLines.join(' '))}</p></blockquote>`;
            continue;
        }

        // Unordered list
        const ulMatch = line.match(/^[-*+]\s+(.+)$/);
        if (ulMatch) {
            if (inList !== 'ul') {
                closeList();
                html += '<ul>';
                inList = 'ul';
            }
            html += `<li>${inlineMarkdown(ulMatch[1])}</li>`;
            i++;
            continue;
        }

        // Ordered list
        const olMatch = line.match(/^\d+\.\s+(.+)$/);
        if (olMatch) {
            if (inList !== 'ol') {
                closeList();
                html += '<ol>';
                inList = 'ol';
            }
            html += `<li>${inlineMarkdown(olMatch[1])}</li>`;
            i++;
            continue;
        }

        // Empty line
        if (line.trim() === '') {
            closeList();
            i++;
            continue;
        }

        // Paragraph (default)
        closeList();
        html += `<p>${inlineMarkdown(line)}</p>`;
        i++;
    }

    closeList();
    return html;
}

/** Process inline markdown: bold, italic, code, links */
function inlineMarkdown(text: string): string {
    let result = escapeHtml(text);

    // Inline code (must be first to avoid bold/italic inside code)
    result = result.replace(/`([^`]+)`/g, '<code>$1</code>');

    // Bold
    result = result.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    result = result.replace(/__(.+?)__/g, '<strong>$1</strong>');

    // Italic
    result = result.replace(/\*(.+?)\*/g, '<em>$1</em>');
    result = result.replace(/_(.+?)_/g, '<em>$1</em>');

    // Links
    result = result.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2">$1</a>');

    return result;
}

function escapeHtml(text: string): string {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
}

/**
 * Convert an HTML string (from Tiptap's getHTML()) back to Markdown.
 * Uses DOMParser for reliable conversion.
 */
export function htmlToMarkdown(html: string): string {
    if (!html || html === '<p></p>') return '';

    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');
    return nodeToMarkdown(doc.body).trim();
}

function nodeToMarkdown(node: Node): string {
    let result = '';

    for (let i = 0; i < node.childNodes.length; i++) {
        const child = node.childNodes[i];

        if (child.nodeType === Node.TEXT_NODE) {
            result += child.textContent || '';
            continue;
        }

        if (child.nodeType !== Node.ELEMENT_NODE) continue;

        const el = child as HTMLElement;
        const tag = el.tagName.toLowerCase();

        switch (tag) {
            case 'h1':
                result += `# ${getInlineMarkdown(el)}\n\n`;
                break;
            case 'h2':
                result += `## ${getInlineMarkdown(el)}\n\n`;
                break;
            case 'h3':
                result += `### ${getInlineMarkdown(el)}\n\n`;
                break;
            case 'p':
                result += `${getInlineMarkdown(el)}\n\n`;
                break;
            case 'blockquote': {
                const inner = nodeToMarkdown(el).trim().split('\n');
                result += inner.map(line => `> ${line}`).join('\n') + '\n\n';
                break;
            }
            case 'ul':
                for (let j = 0; j < el.children.length; j++) {
                    result += `- ${getInlineMarkdown(el.children[j] as HTMLElement)}\n`;
                }
                result += '\n';
                break;
            case 'ol':
                for (let j = 0; j < el.children.length; j++) {
                    result += `${j + 1}. ${getInlineMarkdown(el.children[j] as HTMLElement)}\n`;
                }
                result += '\n';
                break;
            case 'pre': {
                const code = el.querySelector('code');
                const text = code ? code.textContent : el.textContent;
                result += `\`\`\`\n${text}\n\`\`\`\n\n`;
                break;
            }
            case 'hr':
                result += '---\n\n';
                break;
            default:
                result += getInlineMarkdown(el);
                break;
        }
    }

    return result;
}

/** Convert inline elements to markdown: strong→**, em→*, code→`, a→[]() */
function getInlineMarkdown(el: HTMLElement): string {
    let result = '';

    for (let i = 0; i < el.childNodes.length; i++) {
        const child = el.childNodes[i];

        if (child.nodeType === Node.TEXT_NODE) {
            result += child.textContent || '';
            continue;
        }

        if (child.nodeType !== Node.ELEMENT_NODE) continue;

        const childEl = child as HTMLElement;
        const tag = childEl.tagName.toLowerCase();

        switch (tag) {
            case 'strong':
            case 'b':
                result += `**${getInlineMarkdown(childEl)}**`;
                break;
            case 'em':
            case 'i':
                result += `*${getInlineMarkdown(childEl)}*`;
                break;
            case 'code':
                result += `\`${childEl.textContent}\``;
                break;
            case 'a':
                result += `[${getInlineMarkdown(childEl)}](${childEl.getAttribute('href') || ''})`;
                break;
            default:
                result += getInlineMarkdown(childEl);
                break;
        }
    }

    return result;
}

/** Count words in plain text */
export function countWords(text: string): number {
    return text.split(/\s+/).filter(Boolean).length;
}

/** Estimate reading time in minutes (200 wpm average) */
export function readingTime(wordCount: number): number {
    return Math.max(1, Math.ceil(wordCount / 200));
}
