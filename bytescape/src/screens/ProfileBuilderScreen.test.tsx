import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { ProfileBuilderScreen } from './ProfileBuilderScreen';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn().mockResolvedValue(null),
}));

// Mock GrapesJS — heavy library, no JSDOM canvas support
vi.mock('grapesjs', () => {
    const mockEditor = {
        Blocks: {
            add: vi.fn(),
        },
        Canvas: {
            getDocument: () => null,
        },
        getHtml: () => '<div>test</div>',
        getCss: () => '.test { color: red; }',
        setComponents: vi.fn(),
        setStyle: vi.fn(),
        storeData: () => ({ components: [], styles: [] }),
        loadData: vi.fn(),
        runCommand: vi.fn(),
        stopCommand: vi.fn(),
        destroy: vi.fn(),
    };

    return {
        default: {
            init: vi.fn(() => mockEditor),
        },
    };
});

// Mock the GrapesJS CSS import
vi.mock('grapesjs/dist/css/grapes.min.css', () => ({}));

// Mock Zustand navigation store
vi.mock('../store', () => ({
    useNavStore: () => ({
        setScreen: vi.fn(),
        setActiveSidebar: vi.fn(),
    }),
}));

describe('ProfileBuilderScreen', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders loading state initially', () => {
        render(<ProfileBuilderScreen />);
        expect(screen.getByText('Loading Profile Builder...')).toBeInTheDocument();
    });

    it('renders toolbar with expected buttons', () => {
        render(<ProfileBuilderScreen />);
        expect(screen.getByTitle('Back to Profile')).toBeInTheDocument();
        expect(screen.getByTitle('Preview')).toBeInTheDocument();
        expect(screen.getByTitle('Save Layout')).toBeInTheDocument();
    });

    it('displays Profile Builder title', () => {
        render(<ProfileBuilderScreen />);
        expect(screen.getByText('✨ Profile Builder ✨')).toBeInTheDocument();
    });

    it('has a back button', () => {
        render(<ProfileBuilderScreen />);
        const backBtn = screen.getByTitle('Back to Profile');
        expect(backBtn).toBeInTheDocument();
        expect(backBtn.textContent).toContain('Back');
    });

    it('renders save button with correct label', () => {
        render(<ProfileBuilderScreen />);
        const saveBtn = screen.getByTitle('Save Layout');
        expect(saveBtn.textContent).toContain('Save');
    });
});
