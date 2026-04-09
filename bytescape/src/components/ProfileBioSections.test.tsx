import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ProfileBioSections } from './ProfileBioSections';
import { invoke } from '@tauri-apps/api/core';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

const mockInvoke = invoke as ReturnType<typeof vi.fn>;

function setupDefaultMocks() {
    mockInvoke.mockImplementation((cmd: string) => {
        switch (cmd) {
            case 'get_widget_data':
                return Promise.resolve(null); // No saved data — use defaults
            case 'save_widget_data':
                return Promise.resolve();
            case 'list_guestbook_entries':
                return Promise.resolve([]);
            case 'add_guestbook_entry':
                return Promise.resolve({
                    id: 'test-entry',
                    author: 'TestUser',
                    message: 'Hello!',
                    timestamp: Math.floor(Date.now() / 1000),
                    node_id: null,
                });
            case 'delete_guestbook_entry':
                return Promise.resolve();
            case 'list_follows':
                return Promise.resolve([
                    { did: 'did:plc:abc', handle: 'alice.bsky.social', displayName: 'Alice' },
                    { did: 'did:plc:def', handle: 'bob.bsky.social', displayName: 'Bob' },
                ]);
            case 'list_contacts':
                return Promise.resolve([
                    { did: 'did:plc:xyz', handle: 'carol.bsky.social' },
                ]);
            case 'list_linked_accounts':
                return Promise.resolve([]);
            default:
                return Promise.resolve(null);
        }
    });
}

describe('ProfileBioSections', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        setupDefaultMocks();
    });

    it('renders default sections when no saved data', async () => {
        render(<ProfileBioSections handle="testuser.bsky.social" did="did:plc:test123" source="bluesky" />);

        await waitFor(() => {
            expect(screen.getByText('About Me')).toBeTruthy();
        });
        expect(screen.getByText('Connections')).toBeTruthy();
        expect(screen.getByText('Now Playing')).toBeTruthy();
        expect(screen.getByText('Guestbook')).toBeTruthy();
        expect(screen.getByText('Links')).toBeTruthy();
    });

    it('shows Edit Bio button in view mode', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('Edit Bio')).toBeTruthy();
        });
    });

    it('enters edit mode when Edit Bio clicked', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('Edit Bio')).toBeTruthy();
        });

        fireEvent.click(screen.getByText('Edit Bio'));

        expect(screen.getByText('Save')).toBeTruthy();
        expect(screen.getByText('Cancel')).toBeTruthy();
        expect(screen.getByText('Add Section')).toBeTruthy();
    });

    it('cancels edit mode without saving', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('Edit Bio')).toBeTruthy();
        });

        fireEvent.click(screen.getByText('Edit Bio'));
        expect(screen.getByText('Save')).toBeTruthy();

        fireEvent.click(screen.getByText('Cancel'));
        expect(screen.getByText('Edit Bio')).toBeTruthy();
    });

    it('loads saved sections from backend', async () => {
        mockInvoke.mockImplementation((cmd: string) => {
            if (cmd === 'get_widget_data') {
                return Promise.resolve(JSON.stringify({
                    version: 1,
                    sections: [
                        { id: 'about', type: 'about', title: 'My Bio', content: 'I am a test user.', order: 0, visible: true },
                    ],
                }));
            }
            if (cmd === 'list_guestbook_entries') return Promise.resolve([]);
            if (cmd === 'list_follows') return Promise.resolve([]);
            if (cmd === 'list_contacts') return Promise.resolve([]);
            return Promise.resolve(null);
        });

        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('My Bio')).toBeTruthy();
        });
        expect(screen.getByText('I am a test user.')).toBeTruthy();
    });

    it('shows connection counts from backend data', async () => {
        render(<ProfileBioSections handle="testuser.bsky.social" did="did:plc:test123" source="bluesky" />);

        await waitFor(() => {
            expect(screen.getByText('👥 2 following')).toBeTruthy();
        });
        expect(screen.getByText('🤝 1 contacts')).toBeTruthy();
    });

    it('renders Bluesky handle badge in Connections', async () => {
        render(<ProfileBioSections handle="testuser.bsky.social" did="did:plc:test123" source="bluesky" />);

        await waitFor(() => {
            expect(screen.getByText('🦋 testuser.bsky.social')).toBeTruthy();
        });
    });

    it('saves sections when Save is clicked', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('Edit Bio')).toBeTruthy();
        });

        fireEvent.click(screen.getByText('Edit Bio'));
        fireEvent.click(screen.getByText('Save'));

        await waitFor(() => {
            expect(mockInvoke).toHaveBeenCalledWith('save_widget_data', expect.objectContaining({
                widgetData: expect.stringContaining('"version":1'),
            }));
        });
    });

    it('renders guestbook sign form', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByPlaceholderText('Leave a message...')).toBeTruthy();
        });
        expect(screen.getByText('Sign')).toBeTruthy();
    });

    it('adds guestbook entry on submit', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByPlaceholderText('Leave a message...')).toBeTruthy();
        });

        const input = screen.getByPlaceholderText('Leave a message...');
        fireEvent.change(input, { target: { value: 'Hello world!' } });
        fireEvent.click(screen.getByText('Sign'));

        await waitFor(() => {
            expect(mockInvoke).toHaveBeenCalledWith('add_guestbook_entry', {
                author: 'test',
                message: 'Hello world!',
                nodeId: null,
            });
        });
    });

    it('shows placeholder text in About Me when empty', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('Tell the world about yourself.')).toBeTruthy();
        });
    });

    it('shows Now Playing placeholder when empty', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('Nothing playing right now.')).toBeTruthy();
        });
    });

    it('shows No links placeholder when empty', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('No links added yet.')).toBeTruthy();
        });
    });

    it('deletes a section in edit mode', async () => {
        render(<ProfileBioSections handle="test" did="did:plc:test" />);

        await waitFor(() => {
            expect(screen.getByText('Edit Bio')).toBeTruthy();
        });

        fireEvent.click(screen.getByText('Edit Bio'));

        // In edit mode, each section has a delete button in controls and a close button on title bar
        // The Win98Box close button is rendered when onClose is provided
        const closeButtons = screen.getAllByTitle('Close');
        const initialSections = 5; // About, Now Playing, Links, Guestbook, Connections
        expect(closeButtons.length).toBe(initialSections);

        // Click close on the first section (About Me)
        fireEvent.click(closeButtons[0]);

        // Now there should be one fewer section
        const remainingCloseButtons = screen.getAllByTitle('Close');
        expect(remainingCloseButtons.length).toBe(initialSections - 1);
    });
});
