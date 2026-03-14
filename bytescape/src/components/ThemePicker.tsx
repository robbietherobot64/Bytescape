import type { ThemeId } from '../types';
import { useTheme } from '../hooks/useTheme';
import { ThemeCustomizer } from './ThemeCustomizer';

interface ThemeOption {
    id: ThemeId;
    label: string;
    icon: string;
    description: string;
    /** Swatch colours shown as a preview strip */
    swatches: string[];
}

const THEME_OPTIONS: ThemeOption[] = [
    {
        id: 'geocities',
        label: 'Cozy Mode',
        icon: '🏠',
        description: 'Warm parchment, navy sidebar, tiled stars. The GeoCities memorial aesthetic.',
        swatches: ['#336', '#f5f0e6', '#ffffff', '#0000ee', '#008080'],
    },
    {
        id: 'win98',
        label: 'Desktop Mode',
        icon: '🖥️',
        description: 'Classic Win98 silver and teal. Sharper bevels, system font stack, no nonsense.',
        swatches: ['#c0c0c0', '#d4d0c8', '#ffffff', '#000080', '#008080'],
    },
    {
        id: 'midnight',
        label: 'Midnight Mode',
        icon: '🌙',
        description: 'Deep blues and soft purples for late-night browsing. Easy on the eyes.',
        swatches: ['#0f0f1a', '#1a1a2e', '#16213e', '#7c3aed', '#06b6d4'],
    },
];

/**
 * ThemePicker — renders theme cards with live swatch previews.
 * Selecting a theme applies it instantly via useTheme().
 */
export function ThemePicker() {
    const { activeTheme, setTheme } = useTheme();

    return (
        <div style={{ display: 'grid', gap: 12 }}>
            {THEME_OPTIONS.map((theme) => {
                const isActive = activeTheme === theme.id;
                return (
                    <button
                        key={theme.id}
                        onClick={() => setTheme(theme.id)}
                        style={{
                            display: 'flex',
                            alignItems: 'center',
                            gap: 14,
                            padding: '12px 16px',
                            background: isActive ? 'var(--bg-card-hover)' : 'var(--bg-panel)',
                            border: isActive ? '2px solid var(--accent-teal)' : 'var(--border-default)',
                            cursor: 'pointer',
                            textAlign: 'left',
                            fontFamily: 'inherit',
                            transition: 'border-color 0.15s',
                        } as React.CSSProperties}
                        aria-pressed={isActive}
                    >
                        {/* Icon */}
                        <span style={{ fontSize: '1.6rem', lineHeight: 1 }}>{theme.icon}</span>

                        {/* Label + description */}
                        <div style={{ flex: 1, minWidth: 0 }}>
                            <div style={{
                                fontWeight: 700,
                                fontSize: '0.92rem',
                                color: 'var(--text-primary)',
                                marginBottom: 2,
                            }}>
                                {theme.label}
                                {isActive && (
                                    <span style={{ marginLeft: 8, color: 'var(--accent-teal)', fontWeight: 400, fontSize: '0.78rem' }}>
                                        ✓ Active
                                    </span>
                                )}
                            </div>
                            <div style={{ fontSize: '0.78rem', color: 'var(--text-muted)', lineHeight: 1.35 }}>
                                {theme.description}
                            </div>
                        </div>

                        {/* Colour swatch strip */}
                        <div style={{ display: 'flex', gap: 3, flexShrink: 0 }}>
                            {theme.swatches.map((colour, i) => (
                                <div
                                    key={i}
                                    style={{
                                        width: 18,
                                        height: 18,
                                        background: colour,
                                        border: '1px solid rgba(0,0,0,0.2)',
                                    }}
                                />
                            ))}
                        </div>
                    </button>
                );
            })}

            {/* ─── Customizer: per-colour editing + backgrounds ─── */}
            <ThemeCustomizer />
        </div>
    );
}
