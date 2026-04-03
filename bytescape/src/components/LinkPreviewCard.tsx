/**
 * ════════════════════════════════════════════════════════════════════════════
 * PURPOSE
 * ════════════════════════════════════════════════════════════════════════════
 * Render a compact, dismissible link-preview card for Compose flows.
 *
 * HOW IT WORKS
 * - Receives normalized card metadata (`LinkCard`) from parent orchestration.
 * - Computes a readable domain from `card.url` with a safe URL-parse fallback.
 * - Renders optional media (`image_url`, `favicon_url`) with defensive `onError`
 *   handlers that hide failed assets.
 * - Delegates lifecycle actions (dismiss/removal) to the parent via `onDismiss`.
 *
 * RELATED FILES
 * - `src/components/ComposeBox.tsx` (creates/owns preview state and dismissal)
 * - `src/types/index.ts` (`LinkCard` shape)
 * - Styling currently uses inline styles (no dedicated stylesheet module here)
 *
 * CROSS-SYSTEM NOTES
 * - This component is intentionally presentational and stateless.
 * - No direct store/provider/invoke usage exists here; upstream components are
 *   responsible for backend fetch/invoke and state persistence.
 */
import type { LinkCard } from '../types';

interface LinkPreviewCardProps {
    card: LinkCard;
    onDismiss: () => void;
}

/**
 * ════════════════════════════════════════════════════════════════════════════
 * LinkPreviewCard
 * ════════════════════════════════════════════════════════════════════════════
 * Renders link card metadata (thumbnail/title/description/domain) for Compose.
 *
 * @param card Normalized link-card payload to display.
 * @param onDismiss Callback invoked when the user removes this preview.
 */
export function LinkPreviewCard({ card, onDismiss }: LinkPreviewCardProps) {
    // ═══ Derived presentation metadata ═══
    const domain = (() => {
        try {
            return new URL(card.url).hostname.replace(/^www\./, '');
        } catch {
            return card.url;
        }
    })();

    return (
        <div
            className="link-preview-card"
            style={{
                border: '1px solid rgba(255,255,255,0.1)',
                borderRadius: 8,
                overflow: 'hidden',
                marginBottom: 10,
                background: 'rgba(255,255,255,0.03)',
                position: 'relative',
            }}
        >
            {/* ═══ Dismiss action ═══ */}
            <button
                onClick={onDismiss}
                type="button"
                aria-label="Remove link preview"
                style={{
                    position: 'absolute',
                    top: 6,
                    right: 6,
                    background: 'rgba(0,0,0,0.7)',
                    color: 'white',
                    border: 'none',
                    borderRadius: '50%',
                    width: 22,
                    height: 22,
                    cursor: 'pointer',
                    fontSize: 12,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    zIndex: 2,
                }}
            >
                ✕
            </button>

            <div style={{ display: 'flex', gap: 0 }}>
                {/* ═══ Media column (optional thumbnail) ═══ */}
                {card.image_url && (
                    <div
                        style={{
                            width: 120,
                            minHeight: 80,
                            flexShrink: 0,
                            background: 'rgba(255,255,255,0.05)',
                            overflow: 'hidden',
                        }}
                    >
                        <img
                            src={card.image_url}
                            alt=""
                            style={{
                                width: '100%',
                                height: '100%',
                                objectFit: 'cover',
                                display: 'block',
                            }}
                            onError={(e) => {
                                (e.target as HTMLImageElement).style.display = 'none';
                            }}
                        />
                    </div>
                )}

                {/* ═══ Text column ═══ */}
                <div style={{ flex: 1, padding: '8px 12px', minWidth: 0 }}>
                    {/* Domain + favicon */}
                    <div style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 4,
                        marginBottom: 4,
                        fontSize: '0.7rem',
                        color: 'var(--text-muted, #888)',
                    }}>
                        {card.favicon_url && (
                            <img
                                src={card.favicon_url}
                                alt=""
                                style={{ width: 12, height: 12, borderRadius: 2 }}
                                onError={(e) => {
                                    (e.target as HTMLImageElement).style.display = 'none';
                                }}
                            />
                        )}
                        <span>{card.site_name || domain}</span>
                    </div>

                    {/* Title */}
                    {card.title && (
                        <div style={{
                            fontSize: '0.82rem',
                            fontWeight: 600,
                            color: 'var(--text-primary, #fff)',
                            marginBottom: 2,
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            whiteSpace: 'nowrap',
                        }}>
                            {card.title}
                        </div>
                    )}

                    {/* Description */}
                    {card.description && (
                        <div style={{
                            fontSize: '0.72rem',
                            color: 'var(--text-secondary, #aaa)',
                            overflow: 'hidden',
                            display: '-webkit-box',
                            WebkitLineClamp: 2,
                            WebkitBoxOrient: 'vertical' as const,
                            lineHeight: '1.3',
                        } as React.CSSProperties}>
                            {card.description}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
