import { useCallback, useEffect, useState } from "react";
import {
  BrowserRouter,
  Link,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useParams,
} from "react-router-dom";
import { EventsLayout, configureEventsApi, eventsApi } from "@bytescape/events-ui";
import type { EventsViewer } from "@bytescape/events-ui";

// Cookie-only auth on this origin (design decision 8): no token getter.
configureEventsApi({});

// SSO base: the family login lives on the web app. Dev injects it via
// .env.development; deployed builds derive it from the host so the same
// bundle serves events.bytescape.social and events.test.bytescape.social.
const SSO_BASE = (() => {
  const fromEnv = import.meta.env.VITE_SSO_BASE;
  if (fromEnv) return fromEnv;
  const host = window.location.hostname;
  if (host.endsWith(".test.bytescape.social")) return "https://test.bytescape.social";
  return "https://bytescape.social";
})();

function useSession() {
  const [viewer, setViewer] = useState<EventsViewer | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    setLoading(true);
    eventsApi
      .session()
      .then((s) => setViewer({ username: s.username, dids: s.dids }))
      .catch(() =>
        // Idle-origin resume (spec: events/session): access cookie expired →
        // cookie-path refresh (the server reads bytescape_refresh) → retry.
        fetch("/api/auth/refresh", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: "{}",
        })
          .then((r) => (r.ok ? eventsApi.session() : Promise.reject(new Error("refresh failed"))))
          .then((s) => setViewer({ username: s.username, dids: s.dids }))
          .catch(() => setViewer(null)),
      )
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const signOut = useCallback(async () => {
    try {
      await fetch("/api/auth/logout", { method: "POST" });
    } catch {
      /* cookie clearing is best-effort; state below is the visible truth */
    }
    setViewer(null);
  }, []);

  return { viewer, loading, refresh, signOut };
}

function BrowseRoute({
  viewer,
  onRequireSignIn,
}: {
  viewer: EventsViewer | null;
  onRequireSignIn: () => void;
}) {
  const navigate = useNavigate();
  return (
    <EventsLayout
      viewer={viewer}
      onRequireSignIn={onRequireSignIn}
      selectedUri={null}
      onSelectedUriChange={(uri) => {
        if (uri) navigate(`/event/${encodeURIComponent(uri)}`);
      }}
    />
  );
}

function DetailRoute({
  viewer,
  onRequireSignIn,
}: {
  viewer: EventsViewer | null;
  onRequireSignIn: () => void;
}) {
  const { uri } = useParams<{ uri: string }>();
  const navigate = useNavigate();
  const decoded = uri ? decodeURIComponent(uri) : "";
  if (!decoded) {
    navigate("/", { replace: true });
    return null;
  }
  return (
    <EventsLayout
      viewer={viewer}
      onRequireSignIn={onRequireSignIn}
      selectedUri={decoded}
      onSelectedUriChange={(next) => {
        if (!next) navigate("/");
      }}
    />
  );
}

export default function App() {
  return (
    <BrowserRouter>
      <Shell />
    </BrowserRouter>
  );
}

function Shell() {
  const { viewer, loading, refresh, signOut } = useSession();
  const location = useLocation();

  const requireSignIn = useCallback(() => {
    // SSO hop: carry the current URL as ?next (validated by the web app).
    const next = encodeURIComponent(window.location.href);
    window.location.href = `${SSO_BASE}/login?next=${next}`;
  }, []);

  // Coming back from the SSO hop: the family cookie now exists — re-read it.
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    if (params.has("sso") || params.has("oauth_complete")) {
      refresh();
      window.history.replaceState({}, "", location.pathname);
    }
  }, [location.pathname, refresh]);

  return (
    <div className="events-app">
      <header className="events-app-bar">
        <Link to="/" className="events-brand">
          &gt; EVENTS<span className="events-brand-sub">bytescape</span>
        </Link>
        <nav className="events-app-nav">
          <a href={`${SSO_BASE}/`} className="ev-btn">
            MAIN SITE
          </a>
          {loading ? (
            <span className="ev-muted">…</span>
          ) : viewer ? (
            <>
              <span className="ev-muted">@{viewer.username}</span>
              <button type="button" className="ev-btn" onClick={signOut}>
                SIGN OUT
              </button>
            </>
          ) : (
            <button type="button" className="ev-btn ev-btn-primary" onClick={requireSignIn}>
              SIGN IN
            </button>
          )}
        </nav>
      </header>

      <main className="events-app-body">
        <Routes>
          <Route path="/" element={<BrowseRoute viewer={viewer} onRequireSignIn={requireSignIn} />} />
          <Route
            path="/event/:uri"
            element={<DetailRoute viewer={viewer} onRequireSignIn={requireSignIn} />}
          />
          <Route path="*" element={<BrowseRoute viewer={viewer} onRequireSignIn={requireSignIn} />} />
        </Routes>
      </main>
    </div>
  );
}
