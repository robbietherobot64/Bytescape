import React from 'react';

/**
 * Supported visual treatments for the shared Win98 container primitive.
 */
export type Win98Variant = 'raised' | 'sunken' | 'pressed' | 'window' | 'flat';

/**
 * Props for `Win98Box`.
 *
 * Cross-system note:
 * - This component is a pure presentational primitive and is reused by many
 *   feature components across screens to keep retro theming consistent.
 * - Drag/drop handlers are intentionally passthrough to support higher-level
 *   features (e.g., vault/import surfaces) without coupling logic here.
 */
interface Win98BoxProps {
    variant?: Win98Variant;
    className?: string;
    style?: React.CSSProperties;
    children: React.ReactNode;
    title?: string;
    onClose?: () => void;
    onClick?: (e: React.MouseEvent) => void;
    onDragOver?: (e: React.DragEvent) => void;
    onDrop?: (e: React.DragEvent) => void;
    onDragLeave?: (e: React.DragEvent) => void;
}

/** Mapping from variant enum to CSS class contract in App.css/theme files. */
const VARIANT_CLASSES: Record<Win98Variant, string> = {
    raised: 'win98-raised',
    sunken: 'win98-sunken',
    pressed: 'win98-pressed',
    window: 'win98-window',
    flat: 'win98-flat',
};

/**
 * Win98Box — the universal retro container primitive.
 *
 * Wraps content in a Win98-style beveled box. Use `variant` to select
 * the bevel direction:
 * - `raised`: outward bevel (cards, buttons)
 * - `sunken`: inward bevel (input fields, content wells)
 * - `pressed`: active/clicked state
 * - `window`: full window chrome with optional title bar + close button
 * - `flat`: no bevel, just the background
 */
export function Win98Box({
    variant = 'raised',
    className = '',
    style,
    children,
    title,
    onClose,
    onClick,
    onDragOver,
    onDrop,
    onDragLeave,
}: Win98BoxProps) {
    // ===== Section: derived styling =====
    const variantClass = VARIANT_CLASSES[variant];

    // ===== Section: window-chrome composition =====
    if (variant === 'window') {
        return (
            <div
                className={`win98-box ${variantClass} ${className}`}
                style={style}
                onClick={onClick}
                onDragOver={onDragOver}
                onDrop={onDrop}
                onDragLeave={onDragLeave}
            >
                {title && (
                    <div className="win98-title-bar">
                        <span className="win98-title-text">{title}</span>
                        <div className="win98-title-buttons">
                            {onClose && (
                                <button
                                    className="win98-title-btn win98-close-btn"
                                    onClick={(e) => { e.stopPropagation(); onClose(); }}
                                    title="Close"
                                >
                                    ✕
                                </button>
                            )}
                        </div>
                    </div>
                )}
                <div className="win98-window-body">
                    {children}
                </div>
            </div>
        );
    }

    // ===== Section: generic container composition =====
    return (
        <div
            className={`win98-box ${variantClass} ${className}`}
            style={style}
            onClick={onClick}
            onDragOver={onDragOver}
            onDrop={onDrop}
            onDragLeave={onDragLeave}
        >
            {children}
        </div>
    );
}
