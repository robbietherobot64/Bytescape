/**
 * Omnibar (B10.3)
 *
 * Browser-style address bar + search bar hybrid.
 * Shows the current byte:// URL, allows direct URL entry, and doubles
 * as a search bar for vault/feed content.
 *
 * Features:
 * - Displays current page URL from useLocationStore
 * - Back / Forward / Home buttons with keyboard shortcuts
 * - Content origin icon (🏠 local / 🔗 peer)
 * - Editable — type a byte:// URL to navigate directly
 * - Plain text input triggers search
 * - Route autocomplete for known paths
 * - GeoCities beveled-inset aesthetic
 */

import { useState, useRef, useEffect, useCallback } from 'react';
import {
    BsArrowLeft, BsArrowRight, BsHouseFill,
    BsSearch, BsGlobe, BsPeopleFill
} from 'react-icons/bs';
import { useLocationStore, resolveUrl } from '../hooks/useLocation';

interface OmnibarProps {
    /** Called when a plain text search is submitted (not a URL) */
    onSearchSubmit?: (query: string) => void;
}

/** Known routes for autocomplete */
const KNOWN_ROUTES = [
    { url: 'byte://feed', label: 'Feed — All', icon: '🏠' },
    { url: 'byte://feed/following', label: 'Feed — Following', icon: '🏠' },
    { url: 'byte://feed/subscribed', label: 'Feed — Subscribed', icon: '🏠' },
    { url: 'byte://feed/you', label: 'Feed — Your Posts', icon: '🏠' },
    { url: 'byte://chat', label: 'Chat', icon: '🏠' },
    { url: 'byte://vault', label: 'Vault', icon: '🏠' },
    { url: 'byte://profile', label: 'Profile', icon: '🏠' },
    { url: 'byte://profile-builder', label: 'Profile Builder', icon: '🏠' },
    { url: 'byte://profile-browser', label: 'Profile Browser', icon: '🌐' },
    { url: 'byte://import', label: 'Import Archives', icon: '🏠' },
    { url: 'byte://settings', label: 'Settings', icon: '🏠' },
    { url: 'byte://friends', label: 'Social / Friends', icon: '🏠' },
];

