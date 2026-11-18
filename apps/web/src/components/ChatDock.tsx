import { useState, useEffect, useRef, useCallback } from 'react';
import { chatApi, type Conversation, type ChatMessage } from '../utils/apiChat';
import { feedApi } from '../utils/api';

// ── seen-state: what the user has actually laid eyes on ────────────────────
// Bluesky's own unread counters never clear — marking read would need the
// chat.bsky.convo.updateRead scope, which re-breaks account linking against
// Bluesky's cached client metadata. The badge therefore tracks locally
// observed message timestamps instead of the server's unread_count.

function storedSeen(id: string): string {
  return localStorage.getItem(`chat_seen:${id}`) || '';
}

function rememberSeen(id: string, ts: string) {
  if (!ts) return;
  if (ts > storedSeen(id)) localStorage.setItem(`chat_seen:${id}`, ts);
}

// ── one popup per open conversation ─────────────────────────────────────────

interface PopupProps {
  convo: Conversation;
  onClose: (id: string) => void;
  onSeen: (id: string, ts: string) => void;
}

function ChatPopup({ convo, onClose, onSeen }: PopupProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const prevLastId = useRef<string | null>(null);

  // Poll: open windows must show incoming messages without a reopen, and
  // anything visible here counts as seen.
  useEffect(() => {
    let alive = true;
    const load = () => {
      chatApi
        .messages(convo.id)
        .then((msgs) => {
          if (!alive) return;
          setMessages(msgs);
          const latest = msgs[msgs.length - 1]?.sent_at;
          if (latest) {
            rememberSeen(convo.id, latest);
            onSeen(convo.id, latest);
          }
        })
        .catch(() => {});
    };
    load();
    const t = setInterval(load, 6000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, [convo.id, onSeen]);

  // Pin to bottom only when the tail changes — polling must not yank the
  // scroll position while reading history.
  useEffect(() => {
    const last = messages[messages.length - 1];
    if (last && last.id !== prevLastId.current) {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
    prevLastId.current = last?.id ?? null;
  }, [messages]);

  const send = async () => {
    const text = input.trim();
    if (!text) return;
    setInput('');
    try {
      const msg = await chatApi.sendMessage(convo.id, text);
      setMessages((prev) => [...prev, msg]);
      rememberSeen(convo.id, msg.sent_at);
      onSeen(convo.id, msg.sent_at);
    } catch {
      setInput(text);
    }
  };

  return (
    <div className="chat-popup">
      <div className="chat-popup-header">
        <span className="chat-popup-name">
          {convo.is_group ? '👥 ' : ''}
          {convo.name}
          {convo.is_group && <span className="chat-popup-count">{convo.member_count} members</span>}
        </span>
        <button className="chat-popup-close" onClick={() => onClose(convo.id)} aria-label="Close chat">
          ×
        </button>
      </div>
      <div className="chat-popup-messages">
        {messages.map((msg) => (
          <div
            key={msg.id}
            className={`chat-msg${msg.sender_id === 'me' ? ' mine' : ''}`}
          >
            <div className="chat-msg-body">{msg.content}</div>
            <div className="chat-msg-ts">{new Date(msg.sent_at).toLocaleTimeString()}</div>
          </div>
        ))}
        <div ref={messagesEndRef} />
      </div>
      <div className="chat-popup-composer">
        <input
          type="text"
          placeholder="Type a message…"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') send();
          }}
        />
        <button onClick={send} disabled={!input.trim()}>
          Send
        </button>
      </div>
    </div>
  );
}

// ── the dock: launcher + selector panel + tiled popups ─────────────────────

export default function ChatDock() {
  const [panelOpen, setPanelOpen] = useState(false);
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [openConvos, setOpenConvos] = useState<Conversation[]>([]);
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<{ handle: string; displayName?: string; avatar?: string }[]>([]);
  const [searching, setSearching] = useState(false);
  const [seenMap, setSeenMap] = useState<Record<string, string>>({});
  const dockRef = useRef<HTMLDivElement>(null);
  const prevOpenCount = useRef(0);

  // When a chat opens and the strip overflows the canvas, keep the newest
  // window (the row's end, next to the button) in view.
  useEffect(() => {
    const el = dockRef.current;
    if (el && openConvos.length > prevOpenCount.current) el.scrollLeft = el.scrollWidth;
    prevOpenCount.current = openConvos.length;
  }, [openConvos]);

  const refresh = useCallback(() => {
    chatApi.conversations().then(setConversations).catch(() => {});
  }, []);

  // Conversation previews and badge stay live even with the panel closed.
  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 30000);
    return () => clearInterval(t);
  }, [refresh]);

  useEffect(() => {
    if (panelOpen) {
      setLoading(true);
      refresh();
      setLoading(false);
    }
  }, [panelOpen, refresh]);

  const onSeen = useCallback((id: string, ts: string) => {
    setSeenMap((prev) => (ts > (prev[id] || '') ? { ...prev, [id]: ts } : prev));
  }, []);

  // Hybrid read-state: Bluesky's unread_count is the authoritative baseline
  // (it knows your whole history and what you've read on any client). Local
  // seen-timestamps only override it to 0 — when you've read the latest
  // message in OUR popup, which we can't write back to Bluesky (updateRead
  // scope is not grantable). Never the other way around: a fresh local state
  // must not mark old history as unseen.
  const unseenCount = useCallback(
    (c: Conversation) => {
      const seen = seenMap[c.id] || storedSeen(c.id);
      if (c.last_sent_at && c.last_sent_at <= seen) return 0;
      return c.unread_count || 0;
    },
    [seenMap],
  );

  // user search for starting new chats
  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) {
      setResults([]);
      return;
    }
    let cancelled = false;
    setSearching(true);
    const t = setTimeout(() => {
      feedApi
        .searchPeople(q)
        .then((people: { handle: string; displayName?: string; avatar?: string }[]) => {
          if (!cancelled) setResults(people.slice(0, 6));
        })
        .catch(() => {
          if (!cancelled) setResults([]);
        })
        .finally(() => {
          if (!cancelled) setSearching(false);
        });
    }, 350);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query]);

  const unread = conversations.reduce((sum, c) => sum + unseenCount(c), 0);

  // Broadcast unread count to the sidebar Messages button, and let it toggle
  // this panel via the same event bus used elsewhere in the app.
  useEffect(() => {
    window.dispatchEvent(new CustomEvent('bytescape:chatUnread', { detail: { count: unread } }));
  }, [unread]);
  useEffect(() => {
    const toggle = () => setPanelOpen((o) => !o);
    window.addEventListener('bytescape:toggleChatPanel', toggle);
    return () => window.removeEventListener('bytescape:toggleChatPanel', toggle);
  }, []);

  const openConvo = (convo: Conversation) => {
    // Appended at the row's end = growing rightward across the feed canvas;
    // reopening an open chat pulls it back to that slot.
    setOpenConvos((prev) => [...prev.filter((c) => c.id !== convo.id), convo]);
    setPanelOpen(false);
  };

  const startNew = async (handle: string) => {
    try {
      const convo = await chatApi.startConversation(handle);
      setQuery('');
      setResults([]);
      setConversations((prev) => (prev.some((c) => c.id === convo.id) ? prev : [convo, ...prev]));
      openConvo(convo);
    } catch {}
  };

  return (
    <>
      {/* Fixed overlay mirroring the app cluster ([nav 232][feed 600]
          [sources 300]) so the conversation list opens beside the Messages
          button at the bottom of the nav rail and popups tile left-to-right
          across the feed canvas at any viewport width. */}
      <div className="chat-anchor">
        <div className="chat-anchor-cluster">
          <div className="chat-anchor-nav">
            {panelOpen && (
        <div className="chat-selector">
          <div className="chat-selector-header">
            <strong>Messages</strong>
            <button className="chat-popup-close" onClick={() => setPanelOpen(false)} aria-label="Close">
              ×
            </button>
          </div>
          <div className="chat-selector-search">
            <input
              type="text"
              placeholder="Search people to chat…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            {searching && <div className="chat-selector-hint">Searching…</div>}
            {results.map((p) => (
              <button key={p.handle} className="chat-selector-person" onClick={() => startNew(p.handle)}>
                {p.avatar && <img src={p.avatar} alt="" />} @{p.handle}
              </button>
            ))}
          </div>
          <div className="chat-selector-list">
            {loading ? (
              <div className="chat-selector-hint">Loading…</div>
            ) : conversations.length === 0 ? (
              <div className="chat-selector-hint">No conversations yet.</div>
            ) : (
              conversations.map((c) => (
                <button key={c.id} className="chat-selector-item" onClick={() => openConvo(c)}>
                  <span className="chat-selector-name">
                    {c.is_group ? '👥 ' : ''}
                    {c.name}
                  </span>
                  {unseenCount(c) > 0 && <span className="chat-selector-badge">{unseenCount(c)}</span>}
                  {c.last_message && <span className="chat-selector-last">{c.last_message}</span>}
                </button>
              ))
            )}
          </div>
        </div>
            )}
          </div>
          <div className="chat-anchor-feed">
            {openConvos.length > 0 && (
              <div className="chat-dock" ref={dockRef}>
                {openConvos.map((c) => (
                  <ChatPopup
                    key={c.id}
                    convo={c}
                    onSeen={onSeen}
                    onClose={(id) => setOpenConvos((prev) => prev.filter((x) => x.id !== id))}
                  />
                ))}
              </div>
            )}
          </div>
          <div className="chat-anchor-panel" />
        </div>
      </div>

    </>
  );
}
