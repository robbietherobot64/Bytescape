import { useState, useCallback, useRef, type CSSProperties } from 'react';
import { useTheme } from '../hooks/useTheme';

/**
 * PURPOSE
 * - Provide granular runtime theming controls for CSS variable overrides and
 *   background customization (preset, URL, uploaded image).
 *
 * HOW IT WORKS
 * - Reads current customization state from `useTheme()`.
 * - Applies overrides using provider-exposed mutators (`setColorOverride`,
 *   `setCustomBackground`, etc.).
 * - Uses a shared hidden native color input to edit many token swatches.
 *
 * RELATED FILES
 * - src/hooks/useTheme.ts (hook API consumed here)
 * - src/providers/ThemeProvider.tsx (state persistence + CSS variable application)
 * - src/themes/* (base theme token definitions)
 *
 * CROSS-SYSTEM NOTES
 * - This component does not call backend commands directly.
 * - It is tightly coupled to theme provider contract and CSS variable names;
 *   token drift between this file and theme definitions causes silent UX regressions.
 */

// ═══════════════════════════════════════════════════════════════════════════
// Editable CSS variables grouped by section
// ═══════════════════════════════════════════════════════════════════════════

interface ColorToken {
    variable: string;
    label: string;
    /** Default value from the current theme (resolved at runtime) */
    defaultValue?: string;
}

interface ColorGroup {
    label: string;
    icon: string;
    tokens: ColorToken[];
}

const COLOR_GROUPS: ColorGroup[] = [
    {
        label: 'Backgrounds',
        icon: '🎨',
        tokens: [
            { variable: '--bg-page', label: 'Page' },
            { variable: '--bg-content', label: 'Content' },
            { variable: '--bg-panel', label: 'Panel' },
            { variable: '--bg-card', label: 'Card' },
            { variable: '--bg-card-hover', label: 'Card Hover' },
            { variable: '--bg-toolbar', label: 'Toolbar' },
            { variable: '--bg-sidebar', label: 'Sidebar' },
            { variable: '--bg-sidebar-hover', label: 'Sidebar Hover' },
        ],
    },
    {
        label: 'Accents',
        icon: '💎',
        tokens: [
            { variable: '--accent-purple', label: 'Purple' },
            { variable: '--accent-teal', label: 'Teal' },
            { variable: '--accent-green', label: 'Green' },
            { variable: '--accent-red', label: 'Red' },
            { variable: '--accent-orange', label: 'Orange' },
            { variable: '--accent-yellow', label: 'Yellow' },
        ],
    },
    {
        label: 'Text',
        icon: '📝',
        tokens: [
            { variable: '--text-primary', label: 'Primary' },
            { variable: '--text-secondary', label: 'Secondary' },
            { variable: '--text-muted', label: 'Muted' },
            { variable: '--text-on-dark', label: 'On Dark' },
        ],
    },
    {
        label: 'Links',
        icon: '🔗',
        tokens: [
            { variable: '--link-blue', label: 'Link' },
            { variable: '--link-visited', label: 'Visited' },
            { variable: '--link-hover', label: 'Hover' },
        ],
    },
];

// ═══════════════════════════════════════════════════════════════════════════
// Default Background Presets
// ═══════════════════════════════════════════════════════════════════════════

interface BackgroundPreset {
    id: string;
    label: string;
    value: string; // CSS gradient or image URL
    thumbnail: string; // CSS for preview thumbnail
}

