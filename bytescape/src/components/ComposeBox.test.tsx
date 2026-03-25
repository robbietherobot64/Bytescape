import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ComposeBox } from './ComposeBox';
import { INPUT_LIMITS } from '../utils/sanitize';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn().mockImplementation((cmd: string) => {
        if (cmd === 'list_linked_accounts') {
            return Promise.resolve([
                { protocol: 'bluesky', identifier: 'did:plc:test', display_handle: '@test.bsky.social', connected_at: 0, active: true },
                { protocol: 'mastodon', identifier: 'acct:test@mastodon.social', display_handle: '@test@mastodon.social', connected_at: 0, active: true },
            ]);
        }
        if (cmd === 'mastodon_get_session') return Promise.resolve(null);
        return Promise.resolve(0);
    }),
    convertFileSrc: vi.fn((path: string) => `asset://${path}`),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
    open: vi.fn().mockResolvedValue(null),
}));

describe('ComposeBox', () => {
    const defaultProps = {
        onPublish: vi.fn().mockResolvedValue(undefined),
        isPosting: false,
        postStatus: null,
    };

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders the compose box', () => {
        render(<ComposeBox {...defaultProps} />);
        expect(screen.getByText('Create Post')).toBeTruthy();
        expect(screen.getByPlaceholderText(/Share something/)).toBeTruthy();
    });

    it('disables Post button when text is empty', () => {
        render(<ComposeBox {...defaultProps} />);
        const postBtn = screen.getByText('Post');
        expect(postBtn).toBeDisabled();
    });

    it('enables Post button when text is entered', () => {
        render(<ComposeBox {...defaultProps} />);
        const textarea = screen.getByPlaceholderText(/Share something/);
        fireEvent.change(textarea, { target: { value: 'Hello world' } });
        const postBtn = screen.getByText('Post');
        expect(postBtn).not.toBeDisabled();
    });

    it('shows character count when text is entered', () => {
        render(<ComposeBox {...defaultProps} />);
        const textarea = screen.getByPlaceholderText(/Share something/);
        fireEvent.change(textarea, { target: { value: 'Test post' } });
        // Default target is 'local', so shows 💾 9/10000
        expect(screen.getByText('💾 9/10000')).toBeTruthy();
    });

    it('disables Post button when over character limit', () => {
        render(<ComposeBox {...defaultProps} />);
        const textarea = screen.getByPlaceholderText(/Share something/);
        const overLimitText = 'a'.repeat(INPUT_LIMITS.MAX_LOCAL_CHARS + 1);
        fireEvent.change(textarea, { target: { value: overLimitText } });
        const postBtn = screen.getByText('Post');
        expect(postBtn).toBeDisabled();
    });

    it('shows over limit warning text', () => {
        render(<ComposeBox {...defaultProps} />);
        const textarea = screen.getByPlaceholderText(/Share something/);
        const overLimitText = 'a'.repeat(INPUT_LIMITS.MAX_LOCAL_CHARS + 1);
        fireEvent.change(textarea, { target: { value: overLimitText } });
        // When all targets are over, the post button tooltip says "exceeds character limit"
        const postBtn = screen.getByText('Post');
        expect(postBtn.title).toMatch(/exceeds character limit/);
    });

    it('shows posting status when isPosting is true', () => {
        render(<ComposeBox {...defaultProps} isPosting={true} />);
        expect(screen.getByText('Posting...')).toBeTruthy();
    });

    it('shows post status message', () => {
        render(<ComposeBox {...defaultProps} postStatus="Published to Bluesky!" />);
        expect(screen.getByText('Published to Bluesky!')).toBeTruthy();
    });

    it('calls onPublish with correct args', async () => {
        const onPublish = vi.fn().mockResolvedValue(undefined);
        render(<ComposeBox {...defaultProps} onPublish={onPublish} />);
        const textarea = screen.getByPlaceholderText(/Share something/);
        fireEvent.change(textarea, { target: { value: 'My post' } });
        const postBtn = screen.getByText('Post');
        fireEvent.click(postBtn);
        expect(onPublish).toHaveBeenCalledWith('My post', 'local', [], undefined);
    });

    it('uses initialText prop', () => {
        render(<ComposeBox {...defaultProps} initialText="Pre-filled text" />);
        const textarea = screen.getByPlaceholderText(/Share something/) as HTMLTextAreaElement;
        expect(textarea.value).toBe('Pre-filled text');
    });

    it('shows platform dropdown button', () => {
        render(<ComposeBox {...defaultProps} />);
        expect(screen.getByText(/Platform:/)).toBeTruthy();
    });

    it('has a Media button', () => {
        render(<ComposeBox {...defaultProps} />);
        expect(screen.getByText(/Media/)).toBeTruthy();
    });

    // C1.4 — Multi-target character counting
    it('shows bluesky counter when Bluesky target selected', async () => {
        render(<ComposeBox {...defaultProps} />);
        // Wait for linked platforms to load
        await waitFor(() => {
            expect(screen.getByText(/Platform:/)).toBeTruthy();
        });
        // Open the dropdown and check Bluesky
        const platformBtn = screen.getByText(/Platform:/);
        fireEvent.click(platformBtn);
        await waitFor(() => {
            expect(screen.getByText('🦋 Bluesky')).toBeTruthy();
        });
        const bskyCheckbox = screen.getByText('🦋 Bluesky').closest('label')!.querySelector('input')!;
        fireEvent.click(bskyCheckbox);
        // Close dropdown
        fireEvent.click(platformBtn);
        const textarea = screen.getByPlaceholderText(/Share something/);
        fireEvent.change(textarea, { target: { value: 'Hello world' } });
        // Should show bluesky and local counters
        expect(screen.getByText('🦋 11/300')).toBeTruthy();
        expect(screen.getByText('💾 11/10000')).toBeTruthy();
    });

    it('shows all network counters when All platforms selected', async () => {
        render(<ComposeBox {...defaultProps} />);
        // Wait for linked platforms to load then open dropdown
        await waitFor(() => expect(screen.getByText(/Platform:/)).toBeTruthy());
        const platformBtn = screen.getByText(/Platform:/);
        fireEvent.click(platformBtn);
        await waitFor(() => expect(screen.getByText('✨ All')).toBeTruthy());
        const allCheckbox = screen.getByText('✨ All').closest('label')!.querySelector('input')!;
        fireEvent.click(allCheckbox);
        fireEvent.click(platformBtn); // close
        const textarea = screen.getByPlaceholderText(/Share something/);
        fireEvent.change(textarea, { target: { value: 'Test' } });
        expect(screen.getByText('🦋 4/300')).toBeTruthy();
        expect(screen.getByText('🐘 4/500')).toBeTruthy();
        expect(screen.getByText('💾 4/10000')).toBeTruthy();
    });

    it('shows truncation warning when Bluesky limit exceeded but local OK', async () => {
        render(<ComposeBox {...defaultProps} />);
        await waitFor(() => expect(screen.getByText(/Platform:/)).toBeTruthy());
        const platformBtn = screen.getByText(/Platform:/);
        fireEvent.click(platformBtn);
        await waitFor(() => expect(screen.getByText('🦋 Bluesky')).toBeTruthy());
        const bskyCheckbox = screen.getByText('🦋 Bluesky').closest('label')!.querySelector('input')!;
        fireEvent.click(bskyCheckbox);
        fireEvent.click(platformBtn); // close
        const textarea = screen.getByPlaceholderText(/Share something/);
        const longText = 'a'.repeat(310); // Over 300 for Bluesky, under 10K for local
        fireEvent.change(textarea, { target: { value: longText } });
        // Should show truncation warning
        expect(screen.getByText(/Text exceeds limit on/)).toBeTruthy();
        // Post button should still be enabled (only Bluesky is over, local is fine)
        const postBtn = screen.getByText('Post');
        expect(postBtn).not.toBeDisabled();
    });

    // ─── Link Preview Tests (C2.1) ───

    it('shows loading indicator when URL detected in text', async () => {
        const { invoke } = await import('@tauri-apps/api/core');
        (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
            if (cmd === 'unfurl_link') {
                return new Promise(() => {}); // never resolves — keeps loading
            }
            return Promise.resolve(0);
        });

        render(<ComposeBox {...defaultProps} />);
        const textarea = screen.getByPlaceholderText(/Share something/);
        fireEvent.change(textarea, { target: { value: 'Check out https://example.com' } });

        // The link detection is debounced at 500ms
        await waitFor(() => {
            expect(screen.getByText(/Loading link preview/)).toBeTruthy();
        }, { timeout: 1500 });
    });

    it('renders link preview card when unfurl succeeds', async () => {
        const { invoke } = await import('@tauri-apps/api/core');
        (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
            if (cmd === 'unfurl_link') {
                return Promise.resolve({
                    url: 'https://example.com',
                    title: 'Example Domain',
                    description: 'This domain is for examples.',
                    image_url: null,
                    site_name: 'Example',
                    favicon_url: null,
                });
            }
            return Promise.resolve(0);
        });

        render(<ComposeBox {...defaultProps} />);
        const textarea = screen.getByPlaceholderText(/Share something/);
        fireEvent.change(textarea, { target: { value: 'Visit https://example.com today' } });

        await waitFor(() => {
            expect(screen.getByText('Example Domain')).toBeTruthy();
            expect(screen.getByText('This domain is for examples.')).toBeTruthy();
        }, { timeout: 2000 });
    });

    it('dismisses link preview when X button clicked', async () => {
        const { invoke } = await import('@tauri-apps/api/core');
        (invoke as ReturnType<typeof vi.fn>).mockImplementation((cmd: string) => {
            if (cmd === 'unfurl_link') {
                return Promise.resolve({
                    url: 'https://example.com',
                    title: 'Example Domain',
                    description: null,
                    image_url: null,
                    site_name: null,
                    favicon_url: null,
                });
            }
            return Promise.resolve(0);
        });

        render(<ComposeBox {...defaultProps} />);
        const textarea = screen.getByPlaceholderText(/Share something/);
        fireEvent.change(textarea, { target: { value: 'See https://example.com' } });

        await waitFor(() => {
            expect(screen.getByText('Example Domain')).toBeTruthy();
        }, { timeout: 2000 });

        // Click the dismiss button
        const dismissBtn = screen.getByLabelText('Remove link preview');
        fireEvent.click(dismissBtn);

        // Card should be gone
        expect(screen.queryByText('Example Domain')).toBeNull();
    });
});
