import { useEffect, useState } from "react";
import { eventsApi } from "./api";
import type { EventItem } from "./api";
import { formatEventDate } from "./format";

interface Props {
  signedIn: boolean;
  selectedUri: string | null;
  onSelect: (uri: string | null) => void;
  /** Bumped by the layout after mutations so the RSVP panel refetches. */
  refreshKey: number;
}

/** Bottom of the right rail: the events the viewer is going to or maybe
 * attending. The "Your sources" panel above it is the same component the main
 * feed renders. */
export default function EventsRightPanel({ signedIn, selectedUri, onSelect, refreshKey }: Props) {
  const [tab, setTab] = useState<"going" | "maybe">("going");
  const [items, setItems] = useState<EventItem[] | null>(null);
  // An account without a linked identity cannot have RSVPs yet — that is a
  // state, not an error worth parroting. Only the fetch failure surfaces.
  const [unavailable, setUnavailable] = useState(false);

  useEffect(() => {
    if (!signedIn) {
      setItems(null);
      return;
    }
    let alive = true;
    eventsApi
      .myRsvps()
      .then((page) => {
        if (alive) {
          setItems(page.items);
          setUnavailable(false);
        }
      })
      .catch(() => {
        if (alive) setUnavailable(true);
      });
    return () => {
      alive = false;
    };
  }, [signedIn, refreshKey]);

  const goingCount = (items ?? []).filter((e) => e.rsvp_status === "going").length;
  const maybeCount = (items ?? []).filter((e) => e.rsvp_status === "maybe").length;
  const filtered = (items ?? []).filter((e) =>
    tab === "going" ? e.rsvp_status === "going" : e.rsvp_status === "maybe",
  );

  return (
    <section className="ev-panel ev-your-events">
      <h3 className="ev-panel-title">Your events</h3>
      {!signedIn ? (
        <p className="ev-muted">Sign in to track the events you're going to.</p>
      ) : (
        <>
          {!unavailable && (
            <div className="ev-seg ev-seg-small">
              <button
                type="button"
                className={tab === "going" ? "active" : ""}
                onClick={() => setTab("going")}
              >
                Going ({goingCount})
              </button>
              <button
                type="button"
                className={tab === "maybe" ? "active" : ""}
                onClick={() => setTab("maybe")}
              >
                Maybe ({maybeCount})
              </button>
            </div>
          )}
          {unavailable && (
            <p className="ev-muted">
              RSVPs need a linked identity — sign in with Bluesky or a
              fediverse account to track events here.
            </p>
          )}
          {!unavailable && items !== null && filtered.length === 0 && (
            <p className="ev-muted">
              {tab === "going" ? "Nothing yet — RSVP to an event." : "No maybes yet."}
            </p>
          )}
          <ul className="ev-rsvp-list">
            {filtered.map((e) => (
              <li key={e.id}>
                <button
                  type="button"
                  className={e.id === selectedUri ? "ev-rsvp-item active" : "ev-rsvp-item"}
                  onClick={() => onSelect(e.id)}
                >
                  <span className="ev-rsvp-name">{e.name}</span>
                  <span className="ev-muted">
                    {formatEventDate(e.starts_at ?? "", e.ends_at, e.is_upcoming)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
