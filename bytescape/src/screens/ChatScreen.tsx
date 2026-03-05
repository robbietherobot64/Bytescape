import React, { useRef, useState, useEffect, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useChatStore, useAuthStore } from '../store';
import { VaultMessage } from '../types';
import { formatDate } from '../utils/date';
import { Win98Box } from '../components/Win98Box';
import { BsPersonFill, BsCircleFill, BsSearch, BsArrowDown } from 'react-icons/bs';

/* ────────────────────────────────────────────────────────
   ChatScreen — Discord-meets-AIM hybrid
   Layout: [Convo List 220px] [Chat Area flex-1] [Buddy List 200px]
   ──────────────────────────────────────────────────────── */

const LIVE_SOURCES = new Set(['bluesky', 'mastodon']);

interface BuddyInfo {
    handle: string;
    did: string;
    avatar?: string;
}
export function ChatScreen() {
    const auth = useAuthStore(s => s.auth);
    const {
        messages, setMessages,
        selectedConvoId, setSelectedConvoId,
        messageInput, setMessageInput,
        isSending, setIsSending,
    } = useChatStore();

    const [searchFilter, setSearchFilter] = useState('');
    const [buddyFilter, setBuddyFilter] = useState('');
    const [showScrollBtn, setShowScrollBtn] = useState(false);
    const [buddies, setBuddies] = useState<BuddyInfo[]>([]);

    const messagesEndRef = useRef<HTMLDivElement>(null);
    const chatContainerRef = useRef<HTMLDivElement>(null);
    const isAtBottomRef = useRef(true);

    // ── Load messages + buddies on mount ──
    useEffect(() => {
        loadMessages();
        loadBuddies();
    }, []);

    // ── Poll for new messages ──
    useEffect(() => {
        if (!auth) return;
        const poll = async () => {
            try {
                await invoke<any>('import_vault_messages');
                await loadMessages();
            } catch { /* silent */ }
        };
        poll();
        const interval = setInterval(poll, 30000);
        return () => clearInterval(interval);
    }, [auth]);

    async function loadMessages() {
        try {
            const msgs = await invoke<VaultMessage[]>('list_vault_messages');
            setMessages(msgs);
        } catch (e) {
            console.error('Failed to load messages:', e);
        }
    }

    async function loadBuddies() {
        try {
            const contacts = await invoke<BuddyInfo[]>('list_contacts');
            setBuddies(contacts);
        } catch {
            setBuddies([]);
        }
    }

    // ── Group messages by conversation ──
    const byConvo: Record<string, VaultMessage[]> = {};
    for (const m of messages) {
        if (!byConvo[m.convo_id]) byConvo[m.convo_id] = [];
        byConvo[m.convo_id].push(m);
    }

    const convoIds = Object.keys(byConvo).sort((a, b) => {
        const lastA = byConvo[a][0]?.sent_at || '';
        const lastB = byConvo[b][0]?.sent_at || '';
        return lastB.localeCompare(lastA);
    });

    const filteredConvoIds = convoIds.filter(cid => {
        if (!searchFilter.trim()) return true;
        const msgs = byConvo[cid];
        const handles = msgs.map(m => m.sender_handle).join(' ').toLowerCase();
        const peerHandles = msgs.map(m => m.peer_handle || '').join(' ').toLowerCase();
        return handles.includes(searchFilter.toLowerCase()) ||
               peerHandles.includes(searchFilter.toLowerCase());
    });

    const liveConvoIds = filteredConvoIds.filter(cid => {
        const src = (byConvo[cid]?.[0]?.source || '').toLowerCase();
        return LIVE_SOURCES.has(src);
    });
    const archivedConvoIds = filteredConvoIds.filter(cid => !liveConvoIds.includes(cid));

    const activeConvoMsgs = selectedConvoId ? byConvo[selectedConvoId] : null;
    const activeSource = (activeConvoMsgs?.[0]?.source || '').toLowerCase();
    const activeIsLive = activeSource ? LIVE_SOURCES.has(activeSource) : true;

    // Derive title from participants
    let activeTitle = 'Chat';
    if (activeConvoMsgs && activeConvoMsgs.length > 0) {
        const senderHandles = [...new Set(activeConvoMsgs.map(m => m.sender_handle))].filter(h => h !== auth?.handle);
        const peerHandles = [...new Set(activeConvoMsgs.map(m => m.peer_handle).filter(Boolean))].filter(h => h !== auth?.handle);
        const allHandles = [...new Set([...senderHandles, ...peerHandles])];
        activeTitle = allHandles.length > 0 ? allHandles.join(', ') : 'Just You';
    }

    // ── Scroll management ──
    useEffect(() => {
        if (messagesEndRef.current && isAtBottomRef.current) {
            messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
            setShowScrollBtn(false);
        } else if (messagesEndRef.current) {
            setShowScrollBtn(true);
        }
    }, [messages]);

    useEffect(() => {
        if (messagesEndRef.current) {
            messagesEndRef.current.scrollIntoView();
            isAtBottomRef.current = true;
            setShowScrollBtn(false);
        }
    }, [selectedConvoId]);

    const handleChatScroll = useCallback(() => {
        if (chatContainerRef.current) {
            const { scrollTop, scrollHeight, clientHeight } = chatContainerRef.current;
            const atBottom = scrollHeight - scrollTop - clientHeight < 50;
            isAtBottomRef.current = atBottom;
            if (atBottom) setShowScrollBtn(false);
        }
    }, []);

    // ── Send message ──
    const handleSend = async () => {
        if (!selectedConvoId || !messageInput.trim()) return;
        const text = messageInput.trim();
        setMessageInput('');
        setIsSending(true);

        try {
            const now = Date.now() / 1000;
            const optimisticMsg: VaultMessage = {
                id: `temp-${now}`,
                convo_id: selectedConvoId,
                sender_did: auth?.did || '',
                sender_handle: auth?.handle || '',
                text,
                sent_at: new Date().toISOString(),
                source: 'bluesky',
                imported_at: now,
            };
            setMessages(prev => [optimisticMsg, ...prev]);

            await invoke<any>('send_bsky_dm', {
                convoId: selectedConvoId,
                recipientDid: null,
                content: text,
            });
        } catch (e) {
            console.error('Send failed:', e);
        } finally {
            setIsSending(false);
        }
    };

    // ── Helper: convo title ──
    function getConvoTitle(cid: string): string {
        const msgs = byConvo[cid];
        if (!msgs || msgs.length === 0) return 'Unknown';
        const senderHandles = [...new Set(msgs.map(m => m.sender_handle))].filter(h => h && h !== auth?.handle);
        const peerHandles = [...new Set(msgs.map(m => m.peer_handle).filter(Boolean))].filter(h => h && h !== auth?.handle);
        const allHandles = [...new Set([...senderHandles, ...peerHandles])].filter((h): h is string => !!h);
        return allHandles.length > 0 ? allHandles.map(h => `@${h.replace(/^@/, '')}`).join(', ') : 'Just You';
    }

    function getConvoInitial(cid: string): string {
        return getConvoTitle(cid).replace('@', '').charAt(0).toUpperCase();
    }

    // ── Buddy list from contacts or derived from messages ──
    const derivedBuddies: BuddyInfo[] = useMemo(() => {
        if (buddies.length > 0) return [];
        const seen = new Map<string, BuddyInfo>();
        for (const m of messages) {
            if (m.sender_did && m.sender_did !== auth?.did && !seen.has(m.sender_did)) {
                seen.set(m.sender_did, { handle: m.sender_handle, did: m.sender_did });
            }
            if (m.peer_did && m.peer_did !== auth?.did && !seen.has(m.peer_did)) {
                seen.set(m.peer_did, { handle: m.peer_handle || m.peer_did.slice(0, 12), did: m.peer_did });
            }
        }
        return [...seen.values()].sort((a, b) => a.handle.localeCompare(b.handle));
    }, [buddies, messages, auth?.did]);

    const displayBuddies = (buddies.length > 0 ? buddies : derivedBuddies).filter(b => {
        if (!buddyFilter.trim()) return true;
        return b.handle.toLowerCase().includes(buddyFilter.toLowerCase());
    });

    return (
        <div className="chat-discord-layout">
            {/* ═══ Left Column: Conversation List ═══ */}
            <div className="chat-convo-list">
                <div className="chat-convo-list-header">
                    <span className="chat-convo-list-title">Messages</span>
                </div>
                <div className="chat-convo-search">
                    <BsSearch size={10} />
                    <input
                        type="text"
                        placeholder="Filter…"
                        value={searchFilter}
                        onChange={e => setSearchFilter(e.target.value)}
                        className="chat-convo-search-input"
                    />
                </div>
                <div className="chat-convo-scroll">
                    {filteredConvoIds.length === 0 ? (
                        <div className="chat-convo-empty">No conversations found.</div>
                    ) : (
                        <>
                            {liveConvoIds.length > 0 && (
                                <div className="chat-convo-section-label">
                                    <BsCircleFill size={6} color="#22c55e" /> Live
                                </div>
                            )}
                            {liveConvoIds.map(cid => (
                                <ConvoItem
                                    key={cid}
                                    cid={cid}
                                    title={getConvoTitle(cid)}
                                    initial={getConvoInitial(cid)}
                                    lastMsg={byConvo[cid][0]}
                                    isActive={selectedConvoId === cid}
                                    isMine={byConvo[cid][0]?.sender_did === auth?.did}
                                    onClick={() => setSelectedConvoId(cid)}
                                />
                            ))}
                            {archivedConvoIds.length > 0 && (
                                <div className="chat-convo-section-label">📚 Archived</div>
                            )}
                            {archivedConvoIds.map(cid => (
                                <ConvoItem
                                    key={cid}
                                    cid={cid}
                                    title={getConvoTitle(cid)}
                                    initial={getConvoInitial(cid)}
                                    lastMsg={byConvo[cid][0]}
                                    isActive={selectedConvoId === cid}
                                    isMine={byConvo[cid][0]?.sender_did === auth?.did}
                                    onClick={() => setSelectedConvoId(cid)}
                                    archived
                                />
                            ))}
                        </>
                    )}
                </div>
            </div>

            {/* ═══ Center: Active Chat ═══ */}
            <div className={`chat-main-area ${selectedConvoId ? 'has-convo' : ''}`}>
                {selectedConvoId ? (
                    <>
                        <Win98Box variant="raised" className="chat-main-header">
                            <button className="chat-back-btn" onClick={() => setSelectedConvoId(null)}>←</button>
                            <span className="chat-main-title">{activeTitle}</span>
                            {!activeIsLive && <span className="chat-archived-badge">archived</span>}
                        </Win98Box>

                        <div className="chat-msg-scroll" ref={chatContainerRef} onScroll={handleChatScroll}>
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
                                        {showDate && <div className="chat-date-sep">{dateStr}</div>}
                                        <div className={`chat-msg ${isMe ? 'sent' : 'received'}`}>
                                            {!isMe && <span className="chat-msg-handle">{m.sender_handle}</span>}
                                            <span className="chat-msg-text">{m.text}</span>
                                            <span className="chat-msg-time">
                                                {date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                                            </span>
                                        </div>
                                    </React.Fragment>
                                );
                            })}
                            <div ref={messagesEndRef} />
                            {showScrollBtn && (
                                <button className="chat-scroll-to-bottom" onClick={() => {
                                    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
                                    setShowScrollBtn(false);
                                    isAtBottomRef.current = true;
                                }}>
                                    <BsArrowDown size={12} /> New Messages
                                </button>
                            )}
                        </div>

                        {activeIsLive ? (
                            <div className="chat-input-bar">
                                <input
                                    type="text"
                                    className="chat-input-field"
                                    placeholder="Type a message…"
                                    value={messageInput}
                                    onChange={e => setMessageInput(e.target.value)}
                                    onKeyDown={e => e.key === 'Enter' && handleSend()}
                                    disabled={isSending}
                                />
                                <button className="chat-send-btn" onClick={handleSend} disabled={!messageInput.trim() || isSending}>
                                    {isSending ? '…' : 'Send'}
                                </button>
                            </div>
                        ) : (
                            <div className="chat-input-bar chat-readonly">
                                📚 Archived conversation — read only
                            </div>
                        )}
                    </>
                ) : (
                    <div className="chat-empty-state">
                        <div className="chat-empty-icon">💬</div>
                        <div className="chat-empty-title">Select a conversation</div>
                        <div className="chat-empty-hint">Pick a conversation from the list, or browse the buddy list →</div>
                    </div>
                )}
            </div>

            {/* ═══ Right Column: Buddy List (AIM-style) ═══ */}
            <Win98Box variant="sunken" className="chat-buddy-list">
                <div className="chat-buddy-header">
                    <span>Buddy List</span>
                    <span className="chat-buddy-count">{displayBuddies.length}</span>
                </div>
                <div className="chat-buddy-search">
                    <input
                        type="text"
                        placeholder="Find buddy…"
                        value={buddyFilter}
                        onChange={e => setBuddyFilter(e.target.value)}
                        className="chat-buddy-search-input"
                    />
                </div>
                <div className="chat-buddy-scroll">
                    {displayBuddies.length === 0 ? (
                        <div className="chat-buddy-empty">
                            {buddyFilter ? 'No matches' : 'No buddies yet'}
                        </div>
                    ) : (
                        displayBuddies.map(buddy => (
                            <div key={buddy.did} className="chat-buddy-item" title={buddy.handle}>
                                <div className="chat-buddy-avatar">
                                    {buddy.avatar ? <img src={buddy.avatar} alt="" /> : <BsPersonFill size={14} />}
                                </div>
                                <span className="chat-buddy-name">{buddy.handle}</span>
                            </div>
                        ))
                    )}
                </div>
            </Win98Box>
        </div>
    );
}

/* ── Conversation List Item ── */
function ConvoItem({
    title, initial, lastMsg, isActive, isMine, onClick, archived
}: {
    cid: string;
    title: string;
    initial: string;
    lastMsg: VaultMessage;
    isActive: boolean;
    isMine: boolean;
    onClick: () => void;
    archived?: boolean;
}) {
    const time = new Date(lastMsg.sent_at);
    const timeStr = formatDate(time.getTime() / 1000).split(',')[0];

    return (
        <div className={`chat-convo-item ${isActive ? 'active' : ''} ${archived ? 'archived' : ''}`} onClick={onClick}>
            <div className="chat-convo-avatar">{initial}</div>
            <div className="chat-convo-info">
                <div className="chat-convo-name">{title}</div>
                <div className="chat-convo-preview">{isMine ? 'You: ' : ''}{lastMsg.text}</div>
            </div>
            <span className="chat-convo-time">{timeStr}</span>
        </div>
    );
}
