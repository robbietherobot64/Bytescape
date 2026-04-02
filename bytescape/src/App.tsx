/**
 * App.tsx — Main Application Shell
 *
 * The root component that bootstraps the Bytescape application. Manages
 * authentication, P2P connectivity, archive imports, navigation, and
 * renders all 17 screens within a tabbed layout.
 *
 * HOW IT WORKS:
 * 1. On mount, initializes the Iroh P2P node and checks for an existing
 *    auth session (Bluesky OAuth or local). If authenticated, auto-imports
 *    the latest Bluesky feed and chat messages.
 * 2. Registers global event listeners for parse progress (Facebook archives),
 *    import progress (Bluesky), and P2P peer connections. A background
 *    keepalive pings all known peers every 90s.
 * 3. Sidebar navigation maps to screen tabs via a tabbed pane system — all
 *    open tabs render simultaneously (inactive ones use display:none) so
 *    React state is preserved across tab switches.
 * 4. Facebook archive import follows a two-phase flow: parse → audit → shred
 *    (commit_memories). Bluesky import runs feed + chat in parallel.
 * 5. An ErrorBoundary wraps all screen content to catch rendering failures
 *    without crashing the entire app shell.
 *
 * RELATED FILES:
 * - store/index.ts          — Zustand stores (auth, app, nav, chat, peer)
 * - components/AppLayout    — Shell layout with sidebar, top bar, tab strip
 * - screens/*               — Individual screen components rendered by renderTabScreen
 * - types/index.ts          — Shared TypeScript interfaces (AuthStatus, ParseResult, etc.)
 * - hooks/useTheme.ts       — Theme switching (sets data-theme on <html>)
 * - src-tauri/src/lib.rs    — Tauri command definitions invoked via IPC
 */

// ═══════════════════════════════════════════════════════════════
// IMPORTS
// ═══════════════════════════════════════════════════════════════
import React, { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useAuthStore, useAppStore, useNavStore, useChatStore, usePeerStore, TabItem, Screen } from "./store";

import "./themes/geocities.css";
import "./themes/win98.css";
import "./themes/midnight.css";
import "./themes/hacker.css";
import "./App.css";

// ═══════════════════════════════════════════════════════════════
// ERROR BOUNDARY
// ═══════════════════════════════════════════════════════════════

/**
 * Local error boundary for screen-level rendering failures.
 * Keeps the app shell responsive while exposing stack traces in development.
 */
class ErrorBoundary extends React.Component<{ fallback: React.ReactNode, children: React.ReactNode }, { hasError: boolean, error: Error | null }> {
  constructor(props: any) {
    super(props);
    this.state = { hasError: false, error: null };
  }
  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }
  render() {
    if (this.state.hasError) {
      return (
        <div style={{ color: '#ff6b6b', padding: 20, background: '#222', borderRadius: 8, margin: 20 }}>
          <h2>Component Crash</h2>
          <pre style={{ whiteSpace: 'pre-wrap', overflowX: 'auto' }}>{this.state.error?.stack || this.state.error?.message}</pre>
        </div>
      );
    }
    return this.props.children;
  }
}

// ─── Types ───────────────────────────────────────────────────
import {
  AuthStatus,
  ParseProgress,
  ParseResult,
  ImportProgress,
  SidebarItem,
  VaultFileStats
} from "./types";

// ─── Components ──────────────────────────────────────────────
import { AppLayout } from "./components/AppLayout";
import { LoginScreen } from "./screens/LoginScreen";
import { FeedScreen } from "./screens/FeedScreen";
import { ImportScreen } from "./screens/ImportScreen";
import { AuditScreen } from "./screens/AuditScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { FriendsScreen } from "./screens/FriendsScreen";
import { ProfileScreen } from "./screens/ProfileScreen";
import { ProfileViewScreen } from "./screens/ProfileViewScreen";
import { ProfileBuilderScreen } from "./screens/ProfileBuilderScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { VaultScreen } from "./screens/VaultScreen";
import { PeerProfileScreen } from "./screens/PeerProfileScreen";
import { ProfileBrowserScreen } from "./screens/ProfileBrowserScreen";
import { WebViewScreen } from "./screens/WebViewScreen";
import { ForumScreen } from "./screens/ForumScreen";
import { GroupScreen } from "./screens/GroupScreen";
import { CreateGroupModal } from "./components/CreateGroupModal";
import { IdentitySetupModal } from "./components/IdentitySetupModal";
import { RecoverySetupModal } from "./components/RecoverySetupModal";
import { useTheme } from "./hooks/useTheme";
import { useLocationStore } from "./hooks/useLocation";
import { installLinkInterceptor } from "./utils/openInApp";

