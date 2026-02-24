import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import "./App.css";

// Types
import {
  AuthStatus,
  ParseProgress,
  ParseResult,
  ImportProgress,
  Screen,
  SidebarItem,
  VaultMessage,
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
import { StatusBar } from "./components/StatusBar";
function App() {
  const [screen, setScreen] = useState<Screen>("login");
  const [auth, setAuth] = useState<AuthStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>("");

  // Import / Audit
  const [progress, setProgress] = useState<ParseProgress | null>(null);
  const [parseResult, setParseResult] = useState<ParseResult | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [isShredding, setIsShredding] = useState(false);
  const [importComplete, setImportComplete] = useState(false);
  const [selectedArchivePath, setSelectedArchivePath] = useState<string | null>(null);

  // Feed / Archive
  const [archiveCount, setArchiveCount] = useState(0);
  const [isImporting, setIsImporting] = useState(false);
  const [importStatus, setImportStatus] = useState("");

  // Chat
  const [messages, setMessages] = useState<VaultMessage[]>([]);
  const [selectedConvoId, setSelectedConvoId] = useState<string | null>(null);
  const [messageInput, setMessageInput] = useState("");
  const [isSending, setIsSending] = useState(false);

  // Sidebar
  const [activeSidebar, setActiveSidebar] = useState<SidebarItem>("feed");

  // === Init ===
  useEffect(() => {
    initApp();
  }, []);

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
    setMessages([]);
    setSelectedConvoId(null);
    setMessageInput("");
    setParseResult(null);
    setProgress(null);
    setImportStatus("");
    setError("");
  }

  // === Data Loading ===
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

  async function loadMessages() {
    try {
      const msgs = await invoke<VaultMessage[]>("list_vault_messages");
      setMessages(msgs);
    } catch (e: any) {
      console.error("Failed to load messages:", e);
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

      // Pull latest messages in same import flow so import is not post-only.
      await importBlueskyMessages(true);

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

  async function importBlueskyMessages(silent = false) {
    if (!silent) {
      setIsImporting(true);
      setImportStatus("Importing messages...");
      setError("");
    }
    try {
      const result = await invoke<any>("import_vault_messages");
      if (!silent) {
        if (result.new > 0) {
          setImportStatus(`✅ Imported ${result.new} new messages from ${result.convos} conversations!`);
        } else {
          setImportStatus(`✅ Messages up to date.`);
        }
      }
      if (result.new > 0 || !silent) {
        await loadMessages();
      }
    } catch (e: any) {
      if (!silent) {
        const msg = typeof e === "string" ? e : e.message || "Message import failed";
        setError(msg);
        setImportStatus("");
      }
    } finally {
      if (!silent) setIsImporting(false);
    }
  }

  // Poll for messages
  useEffect(() => {
    if (auth && screen === "chat") {
      importBlueskyMessages(true);

      const interval = setInterval(() => {
        importBlueskyMessages(true);
      }, 30000); // Poll every 30s
      return () => clearInterval(interval);
    }
  }, [auth, screen]);

  async function handleSendMessage() {
    if (!selectedConvoId || !messageInput.trim()) return;

    const text = messageInput.trim();
    setMessageInput("");
    setIsSending(true);

    try {
      // Optimistic update
      const now = Date.now() / 1000;
      const optimisticMsg: VaultMessage = {
        id: `temp-${now}`,
        convo_id: selectedConvoId,
        sender_did: auth?.did || "",
        sender_handle: auth?.handle || "",
        text: text,
        sent_at: new Date().toISOString(),
        source: "bluesky",
        imported_at: now
      };
      setMessages(prev => [optimisticMsg, ...prev]);

      await invoke<any>("send_bsky_dm", {
        convoId: selectedConvoId,
        recipientDid: null,
        content: text
      });

    } catch (e: any) {
      console.error("Send failed:", e);
      setError("Failed to send message: " + (typeof e === "string" ? e : "Undefined error"));
    } finally {
      setIsSending(false);
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
        // Need to set sidebar? No, 'audit' is full screen or part of import?
        // In original App.tsx, audit was a screen. import was a screen.
        // Let's assume 'import' sidebar item leads to 'import' screen.
        // 'audit' is a temporary state/screen triggered from import.
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
        
        // Debug: list ZIP contents to help diagnose parsing
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
          await loadMessages();
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
        loadMessages();
        importBlueskyMessages(true);
        break;
      case "followers":
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
      mainStyle={(screen === "chat" || screen === "feed" || screen === "vault") ? {
        padding: 0,
        height: '100vh',
        overflow: 'hidden',
        display: 'flex',
        flexDirection: 'column',
        maxWidth: 'none',
        width: '100%',
        marginRight: 0,
      } : undefined}
    >
      {screen === "feed" && (
        <FeedScreen
          auth={auth}
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
          error={error}
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
        <ChatScreen
          auth={auth}
          messages={messages}
          selectedConvoId={selectedConvoId}
          setSelectedConvoId={setSelectedConvoId}
          messageInput={messageInput}
          setMessageInput={setMessageInput}
          isSending={isSending}
          onSendMessage={handleSendMessage}
        />
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

      {/* Global Status Bar */}
      <StatusBar
        status={importStatus}
        isBusy={isImporting}
        onDismiss={!isImporting ? () => setImportStatus("") : undefined}
      />

    </AppLayout>
  );
}

export default App;
