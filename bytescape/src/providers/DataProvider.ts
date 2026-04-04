/**
 * DataProvider — The universal data access interface for Bytescape.
 *
 * Every data operation the UI needs goes through this interface.
 * Implementations:
 *   - TauriDataProvider  (desktop — uses invoke())
 *   - RestDataProvider   (web    — uses fetch() against bytescape.social REST API)
 *   - WasmProvider       (portal — uses WASM-compiled subset)
 *
 * HOW IT WORKS:
 *   1. UI components obtain a DataProvider via DataProviderContext (React context).
 *   2. Components call typed async methods (e.g. getTimeline, publishPostV2).
 *   3. The active implementation (TauriDataProvider on desktop) translates each
 *      call to the appropriate backend transport (Tauri invoke, fetch, WASM).
 *   4. Parameters are typed objects defined in this file; return types come from
 *      types/index.ts, mirroring Rust #[derive(Serialize)] structs.
 *   5. Errors propagate as rejected Promises — callers use try/catch.
 *
 * RELATED FILES:
 *   - providers/TauriDataProvider.ts   — Desktop implementation (Tauri invoke)
 *   - providers/DataProviderContext.tsx — React context that supplies the provider
 *   - types/index.ts                   — Shared TS interfaces mirroring Rust structs
 *   - src-tauri/src/lib.rs             — Rust command definitions & registration
 *
 * Method names use camelCase TypeScript conventions.
 * Parameters are typed objects. Return types use interfaces from types/index.ts.
 */

import type {
  AuthStatus,
  BlueskyPost,
  BytescapeItem,
  CollabDocMeta,
  CollabStateResponse,
  FeedEntry,
  FeedPage,
  GeneratorView,
  GuestbookEntry,
  IdentityInitResult,
  IdentityRecoveryFinalizeResult,
  IdentityRecoveryStartResult,
  IdentityRecoveryVerifyResult,
  LikeResult,
  LinkCard,
  LinkedAccount,
  MentionSuggestion,
  MyPeerInfo,
  P2pConversation,
  ParseResult,
  PeerConnectResult,
  PeerFeedResponse,
  PeerInfo,
  ProfileLayout,
  ProfileView,
  PublishResult,
  RepostResult,
  ThreadReply,
  VaultFile,
  VaultFileStats,
  VaultMessage,
  VaultStats,
} from '../types';

// ════════════════════════════════════════════════════════════════
//  Parameter types  — typed objects for every command
// ════════════════════════════════════════════════════════════════

export interface GetTimelineParams {
  limit: number;
  cursor?: string | null;
}

export interface GetAuthorFeedParams {
  limit: number;
  cursor?: string | null;
}

export interface GetUnifiedFeedParams {
  tags: string[];
  feedUris: string[];
  limit: number;
  cursor: string | null;
}

export interface GetTimelineUnifiedParams {
  tags: string[];
  feedUris: string[];
  limit: number;
  cursor: string | null;
  vaultOffset: number;
  sourceFilter?: string | null;
}

export interface GetGroupsFeedParams {
  limit: number;
  cursor?: string | null;
}

export interface GetPostThreadParams {
  uri: string;
}

export interface FollowedHashtagParams {
  tag: string;
}

export interface VerifySyndicationsParams {
  itemId: string;
}

export interface LoginParams {
  identifier: string;
  password: string;
}

export interface StartBlueskyOauthParams {
  handle: string;
}

export interface MastodonRegisterAppParams {
  instance: string;
}

export interface MastodonCompleteAuthParams {
  code: string;
}

export interface UnlinkAccountParams {
  protocol: string;
  identifier: string;
}

export interface ResolveProfileParams {
  did: string;
}

export interface FetchRemoteActorParams {
  query: string;
}

export interface ListFollowsParams {
  actor: string;
}

export interface ListFollowersParams {
  actor: string;
}

export interface MastodonGetTimelineParams {
  limit?: number;
}

export interface ListVaultPostsPagedParams {
  limit: number;
  offset: number;
  source?: string | null;
}

export interface VaultFileDataParams {
  id: string;
}

export interface VaultUploadFromPathsParams {
  paths: string[];
}

export interface VaultUploadFileParams {
  name: string;
  data: number[];
  mimeType: string;
}

