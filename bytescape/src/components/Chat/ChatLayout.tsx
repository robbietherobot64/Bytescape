/**
 * PURPOSE
 * - Present a two-pane chat experience (conversation list + active thread) for normalized DM data.
 *
 * HOW IT WORKS
 * - Receives message data and mutation handlers from parent orchestration layers.
 * - Groups messages by `convo_id`, computes ordering/title metadata, and renders list + thread views.
 * - Maintains local UI state for selected conversation, draft input, send state, and scroll behavior.
 *
 * RELATED FILES
 * - Chat data hooks/services that fetch and normalize messages before passing `messages` into this component
 * - Parent screens/providers that implement `onSync` and `onSendMessage`
 * - CSS/theme files defining `chat-*` classes used by this layout
 */
import React, { useState, useRef, useEffect } from 'react';

/**
 * Render-ready message shape consumed by `ChatLayout`.
 */
export interface ChatMessage {
    id: string;
    convo_id: string;
    sender_did: string;
    sender_handle?: string;
    text: string;
    sent_at: string;
}

/**
 * Public props contract for `ChatLayout`.
 */
export interface ChatLayoutProps {
    currentUser: { did: string; handle: string } | null;
    messages: ChatMessage[];
    isSyncing: boolean;
    onSync: () => void;
    onSendMessage: (convoId: string, content: string) => Promise<void>;
}

const BOTTOM_THRESHOLD_PX = 50;

function sortBySentAtDesc(a: ChatMessage, b: ChatMessage): number {
    return b.sent_at.localeCompare(a.sent_at);
}

