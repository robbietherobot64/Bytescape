
interface SourceBadgeProps {
    source: string;
}

export function SourceBadge({ source }: SourceBadgeProps) {
    const config: Record<string, { emoji: string; bg: string; color: string; border: string }> = {
        bluesky: { emoji: "🦋", bg: "rgba(59,130,246,0.15)", color: "#60a5fa", border: "rgba(59,130,246,0.3)" },
        facebook: { emoji: "📘", bg: "rgba(66,103,178,0.15)", color: "#8b9dc3", border: "rgba(66,103,178,0.3)" },
        instagram: { emoji: "📸", bg: "rgba(225,48,108,0.15)", color: "#f472b6", border: "rgba(225,48,108,0.3)" },
        mastodon: { emoji: "🐘", bg: "rgba(99,100,255,0.15)", color: "#858afa", border: "rgba(99,100,255,0.3)" },
        bytescape: { emoji: "✨", bg: "rgba(168,85,247,0.15)", color: "#a855f7", border: "rgba(168,85,247,0.3)" },
        dtp: { emoji: "🔄", bg: "rgba(16,185,129,0.15)", color: "#34d399", border: "rgba(16,185,129,0.3)" },
    };
    const c = config[source] || { emoji: "📝", bg: "rgba(136,136,168,0.15)", color: "var(--text-muted)", border: "var(--border)" };
    return (
        <span className="source-badge" style={{ background: c.bg, color: c.color, borderColor: c.border }}>
            {c.emoji} {source}
        </span>
    );
}
