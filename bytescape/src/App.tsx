import React, { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useAuthStore, useAppStore, useNavStore, useChatStore, usePeerStore, TabItem, Screen } from "./store";

import "./App.css";

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

// Types
import {
  AuthStatus,
  ParseProgress,
  ParseResult,
  ImportProgress,
  SidebarItem,
  VaultFileStats
} from "./types";

// Components
import { AppLayout } from "./components/AppLayout";
import { LoginScreen } from "./screens/LoginScreen";
import { FeedScreen } from "./screens/FeedScreen";
import { ImportScreen } from "./screens/ImportScreen";
import { AuditScreen } from "./screens/AuditScreen";
import { ChatScreen } from "./screens/ChatScreen";
import { FriendsScreen } from "./screens/FriendsScreen";
import { ProfileScreen } from "./screens/ProfileScreen";
import { ProfileBuilderScreen } from "./screens/ProfileBuilderScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { VaultScreen } from "./screens/VaultScreen";
import { PeerProfileScreen } from "./screens/PeerProfileScreen";
import { ProfileBrowserScreen } from "./screens/ProfileBrowserScreen";
import { WebViewScreen } from "./screens/WebViewScreen";
import { IdentitySetupModal } from "./components/IdentitySetupModal";
import { RecoverySetupModal } from "./components/RecoverySetupModal";
import { useTheme } from "./hooks/useTheme";
import { useLocationStore } from "./hooks/useLocation";
import { installLinkInterceptor } from "./utils/openInApp";

function App() {
  // ─── Theme (applies data-theme attribute on <html>) ───
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
    tabs, activeTabId
  } = useNavStore();
  const chatStore = useChatStore();

  // ─── B10.1 / B10.2: Sync NavStore → Location URL ───
  // Whenever screen, feedTab, or peer context changes, update the byte:// URL
  // and push onto the navigation history stack.
  useEffect(() => {
    useLocationStore.getState().syncFromNavStore();
  }, [screen, feedActiveTab, peerProfileNodeId]);

  // ─── Global link interceptor: route all <a target="_blank"> to in-app tabs ───
  useEffect(() => {
    return installLinkInterceptor();
  }, []);

  const importStatus = importStatusRaw || "";
  const [loading, setLoading] = useState(true);
  const [showIdentitySetup, setShowIdentitySetup] = useState(false);
  const [showRecoverySetup, setShowRecoverySetup] = useState(false);

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
  const [searchQuery, setSearchQuery] = useState('');

  const handleSearchSubmit = (query: string) => {
    // Navigate to feed screen when searching
    setScreen('feed');
    setActiveSidebar('feed');
    // Future: wire into feed filtering
    console.log('Search submitted:', query);
  };

  // === Init ===
  useEffect(() => {
    initApp();
  }, []);

  /**
   * Initializes backend services required by feed, vault, and sync features.
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

  // ── Global P2P peer-connected listener ──
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

  // ── Background P2P keepalive ──
  // Every 90 seconds, check connectivity to all known peers so the status
  // stays fresh even when the user isn't on the Social screen.
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

  // === Auth ===
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

  // === Data Loading ===
  /** Loads archive summary counts for UI badges and status text. */
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



  // === Bluesky Import ===
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



  // === Facebook Import ===
  async function handleFilePick() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "ZIP Archive", extensions: ["zip"] }],
      });
      if (selected) {
        // Route into audit flow while keeping import highlighted in navigation.
        setScreen("audit");
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
          console.log("ZIP structure:", zipInfo);
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
      }
    } catch (e: any) {
      setError(typeof e === "string" ? e : "File selection failed");
    }
  }

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

  // === Navigation ===
  function handleNavigate(item: SidebarItem) {
    setActiveSidebar(item);
    const screenMap: Record<string, { screen: Parameters<typeof openTab>[0]; label?: string }> = {
      feed: { screen: 'feed', label: 'Feed' },
      search: { screen: 'feed', label: 'Feed' },
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

  // === Render Helpers ===

  /** Screens that need full-bleed layout (no padding, flex column, overflow hidden) */
  const FULL_BLEED_SCREENS: Set<Screen> = new Set([
    'chat', 'feed', 'vault', 'friends', 'peer-profile', 'profile-builder', 'profile-browser', 'webview',
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

  /** Render the screen content for a given tab */
  function renderTabScreen(tab: TabItem) {
    switch (tab.screen) {
      case 'feed':
        return (
          <FeedScreen
            auth={auth}
            activeTab={feedActiveTab}
            setActiveTab={setFeedActiveTab}
            isImporting={isImporting}
            onOpenChat={(peerNodeId, _peerName) => {
              const { setSelectedConvoId } = useChatStore.getState();
              setSelectedConvoId(`p2p:${peerNodeId}`);
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
            onFilePick={handleFilePick}
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
            onOpenChat={(peerNodeId, _peerName) => {
              const { setSelectedConvoId } = useChatStore.getState();
              setSelectedConvoId(`p2p:${peerNodeId}`);
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
        return <WebViewScreen url={tab.meta?.url || 'about:blank'} title={tab.label} />;
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

  // === Render ===

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
    </AppLayout>
  );
}

export default App;