export function Omnibar({ onSearchSubmit }: OmnibarProps) {
    const currentUrl = useLocationStore(s => s.currentUrl);
    const canGoBack = useLocationStore(s => s.canGoBack);
    const canGoForward = useLocationStore(s => s.canGoForward);
    const navigate = useLocationStore(s => s.navigate);
    const goBack = useLocationStore(s => s.goBack);
    const goForward = useLocationStore(s => s.goForward);

    const [inputValue, setInputValue] = useState('');
    const [isFocused, setIsFocused] = useState(false);
    const [showSuggestions, setShowSuggestions] = useState(false);
    const inputRef = useRef<HTMLInputElement>(null);
    const suggestionsRef = useRef<HTMLDivElement>(null);

    // Sync display value with current URL when not focused
    useEffect(() => {
        if (!isFocused) {
            setInputValue(currentUrl);
        }
    }, [currentUrl, isFocused]);

    // Close suggestions on outside click
    useEffect(() => {
        if (!showSuggestions) return;
        const handler = (e: MouseEvent) => {
            if (
                suggestionsRef.current &&
                !suggestionsRef.current.contains(e.target as Node) &&
                inputRef.current &&
                !inputRef.current.contains(e.target as Node)
            ) {
                setShowSuggestions(false);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, [showSuggestions]);

    // Determine content origin icon
    const getOriginIcon = () => {
        if (currentUrl.startsWith('byte://peer/')) {
            return <BsPeopleFill style={{ fontSize: 12, color: '#50fa7b' }} title="Peer Content (P2P)" />;
        }
        if (currentUrl === 'byte://profile-browser') {
            return <BsGlobe style={{ fontSize: 12, color: 'var(--accent-blue)' }} title="Network" />;
        }
        return <BsHouseFill style={{ fontSize: 12, color: 'var(--text-muted)' }} title="Local Content" />;
    };

    // Filter suggestions based on input
    const getSuggestions = useCallback(() => {
        if (!inputValue || inputValue === currentUrl) return [];
        const lower = inputValue.toLowerCase();

        // Filter known routes
        const matches = KNOWN_ROUTES.filter(r =>
            r.url.toLowerCase().includes(lower) ||
            r.label.toLowerCase().includes(lower)
        );

        // Also suggest from history (recent unique URLs)
        const history = useLocationStore.getState().history;
        const recentUrls = [...new Set([...history].reverse())].slice(0, 5);
        const historyMatches = recentUrls
            .filter(url => url.toLowerCase().includes(lower) && !matches.some(m => m.url === url))
            .map(url => ({ url, label: url.replace('byte://', ''), icon: '📋' }));

        return [...matches, ...historyMatches].slice(0, 8);
    }, [inputValue, currentUrl]);

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        const value = inputValue.trim();
        if (!value) return;

        // Check if it's a valid byte:// URL or path
        const isUrl = value.startsWith('byte://') ||
            value.startsWith('/') ||
            KNOWN_ROUTES.some(r => r.url === value || r.url === `byte://${value}`);

        if (isUrl) {
            const url = value.startsWith('byte://') ? value : `byte://${value}`;
            const resolved = resolveUrl(url);
            if (resolved) {
                navigate(url);
                inputRef.current?.blur();
                setShowSuggestions(false);
                return;
            }
        }

        // Check if it looks like a peer node ID (long hex string)
        if (/^[a-f0-9]{32,}$/i.test(value)) {
            navigate(`byte://peer/${value}`);
            inputRef.current?.blur();
            setShowSuggestions(false);
            return;
        }

        // Otherwise treat as search
        onSearchSubmit?.(value);
        inputRef.current?.blur();
        setShowSuggestions(false);
    };

    const handleSuggestionClick = (url: string) => {
        navigate(url);
        setShowSuggestions(false);
        inputRef.current?.blur();
    };

    const handleFocus = () => {
        setIsFocused(true);
        // Select all text on focus for easy replacement
        inputRef.current?.select();
        if (inputValue) {
            setShowSuggestions(true);
        }
    };

    const handleBlur = () => {
        // Delay to allow suggestion clicks
        setTimeout(() => {
            setIsFocused(false);
            setShowSuggestions(false);
        }, 200);
    };

    const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        setInputValue(e.target.value);
        setShowSuggestions(e.target.value.length > 0);
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Escape') {
            setInputValue(currentUrl);
            setShowSuggestions(false);
            inputRef.current?.blur();
        }
    };

    const suggestions = showSuggestions ? getSuggestions() : [];

    return (
        <div className="omnibar" style={omnibarContainerStyle}>
            {/* Navigation buttons */}
            <button
                onClick={goBack}
                disabled={!canGoBack}
                title="Back (Alt+←)"
                className="omnibar-nav-btn"
                style={{
                    ...navBtnStyle,
                    opacity: canGoBack ? 1 : 0.3,
                } as React.CSSProperties}
            >
                <BsArrowLeft />
            </button>
            <button
                onClick={goForward}
                disabled={!canGoForward}
                title="Forward (Alt+→)"
                className="omnibar-nav-btn"
                style={{
                    ...navBtnStyle,
                    opacity: canGoForward ? 1 : 0.3,
                } as React.CSSProperties}
            >
                <BsArrowRight />
            </button>

            {/* Address bar */}
            <form
                onSubmit={handleSubmit}
                className="omnibar-form"
                style={formStyle}
            >
                <div
                    className="omnibar-input-wrapper"
                    style={{
                        ...inputWrapperStyle,
                        borderColor: isFocused ? 'var(--accent-purple)' : 'var(--border-color)',
                    } as React.CSSProperties}
                >
                    {/* Origin icon */}
                    <span style={originIconStyle}>{getOriginIcon()}</span>

                    <input
                        ref={inputRef}
                        type="text"
                        value={inputValue}
                        onChange={handleInputChange}
                        onFocus={handleFocus}
                        onBlur={handleBlur}
                        onKeyDown={handleKeyDown}
                        placeholder="Enter address or search…"
                        className="omnibar-input"
                        style={inputStyle}
                        spellCheck={false}
                        autoComplete="off"
                    />

                    <button
                        type="submit"
                        className="omnibar-go-btn"
                        style={goBtnStyle}
                        title="Go"
                    >
                        <BsSearch style={{ fontSize: 11 }} />
                    </button>
                </div>

                {/* Autocomplete suggestions */}
                {showSuggestions && suggestions.length > 0 && (
                    <div
                        ref={suggestionsRef}
                        className="omnibar-suggestions"
                        style={suggestionsStyle}
                    >
                        {suggestions.map((s, i) => (
                            <button
                                key={`${s.url}-${i}`}
                                onMouseDown={() => handleSuggestionClick(s.url)}
                                className="omnibar-suggestion"
                                style={suggestionItemStyle}
                            >
                                <span style={{ marginRight: 8, fontSize: 12, flexShrink: 0 } as React.CSSProperties}>
                                    {s.icon}
                                </span>
                                <span style={{
                                    flex: 1,
                                    overflow: 'hidden',
                                    textOverflow: 'ellipsis',
                                    whiteSpace: 'nowrap',
                                } as React.CSSProperties}>
                                    {s.label}
                                </span>
                                <span style={{
                                    fontSize: 10,
                                    color: 'var(--text-muted)',
                                    marginLeft: 8,
                                    fontFamily: 'monospace',
                                    flexShrink: 0,
                                } as React.CSSProperties}>
                                    {s.url.replace('byte://', '')}
                                </span>
                            </button>
                        ))}
                    </div>
                )}
            </form>

            {/* Spacer to keep profile button aligned */}
            <div style={{ width: 4 } as React.CSSProperties} />
        </div>
    );
}

