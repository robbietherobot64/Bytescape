import { useEffect, useRef, useState } from "react";
import { eventsApi } from "./api";
import type { EventItem, LinkPreviewData, ListParams } from "./api";
import { eventStatusLabel, formatEventDate } from "./format";

/** Linkify URLs in the event description; links must not trigger the card. */
function LinkifiedText({ text }: { text: string }) {
  const parts = text.split(/(https?:\/\/[^\s]+)/g);
  return (
    <>
      {parts.map((part, i) =>
        /^https?:\/\//.test(part) ? (
          <a
            key={i}
            href={part}
            target="_blank"
            rel="noreferrer"
            className="ev-desc-link"
            onClick={(e) => e.stopPropagation()}
          >
            {part}
          </a>
        ) : (
          part
        ),
      )}
    </>
  );
}

/** The event's external link: structured `uris` first (the image entry is
 * named "image"), then the first URL in the description. */
function eventLink(event: EventItem): string | null {
  const uri = event.uris?.find(
    (u) => /^https?:\/\//.test(u.uri) && u.name !== "image",
  );
  if (uri) return uri.uri;
  const match = event.description?.match(/https?:\/\/[^\s)]+/);
  return match ? match[0] : null;
}

/** Minimal HTML-entity decode for OG tags (source pages emit &amp;/&mdash;). */
function decodeEntities(value?: string | null): string | undefined {
  if (!value) return undefined;
  return value
    .replace(/&#(\d+);/g, (_, n) => String.fromCharCode(Number(n)))
    .replace(/&#x([0-9a-f]+);/gi, (_, n) => String.fromCharCode(parseInt(n, 16)))
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;|&apos;/g, "'")
    .replace(/&mdash;/g, "\u2014")
    .replace(/&ndash;/g, "\u2013")
    .replace(/&hellip;/g, "\u2026")
    .replace(/&nbsp;/g, " ");
}

/** Link preview in the media slot — only for events without an image. */
function EventLinkPreview({ url }: { url: string }) {
  const [data, setData] = useState<LinkPreviewData | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    setData(null);
    setFailed(false);
    eventsApi
      .og(url)
      .then((d) => {
        if (alive) setData(d);
      })
      .catch(() => {
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [url]);

  const domain = (() => {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  })();
  const title = decodeEntities(data?.title) || decodeEntities(data?.site_name) || domain;
  const description = decodeEntities(data?.description);
  const image = data?.image;

  return (
    <a
      className="ev-link-card"
      href={url}
      target="_blank"
      rel="noreferrer"
      onClick={(e) => e.stopPropagation()}
    >
      {image && (
        <img
          className="ev-link-thumb"
          src={image}
          alt=""
          loading="lazy"
          onError={(e) => {
            (e.target as HTMLImageElement).style.display = "none";
          }}
        />
      )}
      <div className="ev-link-body">
        {failed ? (
          <>
            <strong className="ev-link-title">{domain}</strong>
            <span className="ev-link-domain">{url}</span>
          </>
        ) : (
          <>
            <strong className="ev-link-title">{title}</strong>
            {description ? (
              <p className="ev-link-desc">{description}</p>
            ) : !data ? (
              <p className="ev-link-desc ev-muted">Loading preview…</p>
            ) : null}
            <span className="ev-link-domain">{domain}</span>
          </>
        )}
      </div>
    </a>
  );
}

function EventCard({
  event,
  selected,
  onSelect,
}: {
  event: EventItem;
  selected: boolean;
  onSelect: (uri: string | null) => void;
}) {
  const status = eventStatusLabel(event.status);
  const reactions = Object.values(event.reaction_counts ?? {}).reduce((a, b) => a + b, 0);
  const description = event.description?.trim() ?? "";
  const link = eventLink(event);
  const provenance = event.provenance ?? [];
  const [expanded, setExpanded] = useState(false);
  const descRef = useRef<HTMLParagraphElement>(null);
  const [needsExpand, setNeedsExpand] = useState(false);

  useEffect(() => {
    const el = descRef.current;
    setNeedsExpand(!!el && el.scrollHeight > el.clientHeight + 1);
  }, [description, expanded]);

  return (
    <article
      className={selected ? "ev-card selected" : "ev-card"}
      onClick={() => onSelect(event.id)}
    >
      <div className="ev-card-body">
        <div className="ev-card-provenance">
          {provenance.map((p, i) => (
            <span
              key={`${p.kind}-${p.label}-${i}`}
              className={`ev-prov ev-prov-${p.kind}`}
              title={`Shown because of ${p.label}`}
            >
              {p.kind === "following"
                ? "👥"
                : p.kind === "feeds"
                  ? "📡"
                  : p.kind === "hashtags"
                    ? "#️⃣"
                    : ""}{" "}
              {p.label}
            </span>
          ))}
        </div>
        <div className="ev-card-head">
          <h3 className="ev-card-title">{event.name}</h3>
          {status.tone !== "normal" && (
            <span className={`ev-badge ev-badge-${status.tone}`}>{status.text}</span>
          )}
        </div>
        <p className="ev-card-date">
          {event.starts_at
            ? formatEventDate(event.starts_at, event.ends_at, event.is_upcoming)
            : "Date TBD"}
        </p>
        {event.location && (
          <p className="ev-card-location">
            📍 {event.location.name ?? event.location.address}
          </p>
        )}
        <p className="ev-card-author ev-muted">
          by {event.author.display_name || event.author.handle}
        </p>
        {description && (
          <div className="ev-card-text">
            <p
              ref={descRef}
              className={expanded ? "ev-card-desc expanded" : "ev-card-desc"}
            >
              <LinkifiedText text={description} />
            </p>
            {(needsExpand || expanded) && (
              <button
                type="button"
                className="ev-readmore"
                onClick={(e) => {
                  e.stopPropagation();
                  setExpanded((v) => !v);
                }}
              >
                {expanded ? "Show less" : "Read more"}
              </button>
            )}
          </div>
        )}
        {event.image ? (
          <img className="ev-cover" src={event.image} alt="" loading="lazy" />
        ) : link ? (
          <EventLinkPreview url={link} />
        ) : null}
        <div className="ev-card-meta">
          <span className="ev-chip">👍 {event.rsvp_counts.going} going</span>
          {event.rsvp_counts.interested > 0 && (
            <span className="ev-chip">🤔 {event.rsvp_counts.interested} maybe</span>
          )}
          {reactions > 0 && <span className="ev-chip">❤️ {reactions}</span>}
          {event.replies_count > 0 && <span className="ev-chip">💬 {event.replies_count}</span>}
        </div>
      </div>
    </article>
  );
}

interface Props {
  status: "all" | "upcoming" | "past";
  filters: ListParams;
  selectedUri: string | null;
  onSelect: (uri: string | null) => void;
  refreshKey: number;
  /** Anonymous visitors get a sign-in prompt instead of the empty state. */
  signedIn: boolean;
  onRequireSignIn: () => void;
}

/** The reading column: cursor-paginated event cards, load-more, states. */
export default function EventsColumn({
  status,
  filters,
  selectedUri,
  onSelect,
  refreshKey,
  signedIn,
  onRequireSignIn,
}: Props) {
  const [items, setItems] = useState<EventItem[]>([]);
  const [cursor, setCursor] = useState<{ starts_at?: string; uri: string } | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchPage = async (append: boolean, after?: { starts_at?: string; uri: string }) => {
    if (!append) setLoading(true);
    try {
      const page = await eventsApi.list({
        status,
        limit: 20,
        q: filters.q || undefined,
        date_from: filters.date_from,
        date_to: filters.date_to,
        location: filters.location || undefined,
        source: filters.source,
        followed_did: filters.followed_did,
        feed_id: filters.feed_id,
        hashtag_id: filters.hashtag_id,
        cursor_starts_at: after?.starts_at,
        cursor_uri: after?.uri,
      });
      setItems((prev) => (append ? [...prev, ...page.items] : page.items));
      setHasMore(page.has_more);
      const last = page.items[page.items.length - 1];
      setCursor(last ? { starts_at: last.starts_at ?? undefined, uri: last.id } : null);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "failed to load events");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchPage(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status, filters, refreshKey]);

  return (
    <section className="ev-column">
      {error && <p className="ev-error">{error}</p>}
      {loading && <p className="ev-muted">&gt; loading events…</p>}
      {!loading && !error && items.length === 0 && (signedIn ? (
        <p className="ev-muted">
          No events from your sources yet — follow people, feeds, or hashtags to see their events here.
        </p>
      ) : (
        <p className="ev-muted">
          Events come from the people, feeds, and hashtags you follow.{" "}
          <button type="button" className="btn" onClick={onRequireSignIn}>
            Sign in
          </button>
        </p>
      ))}
      <div className="ev-cards">
        {items.map((e) => (
          <EventCard
            key={e.id}
            event={e}
            selected={e.id === selectedUri}
            onSelect={onSelect}
          />
        ))}
      </div>
      {hasMore && (
        <button
          type="button"
          className="ev-btn ev-load-more"
          disabled={loading}
          onClick={() => fetchPage(true, cursor ?? undefined)}
        >
          &gt; LOAD MORE EVENTS
        </button>
      )}
    </section>
  );
}