const BACKGROUND_PRESETS: BackgroundPreset[] = [
    {
        id: 'none',
        label: 'None',
        value: '',
        thumbnail: 'var(--bg-content)',
    },
    {
        id: 'aurora',
        label: 'Aurora',
        value: 'linear-gradient(135deg, #0f0c29 0%, #302b63 50%, #24243e 100%)',
        thumbnail: 'linear-gradient(135deg, #0f0c29 0%, #302b63 50%, #24243e 100%)',
    },
    {
        id: 'sunset',
        label: 'Sunset',
        value: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
        thumbnail: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
    },
    {
        id: 'ocean',
        label: 'Ocean',
        value: 'linear-gradient(135deg, #0c3547 0%, #1a6b7c 50%, #2d9ea3 100%)',
        thumbnail: 'linear-gradient(135deg, #0c3547 0%, #1a6b7c 50%, #2d9ea3 100%)',
    },
    {
        id: 'mesh-warm',
        label: 'Warm Mesh',
        value: 'radial-gradient(at 40% 20%, #f97316 0px, transparent 50%), radial-gradient(at 80% 40%, #ec4899 0px, transparent 50%), radial-gradient(at 20% 80%, #eab308 0px, transparent 50%), linear-gradient(135deg, #1a1a2e 0%, #16213e 100%)',
        thumbnail: 'radial-gradient(at 40% 20%, #f97316 0px, transparent 50%), radial-gradient(at 80% 40%, #ec4899 0px, transparent 50%), linear-gradient(135deg, #1a1a2e 0%, #16213e 100%)',
    },
    {
        id: 'matrix',
        label: 'Matrix',
        value: 'repeating-linear-gradient(0deg, rgba(0,255,0,0.03) 0px, rgba(0,255,0,0.03) 1px, transparent 1px, transparent 3px), linear-gradient(180deg, #000a00 0%, #001200 100%)',
        thumbnail: 'repeating-linear-gradient(0deg, rgba(0,255,0,0.15) 0px, rgba(0,255,0,0.15) 1px, transparent 1px, transparent 3px), linear-gradient(180deg, #000a00 0%, #001200 100%)',
    },
    {
        id: 'retro-grid',
        label: 'Retro Grid',
        value: 'repeating-linear-gradient(90deg, rgba(128,0,255,0.06) 0px, transparent 1px, transparent 40px), repeating-linear-gradient(0deg, rgba(128,0,255,0.06) 0px, transparent 1px, transparent 40px), linear-gradient(180deg, #0a0a1a 0%, #1a0a2e 100%)',
        thumbnail: 'repeating-linear-gradient(90deg, rgba(128,0,255,0.2) 0px, transparent 1px, transparent 20px), repeating-linear-gradient(0deg, rgba(128,0,255,0.2) 0px, transparent 1px, transparent 20px), linear-gradient(180deg, #0a0a1a 0%, #1a0a2e 100%)',
    },
];

// ═══════════════════════════════════════════════════════════════════════════
// Styles
// ═══════════════════════════════════════════════════════════════════════════

const sectionStyle: CSSProperties = {
    marginTop: 16,
    padding: '12px 14px',
    background: 'var(--bg-panel)',
    border: 'var(--border-default)',
    borderRadius: 'var(--border-radius-md)',
};

const sectionTitleStyle: CSSProperties = {
    fontSize: '0.82rem',
    fontWeight: 700,
    color: 'var(--text-secondary)',
    marginBottom: 10,
    display: 'flex',
    alignItems: 'center',
    gap: 6,
};

const swatchGridStyle: CSSProperties = {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(70px, 1fr))',
    gap: 8,
};

const swatchItemStyle: CSSProperties = {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    gap: 4,
};

const swatchButtonStyle: CSSProperties = {
    width: 32,
    height: 32,
    border: '2px solid rgba(0,0,0,0.2)',
    cursor: 'pointer',
    position: 'relative',
    padding: 0,
    borderRadius: 4,
    transition: 'transform 0.1s, box-shadow 0.15s',
};

const swatchLabelStyle: CSSProperties = {
    fontSize: '0.65rem',
    color: 'var(--text-muted)',
    textAlign: 'center',
    lineHeight: 1.2,
    maxWidth: 70,
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    whiteSpace: 'nowrap',
};

const bgPresetGrid: CSSProperties = {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(80px, 1fr))',
    gap: 8,
};

const bgPresetButtonStyle: CSSProperties = {
    width: '100%',
    aspectRatio: '16/10',
    border: '2px solid rgba(0,0,0,0.2)',
    cursor: 'pointer',
    padding: 0,
    borderRadius: 4,
    position: 'relative',
    overflow: 'hidden',
    transition: 'border-color 0.15s',
};

