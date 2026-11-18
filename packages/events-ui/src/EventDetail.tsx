import { useCallback, useEffect, useState } from "react";
import { eventsApi } from "./api";
import type { EventComment, EventItem } from "./api";
import { eventStatusLabel, formatEventDate } from "./format";

const REACTION_CHOICES = ["❤️", "🎉", "👍", "🔥"];

interface Props {
  uri: string;
  viewer: { username: string; dids: string[] } | null;
  onRequireSignIn: () => void;
  onBack: () => void;
  /** List refresh trigger after mutations. */
  onMutated: () => void;
}

/** Full event surface (spec: events/browse detail + events/participate):
 * all recorded fields, RSVP, reactions, comments, author-only delete. */
export default function EventDetail({ uri, viewer, onRequireSignIn, onBack, onMutated }: Props) {
  const [event, setEvent] = useState<EventItem | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [comments, setComments] = useState<EventComment[] | null>(null);
  const [commentText, setCommentText] = useState("");
  const [busy, setBusy] = useState(false);

  const loadDetail = useCallback(() => {
    eventsApi
      .detail(uri)
      .then((e) => {
        setEvent(e);
        setError(null);
      })
      .catch((e: Error) => setError(e.message));
  }, [uri]);

  const loadComments = useCallback(() => {
    eventsApi
      .comments(uri)
      .then(setComments)
      .catch(() => setComments(null));
  }, [uri]);

  useEffect(() => {
    setEvent(null);
    setError(null);
    loadDetail();
    loadComments();
  }, [loadDetail, loadComments]);

  const rsvp = async (status: "going" | "maybe" | "not_going") => {
    if (!viewer) {
      onRequireSignIn();
      return;
    }
    setBusy(true);
    try {
      const res = await eventsApi.rsvp(uri, status);
      setEvent((prev) =>
        prev ? { ...prev, rsvp_status: status, rsvp_counts: res.rsvp_counts } : prev,
      );
      onMutated();
    } catch (e) {
      setError(e instanceof Error ? e.message : "RSVP failed");
    } finally {
      setBusy(false);
    }
  };

  const react = async (emoji: string) => {
    if (!viewer) {
      onRequireSignIn();
      return;
    }
    const mine = event?.user_reaction;
    try {
      const res = mine === emoji
        ? await eventsApi.removeReaction(uri)
        : await eventsApi.addReaction(uri, emoji);
      setEvent((prev) =>
        prev
          ? { ...prev, reaction_counts: res.reaction_counts ?? null, user_reaction: res.user_reaction ?? null }
          : prev,
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "reaction failed");
    }
  };

  const postComment = async () => {
    if (!viewer) {
      onRequireSignIn();
      return;
    }
    if (!commentText.trim()) return;
    try {
      await eventsApi.addComment(uri, commentText.trim());
      setCommentText("");
      loadComments();
      loadDetail();
      onMutated();
    } catch (e) {
      setError(e instanceof Error ? e.message : "comment failed");
    }
  };

  const removeComment = async (id: string) => {
    try {
      await eventsApi.deleteComment(id);
      loadComments();
      loadDetail();
    } catch {
      /* keep thread state; next load reconciles */
    }
  };

  const deleteEvent = async () => {
    if (!event || !viewer || !viewer.dids.includes(event.author.did)) return;
    if (!window.confirm(`Delete "${event.name}"? The record is removed from your PDS.`)) return;
    setBusy(true);
    try {
      await eventsApi.removeEvent(uri);
      onBack();
      onMutated();
    } catch (e) {
      setError(e instanceof Error ? e.message : "delete failed");
    } finally {
      setBusy(false);
    }
  };

  if (error && !event) {
    return (
      <section className="ev-detail">
        <button type="button" className="ev-btn" onClick={onBack}>
          &lt; BACK
        </button>
        <p className="ev-error">{error}</p>
      </section>
    );
  }
  if (!event) {
    return (
      <section className="ev-detail">
        <p className="ev-muted">&gt; loading…</p>
      </section>
    );
  }

  const status = eventStatusLabel(event.status);
  const reactions = Object.values(event.reaction_counts ?? {}).reduce((a, b) => a + b, 0);
  const isAuthor = !!viewer && viewer.dids.includes(event.author.did);
  const modeLabel = event.mode?.endsWith("#virtual")
    ? "Virtual"
    : event.mode?.endsWith("#hybrid")
      ? "Hybrid"
      : event.mode?.endsWith("#inperson")
        ? "In person"
        : null;

  return (
    <section className="ev-detail">
      <div className="ev-detail-head">
        <button type="button" className="ev-btn" onClick={onBack}>
          &lt; BACK
        </button>
        {isAuthor && (
          <button
            type="button"
            className="ev-btn ev-btn-danger"
            disabled={busy}
            onClick={deleteEvent}
          >
            DELETE EVENT
          </button>
        )}
      </div>

      <h2 className="ev-detail-title">
        {event.name}{" "}
        <span className={`ev-badge ev-badge-${status.tone}`}>{status.text}</span>
      </h2>

      <p className="ev-detail-date">
        {event.starts_at
          ? formatEventDate(event.starts_at, event.ends_at, event.is_upcoming)
          : "Date TBD"}
      </p>

      <div className="ev-detail-facts">
        {event.location && (
          <p>
            📍 {event.location.name}
            {event.location.address && event.location.address !== event.location.name
              ? ` — ${event.location.address}`
              : ""}
          </p>
        )}
        {modeLabel && <p>📡 {modeLabel}</p>}
        <p>
          👤{" "}
          <a
            href={`https://bsky.app/profile/${event.author.did}`}
            target="_blank"
            rel="noreferrer"
          >
            {event.author.display_name || event.author.handle}
          </a>
        </p>
      </div>

      {event.description && <p className="ev-detail-description">{event.description}</p>}

      {event.uris
        .filter((u) => u.name !== "image")
        .map((u) => (
          <a key={u.uri} className="ev-detail-link" href={u.uri} target="_blank" rel="noreferrer">
            🔗 {u.name || u.uri}
          </a>
        ))}

      <div className="ev-detail-at">
        <code>{event.id}</code>
        <button
          type="button"
          className="ev-btn"
          onClick={() => navigator.clipboard?.writeText(event.id)}
        >
          COPY AT URI
        </button>
      </div>

      {/* RSVP (spec: signed-out tap → sign-in prompt, no write) */}
      <div className="ev-rsvp-row">
        {(["going", "maybe", "not_going"] as const).map((s) => (
          <button
            key={s}
            type="button"
            className={
              event.rsvp_status === s ? "ev-btn ev-btn-primary active" : "ev-btn"
            }
            disabled={busy}
            onClick={() => rsvp(s)}
          >
            {s === "going" ? "✓ GOING" : s === "maybe" ? "? MAYBE" : "✕ NOT GOING"}
          </button>
        ))}
        <span className="ev-chip">
          {event.rsvp_counts.going} going · {event.rsvp_counts.interested} maybe
        </span>
      </div>

      {/* Reactions */}
      <div className="ev-react-row">
        {REACTION_CHOICES.map((emoji) => (
          <button
            key={emoji}
            type="button"
            className={event.user_reaction === emoji ? "ev-react active" : "ev-react"}
            onClick={() => react(emoji)}
          >
            {emoji}
          </button>
        ))}
        <span className="ev-muted">{reactions}</span>
      </div>

      {/* Comments */}
      <div className="ev-comments">
        <h3 className="ev-panel-title">&gt; COMMENTS ({comments?.length ?? event.replies_count})</h3>
        {comments?.map((c) => (
          <div key={c.id} className="ev-comment">
            <div className="ev-comment-head">
              <strong>{c.author.display_name || c.author.handle}</strong>
              <span className="ev-muted">{new Date(c.created_at).toLocaleString()}</span>
              {viewer && c.author.handle === viewer.username && (
                <button type="button" className="ev-btn" onClick={() => removeComment(c.id)}>
                  ✕
                </button>
              )}
            </div>
            <p>{c.content}</p>
          </div>
        ))}
        {comments !== null && comments.length === 0 && (
          <p className="ev-muted">No comments yet.</p>
        )}
        <div className="ev-comment-form">
          <textarea
            value={commentText}
            placeholder={viewer ? "Say something…" : "Sign in to comment"}
            onChange={(e) => setCommentText(e.target.value)}
            disabled={!viewer}
            rows={2}
          />
          <button
            type="button"
            className="ev-btn ev-btn-primary"
            disabled={!viewer || !commentText.trim()}
            onClick={postComment}
          >
            POST
          </button>
        </div>
        {!viewer && (
          <button type="button" className="ev-btn" onClick={onRequireSignIn}>
            &gt; SIGN IN TO PARTICIPATE
          </button>
        )}
      </div>

      {error && <p className="ev-error">{error}</p>}
    </section>
  );
}
