import React, { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useAuthStore, useAppStore, useNavStore, useChatStore } from "./store";
import { FeedTab, FEED_TAB_LABELS } from "./hooks/useUnifiedFeed";

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
import { SettingsScreen } from "./screens/SettingsScreen";
import { VaultScreen } from "./screens/VaultScreen";

function App() {
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
    feedActiveTab, setFeedActiveTab
  } = useNavStore();
  const chatStore = useChatStore();

  const importStatus = importStatusRaw || "";
  const [loading, setLoading] = useState(true);

  // Import / Audit
  const [progress, setProgress] = useState<ParseProgress | null>(null);
  const [parseResult, setParseResult] = useState<ParseResult | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [isShredding, setIsShredding] = useState(false);
  const [importComplete, setImportComplete] = useState(false);
  const [selectedArchivePath, setSelectedArchivePath] = useState<string | null>(null);

  // Feed / Archive
  const [isImporting, setIsImporting] = useState(false);

  // Build the feed tab buttons to pass into the mobile top bar
  const feedTabSlot = screen === "feed" ? (
    <>
      {(['all', 'following', 'subscribed', 'you'] as FeedTab[]).map(tab => (
        <button
          key={tab}
          className={`tab ${feedActiveTab === tab ? 'active' : ''}`}
          onClick={() => setFeedActiveTab(tab)}
        >
          {FEED_TAB_LABELS[tab]}
        </button>
      ))}
    </>
  ) : undefined;

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

  const addLog = useCallback((msg: string) => {
    setLogs((prev) => [...prev, msg]);
  }, []);

  // === Auth ===
  async function checkAuth() {
    try {
      const status = await invoke<AuthStatus | null>("check_auth_status");
      if (status) {
        setAuth(status);
        setScreen("feed");
        await loadArchiveStats();
        // Auto-sync Bluesky feed on every boot
        importBluesky();
      }
    } catch { /* not logged in */ }
    setLoading(false);
  }

  const handleLoginSuccess = async (status: AuthStatus, shouldImport: boolean) => {
    setAuth(status);
    setScreen("feed");
    await loadArchiveStats();

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
    setArchiveCount(0);
    chatStore.setMessages([]);
    chatStore.setSelectedConvoId(null);
    chatStore.setMessageInput("");
    setParseResult(null);
    setProgress(null);
    setImportStatus("");
    setError("");
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
    setIsImporting(true);
    setImportStatus("Connecting to Bluesky...");
    setError("");
    try {
      console.log("App: Starting Bluesky import...");
      const count = await invoke<number>("import_bluesky_posts");
      console.log(`App: Import finished. Found ${count} new posts.`);

      if (count > 0) {
        setImportStatus(`✅ ${count} new posts imported!`);
      } else {
        setImportStatus(`✅ Up to date — no new posts.`);
      }

      // Pull latest messages in same import flow (ChatScreen will pick up new data via polling)
      try { await invoke<any>("import_vault_messages"); } catch { /* chat handles its own errors */ }

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
    switch (item) {
      case "feed":
        setScreen("feed");
        break;
      case "search":
        setScreen("feed");
        break;
      case "import":
        setScreen("import");
        break;
      case "profile":
        setScreen("profile");
        break;
      case "chat":
        setScreen("chat");
        break;
      case "social":
        setScreen("friends");
        break;
      case "settings":
        setScreen("settings");
        break;
      case "vault":
        setScreen("vault");
        break;
      default:
        break;
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
      feedTabSlot={feedTabSlot}
      mainStyle={(screen === "chat" || screen === "feed" || screen === "vault" || screen === "friends") ? {
        padding: 0,
        height: '100%',
        overflow: 'hidden',
        display: 'flex',
        flexDirection: 'column',
        maxWidth: 'none',
        width: '100%',
        marginRight: 0,
      } : undefined}
    >
      <ErrorBoundary fallback={<div>Application Error</div>}>
        {screen === "feed" && (
          <FeedScreen
            auth={auth}
            activeTab={feedActiveTab}
            setActiveTab={setFeedActiveTab}
            isImporting={isImporting}
          />
        )}

        {screen === "import" && (
          <ImportScreen
            isImporting={isImporting}
            importStatus={importStatus}
            archiveCount={archiveCount}
            onImportBluesky={importBluesky}
            onFilePick={handleFilePick}
            onNavigate={handleNavigate}
          />
        )}

        {screen === "audit" && (
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
        )}

        {screen === "profile" && (
          <ProfileScreen
            auth={auth}
            archiveCount={archiveCount}
          />
        )}

        {screen === "chat" && (
          <ChatScreen />
        )}

        {screen === "friends" && (
          <FriendsScreen
            auth={auth}
          />
        )}

        {screen === "vault" && (
          <VaultScreen auth={auth} />
        )}

        {screen === "settings" && (
          <SettingsScreen
            auth={auth}
            archiveCount={archiveCount}
            onLogout={handleLogout}
            onClearVault={() => {
              setArchiveCount(0);
            }}
          />
        )}

        {/* Application End */}
      </ErrorBoundary>
    </AppLayout>
  );
}

export default App;
