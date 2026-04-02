/**
 * PURPOSE
 * - Provide lightweight rich-text affordances for plain-text content in the UI.
 * - Detect @mentions, #hashtags, and http(s) links and convert them into
 *   render-ready segments/facets.
 *
 * HOW IT WORKS
 * - Regex-based scanners detect mention/hashtag/link spans.
 * - `detectFacets()` merges detections, sorts by position, and removes overlaps.
 * - `segmentText()` converts the original string into typed segments that
 *   presentation components can map to links/chips/highlights.
 * - `linkifyFacetsInHtml()` performs a conservative text-node-only linkification
 *   pass for already-sanitized HTML content.
 *
 * RELATED FILES
 * - src/components/RichText* (rendering consumers)
 * - src/screens/* (composer/feed usage)
 * - src-tauri/src/richtext.rs (backend mirror logic; byte offsets there)
 *
 * CAVEAT (TECH-DEBT CONTEXT)
 * - Detection is intentionally heuristic and regex-driven rather than parser-backed.
 *   This keeps UI behavior fast/simple but may miss or over-match edge-case syntax.
 */

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

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

// ═══════════════════════════════════════════════════════════════════════════════
// Facet Detection
// ═══════════════════════════════════════════════════════════════════════════════

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
 * Detect all @mentions, #hashtags, and URLs in `text`.
 *
 * Returns facets sorted by ascending start index, then filtered so overlapping
 * ranges do not produce duplicate interactive spans.
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
 * Split plain text into typed render segments.
 *
 * Output ordering always preserves the original source order.
 * Each segment is either `text`, `mention`, `hashtag`, or `link`.
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
 * Create a compact display label for URLs.
 *
 * Removes protocol and truncates to `maxLen` for UI density.
 */
export function truncateUrl(url: string, maxLen = 40): string {
    let display = url.replace(/^https?:\/\//, '');
    if (display.length > maxLen) {
        display = display.slice(0, maxLen - 3) + '…';
    }
    return display;
}

/**
 * Linkify @mentions and #hashtags inside an HTML string.
 *
 * Operates only on text fragments (never mutates existing tag structure), and
 * skips text currently inside `<a>` tags to avoid nested anchors.
 *
 * IMPORTANT: run after sanitization so injected anchors are not stripped.
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

// ═══════════════════════════════════════════════════════════════════════════════
// Autocomplete Helpers
// ═══════════════════════════════════════════════════════════════════════════════

export interface AutocompleteContext {
    type: 'mention' | 'hashtag' | null;
    query: string;
    /** Character index where the trigger (@/#) starts */
    triggerStart: number;
    /** Character index where the query text ends (cursor position) */
    triggerEnd: number;
}

/**
 * Determine whether autocomplete should activate at `cursorPos`.
 *
 * Returns trigger metadata for mention/hashtag completion, or a null-type
 * context when no valid trigger is active.
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
 * Apply an autocomplete choice into the source text.
 *
 * Replaces `[trigger + query]` with the selected token and returns both the
 * updated text and the cursor position immediately after inserted content.
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
