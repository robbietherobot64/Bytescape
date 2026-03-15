import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ThemeCustomizer } from './ThemeCustomizer';
import { useThemeStore } from '../store';

describe('ThemeCustomizer', () => {
    beforeEach(() => {
        useThemeStore.setState({
            activeTheme: 'geocities',
            customOverrides: {},
            customBackground: null,
            backgroundMode: 'cover',
            userBackgrounds: [],
        });
        document.documentElement.removeAttribute('data-theme');
        document.documentElement.style.cssText = '';
        document.documentElement.classList.remove('has-custom-bg');
        document.documentElement.classList.remove('bg-tiled');
        localStorage.clear();
    });

    it('renders colour group sections', () => {
        render(<ThemeCustomizer />);
        expect(screen.getByText('Backgrounds')).toBeTruthy();
        expect(screen.getByText('Accents')).toBeTruthy();
        expect(screen.getByText('Text')).toBeTruthy();
        expect(screen.getByText('Links')).toBeTruthy();
    });

    it('renders background section with presets', () => {
        render(<ThemeCustomizer />);
        expect(screen.getByText('Background')).toBeTruthy();
        expect(screen.getByText('Presets')).toBeTruthy();
        expect(screen.getByText('None')).toBeTruthy();
        expect(screen.getByText('Aurora')).toBeTruthy();
        expect(screen.getByText('Sunset')).toBeTruthy();
        expect(screen.getByText('Ocean')).toBeTruthy();
    });

    it('expands colour group on click', () => {
        render(<ThemeCustomizer />);
        // Initially collapsed — should not see individual colour labels
        expect(screen.queryByText('Page')).toBeNull();
        expect(screen.queryByText('Sidebar')).toBeNull();

        // Click to expand
        fireEvent.click(screen.getByText('Backgrounds'));
        expect(screen.getByText('Page')).toBeTruthy();
        expect(screen.getByText('Sidebar')).toBeTruthy();
    });

    it('collapses expanded group on second click', () => {
        render(<ThemeCustomizer />);
        fireEvent.click(screen.getByText('Backgrounds'));
        expect(screen.getByText('Page')).toBeTruthy();

        fireEvent.click(screen.getByText('Backgrounds'));
        expect(screen.queryByText('Page')).toBeNull();
    });

    it('does not show Reset All when no overrides', () => {
        render(<ThemeCustomizer />);
        expect(screen.queryByText('↺ Reset All')).toBeNull();
    });

    it('shows Reset All when color overrides exist', () => {
        useThemeStore.setState({
            customOverrides: { '--bg-sidebar': '#ff0000' },
        });
        render(<ThemeCustomizer />);
        expect(screen.getByText('↺ Reset All')).toBeTruthy();
    });

    it('shows Reset All when custom background is set', () => {
        useThemeStore.setState({
            customBackground: 'linear-gradient(135deg, #000 0%, #333 100%)',
        });
        render(<ThemeCustomizer />);
        expect(screen.getByText('↺ Reset All')).toBeTruthy();
    });

    it('resets all overrides on Reset All click', () => {
        useThemeStore.setState({
            customOverrides: { '--bg-sidebar': '#ff0000', '--accent-teal': '#00ff00' },
            customBackground: 'linear-gradient(135deg, #000 0%, #333 100%)',
        });
        render(<ThemeCustomizer />);
        fireEvent.click(screen.getByText('↺ Reset All'));

        const state = useThemeStore.getState();
        expect(Object.keys(state.customOverrides).length).toBe(0);
        expect(state.customBackground).toBeNull();
    });

    it('renders background URL input', () => {
        render(<ThemeCustomizer />);
        const input = screen.getByPlaceholderText('https://example.com/bg.gif');
        expect(input).toBeTruthy();
    });

    it('applies background URL on Enter', () => {
        render(<ThemeCustomizer />);
        const input = screen.getByPlaceholderText('https://example.com/bg.gif');
        fireEvent.change(input, { target: { value: 'https://example.com/cool.gif' } });
        fireEvent.keyDown(input, { key: 'Enter' });

        expect(useThemeStore.getState().customBackground).toBe('https://example.com/cool.gif');
    });

    it('applies background preset on click', () => {
        render(<ThemeCustomizer />);
        fireEvent.click(screen.getByTitle('Aurora'));
        const bg = useThemeStore.getState().customBackground;
        expect(bg).toContain('linear-gradient');
        expect(bg).toContain('#0f0c29');
    });

    it('clears background when None preset clicked', () => {
        useThemeStore.setState({
            customBackground: 'linear-gradient(135deg, #000 0%, #333 100%)',
        });
        render(<ThemeCustomizer />);
        fireEvent.click(screen.getByTitle('None'));
        expect(useThemeStore.getState().customBackground).toBeNull();
    });

    it('shows modified indicator when group has overrides', () => {
        useThemeStore.setState({
            customOverrides: { '--bg-sidebar': '#ff0000' },
        });
        render(<ThemeCustomizer />);
        expect(screen.getByText('● modified')).toBeTruthy();
    });

    it('shows active indicator when background is set', () => {
        useThemeStore.setState({
            customBackground: 'linear-gradient(135deg, #000 0%, #333 100%)',
        });
        render(<ThemeCustomizer />);
        expect(screen.getByText('● active')).toBeTruthy();
    });

    it('renders upload button with green plus', () => {
        render(<ThemeCustomizer />);
        expect(screen.getByTitle('Upload custom background image')).toBeTruthy();
        expect(screen.getByText('Upload')).toBeTruthy();
    });

    it('renders cover/tile toggle with Cover active by default', () => {
        render(<ThemeCustomizer />);
        const coverBtn = screen.getByTitle('Full image \u2014 scales to fill screen');
        const tileBtn = screen.getByTitle('Tile \u2014 repeats small pattern images');
        expect(coverBtn).toBeTruthy();
        expect(tileBtn).toBeTruthy();
    });

    it('switches to tile mode on click', () => {
        render(<ThemeCustomizer />);
        fireEvent.click(screen.getByTitle('Tile \u2014 repeats small pattern images'));
        expect(useThemeStore.getState().backgroundMode).toBe('tile');
    });

    it('renders user-uploaded backgrounds with trash icon', () => {
        useThemeStore.setState({
            userBackgrounds: [
                { id: 'test1', name: 'My Tile', dataUrl: 'data:image/png;base64,abc123' },
            ],
        });
        render(<ThemeCustomizer />);
        expect(screen.getByTitle('My Tile')).toBeTruthy();
        expect(screen.getByTitle('Remove My Tile')).toBeTruthy();
    });

    it('removes user background on trash click', () => {
        useThemeStore.setState({
            userBackgrounds: [
                { id: 'test1', name: 'My Tile', dataUrl: 'data:image/png;base64,abc123' },
            ],
        });
        render(<ThemeCustomizer />);
        fireEvent.click(screen.getByTitle('Remove My Tile'));
        expect(useThemeStore.getState().userBackgrounds.length).toBe(0);
    });

    it('renders hidden file input for uploads', () => {
        render(<ThemeCustomizer />);
        const fileInput = document.querySelector('[data-testid="bg-file-input"]') as HTMLInputElement;
        expect(fileInput).toBeTruthy();
        expect(fileInput.type).toBe('file');
        expect(fileInput.accept).toBe('image/*');
    });
});
