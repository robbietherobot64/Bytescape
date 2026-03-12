import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ThemePicker } from './ThemePicker';
import { useThemeStore } from '../store';

describe('ThemePicker', () => {
    beforeEach(() => {
        // Reset the theme store and DOM
        useThemeStore.setState({ activeTheme: 'geocities' });
        document.documentElement.removeAttribute('data-theme');
        localStorage.clear();
    });

    it('renders all three theme options', () => {
        render(<ThemePicker />);
        expect(screen.getByText('Cozy Mode')).toBeTruthy();
        expect(screen.getByText('Desktop Mode')).toBeTruthy();
        expect(screen.getByText('Midnight Mode')).toBeTruthy();
    });

    it('shows Active indicator on the current theme', () => {
        render(<ThemePicker />);
        // geocities is the default
        const cozyButton = screen.getByText('Cozy Mode').closest('button')!;
        expect(cozyButton.getAttribute('aria-pressed')).toBe('true');

        const desktopButton = screen.getByText('Desktop Mode').closest('button')!;
        expect(desktopButton.getAttribute('aria-pressed')).toBe('false');
    });

    it('switches theme on click', () => {
        render(<ThemePicker />);
        const desktopButton = screen.getByText('Desktop Mode').closest('button')!;
        fireEvent.click(desktopButton);

        // Store should be updated
        expect(useThemeStore.getState().activeTheme).toBe('win98');
    });

    it('shows descriptions for each theme', () => {
        render(<ThemePicker />);
        expect(screen.getByText(/GeoCities memorial/)).toBeTruthy();
        expect(screen.getByText(/Classic Win98/)).toBeTruthy();
        expect(screen.getByText(/late-night browsing/)).toBeTruthy();
    });

    it('renders 3 theme buttons', () => {
        const { container } = render(<ThemePicker />);
        const buttons = container.querySelectorAll('button');
        expect(buttons.length).toBe(3);
    });
});
