/**
 * Frontend Rich Text Facet Detection & Segmentation (C1.1b)
 *
 * Detects @mentions, #hashtags, and hyperlinks in plain text and splits the
 * text into typed segments for rendering by the RichText component.
 *
 * Mirrors the detection logic in `src-tauri/src/richtext.rs` but operates
 * on JavaScript character indices (not UTF-8 byte offsets) since this is
 * purely for frontend rendering.
 */

// ─── Types ───

export interface DetectedFacet {
    /** Character index (inclusive) */
    start: number;
    /** Character index (exclusive) */
    end: number;
    type: 'mention' | 'hashtag' | 'link';
    /** Handle (without @), tag (without #), or full URL */
    value: string;
    /** The original matched text including @/# prefix */
    text: string;
}

export interface RichTextSegment {
    text: string;
    type: 'text' | 'mention' | 'hashtag' | 'link';
    /** Handle (without @) for mentions, tag (without #) for hashtags, URL for links */
    value?: string;
}

// ─── Facet Detection ───

// Mention: @handle.domain (must contain at least one dot, domain-like)
const MENTION_RE = /(?:^|[\s(])(@([a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+))/g;

// Hashtag: #tag (must not start with digit, preceded by whitespace or start)
const HASHTAG_RE = /(?:^|\s)(#[^\d\s]\S*)/g;

// URL: http:// or https:// followed by non-whitespace
const LINK_RE = /(?:^|\s)(https?:\/\/\S+)/g;

function detectMentions(text: string): DetectedFacet[] {
    const facets: DetectedFacet[] = [];
    const re = new RegExp(MENTION_RE.source, 'g');
    let match: RegExpExecArray | null;

    while ((match = re.exec(text)) !== null) {
        const fullMatch = match[1]; // The @handle portion
        const handle = match[2];    // Without the @

        if (!handle.includes('.')) continue;

        // Find the actual position of @handle within the full match group
        const matchStart = match.index + match[0].indexOf(fullMatch);

        facets.push({
            start: matchStart,
            end: matchStart + fullMatch.length,
            type: 'mention',
            value: handle,
            text: fullMatch,
        });
    }
    return facets;
}

function detectHashtags(text: string): DetectedFacet[] {
    const facets: DetectedFacet[] = [];
    const re = new RegExp(HASHTAG_RE.source, 'g');
    let match: RegExpExecArray | null;

    while ((match = re.exec(text)) !== null) {
        const fullMatch = match[1]; // The #tag portion
        let tag = fullMatch;

        // Strip trailing punctuation (matching Rust backend logic)
        while (/[.,;!?]$/.test(tag)) {
            tag = tag.slice(0, -1);
        }

        // Max length: # + 64 chars = 65
        if (tag.length > 65) continue;

        const tagValue = tag.slice(1); // Without #
        if (!tagValue) continue;

        const matchStart = match.index + match[0].indexOf(fullMatch);

        facets.push({
            start: matchStart,
            end: matchStart + tag.length,
            type: 'hashtag',
            value: tagValue,
            text: tag,
        });
    }
    return facets;
}

function detectLinks(text: string): DetectedFacet[] {
    const facets: DetectedFacet[] = [];
    const re = new RegExp(LINK_RE.source, 'g');
    let match: RegExpExecArray | null;

    while ((match = re.exec(text)) !== null) {
        const fullMatch = match[1]; // The URL portion
        let uri = fullMatch;

        // Strip trailing punctuation that's likely not part of the URL
        while (/[.,;!?]$/.test(uri)) {
            uri = uri.slice(0, -1);
        }
        // Strip trailing ) only if there's no matching ( in the URL
        if (uri.endsWith(')') && !uri.includes('(')) {
            uri = uri.slice(0, -1);
        }

        const matchStart = match.index + match[0].indexOf(fullMatch);

        facets.push({
            start: matchStart,
            end: matchStart + uri.length,
            type: 'link',
            value: uri,
            text: uri,
        });
    }
    return facets;
}

/**
 * Detect all @mentions, #hashtags, and URLs in text.
 * Returns facets sorted by position with overlaps removed.
 */
export function detectFacets(text: string): DetectedFacet[] {
    const facets = [
        ...detectMentions(text),
        ...detectHashtags(text),
        ...detectLinks(text),
    ];

    // Sort by start position
    facets.sort((a, b) => a.start - b.start);

    // Remove overlapping facets (keep the first one)
    const filtered: DetectedFacet[] = [];
    let lastEnd = 0;
    for (const facet of facets) {
        if (facet.start >= lastEnd) {
            lastEnd = facet.end;
            filtered.push(facet);
        }
    }

    return filtered;
}

/**
 * Split text into typed segments for rendering.
 * Each segment is either plain text, a mention, hashtag, or link.
 */
export function segmentText(text: string): RichTextSegment[] {
    if (!text) return [];

    const facets = detectFacets(text);
    if (facets.length === 0) {
        return [{ text, type: 'text' }];
    }

    const segments: RichTextSegment[] = [];
    let cursor = 0;

    for (const facet of facets) {
        // Plain text before this facet
        if (cursor < facet.start) {
            segments.push({ text: text.slice(cursor, facet.start), type: 'text' });
        }

        segments.push({
            text: text.slice(facet.start, facet.end),
            type: facet.type,
            value: facet.value,
        });

        cursor = facet.end;
    }

    // Trailing plain text
    if (cursor < text.length) {
        segments.push({ text: text.slice(cursor), type: 'text' });
    }

    return segments;
}

/**
 * Truncate a URL for display purposes.
 * Removes protocol, truncates path if too long.
 */
export function truncateUrl(url: string, maxLen = 40): string {
    let display = url.replace(/^https?:\/\//, '');
    if (display.length > maxLen) {
        display = display.slice(0, maxLen - 3) + '…';
    }
    return display;
}

/**
 * Linkify @mentions and #hashtags in an HTML string (for ArticleReader).
 * Operates on text nodes only — doesn't modify content inside existing tags.
 * Must be applied AFTER sanitization to avoid having tags stripped.
 */
export function linkifyFacetsInHtml(html: string): string {
    // Split HTML into tag segments and text segments
    const parts = html.split(/(<[^>]+>)/);
    let insideAnchor = false;

    return parts.map(part => {
        // HTML tag — track whether we're inside an <a> to avoid nested links
        if (part.startsWith('<')) {
            if (/^<a[\s>]/i.test(part)) insideAnchor = true;
            if (/^<\/a>/i.test(part)) insideAnchor = false;
            return part;
        }

        // Don't linkify text inside existing <a> tags
        if (insideAnchor) return part;

        // Linkify text node
        return linkifyTextNode(part);
    }).join('');
}

function linkifyTextNode(text: string): string {
    if (!text.trim()) return text;

    let result = text;

    // Linkify @mentions
    result = result.replace(
        /(^|[\s(])(@([a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+))/g,
        '$1<a class="facet-mention">@$3</a>'
    );

    // Linkify #hashtags
    result = result.replace(
        /(^|\s)(#([^\d\s]\S{0,63}))/g,
        (_match, space: string, _full: string, tag: string) => {
            // Strip trailing punctuation
            const cleanTag = tag.replace(/[.,;!?]+$/, '');
            if (!cleanTag) return _match;
            const trailing = tag.slice(cleanTag.length);
            return `${space}<a class="facet-hashtag">#${cleanTag}</a>${trailing}`;
        }
    );

    return result;
}

// ─── Autocomplete Helpers ───

export interface AutocompleteContext {
    type: 'mention' | 'hashtag' | null;
    query: string;
    /** Character index where the trigger (@/#) starts */
    triggerStart: number;
    /** Character index where the query text ends (cursor position) */
    triggerEnd: number;
}

/**
 * Determine if the cursor is in a position where autocomplete should activate.
 * Returns the autocomplete context (type + query) or null type if no trigger.
 */
export function getAutocompleteContext(text: string, cursorPos: number): AutocompleteContext {
    const beforeCursor = text.substring(0, cursorPos);

    // Look for @mention trigger: @ preceded by whitespace or start, followed by 2+ chars
    const mentionMatch = beforeCursor.match(/(?:^|\s)@(\S{2,})$/);
    if (mentionMatch) {
        const query = mentionMatch[1];
        const triggerStart = cursorPos - query.length - 1; // -1 for the @
        return { type: 'mention', query, triggerStart, triggerEnd: cursorPos };
    }

    // Look for #hashtag trigger: # preceded by whitespace or start, followed by 1+ chars
    const hashtagMatch = beforeCursor.match(/(?:^|\s)#(\S+)$/);
    if (hashtagMatch) {
        const query = hashtagMatch[1];
        const triggerStart = cursorPos - query.length - 1; // -1 for the #
        return { type: 'hashtag', query, triggerStart, triggerEnd: cursorPos };
    }

    return { type: null, query: '', triggerStart: 0, triggerEnd: 0 };
}

/**
 * Replace the autocomplete trigger+query in text with the selected value.
 * Returns the new text and the new cursor position.
 */
export function applyAutocompleteSelection(
    text: string,
    context: AutocompleteContext,
    selectedValue: string,
): { text: string; cursorPos: number } {
    const prefix = context.type === 'mention' ? '@' : '#';
    const replacement = `${prefix}${selectedValue} `;
    const before = text.substring(0, context.triggerStart);
    const after = text.substring(context.triggerEnd);
    const newText = before + replacement + after;
    const newCursor = before.length + replacement.length;
    return { text: newText, cursorPos: newCursor };
}
