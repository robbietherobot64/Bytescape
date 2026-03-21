/**
 * Tests for RichText Component (C1.1b)
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { RichText } from './RichText';

describe('RichText', () => {
    it('renders plain text without any facets', () => {
        render(<RichText text="Hello world" />);
        expect(screen.getByText('Hello world')).toBeInTheDocument();
    });

    it('renders @mentions as styled elements', () => {
        const { container } = render(<RichText text="Hello @alice.bsky.social!" />);
        const mention = container.querySelector('.facet-mention');
        expect(mention).toBeInTheDocument();
        expect(mention?.textContent).toBe('@alice.bsky.social');
    });

    it('renders #hashtags as styled elements', () => {
        const { container } = render(<RichText text="Check #bytescape today" />);
        const hashtag = container.querySelector('.facet-hashtag');
        expect(hashtag).toBeInTheDocument();
        expect(hashtag?.textContent).toBe('#bytescape');
    });

    it('renders URLs as links', () => {
        const { container } = render(<RichText text="Visit https://example.com today" />);
        const link = container.querySelector('.facet-link') as HTMLAnchorElement;
        expect(link).toBeInTheDocument();
        expect(link?.href).toBe('https://example.com/');
        expect(link?.target).toBe('_blank');
        expect(link?.rel).toContain('noopener');
    });

    it('renders mixed facets correctly', () => {
        const { container } = render(
            <RichText text="Hey @alice.bsky.social check #art at https://gallery.com" />
        );
        expect(container.querySelector('.facet-mention')).toBeInTheDocument();
        expect(container.querySelector('.facet-hashtag')).toBeInTheDocument();
        expect(container.querySelector('.facet-link')).toBeInTheDocument();
    });

    it('truncates long URLs in display text', () => {
        const longUrl = 'https://example.com/very/long/path/that/definitely/exceeds/forty/characters/by/a/lot';
        const { container } = render(<RichText text={`Visit ${longUrl}`} />);
        const link = container.querySelector('.facet-link');
        expect(link).toBeInTheDocument();
        // Display text should be truncated but href should be full URL
        expect(link?.textContent?.length).toBeLessThan(longUrl.length);
        expect(link?.getAttribute('title')).toBe(longUrl);
    });

    it('applies custom className', () => {
        const { container } = render(<RichText text="test" className="custom-class" />);
        expect(container.querySelector('.custom-class')).toBeInTheDocument();
    });

    it('handles empty string', () => {
        const { container } = render(<RichText text="" />);
        expect(container.querySelector('span')).toBeInTheDocument();
        expect(container.textContent).toBe('');
    });

    it('renders mention with title attribute', () => {
        const { container } = render(<RichText text="Hello @alice.bsky.social" />);
        const mention = container.querySelector('.facet-mention');
        expect(mention?.getAttribute('title')).toBe('View @alice.bsky.social');
    });

    it('renders hashtag with title attribute', () => {
        const { container } = render(<RichText text="Hello #bytescape" />);
        const hashtag = container.querySelector('.facet-hashtag');
        expect(hashtag?.getAttribute('title')).toBe('View #bytescape in Subscribed');
    });
});