/**
 * Root application component.
 *
 * Orchestrates authentication, P2P networking, data imports (Bluesky +
 * Facebook/Instagram), sidebar navigation, and tabbed screen rendering.
 * All global event listeners and background tasks (keepalive, auto-import)
 * are registered here so they persist regardless of which screen is active.
 *
 * @IPC setup_iroh_node, migrate_willow_paths — called during initialization
 * @IPC check_auth_status, logout — authentication lifecycle
 * @IPC import_bluesky_posts, import_vault_messages — Bluesky data sync
 * @IPC import_archive_auto, commit_memories — Facebook/Instagram archive import
 * @IPC list_peers, check_peer_connectivity — P2P keepalive polling
 * @IPC vault_get_stats, count_archive_posts — archive count for UI badges
 */
function App() {
  // ═══════════════════════════════════════════════════════════
  // STORES & STATE
  // ═══════════════════════════════════════════════════════════

  /** Applies the active theme's data-theme attribute on <html> */
  useTheme();

  // ─── Stores ───
  const { auth, setAuth } = useAuthStore();
  const {
    globalError: error, setGlobalError: setError,
    importProgress: importStatusRaw, setImportProgress: setImportStatus,
    archiveCount, setArchiveCount
  } = useAppStore();
  const {
    screen, setScreen,
    activeSidebar, setActiveSidebar,
    feedActiveTab, setFeedActiveTab,
    peerProfileNodeId, setPeerProfileNodeId,
    peerProfileName, setPeerProfileName,
    openTab,
    navigateInTab,
    tabs, activeTabId
  } = useNavStore();
  const chatStore = useChatStore();

  // ─── B10.1 / B10.2: Sync NavStore → Location URL ───
  // Whenever screen, feedTab, or peer context changes, update the byte:// URL
  // and push onto the navigation history stack.
  useEffect(() => {
    useLocationStore.getState().syncFromNavStore();
  }, [screen, feedActiveTab, peerProfileNodeId, activeTabId]);

  // ─── Global link interceptor: route all <a target="_blank"> to in-app tabs ───
  useEffect(() => {
    return installLinkInterceptor();
  }, []);

  const importStatus = importStatusRaw || "";
  const [loading, setLoading] = useState(true);
  const [showIdentitySetup, setShowIdentitySetup] = useState(false);
  const [showRecoverySetup, setShowRecoverySetup] = useState(false);
  const [showCreateGroup, setShowCreateGroup] = useState(false);

  // Import / Audit
  const [progress, setProgress] = useState<ParseProgress | null>(null);
  const [parseResult, setParseResult] = useState<ParseResult | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [isShredding, setIsShredding] = useState(false);
  const [importComplete, setImportComplete] = useState(false);
  const [selectedArchivePath, setSelectedArchivePath] = useState<string | null>(null);

  // Feed / Archive
  const [isImporting, setIsImporting] = useState(false);
  const importGuardRef = React.useRef(false);

  // Search state (top bar)
  // NOTE: searchQuery is captured but not wired into feed filtering yet.
  // handleSearchSubmit only logs to console — see "Future:" comment below.
  const [searchQuery, setSearchQuery] = useState('');

  const handleSearchSubmit = (query: string) => {
    // Navigate to feed screen when searching
    setScreen('feed');
    setActiveSidebar('feed');
    // Future: wire into feed filtering
    console.log('Search submitted:', query);
  };

  // ═══════════════════════════════════════════════════════════
  // EVENT LISTENERS & INITIALIZATION
  // ═══════════════════════════════════════════════════════════

  /** Bootstrap effect — runs once on mount to initialize backend services. */
  useEffect(() => {
    initApp();
  }, []);

  /**
   * Initializes backend services required by feed, vault, and sync features.
   *
   * Sequence: setup Iroh P2P node → run Willow path migration → check auth.
   * If any step fails, subsequent steps still attempt to run so the app
   * degrades gracefully (e.g., offline-only mode without P2P).
   *
   * @IPC setup_iroh_node — starts the Iroh networking node
   * @IPC migrate_willow_paths — one-time migration from legacy path format
   */
  async function initApp() {
    // Initialize Iroh node first — vault, blobs, and P2P all depend on it
    try {
      await invoke<string>('setup_iroh_node');
      console.log('Iroh node initialized');
    } catch (e) {
      console.error('Failed to initialize Iroh node:', e);
    }

    try {
      const migrated = await invoke<{ scanned: number; migrated: number }>('migrate_willow_paths');
      console.log('Willow migration:', migrated);
    } catch (e) {
      console.warn('Willow migration skipped:', e);
    }

    await checkAuth();
  }

  // Listen for parse progress events (Facebook)
  useEffect(() => {
    const unlisten = listen<ParseProgress>("parse-progress", (event) => {
      setProgress(event.payload);
      addLog(`[${event.payload.stage}] ${event.payload.current_file}`);
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // Listen for import progress events (Bluesky, etc)
  useEffect(() => {
    const unlisten = listen<ImportProgress>("import-progress", (event) => {
      setImportStatus(`${event.payload.current_item} (${event.payload.posts_imported} posts)`);
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // ─── Global P2P peer-connected listener ────────────────────
  // Lives at App level so it works regardless of which screen is active.
  // When a remote peer connects TO us, the backend emits "peer-connected".
  useEffect(() => {
    const { setPeerStatus } = usePeerStore.getState();
    const unlisten = listen<{ node_id: string; username?: string; online?: boolean }>(
      'peer-connected',
      (event) => {
        const { node_id } = event.payload;
        console.log('[P2P] Global: inbound peer connection from', node_id);
        setPeerStatus(node_id, 'online');
      }
    );
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // ─── Background P2P keepalive ──────────────────────────────
  // Every 90 seconds, check connectivity to all known peers so the status
  // stays fresh even when the user isn't on the Social screen.
  // NOTE: Peers are pinged sequentially (await inside for-loop). With many
  // peers this could take a while; consider Promise.all for parallelism.
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const peers = await invoke<import('./types').PeerInfo[]>('list_peers');
        const { setPeerStatus } = usePeerStore.getState();
        for (const peer of peers) {
          try {
            const result = await invoke<import('./types').PeerConnectResult>(
              'check_peer_connectivity',
              { nodeId: peer.node_id }
            );
            setPeerStatus(peer.node_id, result.online ? 'online' : 'offline');
          } catch {
            setPeerStatus(peer.node_id, 'offline');
          }
        }
      } catch {
        // list_peers failed — node not initialized yet, skip
      }
    }, 90_000);
    return () => clearInterval(interval);
  }, []);

  const addLog = useCallback((msg: string) => {
    setLogs((prev) => [...prev, msg]);
  }, []);

  // ═══════════════════════════════════════════════════════════
  // AUTH
  // ═══════════════════════════════════════════════════════════

  /**
   * Checks for an existing authenticated session on startup.
   *
   * If a valid session exists, opens the Feed tab, loads archive stats,
   * and triggers identity setup if the user hasn’t chosen a username.
   * For Bluesky sessions, also kicks off an auto-import of the latest feed.
   *
   * @IPC check_auth_status — returns the current AuthStatus or null
   */
  async function checkAuth() {
    try {
      const status = await invoke<AuthStatus | null>("check_auth_status");
      if (status) {
        setAuth(status);
        // Create initial Feed tab on auto-login (same as handleLoginSuccess)
        openTab("feed", "Feed");
        setActiveSidebar("feed");
        await loadArchiveStats();
        // Show identity setup if the user hasn't chosen a username yet
        if (!status.identity_ready) {
          setShowIdentitySetup(true);
        }
        // Auto-sync Bluesky feed on every boot (only if user has a Bluesky session)
        if (status.source === 'bluesky') {
          importBluesky();
        }
      }
    } catch { /* not logged in */ }
    setLoading(false);
  }

  /**
   * Callback passed to LoginScreen after successful authentication.
   *
   * Sets up the initial app state (Feed tab, archive stats, identity check)
   * and optionally triggers a Bluesky import if the user chose to sync.
   */
  const handleLoginSuccess = async (status: AuthStatus, shouldImport: boolean) => {
    setAuth(status);
    // Create the initial Feed tab after login
    openTab("feed", "Feed");
    setActiveSidebar("feed");
    await loadArchiveStats();

    // Show identity setup if the user hasn't chosen a username yet
    if (!status.identity_ready) {
      setShowIdentitySetup(true);
    }

    if (shouldImport) {
      importBluesky();
    }
  };

  /**
   * Signs the user out and resets all application state.
   *
   * Clears auth, navigation tabs, chat messages, archive data, and
   * import progress. Navigates back to the login screen.
   *
   * @IPC logout — destroys the backend session
   */
  async function handleLogout() {
    try {
      await invoke("logout");
    } catch { /* ignore */ }
    setAuth(null);
    setScreen("login");
    // Clear all tabs on logout
    useNavStore.setState({ tabs: [], activeTabId: null });
    setArchiveCount(0);
    chatStore.setMessages([]);
    chatStore.setSelectedConvoId(null);
    chatStore.setMessageInput("");
    setParseResult(null);
    setProgress(null);
    setImportStatus("");
    setError("");
  }

  function handleIdentitySetupComplete(updatedAuth: AuthStatus) {
    setAuth(updatedAuth);
    setShowIdentitySetup(false);
    // After username is set, prompt user to create a real recovery code
    setShowRecoverySetup(true);
  }

  function handleRecoverySetupComplete() {
    setShowRecoverySetup(false);
  }

  // ═══════════════════════════════════════════════════════════
  // DATA LOADING
  // ═══════════════════════════════════════════════════════════

  /**
   * Loads archive summary counts for UI badges and status text.
   *
   * Tries the new vault_get_stats command first; falls back to the legacy
   * count_archive_posts if the vault isn’t initialized yet.
   *
   * @IPC vault_get_stats — returns VaultFileStats with total_files
   * @IPC count_archive_posts — legacy fallback returning a plain count
   */
  async function loadArchiveStats() {
    try {
      const stats = await invoke<VaultFileStats>("vault_get_stats");
      setArchiveCount(stats.total_files);
    } catch (e) {
      // Fallback to legacy count if vault not initialized yet
      try {
        const count = await invoke<number>("count_archive_posts");
        setArchiveCount(count);
      } catch {
        console.error("Failed to load archive count:", e);
      }
    }
  }



  // ═══════════════════════════════════════════════════════════
  // BLUESKY IMPORT
  // ═══════════════════════════════════════════════════════════

  /**
   * Imports Bluesky feed posts and chat messages into the local vault.
   *
   * Runs feed import (import_bluesky_posts) and chat import
   * (import_vault_messages) in parallel. Uses a ref-based guard to
   * prevent concurrent invocations. After completion, reloads archive
   * stats so UI badges update immediately.
   *
   * @IPC import_bluesky_posts — downloads and stores new feed posts
   * @IPC import_vault_messages — syncs Bluesky DM conversations
   */
  async function importBluesky() {
    if (importGuardRef.current) return;
    importGuardRef.current = true;
    setIsImporting(true);
    setImportStatus("Connecting to Bluesky...");
    setError("");
    try {
      console.log("App: Starting Bluesky import + chat import in parallel...");

      // Launch chat import in parallel — don't block on feed import
      // (feed import can take minutes downloading media for many posts)
      const chatPromise = invoke<any>("import_vault_messages")
        .then((chatResult) => {
          console.log("App: Chat import result:", chatResult);
          if (chatResult?.error === "chat_scope_missing") {
            console.warn("App: Chat scope missing — user needs to re-login for chat support");
          }
        })
        .catch((chatErr: any) => {
          console.error("App: Chat import FAILED:", chatErr);
        });

      const count = await invoke<number>("import_bluesky_posts");
      console.log(`App: Import finished. Found ${count} new posts.`);

      if (count > 0) {
        setImportStatus(`✅ ${count} new posts imported!`);
      } else {
        setImportStatus(`✅ Up to date — no new posts.`);
      }

      // Wait for chat import to finish too (it likely already has)
      await chatPromise;

      // Ensure stats and feed are reloaded AFTER the import is confirmed finished
      console.log("App: Reloading archive stats and feed posts...");
      await loadArchiveStats();
      console.log("App: Refresh complete.");
    } catch (e: any) {
      const msg = typeof e === "string" ? e : e.message || "Import failed";
      console.error("App: Import failed:", e);
      setError(msg);
      setImportStatus("");
    } finally {
      setIsImporting(false);
      importGuardRef.current = false;
    }
  }



  // ═══════════════════════════════════════════════════════════
  // FACEBOOK / INSTAGRAM ARCHIVE IMPORT
  // ═══════════════════════════════════════════════════════════

  /**
   * Opens a file picker (or accepts a drop path) and parses a ZIP archive.
   *
   * Navigates to the Audit screen, logs the ZIP structure for diagnostics,
   * auto-detects the archive source (Facebook vs Instagram) from the filename,
   * and invokes the backend parser. Results are stored in parseResult state
   * for the AuditScreen to display.
   *
   * @IPC debug_zip_contents — lists files inside the ZIP for diagnostic logging
   * @IPC import_archive_auto — parses the archive and returns ParseResult
   */
  async function handleFilePick(pathOverride?: string) {
    try {
      const rawSelected = pathOverride || await open({
        multiple: false,
        filters: [{ name: "ZIP Archive", extensions: ["zip"] }],
      });
      // Ensure we have a valid string path (guard against non-string values)
      const selected = typeof rawSelected === 'string' ? rawSelected : null;
      if (!selected) return;

      // Navigate the current tab (Import) to audit — stays in the same tab.
      navigateInTab("audit", "Audit");
      setActiveSidebar("import");

      setProgress(null);
      setParseResult(null);
      setLogs([]);
      setIsShredding(false);
      setImportComplete(false);
      setSelectedArchivePath(selected);
      setError("");
      addLog("[start] Parsing archive...");

      // Capture archive structure in logs to aid parser diagnostics.
      try {
        const zipInfo = await invoke<string[]>("debug_zip_contents", { path: selected });
        addLog(`[debug] ZIP structure (${zipInfo.length} lines):`);
        // Show all directory structure, but limit JSON files to 50
        let jsonCount = 0;
        for (const line of zipInfo) {
          if (line.startsWith("  📄") && ++jsonCount > 50) {
            addLog(`  ... (truncated, ${zipInfo.length - jsonCount} more)`);
            break;
          }
          addLog(line);
        }
      } catch (e) {
        console.error("debug_zip_contents failed:", e);
      }

      const lowerSelected = selected.toLowerCase();
      const sourceHint = lowerSelected.includes("instagram")
        ? "instagram"
        : lowerSelected.includes("facebook")
          ? "facebook"
          : null;

      const result = await invoke<ParseResult>("import_archive_auto", {
        path: selected,
        sourceHint,
      });
      setParseResult(result);
      addLog(`[done] Found ${result.posts.length} posts, ${result.trackers.length} trackers`);
      addLog(`[source] Detected archive source: ${result.source || 'unknown'}`);
    } catch (e: any) {
      setError(typeof e === "string" ? e : "File selection failed");
    }
  }

  /**
   * Commits parsed archive data to the vault and imports archived conversations.
   *
   * This is the second phase of archive import (after parse + user review).
   * Persists posts via commit_memories, then imports archived chat messages
   * if the source is known. Updates archive stats on completion.
   *
   * @IPC commit_memories — persists staged posts to redb
   * @IPC import_archive_messages — imports chat threads from the archive
   */
  async function handleShred() {
    setIsShredding(true);
    addLog("[shred] Committing memories...");
    try {
      const saved = await invoke<number>("commit_memories");
      if (selectedArchivePath && parseResult?.source) {
        addLog("[chat] Importing archived conversations...");
        try {
          const chatResult = await invoke<{ new: number; threads: number; parsed: number }>("import_archive_messages", {
            path: selectedArchivePath,
            source: parseResult.source,
          });
          addLog(`[chat] Imported ${chatResult.new} new messages across ${chatResult.threads} threads (${chatResult.parsed} parsed)`);
        } catch (chatError: any) {
          addLog(`[chat:error] ${typeof chatError === "string" ? chatError : "Chat import failed"}`);
        }
      }
      addLog(`[complete] ✅ ${saved} new memories saved. Surveillance data shredded.`);
      setImportComplete(true);
      await loadArchiveStats();
    } catch (e: any) {
      addLog(`[error] ${typeof e === "string" ? e : "Shred failed"}`);
    }
    setIsShredding(false);
  }

  // ═══════════════════════════════════════════════════════════
  // NAVIGATION
  // ═══════════════════════════════════════════════════════════

  /**
   * Maps a sidebar item to the corresponding screen tab.
   *
   * Uses a lookup table to translate sidebar identifiers (e.g., 'feed',
   * 'chat', 'social') into screen/tab pairs, then opens or focuses that tab.
   */
  function handleNavigate(item: SidebarItem) {
    setActiveSidebar(item);
    const screenMap: Record<string, { screen: Parameters<typeof openTab>[0]; label?: string }> = {
      feed: { screen: 'feed', label: 'Feed' },
      search: { screen: 'feed', label: 'Feed' },
      forums: { screen: 'forum', label: 'Forums' },
      import: { screen: 'import', label: 'Import' },
      profile: { screen: 'profile', label: 'Profile' },
      chat: { screen: 'chat', label: 'Chat' },
      social: { screen: 'friends', label: 'Social' },
      settings: { screen: 'settings', label: 'Settings' },
      vault: { screen: 'vault', label: 'Vault' },
    };
    const target = screenMap[item];
    if (target) {
      openTab(target.screen, target.label);
    }
  }

  function handleOpenGroup(groupId: string) {
    openTab('group', 'Group', { groupId });
    setActiveSidebar('forums');
  }

  function handleGroupCreated(groupId: string) {
    setShowCreateGroup(false);
    handleOpenGroup(groupId);
  }

  // ═══════════════════════════════════════════════════════════
  // RENDER HELPERS
  // ═══════════════════════════════════════════════════════════

  /** Screens that need full-bleed layout (no padding, flex column, overflow hidden) */
  const FULL_BLEED_SCREENS: Set<Screen> = new Set([
    'chat', 'feed', 'vault', 'friends', 'peer-profile', 'profile-builder', 'profile-browser', 'webview', 'forum', 'group',
  ]);

  const fullBleedStyle: React.CSSProperties = {
    padding: 0,
    height: '100%',
    overflow: 'hidden',
    display: 'flex',
    flexDirection: 'column',
    maxWidth: 'none',
    width: '100%',
  };

  const standardStyle: React.CSSProperties = {
    height: '100%',
    overflowY: 'auto',
  };

  /**
   * Renders the appropriate screen component for a given tab.
   *
   * Each case maps a Screen identifier to its React component, wiring in
   * the necessary props (auth, callbacks, navigation handlers). All tabs
   * are rendered simultaneously — inactive ones are hidden via CSS so their
   * state is preserved across switches.
   *
   * NOTE: The onOpenChat callback is duplicated between FeedScreen and
   * FriendsScreen with slightly different implementations. Both resolve a
   * convo ID from the peer's DID/node-id and navigate to Chat. Consider
   * extracting a shared openChatWithPeer() helper.
   */
  function renderTabScreen(tab: TabItem) {
    switch (tab.screen) {
      case 'feed':
        return (
          <FeedScreen
            auth={auth}
            activeTab={feedActiveTab}
            setActiveTab={setFeedActiveTab}
            isImporting={isImporting}
            // NOTE: This onOpenChat logic is largely duplicated in FriendsScreen below.
            // Consider extracting a shared openChatWithPeer(peerNodeId, peerName) helper.
            onOpenChat={async (peerNodeId, peerName) => {
              const { setSelectedConvoId, setPendingConvoMeta, messages } = useChatStore.getState();
              
              if (peerNodeId.startsWith('did:plc:') || peerNodeId.startsWith('did:web:')) {
                try {
                  // Bypass backend command if possible by finding an existing chat in local state
                  const existingConvo = Array.from(messages.values()).find(
                    msg => msg.protocol === 'bluesky' && (msg.sender_did === peerNodeId || msg.recipient_did === peerNodeId || msg.convo_id?.includes(peerNodeId))
                  );
                  
                  let convoId;
                  if (existingConvo && existingConvo.convo_id) {
                     convoId = existingConvo.convo_id;
                  } else {
                     convoId = `pending_bsky_${peerNodeId}`;
                  }
                  
                  setSelectedConvoId(convoId);
                  setPendingConvoMeta({ did: peerNodeId, handle: peerName || peerNodeId, protocol: 'bluesky' });
                } catch (e) {
                  console.error('Failed to get/create convo:', e);
                }
              } else if (peerNodeId.startsWith('http') || (peerName && peerName.includes('@'))) {
                try {
                  const convoId = peerNodeId;
                  setSelectedConvoId(convoId);
                  setPendingConvoMeta({ did: peerNodeId, handle: peerName || peerNodeId, protocol: 'activitypub' });
                } catch (e) {
                  console.error('Failed to get/create AP convo:', e);
                }
              } else {
                setSelectedConvoId(`p2p:${peerNodeId}`);
                setPendingConvoMeta(null);
              }
              openTab("chat", "Chat");
              setActiveSidebar("chat");
            }}
            onViewProfile={(peerNodeId, peerName) => {
              setPeerProfileNodeId(peerNodeId);
              setPeerProfileName(peerName || null);
              openTab('peer-profile', peerName || 'Peer Profile', { nodeId: peerNodeId });
              setActiveSidebar("social");
            }}
          />
        );
      case 'import':
        return (
          <ImportScreen
            auth={auth}
            isImporting={isImporting}
            importStatus={importStatus}
            archiveCount={archiveCount}
            onImportBluesky={importBluesky}
            onFilePick={() => handleFilePick()}
            onFileDropPath={(path: string) => handleFilePick(path)}
            onNavigate={handleNavigate}
            error={error || ""}
          />
        );
      case 'audit':
        return (
          <AuditScreen
            progress={progress}
            parseResult={parseResult}
            logs={logs}
            isShredding={isShredding}
            importComplete={importComplete}
            error={error || ""}
            onShred={handleShred}
            onNavigate={handleNavigate}
          />
        );
      case 'profile':
        return <ProfileScreen auth={auth} archiveCount={archiveCount} />;
      case 'profile-view':
        return <ProfileViewScreen handle={tab.meta?.handle || ''} protocol={tab.meta?.protocol as "bluesky" | "activitypub" | "mastodon" | undefined} />;
      case 'profile-builder':
        return <ProfileBuilderScreen />;
      case 'profile-browser':
        return <ProfileBrowserScreen />;
      case 'chat':
        return <ChatScreen />;
      case 'friends':
        return (
          <FriendsScreen
            auth={auth}
            // NOTE: Duplicates FeedScreen's onOpenChat above with minor differences
            // (Mastodon guard, messages.find vs Array.from). Unify into shared helper.
            onOpenChat={(peerNodeId, peerName) => {
              const { setSelectedConvoId, setPendingConvoMeta, messages } = useChatStore.getState();

              if (peerNodeId.startsWith('mastodon:') || peerNodeId.startsWith('acct:')) {
                alert('Mastodon DMs coming soon!');
                return;
              }

              if (peerNodeId.startsWith('did:plc:') || peerNodeId.startsWith('did:web:')) {
                // Find existing convo in local message state — instant, no network call
                const existingConvo = messages.find(
                  msg => (msg.source === 'bluesky' || msg.protocol === 'bluesky') &&
                    (msg.peer_did === peerNodeId || msg.sender_did === peerNodeId || msg.recipient_did === peerNodeId)
                );
                const convoId = existingConvo?.convo_id ?? `pending_bsky_${peerNodeId}`;
                setSelectedConvoId(convoId);
                setPendingConvoMeta({ did: peerNodeId, handle: peerName || peerNodeId, protocol: 'bluesky' });
              } else if (peerNodeId.startsWith('http') || (peerName && peerName.includes('@'))) {
                setSelectedConvoId(`pending_ap_${peerNodeId}`);
                setPendingConvoMeta({ did: peerNodeId, handle: peerName || peerNodeId, protocol: 'activitypub' });
              } else {
                // P2P node ID
                setSelectedConvoId(`p2p:${peerNodeId}`);
                setPendingConvoMeta(null);
              }
              openTab("chat", "Chat");
              setActiveSidebar("chat");
            }}
            onViewProfile={(peerNodeId, peerName) => {
              setPeerProfileNodeId(peerNodeId);
              setPeerProfileName(peerName || null);
              openTab('peer-profile', peerName || 'Peer Profile', { nodeId: peerNodeId });
              setActiveSidebar("social");
            }}
          />
        );
      case 'peer-profile':
        return (
          <PeerProfileScreen
            nodeId={tab.meta?.nodeId || peerProfileNodeId || ''}
            initialPeerName={tab.meta?.peerName || peerProfileName || undefined}
          />
        );
      case 'vault':
        return <VaultScreen auth={auth} />;
      case 'webview':
        return <WebViewScreen url={tab.meta?.url || 'about:blank'} title={tab.label} isActive={tab.id === activeTabId} tabId={tab.id} />;
      case 'forum':
        return (
          <ForumScreen
            onOpenGroup={handleOpenGroup}
            onCreateGroup={() => setShowCreateGroup(true)}
          />
        );
      case 'group':
        return (
          <GroupScreen
            groupId={tab.meta?.groupId || ''}
            onBack={() => {
              openTab('forum', 'Forums');
              setActiveSidebar('forums');
            }}
          />
        );
      case 'settings':
        return (
          <SettingsScreen
            auth={auth}
            archiveCount={archiveCount}
            onLogout={handleLogout}
            onClearVault={() => setArchiveCount(0)}
            onAuthUpdated={(updatedAuth) => setAuth(updatedAuth)}
          />
        );
      default:
        return null;
    }
  }

  // ═══════════════════════════════════════════════════════════
  // RENDER
  // ═══════════════════════════════════════════════════════════

  if (loading) {
    return (
      <div className="app-container">
        <div className="login-screen">
          <div className="card" style={{ textAlign: "center", padding: 48 }}>
            <div style={{ fontSize: "2rem", marginBottom: 12 }}>⏳</div>
            <p className="text-muted">Starting bytescape...</p>
          </div>
        </div>
      </div>
    );
  }

  if (screen === "login") {
    return (
      <LoginScreen
        onLoginSuccess={handleLoginSuccess}
        onNavigateImport={() => { setScreen("import"); setActiveSidebar("import"); }}
      />
    );
  }

  return (
    <AppLayout
      activeSidebar={activeSidebar}
      onNavigate={handleNavigate}
      searchQuery={searchQuery}
      onSearchChange={setSearchQuery}
      onSearchSubmit={handleSearchSubmit}
      avatarInitial={auth?.local_username?.[0]?.toUpperCase() || auth?.handle?.[0]?.toUpperCase()}
    >
      <ErrorBoundary fallback={<div>Application Error</div>}>
        {/* Render ALL open tabs simultaneously — inactive tabs are hidden via display:none
            so their React state is preserved and switching is instant (no re-mount/re-fetch). */}
        {tabs.map((tab) => {
          const isActive = tab.id === activeTabId;
          const isFullBleed = FULL_BLEED_SCREENS.has(tab.screen);
          return (
            <div
              key={tab.id}
              className={`tab-pane ${isActive ? 'tab-pane--active' : ''} ${isFullBleed ? 'tab-pane--full-bleed' : 'tab-pane--standard'}`}
              style={isActive ? (isFullBleed ? fullBleedStyle : standardStyle) : { display: 'none' }}
            >
              {renderTabScreen(tab)}
            </div>
          );
        })}
      </ErrorBoundary>

      {/* Identity setup modal — shown when user hasn't chosen a username yet */}
      {showIdentitySetup && auth && (
        <IdentitySetupModal
          auth={auth}
          onComplete={handleIdentitySetupComplete}
        />
      )}

      {/* Recovery setup modal — shown after username setup to create real recovery code */}
      {showRecoverySetup && !showIdentitySetup && (
        <RecoverySetupModal
          onComplete={handleRecoverySetupComplete}
        />
      )}

      {/* Group creation modal */}
      {showCreateGroup && (
        <CreateGroupModal
          onClose={() => setShowCreateGroup(false)}
          onCreated={handleGroupCreated}
        />
      )}
    </AppLayout>
  );
}

export default App;
