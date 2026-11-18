/**
 * Minimal, SAFE markdown handling for fediverse (Lemmy/PieFed) post bodies.
 *
 * Fediverse posts arrive with raw markdown (links, emphasis, headings, code)
 * that currently renders as literal text. This module parses a small, safe
 * subset into segments that the React component renders as elements — React
 * escapes all text, so no raw HTML can execute. Source HTML is stripped up
 * front (tags + script bodies), and link URLs are scheme-sanitized (only
 * http/https/mailto survive), so crafted markdown cannot inject script.
 *
 * Scope (owner spec): links, bold, italic, headings, inline code, bare URLs.
 * Everything else renders as escaped plain text.
 */

export type Seg =
  | { t: 'text'; v: string }
  | { t: 'link'; text: string; href: string }
  | { t: 'strong'; v: string }
  | { t: 'em'; v: string }
  | { t: 'code'; v: string }
  | { t: 'heading'; level: number; v: string };

/** Remove raw HTML: script/style bodies first, then any tag. */
export function stripHtml(input: string): string {
  return input
    .replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, '')
    .replace(/<style\b[^<]*(?:(?!<\/style>)<[^<]*)*<\/style>/gi, '')
    .replace(/<[^>]*>/g, '');
}

/** Reject strings containing whitespace or control characters (smuggling). */
function hasControlOrSpace(u: string): boolean {
  for (const ch of u) {
    const c = ch.codePointAt(0)!;
    if (c <= 0x20 || c === 0x7f) return true;
  }
  return false;
}

/** Allow only http/https/mailto URLs; anything else (javascript:, data:, ...) is rejected. */
export function sanitizeUrl(raw: string): string | null {
  const u = raw.trim();
  if (hasControlOrSpace(u)) return null;
  if (/^(https?:\/\/|mailto:)/i.test(u)) return u;
  return null;
}

/** Unescape markdown backslash escapes (e.g. backslash-hash to hash). */
function unescapeMd(s: string): string {
  return s.replace(/\\([\\`*_{}[\]()#+\-.!>])/g, '$1');
}

// Inline token order matters: code -> link -> bold -> italic -> bare URL.
// Link URLs allow one level of balanced parens (e.g. `javascript:alert(1)` or
// real `.../wiki/Foo_(bar)`), so the extraction below can still see them.
const INLINE_RE =
  /(`[^`\n]+`)|(\[[^\]\n]*\]\([^()\n]*(?:\([^()\n]*\)[^()\n]*)*\))|(\*\*[^*\n]+\*\*)|(\*[^*\n]+\*)|(_[^_\n]+_)|(https?:\/\/[^\s<>"')\]]+)/g;

/**
 * Parse a body string into render segments. Deterministic and pure — the
 * escaping guarantee comes from rendering text as React children (never
 * `dangerouslySetInnerHTML`), plus `stripHtml`/`sanitizeUrl` here.
 */
export function parseMarkdown(src: string): Seg[] {
  const clean = stripHtml(src);
  const segs: Seg[] = [];
  let pos = 0;
  let match: RegExpExecArray | null;
  INLINE_RE.lastIndex = 0;
  while ((match = INLINE_RE.exec(clean)) !== null) {
    if (match.index > pos) {
      segs.push(...splitHeading(clean.slice(pos, match.index)));
    }
    const [full, code, link, strong, emStar, emUnder, bareUrl] = match;
    const em = emStar ?? emUnder;
    if (code) {
      segs.push({ t: 'code', v: code.slice(1, -1) });
    } else if (link) {
      const m = link.match(/^\[([^\]]*)\]\(([^()]*(?:\([^()]*\)[^()]*)*)\)$/);
      if (m) {
        const text = unescapeMd(m[1]);
        const href = sanitizeUrl(m[2]);
        if (href) segs.push({ t: 'link', text, href });
        else segs.push({ t: 'text', v: text });
      } else {
        segs.push({ t: 'text', v: link });
      }
    } else if (strong) {
      segs.push({ t: 'strong', v: unescapeMd(strong.slice(2, -2)) });
    } else if (em) {
      segs.push({ t: 'em', v: unescapeMd(em.slice(1, -1)) });
    } else if (bareUrl) {
      const href = sanitizeUrl(bareUrl);
      if (href) segs.push({ t: 'link', text: bareUrl, href });
      else segs.push({ t: 'text', v: bareUrl });
    }
    pos = match.index + full.length;
  }
  if (pos < clean.length) {
    segs.push(...splitHeading(clean.slice(pos)));
  }
  return mergeText(segs);
}

/** Detect heading prefixes (#, ##, ###) at the start of a line. */
function splitHeading(s: string): Seg[] {
  const out: Seg[] = [];
  const lines = s.split('\n');
  lines.forEach((line, i) => {
    const m = line.match(/^(#{1,3})\s+(.*)$/);
    if (m) {
      out.push({ t: 'heading', level: m[1].length, v: unescapeMd(m[2]) });
    } else {
      out.push({ t: 'text', v: unescapeMd(line) });
    }
    if (i < lines.length - 1) out.push({ t: 'text', v: '\n' });
  });
  return out;
}

/** Collapse adjacent text segments for simpler output. */
function mergeText(segs: Seg[]): Seg[] {
  const out: Seg[] = [];
  for (const s of segs) {
    const last = out[out.length - 1];
    if (s.t === 'text' && last && last.t === 'text') {
      last.v += s.v;
    } else {
      out.push(s);
    }
  }
  return out;
}