// ─── Styles ───
// Using inline styles with React.CSSProperties typing to avoid csstype issues.
// These match the GeoCities beveled aesthetic.

const omnibarContainerStyle: React.CSSProperties = {
    display: 'flex',
    alignItems: 'center',
    gap: 4,
    flex: 1,
};

const navBtnStyle: React.CSSProperties = {
    background: 'var(--bg-tertiary)',
    border: '1px outset var(--border-color)',
    borderRadius: 2,
    padding: '4px 6px',
    color: 'var(--text-primary)',
    cursor: 'pointer',
    fontSize: 14,
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    transition: 'opacity 0.15s',
};

const formStyle: React.CSSProperties = {
    flex: 1,
    position: 'relative',
};

const inputWrapperStyle: React.CSSProperties = {
    display: 'flex',
    alignItems: 'center',
    background: '#0f0f23',
    border: '2px inset var(--border-color)',
    borderRadius: 2,
    padding: '2px 4px',
    transition: 'border-color 0.15s',
};

const originIconStyle: React.CSSProperties = {
    display: 'flex',
    alignItems: 'center',
    padding: '0 4px',
    flexShrink: 0,
};

const inputStyle: React.CSSProperties = {
    flex: 1,
    background: 'transparent',
    border: 'none',
    color: 'var(--text-primary)',
    fontSize: 12,
    fontFamily: 'monospace',
    outline: 'none',
    padding: '3px 4px',
    minWidth: 0,
};

const goBtnStyle: React.CSSProperties = {
    background: 'var(--bg-tertiary)',
    border: '1px outset var(--border-color)',
    borderRadius: 2,
    padding: '3px 6px',
    color: 'var(--text-muted)',
    cursor: 'pointer',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
};

const suggestionsStyle: React.CSSProperties = {
    position: 'absolute',
    top: '100%',
    left: 0,
    right: 0,
    background: 'var(--bg-secondary)',
    border: '1px solid var(--border-color)',
    borderTop: 'none',
    borderRadius: '0 0 4px 4px',
    zIndex: 100,
    maxHeight: 280,
    overflowY: 'auto',
    boxShadow: '0 4px 12px rgba(0,0,0,0.5)',
};

const suggestionItemStyle: React.CSSProperties = {
    display: 'flex',
    alignItems: 'center',
    width: '100%',
    padding: '6px 10px',
    background: 'transparent',
    border: 'none',
    borderBottom: '1px solid var(--border-color)',
    color: 'var(--text-primary)',
    cursor: 'pointer',
    fontSize: 12,
    textAlign: 'left',
    transition: 'background 0.1s',
};
