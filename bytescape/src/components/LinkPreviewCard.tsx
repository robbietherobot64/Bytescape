import type { LinkCard } from '../types';

interface LinkPreviewCardProps {
    card: LinkCard;
    onDismiss: () => void;
}

/**
 * Renders a link card preview in the ComposeBox.
 * Shows thumbnail, title, description, and domain — similar to Twitter/Bluesky link cards.
 */
export function LinkPreviewCard({ card, onDismiss }: LinkPreviewCardProps) {
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
            {/* Dismiss button */}
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
                {/* Thumbnail */}
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

                {/* Text content */}
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
