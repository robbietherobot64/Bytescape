import React, { useRef, useState, useEffect } from 'react';
import { AuthStatus, VaultMessage } from '../types';
import { formatDate } from '../utils/date';

interface ChatScreenProps {
    auth: AuthStatus | null;
    messages: VaultMessage[];
    selectedConvoId: string | null;
    setSelectedConvoId: (id: string | null) => void;
    messageInput: string;
    setMessageInput: (val: string) => void;
    isSending: boolean;
    onSendMessage: () => void;
}

export function ChatScreen({
    auth,
    messages,
    selectedConvoId,
    setSelectedConvoId,
    messageInput,
    setMessageInput,
    isSending,
    onSendMessage
}: ChatScreenProps) {
    const LIVE_SOURCES = new Set(['bluesky', 'mastodon']);

    const messagesEndRef = useRef<HTMLDivElement>(null);
    const chatContainerRef = useRef<HTMLDivElement>(null);
    const isAtBottomRef = useRef(true);
    const [showScrollButton, setShowScrollButton] = useState(false);

    // Group messages
    const byConvo: Record<string, VaultMessage[]> = {};
    for (const m of messages) {
        if (!byConvo[m.convo_id]) byConvo[m.convo_id] = [];
        byConvo[m.convo_id].push(m);
    }

    // Sort conversations by latest message time
    const convoIds = Object.keys(byConvo).sort((a, b) => {
        const lastA = byConvo[a][0]?.sent_at || "";
        const lastB = byConvo[b][0]?.sent_at || "";
        return lastB.localeCompare(lastA);
    });

    const liveConvoIds = convoIds.filter(cid => {
        const src = (byConvo[cid]?.[0]?.source || '').toLowerCase();
        return LIVE_SOURCES.has(src);
    });

    const archivedConvoIds = convoIds.filter(cid => !liveConvoIds.includes(cid));

    const activeConvoMsgs = selectedConvoId ? byConvo[selectedConvoId] : null;
    const activeSource = (activeConvoMsgs?.[0]?.source || '').toLowerCase();
    const activeIsLive = activeSource ? LIVE_SOURCES.has(activeSource) : true;

    // Derive title from participants
    let activeTitle = "Chat";
    if (activeConvoMsgs && activeConvoMsgs.length > 0) {
        const handles = [...new Set(activeConvoMsgs.filter((m) => m.sender_did !== auth?.did).map((m) => m.sender_handle))];
        activeTitle = handles.length > 0 ? handles.join(", ") : "Just You";
    }

    // Auto-scroll logic
    useEffect(() => {
        if (messagesEndRef.current) {
            if (isAtBottomRef.current) {
                messagesEndRef.current.scrollIntoView({ behavior: "smooth" });
                setShowScrollButton(false);
            } else {
                setShowScrollButton(true);
            }
        }
    }, [messages]);

    // Reset scroll on convo change
    useEffect(() => {
        if (messagesEndRef.current) {
            messagesEndRef.current.scrollIntoView();
            isAtBottomRef.current = true;
            setShowScrollButton(false);
        }
    }, [selectedConvoId]);

    const handleChatScroll = () => {
        if (chatContainerRef.current) {
            const { scrollTop, scrollHeight, clientHeight } = chatContainerRef.current;
            const atBottom = scrollHeight - scrollTop - clientHeight < 50;
            isAtBottomRef.current = atBottom;
            if (atBottom) setShowScrollButton(false);
        }
    };

    return (
        <>
            <div className="chat-layout">
                {/* Sidebar List */}
                <div className="chat-list">
                    <div className="chat-list-header">
                        <span>Messages</span>

                    </div>
                    <div className="chat-list-scroll">
                        {convoIds.length === 0 ? (
                            <div style={{ padding: 20, textAlign: 'center', color: 'var(--text-muted)' }}>
                                No conversations found.<br />Sync to get started.
                            </div>
                        ) : (
                            <>
                                {liveConvoIds.length > 0 && (
                                    <div style={{ padding: '8px 12px', fontSize: '0.72rem', fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.4px' }}>
                                        Live (Bluesky / Mastodon)
                                    </div>
                                )}
                                {liveConvoIds.map(cid => {
                                    const msgs = byConvo[cid];
                                    const last = msgs[0];
                                    const handles = [...new Set(msgs.map(m => m.sender_handle))].filter(h => h !== auth?.handle);
                                    const title = handles.length > 0 ? handles.join(", ") : "Just You";

                                    return (
                                        <div
                                            key={cid}
                                            className={`chat-item ${selectedConvoId === cid ? 'active' : ''}`}
                                            onClick={() => setSelectedConvoId(cid)}
                                        >
                                            <div className="chat-item-header">
                                                <span className="chat-item-title">{title}</span>
                                                <span className="chat-item-time">{formatDate(new Date(last.sent_at).getTime() / 1000).split(',')[0]}</span>
                                            </div>
                                            <div className="chat-item-preview">
                                                {last.sender_did === auth?.did ? "You: " : ""}{last.text}
                                            </div>
                                        </div>
                                    );
                                })}

                                {archivedConvoIds.length > 0 && (
                                    <div style={{ padding: '8px 12px', marginTop: 10, fontSize: '0.72rem', fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.4px' }}>
                                        Archived (Walled Garden)
                                    </div>
                                )}
                                {archivedConvoIds.map(cid => {
                                    const msgs = byConvo[cid];
                                    const last = msgs[0];
                                    const handles = [...new Set(msgs.map(m => m.sender_handle))].filter(h => h !== auth?.handle);
                                    const title = handles.length > 0 ? handles.join(", ") : "Archived Thread";

                                    return (
                                        <div
                                            key={cid}
                                            className={`chat-item ${selectedConvoId === cid ? 'active' : ''}`}
                                            onClick={() => setSelectedConvoId(cid)}
                                        >
                                            <div className="chat-item-header">
                                                <span className="chat-item-title">{title}</span>
                                                <span className="chat-item-time">{formatDate(new Date(last.sent_at).getTime() / 1000).split(',')[0]}</span>
                                            </div>
                                            <div className="chat-item-preview">
                                                📚 {last.text}
                                            </div>
                                        </div>
                                    );
                                })}
                            </>
                        )}
                    </div>
                </div>

                {/* Main Chat Area */}
                <div className={`chat-main ${selectedConvoId ? 'open' : ''}`}>
                    {selectedConvoId ? (
                        <>
                            <div className="chat-header">
                                <button className="chat-back-btn" onClick={() => setSelectedConvoId(null)}>←</button>
                                <div style={{ fontWeight: 600 }}>{activeTitle}</div>
                            </div>

                            <div
                                className="chat-messages"
                                ref={chatContainerRef}
                                onScroll={handleChatScroll}
                            >
                                {activeConvoMsgs?.slice().reverse().map((m, i, arr) => {
                                    const isMe = m.sender_did === auth?.did;

                                    const date = new Date(m.sent_at);
                                    const dateStr = date.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
                                    let showDate = false;
                                    if (i === 0) showDate = true;
                                    else {
                                        const prevDate = new Date(arr[i - 1].sent_at);
                                        if (prevDate.toLocaleDateString() !== date.toLocaleDateString()) showDate = true;
                                    }

                                    return (
                                        <React.Fragment key={m.id}>
                                            {showDate && <div className="date-separator">{dateStr}</div>}
                                            <div className={`message-bubble ${isMe ? 'sent' : 'received'}`}>
                                                {!isMe && <div className="text-muted text-sm" style={{ fontSize: '0.7rem', marginBottom: 2 }}>{m.sender_handle}</div>}
                                                {m.text}
                                                <div className="message-time">
                                                    {date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                                                </div>
                                            </div>
                                        </React.Fragment>
                                    );
                                })}
                                <div ref={messagesEndRef} />

                                {showScrollButton && (
                                    <button
                                        style={{
                                            position: 'fixed',
                                            bottom: '80px',
                                            right: '20px',
                                            background: 'var(--accent-blue)',
                                            color: 'white',
                                            border: 'none',
                                            borderRadius: '20px',
                                            padding: '8px 16px',
                                            boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
                                            cursor: 'pointer',
                                            zIndex: 100,
                                            display: 'flex',
                                            alignItems: 'center',
                                            gap: 6
                                        }}
                                        onClick={() => {
                                            messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
                                            setShowScrollButton(false);
                                            isAtBottomRef.current = true;
                                        }}
                                    >
                                        ⬇ New Messages
                                    </button>
                                )}
                            </div>

                            {activeIsLive ? (
                                <div className="chat-input-area">
                                    <input
                                        type="text"
                                        className="chat-input"
                                        placeholder="Type a message..."
                                        value={messageInput}
                                        onChange={e => setMessageInput(e.target.value)}
                                        onKeyDown={e => e.key === 'Enter' && onSendMessage()}
                                        disabled={isSending}
                                    />
                                    <button
                                        className="chat-send-btn"
                                        onClick={onSendMessage}
                                        disabled={!messageInput.trim() || isSending}
                                    >
                                        {isSending ? "..." : "➤"}
                                    </button>
                                </div>
                            ) : (
                                <div className="chat-input-area" style={{ color: 'var(--text-muted)', fontSize: '0.85rem' }}>
                                    Archived conversation (read-only).
                                </div>
                            )}
                        </>
                    ) : (
                        <div className="empty-state" style={{ height: '100%', display: 'flex', flexDirection: 'column', justifyContent: 'center' }}>
                            <div className="empty-icon">💬</div>
                            <div className="empty-title">Select a conversation</div>
                            <div className="empty-hint">Start a conversation from your friends list</div>
                        </div>
                    )}
                </div>
            </div>
        </>
    );
}