export interface ExportVaultDataParams {
  outputPath: string;
  includeMedia: boolean;
}

export interface ExportVaultDataResult {
  path: string;
  item_count: number;
  contact_count: number;
  message_count: number;
  media_count: number;
  size_bytes: number;
}

export interface PublishPostParams {
  content: string;
  audiences: string[];
  mediaBlobs?: string[];
  mediaMimes?: string[];
  title?: string | null;
  itemType?: string;
  replyTo?: string | null;
  linkCard?: LinkCard | null;
}

export interface SyndicateVaultItemParams {
  itemId: string;
  platforms: string[];
}

export interface SearchMentionsParams {
  query: string;
}

export interface SearchHashtagsParams {
  prefix: string;
}

export interface UnfurlLinkParams {
  url: string;
}

export interface GetFileSizeParams {
  path: string;
}

export interface MastodonReplyParams {
  inReplyToId: string;
  text: string;
}

export interface LikePostParams {
  uri: string;
  cid: string;
}

export interface UnlikePostParams {
  likeUri: string;
}

export interface RepostPostParams {
  uri: string;
  cid: string;
}

export interface UnrepostPostParams {
  repostUri: string;
}

export interface MastodonInteractionParams {
  statusId: string;
}

export interface MastodonHashtagParams {
  tag: string;
}

export interface GetMediaBlobParams {
  hash: string;
}

export interface ResolveHlsVideoParams {
  playlistUrl: string;
}

export interface CancelVideoResolveParams {
  playlistUrl: string;
}

export interface ResolveBlobUrlsBatchParams {
  hashes: string[];
}

export interface BrowserCreateParams {
  label: string;
  url: string;
}

export interface BrowserNavigateParams {
  label: string;
  url: string;
}

export interface BrowserEvalParams {
  label: string;
  js: string;
}

