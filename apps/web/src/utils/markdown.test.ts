import { describe, it, expect } from 'vitest';
import { parseMarkdown, sanitizeUrl, stripHtml } from './markdown';

describe('sanitizeUrl', () => {
  it('allows http/https/mailto', () => {
    expect(sanitizeUrl('https://example.com/a')).toBe('https://example.com/a');
    expect(sanitizeUrl('http://example.com')).toBe('http://example.com');
    expect(sanitizeUrl('mailto:a@b.com')).toBe('mailto:a@b.com');
  });

  it('rejects javascript/data/vbscript schemes (XSS)', () => {
    expect(sanitizeUrl('javascript:alert(1)')).toBeNull();
    expect(sanitizeUrl('JavaScript:alert(1)')).toBeNull();
    expect(sanitizeUrl('data:text/html,<script>x</script>')).toBeNull();
    expect(sanitizeUrl('vbscript:msgbox')).toBeNull();
  });

  it('rejects scheme smuggling via whitespace/control chars', () => {
    expect(sanitizeUrl('java\nscript:alert(1)')).toBeNull();
    expect(sanitizeUrl(' javascript:alert(1)')).toBeNull();
    expect(sanitizeUrl('java\tscript:alert(1)')).toBeNull();
  });
});

describe('stripHtml', () => {
  it('removes script tags and their bodies', () => {
    expect(stripHtml('hi <script>alert(1)</script> there')).toBe('hi  there');
    expect(stripHtml('<script src="x.js"></script>ok')).toBe('ok');
  });

  it('removes all tags but keeps inner text', () => {
    expect(stripHtml('<b>bold</b> and <img src=x>')).toBe('bold and ');
    expect(stripHtml('<a href="javascript:alert(1)">click</a>')).toBe('click');
  });
});

describe('parseMarkdown links', () => {
  it('extracts a markdown link with sanitized href', () => {
    const segs = parseMarkdown('[Cats](https://example.com/cats)');
    expect(segs).toEqual([{ t: 'link', text: 'Cats', href: 'https://example.com/cats' }]);
  });

  it('drops an unsafe href but keeps the link text', () => {
    const segs = parseMarkdown('[x](javascript:alert(1))');
    expect(segs).toEqual([{ t: 'text', v: 'x' }]);
    expect(JSON.stringify(segs)).not.toContain('javascript');
  });

  it('unescapes escaped chars in link text', () => {
    const segs = parseMarkdown('[\\#Cats](https://e.com)');
    expect(segs[0]).toEqual({ t: 'link', text: '#Cats', href: 'https://e.com' });
  });

  it('auto-links bare URLs', () => {
    const segs = parseMarkdown('see https://example.com/x now');
    expect(segs).toContainEqual({ t: 'link', text: 'https://example.com/x', href: 'https://example.com/x' });
  });
});

describe('parseMarkdown emphasis/structure', () => {
  it('parses bold, italic, inline code', () => {
    const segs = parseMarkdown('**bold** *it* `code`');
    expect(segs).toContainEqual({ t: 'strong', v: 'bold' });
    expect(segs).toContainEqual({ t: 'em', v: 'it' });
    expect(segs).toContainEqual({ t: 'code', v: 'code' });
  });

  it('parses headings at line start', () => {
    const segs = parseMarkdown('## Section title');
    expect(segs).toContainEqual({ t: 'heading', level: 2, v: 'Section title' });
  });

  it('keeps plain text escaped (no raw HTML passes through)', () => {
    const segs = parseMarkdown('<img src=x onerror=alert(1)> hello');
    const text = segs.map((s) => ('v' in s ? s.v : '')).join('');
    expect(text).toBe(' hello');
    expect(text).not.toContain('<img');
    expect(text).not.toContain('onerror');
  });
});

describe('XSS through crafted markdown', () => {
  it('neutralizes script injection in link text and body', () => {
    const segs = parseMarkdown('[<script>alert(1)</script>](https://e.com)');
    // The script tags are stripped before parsing; text is rendered escaped.
    const rendered = JSON.stringify(segs);
    expect(rendered).not.toContain('<script');
    expect(rendered).not.toContain('alert(1)</script>');
  });

  it('never emits an href with a dangerous scheme', () => {
    const segs = parseMarkdown('[a](javascript:alert(1)) [b](data:text/html,x) [c](https://ok.com)');
    const hrefs: string[] = [];
    for (const s of segs) {
      if (s.t === 'link') hrefs.push(s.href); // narrowed: link has href
    }
    expect(hrefs).toEqual(['https://ok.com']);
    for (const h of hrefs) {
      expect(h).toMatch(/^(https?:\/\/|mailto:)/);
    }
  });
});
