import React, { useRef, useState, useEffect, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useChatStore, useAuthStore } from '../store';
import { openProfile } from '../utils/openInApp';
import { VaultMessage, P2pConversation } from '../types';
import { formatDate } from '../utils/date';
import { Win98Box } from '../components/Win98Box';
import { BsPersonFill, BsSearch, BsArrowDown, BsChatDotsFill } from 'react-icons/bs';
import './ChatScreen.css';

/* ────────────────────────────────────────────────────────
   ChatScreen — Tabbed chat with Bluesky / P2P / Archives
   Layout: [Tabs + Convo List 240px] [Chat Area flex-1] [Buddy List 200px]
   ──────────────────────────────────────────────────────── */

type ChatTab = 'live' | 'p2p' | 'archives';

const P2P_PREFIX = 'p2p:';
const LIVE_SOURCES = new Set(['bluesky', 'activitypub', 'mastodon']);

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
        pendingConvoMeta, setPendingConvoMeta,
        messageInput, setMessageInput,
        isSending, setIsSending,
    } = useChatStore();

    const [searchFilter, setSearchFilter] = useState('');
    const [buddyFilter, setBuddyFilter] = useState('');
    const [showScrollBtn, setShowScrollBtn] = useState(false);
    const [buddies, setBuddies] = useState<BuddyInfo[]>([]);
    const [p2pConvos, setP2pConvos] = useState<P2pConversation[]>([]);
    const [activeTab, setActiveTab] = useState<ChatTab>('live');
    const [messagesLoading, setMessagesLoading] = useState(true);

    const messagesEndRef = useRef<HTMLDivElement>(null);
    const chatContainerRef = useRef<HTMLDivElement>(null);
    const isAtBottomRef = useRef(true);

    // ── Load messages + buddies + P2P convos on mount ──
    useEffect(() => {
        loadMessages();
        loadBuddies();
        loadP2pConvos();
    }, []);

    // ── Listen for incoming P2P messages ──
    useEffect(() => {
        const unlisten = listen<{ convo_id: string; message: any }>('p2p-message', () => {
            loadMessages();
        });
        return () => { unlisten.then(fn => fn()); };
    }, []);

    // ── Poll for new messages ──
    useEffect(() => {
        if (!auth) return;
        const poll = async () => {
            try {
                const result = await invoke<any>('import_vault_messages');
                if (result?.error === 'chat_scope_missing') {
                    console.warn('ChatScreen: Chat scope missing — re-login required');
                }
                await loadMessages();
            } catch (e) {
                console.error('ChatScreen: import_vault_messages failed:', e);
            }
        };
        poll();
        const interval = setInterval(poll, 30000);
        return () => clearInterval(interval);
    }, [auth]);

    async function loadMessages() {
        try {
            setMessagesLoading(true);
            const msgs = await invoke<VaultMessage[]>('list_vault_messages');
            setMessages(msgs);
        } catch (e) {
            console.error('Failed to load messages:', e);
        } finally {
            setMessagesLoading(false);
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

    async function loadP2pConvos() {
        try {
            const convos = await invoke<P2pConversation[]>('list_p2p_conversations');
            setP2pConvos(convos);
        } catch {
            setP2pConvos([]);
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

    // Classify conversations by tab:
    // P2P: convo_id starts with "p2p:" (at least one user is P2P-only)
    // Bluesky: source is "bluesky" and NOT p2p
    // Archives: everything else (Facebook, Instagram, etc.)
    const p2pConvoIds = filteredConvoIds.filter(cid => cid.startsWith(P2P_PREFIX));
    const liveConvoIds = filteredConvoIds.filter(cid => {
        if (cid.startsWith(P2P_PREFIX)) return false;
        const src = (byConvo[cid]?.[0]?.source || '').toLowerCase();
        return LIVE_SOURCES.has(src);
    });
    const archivedConvoIds = filteredConvoIds.filter(cid =>
        !cid.startsWith(P2P_PREFIX) && !liveConvoIds.includes(cid)
    );

    // P2P peers that have no messages yet — show them as empty convos
    const emptyP2pConvos = p2pConvos.filter(pc =>
        !p2pConvoIds.includes(pc.convo_id) &&
        (!searchFilter.trim() || pc.display_name.toLowerCase().includes(searchFilter.toLowerCase()))
    );

    // Get the convo list for the active tab
    const tabConvoIds = activeTab === 'p2p' ? p2pConvoIds
        : activeTab === 'live' ? liveConvoIds
        : archivedConvoIds;

    // Tab counts for badges
    const tabCounts: Record<ChatTab, number> = {
        live: liveConvoIds.length,
        p2p: p2pConvoIds.length + emptyP2pConvos.length,
        archives: archivedConvoIds.length,
    };

    const activeConvoMsgs = selectedConvoId ? byConvo[selectedConvoId] : null;
    const activeIsP2p = selectedConvoId?.startsWith(P2P_PREFIX) ?? false;
    const activeSource = (activeConvoMsgs?.[0]?.source || '').toLowerCase();
    // A convo is "live" (can send messages) if it's P2P, Bluesky, or a newly created Bluesky convo with no messages yet
    const isNewConvo = !!(selectedConvoId && !activeIsP2p && !activeConvoMsgs && pendingConvoMeta);
    const activeIsLive = activeIsP2p || LIVE_SOURCES.has(activeSource) || isNewConvo;

    // Derive title from participants
    let activeTitle = 'Chat';
    if (activeIsP2p && selectedConvoId) {
        const nodeId = selectedConvoId.slice(P2P_PREFIX.length);
        const p2pMatch = p2pConvos.find(pc => pc.node_id === nodeId);
        activeTitle = p2pMatch?.display_name || nodeId.slice(0, 12) + '…';
    } else if (activeConvoMsgs && activeConvoMsgs.length > 0) {
        const senderHandles = [...new Set(activeConvoMsgs.map(m => m.sender_handle))].filter(h => h !== auth?.handle);
        const peerHandles = [...new Set(activeConvoMsgs.map(m => m.peer_handle).filter(Boolean))].filter(h => h !== auth?.handle);
        const allHandles = [...new Set([...senderHandles, ...peerHandles])];
        activeTitle = allHandles.length > 0 ? allHandles.join(', ') : 'Just You';
    } else if (isNewConvo && pendingConvoMeta) {
        activeTitle = pendingConvoMeta.handle;
    }

    // Auto-switch to Bluesky tab when a Bluesky convo is selected (new or existing)
    useEffect(() => {
        if (!selectedConvoId || activeIsP2p) return;
        if (isNewConvo || LIVE_SOURCES.has(activeSource)) {
            setActiveTab('live');
        }
    }, [selectedConvoId]);

    // Clear pendingConvoMeta once real messages arrive for that convo
    useEffect(() => {
        if (pendingConvoMeta && selectedConvoId && byConvo[selectedConvoId]) {
            setPendingConvoMeta(null);
        }
    }, [messages, selectedConvoId, pendingConvoMeta]);

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

            if (selectedConvoId.startsWith(P2P_PREFIX)) {
                // P2P message — send via Bytescape protocol
                const nodeId = selectedConvoId.slice(P2P_PREFIX.length);
                const optimisticMsg: VaultMessage = {
                    id: `temp-${now}`,
                    convo_id: selectedConvoId,
                    sender_did: auth?.did || '',
                    sender_handle: auth?.handle || auth?.local_username || 'Me',
                    text,
                    sent_at: new Date().toISOString(),
                    source: 'p2p',
                    imported_at: now,
                };
                setMessages(prev => [optimisticMsg, ...prev]);

                const result = await invoke<{ id: string; delivered: boolean }>('send_p2p_chat_message', {
                    nodeId,
                    text,
                });
                if (!result.delivered) {
                    console.warn('P2P message saved locally but peer was unreachable');
                }
                // Reload to replace optimistic message with the persisted one
                await loadMessages();
            } else {
                // Live protocol message (Bluesky, ActivityPub, etc)
                const msgSource = activeSource || pendingConvoMeta?.protocol || 'bluesky';
                let realConvoId = selectedConvoId;
                if (selectedConvoId.startsWith('pending_bsky_')) {
                    const targetDid = pendingConvoMeta?.did || selectedConvoId.slice('pending_bsky_'.length);
                    realConvoId = await invoke<string>('get_or_create_bsky_convo', { memberDid: targetDid });
                    useChatStore.getState().setSelectedConvoId(realConvoId);
                }

                const optimisticMsg: VaultMessage = {
                    id: `temp-${now}`,
                    convo_id: selectedConvoId,
                    sender_did: auth?.did || '',
                    sender_handle: auth?.handle || '',
                    text,
                    sent_at: new Date().toISOString(),
                    source: msgSource,
                    imported_at: now,
                };
                setMessages(prev => [optimisticMsg, ...prev]);

                try {
                    await invoke('send_message', {
                        protocol: msgSource,
                        convoId: realConvoId,
                        text: text,
                    });
                } catch (err) {
                    // Fallback for Bluesky until unified command is deployed
                    if (msgSource === 'bluesky') {
                        await invoke<any>('send_bsky_dm', {
                            convoId: realConvoId,
                            recipientDid: null,
                            content: text,
                        });
                    } else {
                        console.error('Unified send_message failed or unavailable for protocol:', msgSource, err);
                        throw err;
                    }
                }
            }
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
            // Resolve best available handle: sender_handle, peer_handle (if same DID), or truncated DID
            if (m.sender_did && m.sender_did !== auth?.did) {
                const existing = seen.get(m.sender_did);
                const bestHandle = m.sender_handle || m.peer_handle || existing?.handle || m.sender_did.slice(0, 12);
                if (!existing || !existing.handle || existing.handle === existing.did?.slice(0, 12)) {
                    seen.set(m.sender_did, { handle: bestHandle, did: m.sender_did });
                }
            }
            if (m.peer_did && m.peer_did !== auth?.did) {
                const existing = seen.get(m.peer_did);
                const bestHandle = m.peer_handle || m.sender_handle || existing?.handle || m.peer_did.slice(0, 12);
                if (!existing || !existing.handle || existing.handle === existing.did?.slice(0, 12)) {
                    seen.set(m.peer_did, { handle: bestHandle, did: m.peer_did });
                }
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
            {/* ═══ Left Column: Tabs + Conversation List ═══ */}
            <div className="chat-convo-list">
                <div className="chat-convo-list-header">
                    <span className="chat-convo-list-title">Messages</span>
                </div>

                {/* ── Tab Bar ── */}
                <div className="chat-tab-bar">
                    {([
                        { key: 'live' as ChatTab, label: '💬 Messages', color: '#1185fe' },
                        { key: 'p2p' as ChatTab,     label: '🔗 P2P',     color: '#a78bfa' },
                        { key: 'archives' as ChatTab, label: '📚 Archives', color: '#f59e0b' },
                    ]).map(tab => (
                        <button
                            key={tab.key}
                            className={`chat-tab-btn ${activeTab === tab.key ? 'active' : ''}`}
                            onClick={() => setActiveTab(tab.key)}
                            style={activeTab === tab.key ? { borderBottomColor: tab.color } : undefined}
                        >
                            {tab.label}
                            {tabCounts[tab.key] > 0 && (
                                <span className="chat-tab-count">{tabCounts[tab.key]}</span>
                            )}
                        </button>
                    ))}
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
                    {messagesLoading && Object.keys(byConvo).length === 0 ? (
                        <div style={{ padding: 12 }}>
                            {Array.from({ length: 5 }).map((_, i) => (
                                <div key={i} style={{ 
                                    height: 50, 
                                    background: 'var(--bg-tertiary)', 
                                    marginBottom: 8, 
                                    borderRadius: 4, 
                                    opacity: 0.6 - (i * 0.1),
                                    animation: 'pulse 1.5s infinite' 
                                }} />
                            ))}
                        </div>
                    ) : tabConvoIds.length === 0 && (activeTab !== 'p2p' || emptyP2pConvos.length === 0) ? (
                        <div className="chat-convo-empty">
                            {activeTab === 'live' && 'No Bluesky conversations yet.'}
                            {activeTab === 'p2p' && 'No P2P conversations yet.'}
                            {activeTab === 'archives' && 'No archived conversations.'}
                        </div>
                    ) : (
                        <>
                            {/* P2P tab: show peers with messages + empty peers */}
                            {activeTab === 'p2p' && (
                                <>
                                    {p2pConvoIds.map(cid => {
                                        const nodeId = cid.slice(P2P_PREFIX.length);
                                        const p2pMatch = p2pConvos.find(pc => pc.node_id === nodeId);
                                        const title = p2pMatch?.display_name || getConvoTitle(cid);
                                        return (
                                            <ConvoItem
                                                key={cid}
                                                cid={cid}
                                                title={title}
                                                initial={title.charAt(0).toUpperCase()}
                                                lastMsg={byConvo[cid][0]}
                                                isActive={selectedConvoId === cid}
                                                isMine={byConvo[cid][0]?.sender_did === auth?.did}
                                                onClick={() => setSelectedConvoId(cid)}
                                                online={p2pMatch?.online}
                                            />
                                        );
                                    })}
                                    {emptyP2pConvos.map(pc => (
                                        <div
                                            key={pc.convo_id}
                                            className={`chat-convo-item ${selectedConvoId === pc.convo_id ? 'active' : ''}`}
                                            onClick={() => setSelectedConvoId(pc.convo_id)}
                                        >
                                            <div className="chat-convo-avatar">{pc.display_name.charAt(0).toUpperCase()}</div>
                                            <div className="chat-convo-info">
                                                <div className="chat-convo-name">
                                                    {pc.display_name}
                                                    {pc.online && <span style={{ marginLeft: 6, color: '#50fa7b', fontSize: '0.7rem' }}>●</span>}
                                                </div>
                                                <div className="chat-convo-preview" style={{ fontStyle: 'italic', opacity: 0.5 }}>No messages yet</div>
                                            </div>
                                        </div>
                                    ))}
                                </>
                            )}

                            {/* Bluesky tab */}
                            {activeTab === 'live' && liveConvoIds.map(cid => (
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

                            {/* Archives tab */}
                            {activeTab === 'archives' && archivedConvoIds.map(cid => (
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
                            {activeIsP2p && <span className="chat-tab-badge" style={{ background: '#a78bfa', color: '#fff' }}>P2P</span>}
                            {!activeIsLive && !activeIsP2p && <span className="chat-tab-badge" style={{ background: '#f59e0b', color: '#fff' }}>Archive</span>}
                            {!activeIsP2p && (LIVE_SOURCES.has(activeSource) || isNewConvo) && (
                                <span className="chat-tab-badge" style={{ 
                                    background: activeSource === 'activitypub' || activeSource === 'mastodon' ? '#10b981' : '#1185fe', 
                                    color: '#fff' 
                                }}>
                                    {activeSource === 'activitypub' || activeSource === 'mastodon' ? '🌐 Fediverse' : '🦋 Bluesky'}
                                </span>
                            )}
                        </Win98Box>

                        <div className="chat-msg-scroll" ref={chatContainerRef} onScroll={handleChatScroll}>
                            {(!activeConvoMsgs || activeConvoMsgs.length === 0) && activeIsP2p && (
                                <div style={{ textAlign: 'center', padding: '40px 16px', color: 'var(--text-muted)' }}>
                                    <div style={{ fontSize: '2rem', marginBottom: 8 }}>🔗</div>
                                    <div>Start a P2P conversation</div>
                                    <div style={{ fontSize: '0.8rem', marginTop: 4, opacity: 0.7 }}>
                                        Messages are sent directly over the Iroh network — no servers involved.
                                    </div>
                                </div>
                            )}
                            {(!activeConvoMsgs || activeConvoMsgs.length === 0) && isNewConvo && (
                                <div style={{ textAlign: 'center', padding: '40px 16px', color: 'var(--text-muted)' }}>
                                    <div style={{ fontSize: '2rem', marginBottom: 8 }}>
                                        {pendingConvoMeta?.protocol === 'activitypub' || pendingConvoMeta?.protocol === 'mastodon' ? '🌐' : '🦋'}
                                    </div>
                                    <div>New conversation with {pendingConvoMeta?.handle}</div>
                                    <div style={{ fontSize: '0.8rem', marginTop: 4, opacity: 0.7 }}>
                                        Send a message to start chatting via {pendingConvoMeta?.protocol === 'activitypub' || pendingConvoMeta?.protocol === 'mastodon' ? 'Fediverse' : 'Bluesky'} DM.
                                    </div>
                                </div>
                            )}
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
                        <div className="chat-empty-hint">Pick a conversation from the list, or browse your friends →</div>
                    </div>
                )}
            </div>

            {/* ═══ Right Column: Friends List ═══ */}
            <div className="friends-panel chat-friends-panel">
                <div className="friends-panel-header">
                    <span className="friends-panel-title">Friends</span>
                    <span className="friends-panel-count">{displayBuddies.length}</span>
                </div>
                <div className="chat-buddy-search">
                    <input
                        type="text"
                        placeholder="Find friend…"
                        value={buddyFilter}
                        onChange={e => setBuddyFilter(e.target.value)}
                        className="chat-buddy-search-input"
                    />
                </div>
                <div className="friends-panel-list">
                    {displayBuddies.length === 0 ? (
                        <div className="friends-panel-empty">
                            <div style={{ fontSize: '1.4rem', marginBottom: 6 } as React.CSSProperties}>👤</div>
                            <div style={{ fontWeight: 700 } as React.CSSProperties}>
                                {buddyFilter ? 'No matches' : 'No friends yet'}
                            </div>
                        </div>
                    ) : (
                        <div className="friends-panel-group">
                            <div className="friends-panel-group-label" style={{ display: 'flex', alignItems: 'center' }}>
                                <span className="friends-group-source-icon" style={{ marginRight: 6 }}>👥</span>
                                Contacts ({displayBuddies.length})
                            </div>
                            {displayBuddies.map(buddy => {
                                const initial = (buddy.handle || '?')[0]?.toUpperCase() || '?';
                                return (
                                    <div key={buddy.did} className="friend-item friend-item--bluesky">
                                        <button
                                            className="friend-item-main"
                                            title={`@${buddy.handle}`}
                                            style={{ cursor: 'default' } as React.CSSProperties}
                                        >
                                            <div className="friend-item-avatar">
                                                {buddy.avatar ? (
                                                    <img src={buddy.avatar} alt="" className="friend-item-avatar-img" />
                                                ) : (
                                                    <span className="friend-item-initial">{initial}</span>
                                                )}
                                                <span
                                                    className="friend-item-dot cloud"
                                                    style={{ background: '#1185fe' } as React.CSSProperties}
                                                />
                                            </div>
                                            <div className="friend-item-info">
                                                <div className="friend-item-name">{buddy.handle}</div>
                                                <div className="friend-item-away cloud">@{buddy.handle}</div>
                                            </div>
                                        </button>
                                        <div className="friend-item-actions">
                                            <button
                                                className="friend-item-action-btn"
                                                onClick={() => {
                                                    if (buddy.did.startsWith('mastodon:') || buddy.did.startsWith('acct:')) {
                                                        alert('Mastodon DMs coming soon!');
                                                        return;
                                                    }
                                                    // Find existing convo in local state — instant, no network call
                                                    const existingConvoId = Object.keys(byConvo).find(cid => {
                                                        const msgs = byConvo[cid];
                                                        return msgs.some(m =>
                                                            m.peer_did === buddy.did || m.sender_did === buddy.did
                                                        );
                                                    });
                                                    if (existingConvoId) {
                                                        setSelectedConvoId(existingConvoId);
                                                    } else {
                                                        // Optimistic pending convo — real convo created on first send
                                                        setSelectedConvoId(`pending_bsky_${buddy.did}`);
                                                        setPendingConvoMeta({ did: buddy.did, handle: buddy.handle });
                                                    }
                                                }}
                                                title={`Message ${buddy.handle}`}
                                            >
                                                <BsChatDotsFill size={11} />
                                            </button>
                                            <button
                                                className="friend-item-action-btn"
                                                onClick={() => {
                                                    openProfile(buddy.handle);
                                                }}
                                                title={`${buddy.handle}'s profile`}
                                            >
                                                <BsPersonFill size={11} />
                                            </button>
                                        </div>
                                    </div>
                                );
                            })}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}

/* ── Conversation List Item ── */
function ConvoItem({
    title, initial, lastMsg, isActive, isMine, onClick, archived, online
}: {
    cid: string;
    title: string;
    initial: string;
    lastMsg: VaultMessage;
    isActive: boolean;
    isMine: boolean;
    onClick: () => void;
    archived?: boolean;
    online?: boolean;
}) {
    const time = new Date(lastMsg.sent_at);
    const timeStr = formatDate(time.getTime() / 1000).split(',')[0];

    return (
        <div className={`chat-convo-item ${isActive ? 'active' : ''} ${archived ? 'archived' : ''}`} onClick={onClick}>
            <div className="chat-convo-avatar">{initial}</div>
            <div className="chat-convo-info">
                <div className="chat-convo-name">
                    {title}
                    {online && <span style={{ marginLeft: 6, color: '#50fa7b', fontSize: '0.7rem' }}>●</span>}
                </div>
                <div className="chat-convo-preview">{isMine ? 'You: ' : ''}{lastMsg.text}</div>
            </div>
            <span className="chat-convo-time">{timeStr}</span>
        </div>
    );
}