export interface BrowserShowAtParams {
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface BrowserLabelParams {
  label: string;
}

export interface BrowserSetBoundsParams {
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface AddPeerParams {
  nodeId: string;
  displayName?: string | null;
}

export interface RemovePeerParams {
  nodeId: string;
}

export interface CheckPeerConnectivityParams {
  nodeId: string;
}

export interface RequestPeerFeedParams {
  nodeId: string;
  offset?: number;
  limit?: number;
}

export interface DownloadPeerBlobParams {
  nodeId: string;
  hash: string;
}

export interface SaveProfileLayoutParams {
  html: string;
  css: string;
  componentsJson?: string | null;
  stylesJson?: string | null;
  widgetData?: string | null;
  renderMode?: string | null;
  js?: string | null;
}

export interface SaveWidgetDataParams {
  data: string;
}

export interface AddGuestbookEntryParams {
  author: string;
  message: string;
}

export interface DeleteGuestbookEntryParams {
  id: string;
}

export interface SetLocalUsernameParams {
  username: string;
}

export interface IdentityInitRecoveryParams {
  passphrase: string;
}

export interface IdentityStartRecoveryParams {
  escrowBundle: string;
}

export interface IdentityVerifyRecoveryParams {
  challengeId: string;
  passphrase: string;
}

export interface ImportArchiveAutoParams {
  path: string;
}

export interface ImportArchiveMessagesParams {
  path: string;
}

export interface DebugZipContentsParams {
  path: string;
}

export interface JoinSyncDocParams {
  ticketStr: string;
}

export interface SendP2pChatMessageParams {
  nodeId: string;
  text: string;
}

export interface GetOrCreateBskyConvoParams {
  did: string;
}

export interface SendMessageParams {
  convoId: string;
  text: string;
}

export interface SendBskyDmParams {
  convoId: string;
  text: string;
}

export interface GetCollabStateParams {
  docId: string;
}

export interface CreateCollabDocParams {
  title?: string | null;
}

export interface ApplyCollabUpdateParams {
  docId: string;
  updateB64: string;
}

export interface SnapshotCollabToItemParams {
  docId: string;
}

export interface VaultFileDeleteParams {
  id: string;
}

export interface ReplyToPostParams {
  parentUri: string;
  parentCid: string;
  text: string;
}

export interface PagedVaultResult {
  items: BytescapeItem[];
  total: number;
}

export interface SyncDocResult {
  ticket: string;
  namespace_id: string;
}

// ════════════════════════════════════════════════════════════════
//  Relay types (Phase 4.5 stubs — backend not yet deployed)
// ════════════════════════════════════════════════════════════════

export type RelayStatus = 'connected' | 'syncing' | 'offline';

export interface RelaySyncStatus {
  status: RelayStatus;
  lastSync?: number | null;
  error?: string | null;
}

// ════════════════════════════════════════════════════════════════
//  DataProvider Interface
// ════════════════════════════════════════════════════════════════

/**
 * DataProvider — The contract every data backend must implement.
 *
 * This monolithic interface defines ~100 methods spanning feed retrieval,
 * auth, vault CRUD, P2P networking, media, chat, and more.  The active
 * implementation is injected via React context (DataProviderContext).
 *
 * @see TauriDataProvider — the desktop implementation (Tauri invoke)
 *
 * TECH DEBT:
 *   - 6 methods return `Promise<unknown>` (fetchRemoteActor, importVaultMessages,
 *     sendP2pChatMessage, sendBskyDm, importArchiveMessages, migrateWillowPaths) —
 *     these need proper return types once their Rust commands stabilize.
 *   - This interface is monolithic; consider splitting into sub-interfaces
 *     (FeedProvider, AuthProvider, VaultProvider, etc.) for testability
 *     and narrower dependency injection.
 */
export interface DataProvider {
  /** Feed — retrieval and discovery for Bluesky, Mastodon, unified, and groups feeds. */
  // ═══ Feed ═════════════════════════════════════════════════════
  getTimeline(params: GetTimelineParams): Promise<[BlueskyPost[], string | null]>;
  getAuthorFeed(params: GetAuthorFeedParams): Promise<[BlueskyPost[], string | null]>;
  getUnifiedFeed(params: GetUnifiedFeedParams): Promise<[BlueskyPost[], string | null]>;
  getTimelineUnified(params: GetTimelineUnifiedParams): Promise<FeedPage>;
  listSavedFeeds(): Promise<GeneratorView[]>;
  listFollowedHashtags(): Promise<string[]>;
  addFollowedHashtag(params: FollowedHashtagParams): Promise<string[]>;
  removeFollowedHashtag(params: FollowedHashtagParams): Promise<string[]>;
  getPostThread(params: GetPostThreadParams): Promise<ThreadReply[]>;
  verifySyndications(params: VerifySyndicationsParams): Promise<boolean>;
  getGroupsFeed(params: GetGroupsFeedParams): Promise<FeedPage>;

  /** Auth — authentication and session management for all linked platforms. */
  // ═══ Auth ═════════════════════════════════════════════════════
  checkAuthStatus(): Promise<AuthStatus | null>;
  login(params: LoginParams): Promise<AuthStatus>;
  logout(): Promise<void>;
  startBlueskyOauth(params: StartBlueskyOauthParams): Promise<string>;
  cancelBlueskyOauth(): Promise<void>;
  logoutBluesky(): Promise<void>;
  mastodonRegisterApp(params: MastodonRegisterAppParams): Promise<string>;
  mastodonCompleteAuth(params: MastodonCompleteAuthParams): Promise<AuthStatus>;
  mastodonGetSession(): Promise<{ username: string; instance: string } | null>;
  mastodonDisconnect(): Promise<void>;
  listLinkedAccounts(): Promise<LinkedAccount[]>;
  unlinkAccount(params: UnlinkAccountParams): Promise<void>;
  deleteAccount(): Promise<void>;

  /** Profile — profile resolution and social graph queries across protocols. */
  // ═══ Profile ══════════════════════════════════════════════════
  resolveProfile(params: ResolveProfileParams): Promise<ProfileView>;
  fetchRemoteActor(params: FetchRemoteActorParams): Promise<unknown>;
  listFollows(params: ListFollowsParams): Promise<ProfileView[]>;
  listFollowers(params: ListFollowersParams): Promise<ProfileView[]>;
  mastodonGetFollowing(): Promise<ProfileView[]>;
  listContacts(): Promise<ProfileView[]>;
  mastodonGetTimeline(params: MastodonGetTimelineParams): Promise<FeedEntry[]>;