const resetButtonStyle: CSSProperties = {
    padding: '6px 14px',
    background: 'var(--bg-toolbar)',
    border: 'var(--border-default)',
    cursor: 'pointer',
    fontSize: '0.78rem',
    fontFamily: 'inherit',
    color: 'var(--text-secondary)',
    borderRadius: 'var(--border-radius-sm)',
};

// ═══════════════════════════════════════════════════════════════════════════
// Component
// ═══════════════════════════════════════════════════════════════════════════

/**
 * ThemeCustomizer — granular per-color customization panel.
 *
 * Renders below the ThemePicker cards. Each colour swatch opens an
 * HTML5 colour picker. Background section offers preset gradients
 * and a URL input for custom images/gifs.
 *
 * Integration notes:
 * - Uses `useTheme()` as the sole state boundary for theme writes.
 * - Assumes provider handles persistence and DOM variable propagation.
 */
export function ThemeCustomizer() {
    const {
        customOverrides,
        customBackground,
        backgroundMode,
        userBackgrounds,
        setColorOverride,
        removeColorOverride,
        resetAllOverrides,
        setCustomBackground,
        setBackgroundMode,
        addUserBackground,
        removeUserBackground,
    } = useTheme();

    const [bgUrlInput, setBgUrlInput] = useState('');
    const [expandedGroup, setExpandedGroup] = useState<string | null>(null);
    const colorInputRef = useRef<HTMLInputElement>(null);
    const fileInputRef = useRef<HTMLInputElement>(null);
    const [activePickerVar, setActivePickerVar] = useState<string | null>(null);

    /** Resolve the current computed value of a CSS variable */
    const getComputedColor = useCallback((variable: string): string => {
        // Check override first
        if (customOverrides[variable]) return customOverrides[variable];
        // Fall back to computed value from the DOM
        const computed = getComputedStyle(document.documentElement).getPropertyValue(variable).trim();
        return computed || '#888888';
    }, [customOverrides]);

    /** Handle clicking a swatch — opens the hidden color picker */
    const handleSwatchClick = useCallback((variable: string) => {
        setActivePickerVar(variable);
        // Small delay to let state settle, then trigger click on hidden input
        setTimeout(() => {
            if (colorInputRef.current) {
                colorInputRef.current.value = getComputedColor(variable);
                colorInputRef.current.click();
            }
        }, 50);
    }, [getComputedColor]);

    /** Handle colour change from the native picker */
    const handleColorChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
        if (activePickerVar) {
            setColorOverride(activePickerVar, e.target.value);
        }
    }, [activePickerVar, setColorOverride]);

    /** Reset a single colour back to theme default */
    const handleResetSingle = useCallback((variable: string, e: React.MouseEvent) => {
        e.stopPropagation();
        removeColorOverride(variable);
    }, [removeColorOverride]);

    /** Apply a background preset */
    const handlePresetClick = useCallback((preset: BackgroundPreset) => {
        if (preset.id === 'none') {
            setCustomBackground(null);
        } else {
            setCustomBackground(preset.value);
        }
    }, [setCustomBackground]);

    /** Apply a custom background URL */
    const handleBgUrlSubmit = useCallback(() => {
        const url = bgUrlInput.trim();
        if (url) {
            setCustomBackground(url);
            setBgUrlInput('');
        }
    }, [bgUrlInput, setCustomBackground]);

    /** Handle file upload for custom backgrounds */
    const handleFileUpload = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;
        // Limit to 4MB for localStorage
        if (file.size > 4 * 1024 * 1024) {
            alert('Image too large (max 4MB). For large backgrounds, use a URL instead.');
            return;
        }
        const reader = new FileReader();
        reader.onload = () => {
            const dataUrl = reader.result as string;
            const id = Date.now().toString(36);
            const name = file.name.replace(/\.[^.]+$/, '');
            addUserBackground({ id, name, dataUrl });
            setCustomBackground(dataUrl);
        };
        reader.readAsDataURL(file);
        // Reset input so the same file can be re-selected
        e.target.value = '';
    }, [addUserBackground, setCustomBackground]);

    /** Remove a user-uploaded background */
    const handleRemoveUserBg = useCallback((id: string, e: React.MouseEvent) => {
        e.stopPropagation();
        removeUserBackground(id);
    }, [removeUserBackground]);

    const hasAnyOverrides = Object.keys(customOverrides).length > 0 || customBackground !== null;

    return (
        <div style={{ marginTop: 14 }}>
            {/* ─── Reset All Button ─── */}
            {hasAnyOverrides && (
                <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 8 }}>
                    <button
                        onClick={resetAllOverrides}
                        style={resetButtonStyle}
                        title="Reset all customizations to theme defaults"
                    >
                        ↺ Reset All
                    </button>
                </div>
            )}

            {/* Hidden colour input — shared by all swatches */}
            <input
                ref={colorInputRef}
                type="color"
                onChange={handleColorChange}
                style={{
                    position: 'absolute',
                    opacity: 0,
                    width: 0,
                    height: 0,
                    pointerEvents: 'none',
                } as CSSProperties}
                tabIndex={-1}
            />

            {/* ─── Colour Groups ─── */}
            {COLOR_GROUPS.map((group) => {
                const isExpanded = expandedGroup === group.label;
                return (
                    <div key={group.label} style={sectionStyle}>
                        <button
                            onClick={() => setExpandedGroup(isExpanded ? null : group.label)}
                            style={{
                                ...sectionTitleStyle,
                                background: 'none',
                                border: 'none',
                                cursor: 'pointer',
                                padding: 0,
                                width: '100%',
                                justifyContent: 'space-between',
                                fontFamily: 'inherit',
                            } as CSSProperties}
                        >
                            <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                                <span>{group.icon}</span>
                                <span>{group.label}</span>
                                {/* Show indicator if any overrides in this group */}
                                {group.tokens.some((t) => customOverrides[t.variable]) && (
                                    <span style={{ fontSize: '0.65rem', color: 'var(--accent-teal)' }}>● modified</span>
                                )}
                            </span>
                            <span style={{ fontSize: '0.7rem', color: 'var(--text-light)' }}>
                                {isExpanded ? '▲' : '▼'}
                            </span>
                        </button>
                        {isExpanded && (
                            <div style={swatchGridStyle}>
                                {group.tokens.map((token) => {
                                    const currentColor = getComputedColor(token.variable);
                                    const isOverridden = !!customOverrides[token.variable];
                                    return (
                                        <div key={token.variable} style={swatchItemStyle}>
                                            <button
                                                onClick={() => handleSwatchClick(token.variable)}
                                                style={{
                                                    ...swatchButtonStyle,
                                                    background: currentColor,
                                                    borderColor: isOverridden ? 'var(--accent-teal)' : 'rgba(0,0,0,0.2)',
                                                    boxShadow: isOverridden ? '0 0 0 1px var(--accent-teal)' : 'none',
                                                } as CSSProperties}
                                                title={`${token.label}: ${currentColor}${isOverridden ? ' (custom)' : ''}\nClick to change`}
                                            >
                                                {isOverridden && (
                                                    <span
                                                        onClick={(e) => handleResetSingle(token.variable, e)}
                                                        style={{
                                                            position: 'absolute',
                                                            top: -6,
                                                            right: -6,
                                                            width: 14,
                                                            height: 14,
                                                            borderRadius: '50%',
                                                            background: 'var(--accent-red)',
                                                            color: '#fff',
                                                            fontSize: '0.55rem',
                                                            display: 'flex',
                                                            alignItems: 'center',
                                                            justifyContent: 'center',
                                                            lineHeight: 1,
                                                            cursor: 'pointer',
                                                        } as CSSProperties}
                                                        title="Reset to default"
                                                    >
                                                        ✕
                                                    </span>
                                                )}
                                            </button>
                                            <span style={swatchLabelStyle}>{token.label}</span>
                                        </div>
                                    );
                                })}
                            </div>
                        )}
                    </div>
                );
            })}

            {/* ─── Background Section ─── */}
            <div style={sectionStyle}>
                <div style={sectionTitleStyle}>
                    <span>🖼️</span>
                    <span>Background</span>
                    {customBackground && (
                        <span style={{ fontSize: '0.65rem', color: 'var(--accent-teal)' }}>● active</span>
                    )}
                </div>

                {/* Preset grid */}
                <div style={{ marginBottom: 10 }}>
                    <div style={{ fontSize: '0.72rem', color: 'var(--text-muted)', marginBottom: 6 }}>
                        Presets
                    </div>
                    <div style={bgPresetGrid}>
                        {BACKGROUND_PRESETS.map((preset) => {
                            const isActive = preset.id === 'none'
                                ? customBackground === null
                                : customBackground === preset.value;
                            return (
                                <div key={preset.id} style={{ textAlign: 'center' }}>
                                    <button
                                        onClick={() => handlePresetClick(preset)}
                                        style={{
                                            ...bgPresetButtonStyle,
                                            background: preset.thumbnail,
                                            borderColor: isActive ? 'var(--accent-teal)' : 'rgba(0,0,0,0.2)',
                                            boxShadow: isActive ? '0 0 0 1px var(--accent-teal)' : 'none',
                                        } as CSSProperties}
                                        title={preset.label}
                                    >
                                        {preset.id === 'none' && (
                                            <span style={{
                                                position: 'absolute',
                                                inset: 0,
                                                display: 'flex',
                                                alignItems: 'center',
                                                justifyContent: 'center',
                                                fontSize: '1.2rem',
                                                color: 'var(--text-muted)',
                                            } as CSSProperties}>
                                                ∅
                                            </span>
                                        )}
                                    </button>
                                    <div style={{ fontSize: '0.6rem', color: 'var(--text-muted)', marginTop: 3 }}>
                                        {preset.label}
                                    </div>
                                </div>
                            );
                        })}
                        {/* User-uploaded backgrounds */}
                        {userBackgrounds.map((ubg) => {
                            const isActive = customBackground === ubg.dataUrl;
                            return (
                                <div key={ubg.id} style={{ textAlign: 'center', position: 'relative' }}>
                                    <button
                                        onClick={() => setCustomBackground(ubg.dataUrl)}
                                        style={{
                                            ...bgPresetButtonStyle,
                                            backgroundImage: `url(${ubg.dataUrl})`,
                                            backgroundSize: backgroundMode === 'tile' ? 'auto' : 'cover',
                                            backgroundRepeat: backgroundMode === 'tile' ? 'repeat' : 'no-repeat',
                                            backgroundPosition: 'center',
                                            borderColor: isActive ? 'var(--accent-teal)' : 'rgba(0,0,0,0.2)',
                                            boxShadow: isActive ? '0 0 0 1px var(--accent-teal)' : 'none',
                                        } as CSSProperties}
                                        title={ubg.name}
                                    />
                                    <button
                                        onClick={(e) => handleRemoveUserBg(ubg.id, e)}
                                        style={{
                                            position: 'absolute',
                                            top: -4,
                                            right: -4,
                                            width: 16,
                                            height: 16,
                                            borderRadius: '50%',
                                            background: 'var(--accent-red)',
                                            color: '#fff',
                                            fontSize: '0.6rem',
                                            display: 'flex',
                                            alignItems: 'center',
                                            justifyContent: 'center',
                                            border: 'none',
                                            cursor: 'pointer',
                                            padding: 0,
                                            lineHeight: 1,
                                        } as CSSProperties}
                                        title={`Remove ${ubg.name}`}
                                    >
                                        ✕
                                    </button>
                                    <div style={{
                                        fontSize: '0.6rem',
                                        color: 'var(--text-muted)',
                                        marginTop: 3,
                                        maxWidth: 80,
                                        overflow: 'hidden',
                                        textOverflow: 'ellipsis',
                                        whiteSpace: 'nowrap',
                                    } as CSSProperties}>
                                        {ubg.name}
                                    </div>
                                </div>
                            );
                        })}
                        {/* Upload button */}
                        <div style={{ textAlign: 'center' }}>
                            <button
                                onClick={() => fileInputRef.current?.click()}
                                style={{
                                    ...bgPresetButtonStyle,
                                    background: 'transparent',
                                    borderColor: 'var(--accent-green)',
                                    borderStyle: 'dashed',
                                    display: 'flex',
                                    alignItems: 'center',
                                    justifyContent: 'center',
                                    flexDirection: 'column',
                                    gap: 2,
                                } as CSSProperties}
                                title="Upload custom background image"
                            >
                                <span style={{ fontSize: '1.4rem', color: 'var(--accent-green)', lineHeight: 1 }}>+</span>
                            </button>
                            <div style={{ fontSize: '0.6rem', color: 'var(--accent-green)', marginTop: 3 }}>
                                Upload
                            </div>
                        </div>
                    </div>
                </div>

                {/* Hidden file input for uploads */}
                <input
                    ref={fileInputRef}
                    type="file"
                    accept="image/*"
                    onChange={handleFileUpload}
                    style={{ display: 'none' } as CSSProperties}
                    data-testid="bg-file-input"
                />

                {/* Cover / Tile toggle */}
                <div style={{ marginTop: 10, display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span style={{ fontSize: '0.72rem', color: 'var(--text-muted)' }}>Mode:</span>
                    <div style={{
                        display: 'flex',
                        gap: 0,
                        borderRadius: 4,
                        overflow: 'hidden',
                        border: 'var(--border-default)',
                    } as CSSProperties}>
                        <button
                            onClick={() => setBackgroundMode('cover')}
                            style={{
                                padding: '4px 12px',
                                fontSize: '0.72rem',
                                fontFamily: 'inherit',
                                border: 'none',
                                cursor: 'pointer',
                                background: backgroundMode === 'cover' ? 'var(--accent-teal)' : 'var(--bg-toolbar)',
                                color: backgroundMode === 'cover' ? '#fff' : 'var(--text-secondary)',
                                transition: 'background 0.15s, color 0.15s',
                            } as CSSProperties}
                            title="Full image — scales to fill screen"
                        >
                            Cover
                        </button>
                        <button
                            onClick={() => setBackgroundMode('tile')}
                            style={{
                                padding: '4px 12px',
                                fontSize: '0.72rem',
                                fontFamily: 'inherit',
                                border: 'none',
                                borderLeft: '1px solid rgba(0,0,0,0.2)',
                                cursor: 'pointer',
                                background: backgroundMode === 'tile' ? 'var(--accent-teal)' : 'var(--bg-toolbar)',
                                color: backgroundMode === 'tile' ? '#fff' : 'var(--text-secondary)',
                                transition: 'background 0.15s, color 0.15s',
                            } as CSSProperties}
                            title="Tile — repeats small pattern images"
                        >
                            Tile
                        </button>
                    </div>
                </div>

                {/* Custom URL input */}
                <div style={{ marginTop: 10 }}>
                    <div style={{ fontSize: '0.72rem', color: 'var(--text-muted)', marginBottom: 4 }}>
                        Custom Image / GIF URL
                    </div>
                    <div style={{ display: 'flex', gap: 6 }}>
                        <input
                            type="text"
                            value={bgUrlInput}
                            onChange={(e) => setBgUrlInput(e.target.value)}
                            onKeyDown={(e) => e.key === 'Enter' && handleBgUrlSubmit()}
                            placeholder="https://example.com/bg.gif"
                            style={{
                                flex: 1,
                                padding: '5px 8px',
                                fontSize: '0.78rem',
                                background: 'var(--bg-input)',
                                border: 'var(--border-default)',
                                color: 'var(--text-primary)',
                                fontFamily: 'var(--font-mono)',
                                borderRadius: 'var(--border-radius-sm)',
                            } as CSSProperties}
                        />
                        <button
                            onClick={handleBgUrlSubmit}
                            disabled={!bgUrlInput.trim()}
                            style={{
                                ...resetButtonStyle,
                                opacity: bgUrlInput.trim() ? 1 : 0.5,
                            } as CSSProperties}
                        >
                            Apply
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
