import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ComposeBox } from './ComposeBox';
import { INPUT_LIMITS } from '../utils/sanitize';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn().mockResolvedValue(0),
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
        expect(screen.getByText(`9 / ${INPUT_LIMITS.MAX_LOCAL_CHARS.toLocaleString()}`)).toBeTruthy();
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
        expect(screen.getByText(/over limit/)).toBeTruthy();
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
        expect(onPublish).toHaveBeenCalledWith('My post', 'local', []);
    });

    it('uses initialText prop', () => {
        render(<ComposeBox {...defaultProps} initialText="Pre-filled text" />);
        const textarea = screen.getByPlaceholderText(/Share something/) as HTMLTextAreaElement;
        expect(textarea.value).toBe('Pre-filled text');
    });

    it('shows target selector buttons', () => {
        render(<ComposeBox {...defaultProps} />);
        expect(screen.getByText('🔒 Local Only')).toBeTruthy();
        expect(screen.getByText('🦋 Bluesky')).toBeTruthy();
        expect(screen.getByText('🌐 All Networks')).toBeTruthy();
    });

    it('has a Media button', () => {
        render(<ComposeBox {...defaultProps} />);
        expect(screen.getByText(/Media/)).toBeTruthy();
    });
});