  /** Vault — local vault CRUD for posts, files, stats, and data export. */
  // ═══ Vault ════════════════════════════════════════════════════
  listVaultPostsPaged(params: ListVaultPostsPagedParams): Promise<PagedVaultResult>;
  listVaultSources(): Promise<string[]>;
  vaultListFiles(): Promise<VaultFile[]>;
  vaultGetStats(): Promise<VaultFileStats>;
  getVaultStats(): Promise<VaultStats>;
  vaultGetFileData(params: VaultFileDataParams): Promise<number[]>;
  vaultGetThumbnail(params: VaultFileDataParams): Promise<number[]>;
  vaultUploadFilesFromPaths(params: VaultUploadFromPathsParams): Promise<VaultFile[]>;
  vaultUploadFile(params: VaultUploadFileParams): Promise<VaultFile>;
  vaultDeleteFile(params: VaultFileDeleteParams): Promise<void>;
  vaultDeduplicateFiles(): Promise<number>;
  countArchivePosts(): Promise<number>;
  exportVaultData(params: ExportVaultDataParams): Promise<ExportVaultDataResult>;

  /** Posting — content creation, cross-platform syndication, and compose-time helpers. */
  // ═══ Posting ══════════════════════════════════════════════════
  publishPostV2(params: PublishPostParams): Promise<PublishResult>;
  syndicateVaultItem(params: SyndicateVaultItemParams): Promise<string>;
  searchMentions(params: SearchMentionsParams): Promise<MentionSuggestion[]>;
  searchHashtagsAutocomplete(params: SearchHashtagsParams): Promise<string[]>;
  unfurlLink(params: UnfurlLinkParams): Promise<LinkCard>;
  getFileSize(params: GetFileSizeParams): Promise<number>;
  replyToPost(params: ReplyToPostParams): Promise<void>;
  mastodonReply(params: MastodonReplyParams): Promise<void>;

  /** Interactions — likes, reposts, and their Mastodon equivalents (favourite/reblog). */
  // ═══ Interactions ═════════════════════════════════════════════
  likePost(params: LikePostParams): Promise<LikeResult>;
  unlikePost(params: UnlikePostParams): Promise<boolean>;
  repostPost(params: RepostPostParams): Promise<RepostResult>;
  unrepostPost(params: UnrepostPostParams): Promise<boolean>;
  mastodonFavourite(params: MastodonInteractionParams): Promise<void>;
  mastodonUnfavourite(params: MastodonInteractionParams): Promise<void>;
  mastodonReblog(params: MastodonInteractionParams): Promise<void>;
  mastodonUnreblog(params: MastodonInteractionParams): Promise<void>;
  mastodonFollowHashtag(params: MastodonHashtagParams): Promise<void>;
  mastodonUnfollowHashtag(params: MastodonHashtagParams): Promise<void>;

  /** Media — blob retrieval, HLS video resolution, and batch URL mapping. */
  // ═══ Media ════════════════════════════════════════════════════
  getMediaBlob(params: GetMediaBlobParams): Promise<string>;
  getMediaServerPort(): Promise<number>;
  resolveHlsVideo(params: ResolveHlsVideoParams): Promise<string>;
  cancelVideoResolve(params: CancelVideoResolveParams): Promise<void>;
  resolveBlobUrlsBatch(params: ResolveBlobUrlsBatchParams): Promise<Record<string, string>>;

  /** Chat — vault-archived messages, P2P chat, and Bluesky DMs. */
  // ═══ Chat ═════════════════════════════════════════════════════
  listVaultMessages(): Promise<VaultMessage[]>;
  importVaultMessages(): Promise<unknown>;
  listP2pConversations(): Promise<P2pConversation[]>;
  sendP2pChatMessage(params: SendP2pChatMessageParams): Promise<unknown>;
  getOrCreateBskyConvo(params: GetOrCreateBskyConvoParams): Promise<string>;
  sendMessage(params: SendMessageParams): Promise<void>;
  sendBskyDm(params: SendBskyDmParams): Promise<unknown>;

