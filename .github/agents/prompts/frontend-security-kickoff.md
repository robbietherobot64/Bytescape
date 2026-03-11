# Frontend Agent — Security Hardening Kickoff

## Priority: Address the 2 security findings from the infrastructure audit

Read `.github/agents/security-addendum.md` for full context. You have 2 assigned items:

---

### Task 1: HIGH-3 — XSS Sanitization on User Content

**Problem**: `BytescapeItem.content` may contain raw HTML from imported Facebook/Instagram archives. If any component renders this via `dangerouslySetInnerHTML` or trusts it as safe HTML, it's a stored XSS vector.

**What to implement**:

1. **Audit all components** for unsafe HTML rendering. Search for:
   - `dangerouslySetInnerHTML`
   - Any component that renders `item.content`, `entry.content`, `post.text`, or imported archive text as HTML
   - Check: `FeedCard`, `PostCard`, `ProfileScreen`, `FeedScreen`, any component displaying post content

2. **Install `dompurify`**:
   ```bash
   cd bytescape && pnpm add dompurify && pnpm add -D @types/dompurify
   ```

3. **Create a sanitization utility** at `src/utils/sanitize.ts`:
   ```typescript
   import DOMPurify from 'dompurify';
   
   const ALLOWED_TAGS = ['b', 'i', 'em', 'strong', 'a', 'br', 'p', 'span', 'ul', 'ol', 'li'];
   const ALLOWED_ATTR = ['href', 'target', 'rel'];
   
   export function sanitizeHtml(dirty: string): string {
     return DOMPurify.sanitize(dirty, { 
       ALLOWED_TAGS, 
       ALLOWED_ATTR,
       ADD_ATTR: ['target'],  // Allow target="_blank"
     });
   }
   
   /** Strip ALL HTML — returns plain text only */
   export function stripHtml(dirty: string): string {
     return DOMPurify.sanitize(dirty, { ALLOWED_TAGS: [] });
   }
   ```

4. **Replace all unsafe rendering** with the sanitized version:
   - If HTML rendering is needed: use `dangerouslySetInnerHTML={{ __html: sanitizeHtml(content) }}`
   - If plain text is sufficient (most cases): just render `{content}` directly (React auto-escapes)
   - For search/preview contexts: use `stripHtml(content).slice(0, 200)`

5. **Add a test** in a new file `src/utils/sanitize.test.ts`:
   - Test that `<script>alert('xss')</script>` is stripped
   - Test that `<b>bold</b>` is preserved
   - Test that `<a href="javascript:alert('xss')">click</a>` has the href removed
   - Test that `<img src=x onerror=alert('xss')>` is stripped

---

### Task 2: MED-4 — Input Length Validation

**Problem**: Local/P2P posts have no maximum length. A bug or adversarial input could create posts with megabytes of text.

**What to implement**:

1. **In `ComposeBox.tsx`** (or wherever the compose input lives):
   - Add a constant: `const MAX_LOCAL_CHARS = 10_000;`
   - Show a warning when text exceeds 8,000 characters
   - Disable the Post button when text exceeds 10,000 characters
   - Show the limit in the character counter: `"9,500 / 10,000"`

2. **Attachment limits**:
   - Maximum 20 attachments per post
   - Disable the "add attachment" button when at 20
   - Show count: `"3 / 20 attachments"`

3. **File size validation** (in the attachment picker):
   - Warn at 50MB per file (yellow indicator)
   - Block at 100MB per file (red indicator, don't add to list)
   - Show human-readable size: `"47.3 MB"` next to each attachment

---

## Workflow

1. Create branch `security/frontend-hardening` from current HEAD
2. Install `dompurify` + types
3. Implement sanitization utility + tests
4. Audit and fix all content rendering components
5. Add input length validation to ComposeBox
6. Run `pnpm test` from `bytescape/` — all tests must pass
7. Run `pnpm tauri dev` from `bytescape/` — verify app loads and content displays correctly
8. Commit with: `fix(security): XSS sanitization, input length validation`