function formatListDate(tsStr: string): string {
    return new Date(tsStr).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

/**
 * Chat conversation layout component.
 *
 * Cross-system notes:
 * - This component is presentational/stateful UI and does not directly call `invoke()`.
 * - Data fetching/sync/send behaviors are injected via `messages`, `onSync`, and `onSendMessage`,
 *   typically from hooks/store/provider orchestration layers.
 */
export function ChatLayout({
    currentUser,
    messages,
    isSyncing,
    onSync,
    onSendMessage,
}: ChatLayoutProps) {
    // ═══════════════════════════════════════════════════════════════════════
    // Local UI state
    // ═══════════════════════════════════════════════════════════════════════
    const [selectedConvoId, setSelectedConvoId] = useState<string | null>(null);
    const [messageInput, setMessageInput] = useState("");
    const [isSending, setIsSending] = useState(false);
    const [showScrollButton, setShowScrollButton] = useState(false);

    const messagesEndRef = useRef<HTMLDivElement>(null);
    const chatContainerRef = useRef<HTMLDivElement>(null);
    const isAtBottomRef = useRef(true);

    // ═══════════════════════════════════════════════════════════════════════
    // Derived conversation model
    // ═══════════════════════════════════════════════════════════════════════
    // Cross-system note: upstream hooks/providers are expected to deliver a flat, normalized message list.
    // Group messages by conversation and normalize to newest-first ordering.
    const byConvo: Record<string, ChatMessage[]> = {};
    for (const m of messages) {
        if (!byConvo[m.convo_id]) byConvo[m.convo_id] = [];
        byConvo[m.convo_id].push(m);
    }
    for (const convoId of Object.keys(byConvo)) {
        byConvo[convoId].sort(sortBySentAtDesc);
    }

    // Sort conversations by latest message time
    const convoIds = Object.keys(byConvo).sort((a, b) => {
        const lastA = byConvo[a][0]?.sent_at || "";
        const lastB = byConvo[b][0]?.sent_at || "";
        return lastB.localeCompare(lastA);
    });

    const activeConvoMsgs = selectedConvoId ? byConvo[selectedConvoId] : null;

    // Derive title
    let activeTitle = "Chat";
    if (activeConvoMsgs && activeConvoMsgs.length > 0) {
        const handles = [...new Set(activeConvoMsgs.filter((m) => m.sender_did !== currentUser?.did).map((m) => m.sender_handle))];
        activeTitle = handles.length > 0 ? handles.join(", ") : "Just You";
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Scroll management
    // ═══════════════════════════════════════════════════════════════════════
    // Scroll logic
    const handleChatScroll = () => {
        if (chatContainerRef.current) {
            const { scrollTop, scrollHeight, clientHeight } = chatContainerRef.current;
            const atBottom = scrollHeight - scrollTop - clientHeight < BOTTOM_THRESHOLD_PX;
            isAtBottomRef.current = atBottom;
            if (atBottom) setShowScrollButton(false);
        }
    };

    // Keep autoscroll behavior predictable: only force-scroll when already at bottom.
    useEffect(() => {
        if (messagesEndRef.current) {
            if (isAtBottomRef.current) {
                messagesEndRef.current.scrollIntoView({ behavior: "smooth" });
                setShowScrollButton(false);
            } else {
                setShowScrollButton(true);
            }
        }
    }, [messages, selectedConvoId]); // Added selectedConvoId to dependency to ensure scroll on switch

    useEffect(() => {
        if (messagesEndRef.current) {
            messagesEndRef.current.scrollIntoView();
            isAtBottomRef.current = true;
            setShowScrollButton(false);
        }
    }, [selectedConvoId]);

    // ═══════════════════════════════════════════════════════════════════════
    // Message send action
    // ═══════════════════════════════════════════════════════════════════════
    // Cross-system note: send side effects are delegated through `onSendMessage` (typically hook/provider backed).
    const handleSend = async () => {
        if (!selectedConvoId || !messageInput.trim()) return;
        setIsSending(true);
        try {
            await onSendMessage(selectedConvoId, messageInput);
            setMessageInput("");
        } finally {
            setIsSending(false);
        }
    };

    // ═══════════════════════════════════════════════════════════════════════
    // Render
    // ═══════════════════════════════════════════════════════════════════════
    return (
        <div className="chat-layout">
            {/* Sidebar List */}
            <div className="chat-list">
                <div className="chat-list-header">
                    <span>Messages</span>
                    <button
                        className="btn btn-primary btn-sm"
                        onClick={onSync}
                        disabled={isSyncing}
                        style={{ fontSize: '0.75rem', padding: '4px 8px' }}
                    >
                        {isSyncing ? "⏳" : "🔄 Sync"}
                    </button>
                </div>
                <div className="chat-list-scroll">
                    {convoIds.length === 0 ? (
                        <div style={{ padding: 20, textAlign: 'center', color: 'var(--text-muted)' }}>
                            No conversations found.<br />Sync to get started.
                        </div>
                    ) : (
                        convoIds.map(cid => {
                            const msgs = byConvo[cid];
                            const last = msgs[0];

                            const handles = [...new Set(msgs.map((m) => m.sender_handle))].filter(h => h !== currentUser?.handle);
                            const title = handles.length > 0 ? handles.join(", ") : "Just You";

                            return (
                                <div
                                    key={cid}
                                    className={`chat-item ${selectedConvoId === cid ? 'active' : ''}`}
                                    onClick={() => setSelectedConvoId(cid)}
                                >
                                    <div className="chat-item-header">
                                        <span className="chat-item-title">{title}</span>
                                        <span className="chat-item-time">{formatListDate(last.sent_at)}</span>
                                    </div>
                                    <div className="chat-item-preview">
                                        {last.sender_did === currentUser?.did ? "You: " : ""}{last.text}
                                    </div>
                                </div>
                            );
                        })
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
                            {[...(activeConvoMsgs || [])].reverse().map((m, i, arr) => {
                                const isMe = m.sender_did === currentUser?.did;
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
                                    } as React.CSSProperties}
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

                        <div className="chat-input-area">
                            <input
                                type="text"
                                className="chat-input"
                                placeholder="Type a message..."
                                value={messageInput}
                                onChange={e => setMessageInput(e.target.value)}
                                onKeyDown={e => e.key === 'Enter' && handleSend()}
                                disabled={isSending}
                            />
                            <button
                                className="chat-send-btn"
                                onClick={handleSend}
                                disabled={!messageInput.trim() || isSending}
                            >
                                {isSending ? "..." : "➤"}
                            </button>
                        </div>
                    </>
                ) : (
                    <div className="empty-state" style={{ height: '100%', display: 'flex', flexDirection: 'column', justifyContent: 'center' }}>
                        <div className="empty-icon">💬</div>
                        <div className="empty-title">Select a conversation</div>
                        <div className="empty-hint">Or sync to update your messages</div>
                    </div>
                )}
            </div>
        </div>
    );
}