  /** Browser — embedded webview lifecycle (create, navigate, eval, show/hide). */
  // ═══ Browser ══════════════════════════════════════════════════
  browserCreate(params: BrowserCreateParams): Promise<void>;
  browserNavigate(params: BrowserNavigateParams): Promise<void>;
  browserEval(params: BrowserEvalParams): Promise<void>;
  browserShowAt(params: BrowserShowAtParams): Promise<void>;
  browserHide(params: BrowserLabelParams): Promise<void>;
  browserClose(params: BrowserLabelParams): Promise<void>;
  browserSetBounds(params: BrowserSetBoundsParams): Promise<void>;

  /** Peer / P2P — Iroh-based networking, feed exchange, and blob transfer. */
  // ═══ Peer / P2P ═══════════════════════════════════════════════
  setupIrohNode(): Promise<string>;
  listPeers(): Promise<PeerInfo[]>;
  getMyPeerInfo(): Promise<MyPeerInfo>;
  addPeer(params: AddPeerParams): Promise<PeerInfo>;
  removePeer(params: RemovePeerParams): Promise<void>;
  checkPeerConnectivity(params: CheckPeerConnectivityParams): Promise<PeerConnectResult>;
  requestPeerFeed(params: RequestPeerFeedParams): Promise<PeerFeedResponse>;
  downloadPeerBlob(params: DownloadPeerBlobParams): Promise<boolean>;

  /** Widget / Profile Builder — layout persistence, widget data, and guestbook. */
  // ═══ Widget / Profile Builder ═════════════════════════════════
  getProfileLayout(): Promise<ProfileLayout | null>;
  saveProfileLayout(params: SaveProfileLayoutParams): Promise<void>;
  deleteProfileLayout(): Promise<void>;
  getWidgetData(): Promise<string | null>;
  saveWidgetData(params: SaveWidgetDataParams): Promise<void>;
  listGuestbookEntries(): Promise<GuestbookEntry[]>;
  addGuestbookEntry(params: AddGuestbookEntryParams): Promise<GuestbookEntry>;
  deleteGuestbookEntry(params: DeleteGuestbookEntryParams): Promise<void>;

  /** Identity — local identity creation and passphrase-based recovery flow. */
  // ═══ Identity ═════════════════════════════════════════════════
  setLocalUsername(params: SetLocalUsernameParams): Promise<AuthStatus>;
  createLocalIdentity(): Promise<AuthStatus>;
  identityInitFullRecovery(params: IdentityInitRecoveryParams): Promise<IdentityInitResult>;
  identityStartRecovery(params: IdentityStartRecoveryParams): Promise<IdentityRecoveryStartResult>;
  identityVerifyRecovery(params: IdentityVerifyRecoveryParams): Promise<IdentityRecoveryVerifyResult>;
  identityFinalizeRecovery(): Promise<IdentityRecoveryFinalizeResult>;

  /** Import — archive import pipeline: Bluesky sync, ZIP parsing, and commit. */
  // ═══ Import ═══════════════════════════════════════════════════
  importBlueskyPosts(): Promise<number>;
  importArchiveAuto(params: ImportArchiveAutoParams): Promise<ParseResult>;
  commitMemories(): Promise<number>;
  importArchiveMessages(params: ImportArchiveMessagesParams): Promise<unknown>;
  debugZipContents(params: DebugZipContentsParams): Promise<string[]>;

  /** Sync — Iroh Docs-based CRDT synchronization between devices. */
  // ═══ Sync ═════════════════════════════════════════════════════
  createSyncDoc(): Promise<SyncDocResult>;
  joinSyncDoc(params: JoinSyncDocParams): Promise<string>;
  migrateWillowPaths(): Promise<unknown>;

  /** Collab — real-time collaborative editing via Yjs documents. */
  // ═══ Collab ═══════════════════════════════════════════════════
  getCollabState(params: GetCollabStateParams): Promise<CollabStateResponse>;
  createCollabDoc(params: CreateCollabDocParams): Promise<CollabDocMeta>;
  applyCollabUpdate(params: ApplyCollabUpdateParams): Promise<void>;
  snapshotCollabToItem(params: SnapshotCollabToItemParams): Promise<string>;

  /** Relay — cloud-based sync (Phase 4.5 — backend not yet deployed). */
  // ═══ Relay (Phase 4.5 — stubs) ═══════════════════════════════
  relaySyncStatus(): Promise<RelaySyncStatus>;
}
