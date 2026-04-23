import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { BlogModule } from './BlogModule';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../../utils/openInApp', () => ({ openInApp: vi.fn() }));

describe('BlogModule', () => {
    const baseMod = {
        id: 'mod-blogs',
        type: 'blogs' as const,
        title: 'Blogs',
        order: 0,
        visible: true,
        width: 'full' as const,
        content: JSON.stringify({
            entries: [
                { id: 'b1', title: 'My Post', body: 'Hello world', timestamp: 1700000000, external_url: 'https://example.com/post' },
                { id: 'b2', title: 'Local Post', body: 'No link', timestamp: 1700000001 },
            ],
        }),
    };

    it('defaults entries with external_url to inline iframe view', () => {
        render(<BlogModule mod={baseMod} editMode={false} onSaveDirect={vi.fn()} />);
        // Expand the entry with external_url
        fireEvent.click(screen.getByText('My Post'));
        // The "View Inline" button should have the active class (default)
        const inlineBtn = screen.getByText(/View Inline/);
        expect(inlineBtn.className).toContain('active');
        // The "Text" button should NOT have the active class
        const textBtn = screen.getByText(/Text/);
        expect(textBtn.className).not.toContain('active');
    });

    it('allows opting out to text view', () => {
        render(<BlogModule mod={baseMod} editMode={false} onSaveDirect={vi.fn()} />);
        fireEvent.click(screen.getByText('My Post'));
        // Click "Text" to opt out
        fireEvent.click(screen.getByText(/Text/));
        const textBtn = screen.getByText(/Text/);
        expect(textBtn.className).toContain('active');
    });

    it('does not show iframe toggle for entries without external_url', () => {
        render(<BlogModule mod={baseMod} editMode={false} onSaveDirect={vi.fn()} />);
        fireEvent.click(screen.getByText('Local Post'));
        expect(screen.queryByText(/View Inline/)).toBeNull();
    });
});
