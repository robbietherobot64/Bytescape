/**
 * TauriDataProvider — Desktop implementation of the DataProvider interface.
 *
 * Every method delegates to `invoke()` from `@tauri-apps/api/core`,
 * mapping camelCase TypeScript method names to snake_case Rust commands.
 * This is a 1:1 drop-in replacement for direct invoke() calls.
 *
 * HOW IT WORKS:
 *   1. Each method receives a typed params object from the DataProvider interface.
 *   2. The method spreads (or manually maps) params into a plain object.
 *   3. `invoke('snake_case_command', { ...params })` sends the call over
 *      Tauri's IPC bridge to the Rust backend (src-tauri/src/lib.rs).
 *   4. The Rust command executes and returns a JSON-serialized result.
 *   5. `invoke()` deserializes the result into the expected TypeScript type.
 *
 * RELATED FILES:
 *   - providers/DataProvider.ts        — The interface this class implements
 *   - providers/DataProviderContext.tsx — React context that injects this provider
 *   - types/index.ts                   — Shared TS interfaces mirroring Rust structs
 *   - src-tauri/src/lib.rs             — Rust command definitions & registration
 *
 * TECH DEBT:
 *   - Every method spreads params (`{ ...params }`) which is unnecessary when
 *     the param keys already match the Rust command's expected field names.
 *     A generic helper like `invokeCmd<T>(cmd, params)` would reduce boilerplate.
 *   - No error wrapping or normalization — Rust errors surface as raw strings.
 *     Consider a unified error type or at least consistent error logging.
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  DataProvider,
  GetTimelineParams,
  GetAuthorFeedParams,
  GetUnifiedFeedParams,
  GetTimelineUnifiedParams,
  GetPostThreadParams,
  FollowedHashtagParams,
  VerifySyndicationsParams,
  LoginParams,
  StartBlueskyOauthParams,
  MastodonRegisterAppParams,
  MastodonCompleteAuthParams,
  UnlinkAccountParams,
  ResolveProfileParams,
  FetchRemoteActorParams,
  ListFollowsParams,
  ListFollowersParams,
  MastodonGetTimelineParams,
  ListVaultPostsPagedParams,
  VaultFileDataParams,
  VaultUploadFromPathsParams,
  VaultUploadFileParams,
  ExportVaultDataParams,
  ExportVaultDataResult,
  PublishPostParams,
  SyndicateVaultItemParams,
  SearchMentionsParams,
  SearchHashtagsParams,
  UnfurlLinkParams,
  GetFileSizeParams,
  ReplyToPostParams,
  MastodonReplyParams,
  LikePostParams,
  UnlikePostParams,
  RepostPostParams,
  UnrepostPostParams,
  MastodonInteractionParams,
  MastodonHashtagParams,
  GetMediaBlobParams,
  ResolveHlsVideoParams,
  CancelVideoResolveParams,
  ResolveBlobUrlsBatchParams,
  BrowserCreateParams,
  BrowserNavigateParams,
  BrowserEvalParams,
  BrowserShowAtParams,
  BrowserLabelParams,
  BrowserSetBoundsParams,
  AddPeerParams,
  RemovePeerParams,
  CheckPeerConnectivityParams,
  RequestPeerFeedParams,
  DownloadPeerBlobParams,
  SaveProfileLayoutParams,
  SaveWidgetDataParams,
  AddGuestbookEntryParams,
  DeleteGuestbookEntryParams,
  SetLocalUsernameParams,
  IdentityInitRecoveryParams,
  IdentityStartRecoveryParams,
  IdentityVerifyRecoveryParams,
  ImportArchiveAutoParams,
  ImportArchiveMessagesParams,
  DebugZipContentsParams,
  JoinSyncDocParams,
  SendP2pChatMessageParams,
  GetOrCreateBskyConvoParams,
  SendMessageParams,
  SendBskyDmParams,
  GetCollabStateParams,
  CreateCollabDocParams,
  ApplyCollabUpdateParams,
  SnapshotCollabToItemParams,
  VaultFileDeleteParams,
  PagedVaultResult,
  SyncDocResult,
  RelaySyncStatus,
} from './DataProvider';
import type {
  AuthStatus,
  BlueskyPost,
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

/**
 * TauriDataProvider — Desktop-only DataProvider backed by Tauri IPC.
 *
 * IPC CONTRACT: Every method maps a camelCase TypeScript name to a snake_case
 * Rust command registered in `src-tauri/src/lib.rs`.  Example:
 *   TS: `getTimelineUnified(params)`  →  Rust: `get_timeline_unified`
 *
 * The mapping is manual (no codegen). When adding a new command, you must:
 *   1. Define the `#[tauri::command]` fn in Rust
 *   2. Register it in the `invoke_handler` macro in lib.rs
 *   3. Add the method here with the matching snake_case invoke string
 *   4. Add the method signature to the DataProvider interface
 */
export class TauriDataProvider implements DataProvider {

  // ═══ Feed ═══════════════════════════════════════════════════════

  async getTimeline(params: GetTimelineParams): Promise<[BlueskyPost[], string | null]> {
    return invoke('get_timeline', { ...params });
  }

  async getAuthorFeed(params: GetAuthorFeedParams): Promise<[BlueskyPost[], string | null]> {
    return invoke('get_author_feed', { ...params });
  }

  async getUnifiedFeed(params: GetUnifiedFeedParams): Promise<[BlueskyPost[], string | null]> {
    return invoke('get_unified_feed', {
      tags: params.tags,
      feedUris: params.feedUris,
      limit: params.limit,
      cursor: params.cursor,
    });
  }

  async getTimelineUnified(params: GetTimelineUnifiedParams): Promise<FeedPage> {
    return invoke('get_timeline_unified', {
      tags: params.tags,
      feedUris: params.feedUris,
      limit: params.limit,
      cursor: params.cursor,
      vaultOffset: params.vaultOffset,
      sourceFilter: params.sourceFilter ?? null,
    });
  }

  async getGroupsFeed(params: { limit: number; cursor?: string | null }): Promise<FeedPage> {
    return invoke('get_groups_feed', {
      limit: params.limit,
      cursor: params.cursor ?? null,
    });
  }

  async listSavedFeeds(): Promise<GeneratorView[]> {
    return invoke('list_saved_feeds');
  }

  async listFollowedHashtags(): Promise<string[]> {
    return invoke('list_followed_hashtags');
  }

  async addFollowedHashtag(params: FollowedHashtagParams): Promise<string[]> {
    return invoke('add_followed_hashtag', { ...params });
  }

  async removeFollowedHashtag(params: FollowedHashtagParams): Promise<string[]> {
    return invoke('remove_followed_hashtag', { ...params });
  }

  async getPostThread(params: GetPostThreadParams): Promise<ThreadReply[]> {
    return invoke('get_post_thread', { ...params });
  }

  async verifySyndications(params: VerifySyndicationsParams): Promise<boolean> {
    return invoke('verify_syndications', { ...params });
  }

  // ═══ Auth ═══════════════════════════════════════════════════════

  async checkAuthStatus(): Promise<AuthStatus | null> {
    return invoke('check_auth_status');
  }

  async login(params: LoginParams): Promise<AuthStatus> {
    return invoke('login', { ...params });
  }

  async logout(): Promise<void> {
    return invoke('logout');
  }

  async startBlueskyOauth(params: StartBlueskyOauthParams): Promise<string> {
    return invoke('start_bluesky_oauth', { ...params });
  }

  async cancelBlueskyOauth(): Promise<void> {
    return invoke('cancel_bluesky_oauth');
  }

  async logoutBluesky(): Promise<void> {
    return invoke('logout_bluesky');
  }

  async mastodonRegisterApp(params: MastodonRegisterAppParams): Promise<string> {
    return invoke('mastodon_register_app', { ...params });
  }

  async mastodonCompleteAuth(params: MastodonCompleteAuthParams): Promise<AuthStatus> {
    return invoke('mastodon_complete_auth', { ...params });
  }

  async mastodonGetSession(): Promise<{ username: string; instance: string } | null> {
    return invoke('mastodon_get_session');
  }

  async mastodonDisconnect(): Promise<void> {
    return invoke('mastodon_disconnect');
  }

  async listLinkedAccounts(): Promise<LinkedAccount[]> {
    return invoke('list_linked_accounts');
  }

  async unlinkAccount(params: UnlinkAccountParams): Promise<void> {
    return invoke('unlink_account', { ...params });
  }

  async deleteAccount(): Promise<void> {
    return invoke('delete_account');
  }

  // ═══ Profile ════════════════════════════════════════════════════

  async resolveProfile(params: ResolveProfileParams): Promise<ProfileView> {
    return invoke('resolve_profile', { ...params });
  }

  async fetchRemoteActor(params: FetchRemoteActorParams): Promise<unknown> {
    return invoke('fetch_remote_actor', { ...params });
  }

  async listFollows(params: ListFollowsParams): Promise<ProfileView[]> {
    return invoke('list_follows', { ...params });
  }

  async listFollowers(params: ListFollowersParams): Promise<ProfileView[]> {
    return invoke('list_followers', { ...params });
  }

  async mastodonGetFollowing(): Promise<ProfileView[]> {
    return invoke('mastodon_get_following');
  }

  async listContacts(): Promise<ProfileView[]> {
    return invoke('list_contacts');
  }

  async mastodonGetTimeline(params: MastodonGetTimelineParams): Promise<FeedEntry[]> {
    return invoke('mastodon_get_timeline', { ...params });
  }

  // ═══ Vault ══════════════════════════════════════════════════════

  async listVaultPostsPaged(params: ListVaultPostsPagedParams): Promise<PagedVaultResult> {
    return invoke('list_vault_posts_paged', { ...params });
  }

  async listVaultSources(): Promise<string[]> {
    return invoke('list_vault_sources');
  }

  async vaultListFiles(): Promise<VaultFile[]> {
    return invoke('vault_list_files');
  }

  async vaultGetStats(): Promise<VaultFileStats> {
    return invoke('vault_get_stats');
  }

  async getVaultStats(): Promise<VaultStats> {
    return invoke('get_vault_stats');
  }

  async vaultGetFileData(params: VaultFileDataParams): Promise<number[]> {
    return invoke('vault_get_file_data', { ...params });
  }

  async vaultGetThumbnail(params: VaultFileDataParams): Promise<number[]> {
    return invoke('vault_get_thumbnail', { ...params });
  }

  async vaultUploadFilesFromPaths(params: VaultUploadFromPathsParams): Promise<VaultFile[]> {
    return invoke('vault_upload_files_from_paths', { ...params });
  }

  async vaultUploadFile(params: VaultUploadFileParams): Promise<VaultFile> {
    return invoke('vault_upload_file', { ...params });
  }

  async vaultDeleteFile(params: VaultFileDeleteParams): Promise<void> {
    return invoke('vault_delete_file', { ...params });
  }

  async vaultDeduplicateFiles(): Promise<number> {
    return invoke('vault_deduplicate_files');
  }

  async countArchivePosts(): Promise<number> {
    return invoke('count_archive_posts');
  }

  async exportVaultData(params: ExportVaultDataParams): Promise<ExportVaultDataResult> {
    return invoke('export_vault_data', { ...params });
  }

  // ═══ Posting ════════════════════════════════════════════════════

  async publishPostV2(params: PublishPostParams): Promise<PublishResult> {
    return invoke('publish_post_v2', { ...params });
  }

  async syndicateVaultItem(params: SyndicateVaultItemParams): Promise<string> {
    return invoke('syndicate_vault_item', { ...params });
  }

  async searchMentions(params: SearchMentionsParams): Promise<MentionSuggestion[]> {
    return invoke('search_mentions', { ...params });
  }

  async searchHashtagsAutocomplete(params: SearchHashtagsParams): Promise<string[]> {
    return invoke('search_hashtags_autocomplete', { ...params });
  }

  async unfurlLink(params: UnfurlLinkParams): Promise<LinkCard> {
    return invoke('unfurl_link', { ...params });
  }

  async getFileSize(params: GetFileSizeParams): Promise<number> {
    return invoke('get_file_size', { ...params });
  }

  async replyToPost(params: ReplyToPostParams): Promise<void> {
    return invoke('reply_to_post', { ...params });
  }

  async mastodonReply(params: MastodonReplyParams): Promise<void> {
    return invoke('mastodon_reply', { ...params });
  }

  // ═══ Interactions ═══════════════════════════════════════════════

  async likePost(params: LikePostParams): Promise<LikeResult> {
    return invoke('like_post', { ...params });
  }

  async unlikePost(params: UnlikePostParams): Promise<boolean> {
    return invoke('unlike_post', { ...params });
  }

  async repostPost(params: RepostPostParams): Promise<RepostResult> {
    return invoke('repost_post', { ...params });
  }

  async unrepostPost(params: UnrepostPostParams): Promise<boolean> {
    return invoke('unrepost_post', { ...params });
  }

  async mastodonFavourite(params: MastodonInteractionParams): Promise<void> {
    return invoke('mastodon_favourite', { ...params });
  }

  async mastodonUnfavourite(params: MastodonInteractionParams): Promise<void> {
    return invoke('mastodon_unfavourite', { ...params });
  }

  async mastodonReblog(params: MastodonInteractionParams): Promise<void> {
    return invoke('mastodon_reblog', { ...params });
  }

  async mastodonUnreblog(params: MastodonInteractionParams): Promise<void> {
    return invoke('mastodon_unreblog', { ...params });
  }

  async mastodonFollowHashtag(params: MastodonHashtagParams): Promise<void> {
    return invoke('mastodon_follow_hashtag', { ...params });
  }

  async mastodonUnfollowHashtag(params: MastodonHashtagParams): Promise<void> {
    return invoke('mastodon_unfollow_hashtag', { ...params });
  }

  // ═══ Media ══════════════════════════════════════════════════════

  async getMediaBlob(params: GetMediaBlobParams): Promise<string> {
    return invoke('get_media_blob', { ...params });
  }

  async getMediaServerPort(): Promise<number> {
    return invoke('get_media_server_port');
  }

  async resolveHlsVideo(params: ResolveHlsVideoParams): Promise<string> {
    return invoke('resolve_hls_video', { ...params });
  }

  async cancelVideoResolve(params: CancelVideoResolveParams): Promise<void> {
    return invoke('cancel_video_resolve', { ...params });
  }

  async resolveBlobUrlsBatch(params: ResolveBlobUrlsBatchParams): Promise<Record<string, string>> {
    return invoke('resolve_blob_urls_batch', { ...params });
  }

  // ═══ Chat ═══════════════════════════════════════════════════════

  async listVaultMessages(): Promise<VaultMessage[]> {
    return invoke('list_vault_messages');
  }

  async importVaultMessages(): Promise<unknown> {
    return invoke('import_vault_messages');
  }

  async listP2pConversations(): Promise<P2pConversation[]> {
    return invoke('list_p2p_conversations');
  }

  async sendP2pChatMessage(params: SendP2pChatMessageParams): Promise<unknown> {
    return invoke('send_p2p_chat_message', { ...params });
  }

  async getOrCreateBskyConvo(params: GetOrCreateBskyConvoParams): Promise<string> {
    return invoke('get_or_create_bsky_convo', { ...params });
  }

  async sendMessage(params: SendMessageParams): Promise<void> {
    return invoke('send_message', { ...params });
  }

  async sendBskyDm(params: SendBskyDmParams): Promise<unknown> {
    return invoke('send_bsky_dm', { ...params });
  }

  // ═══ Browser ════════════════════════════════════════════════════

  async browserCreate(params: BrowserCreateParams): Promise<void> {
    return invoke('browser_create', { ...params });
  }

  async browserNavigate(params: BrowserNavigateParams): Promise<void> {
    return invoke('browser_navigate', { ...params });
  }

  async browserEval(params: BrowserEvalParams): Promise<void> {
    return invoke('browser_eval', { ...params });
  }

  async browserShowAt(params: BrowserShowAtParams): Promise<void> {
    return invoke('browser_show_at', { ...params });
  }

  async browserHide(params: BrowserLabelParams): Promise<void> {
    return invoke('browser_hide', { ...params });
  }

  async browserClose(params: BrowserLabelParams): Promise<void> {
    return invoke('browser_close', { ...params });
  }

  async browserSetBounds(params: BrowserSetBoundsParams): Promise<void> {
    return invoke('browser_set_bounds', { ...params });
  }

  // ═══ Peer / P2P ═════════════════════════════════════════════════

  async setupIrohNode(): Promise<string> {
    return invoke('setup_iroh_node');
  }

  async listPeers(): Promise<PeerInfo[]> {
    return invoke('list_peers');
  }

  async getMyPeerInfo(): Promise<MyPeerInfo> {
    return invoke('get_my_peer_info');
  }

  async addPeer(params: AddPeerParams): Promise<PeerInfo> {
    return invoke('add_peer', { ...params });
  }

  async removePeer(params: RemovePeerParams): Promise<void> {
    return invoke('remove_peer', { ...params });
  }

  async checkPeerConnectivity(params: CheckPeerConnectivityParams): Promise<PeerConnectResult> {
    return invoke('check_peer_connectivity', { ...params });
  }

  async requestPeerFeed(params: RequestPeerFeedParams): Promise<PeerFeedResponse> {
    return invoke('request_peer_feed', { ...params });
  }

  async downloadPeerBlob(params: DownloadPeerBlobParams): Promise<boolean> {
    return invoke('download_peer_blob', { ...params });
  }

  // ═══ Widget / Profile Builder ═══════════════════════════════════

  async getProfileLayout(): Promise<ProfileLayout | null> {
    return invoke('get_profile_layout');
  }

  async saveProfileLayout(params: SaveProfileLayoutParams): Promise<void> {
    return invoke('save_profile_layout', { ...params });
  }

  async deleteProfileLayout(): Promise<void> {
    return invoke('delete_profile_layout');
  }

  async getWidgetData(): Promise<string | null> {
    return invoke('get_widget_data');
  }

  async saveWidgetData(params: SaveWidgetDataParams): Promise<void> {
    return invoke('save_widget_data', { ...params });
  }

  async listGuestbookEntries(): Promise<GuestbookEntry[]> {
    return invoke('list_guestbook_entries');
  }

  async addGuestbookEntry(params: AddGuestbookEntryParams): Promise<GuestbookEntry> {
    return invoke('add_guestbook_entry', { ...params });
  }

  async deleteGuestbookEntry(params: DeleteGuestbookEntryParams): Promise<void> {
    return invoke('delete_guestbook_entry', { ...params });
  }

  // ═══ Identity ═══════════════════════════════════════════════════

  async setLocalUsername(params: SetLocalUsernameParams): Promise<AuthStatus> {
    return invoke('set_local_username', { ...params });
  }

  async createLocalIdentity(): Promise<AuthStatus> {
    return invoke('create_local_identity');
  }

  async identityInitFullRecovery(params: IdentityInitRecoveryParams): Promise<IdentityInitResult> {
    return invoke('identity_init_full_recovery', { ...params });
  }

  async identityStartRecovery(params: IdentityStartRecoveryParams): Promise<IdentityRecoveryStartResult> {
    return invoke('identity_start_recovery', { ...params });
  }

  async identityVerifyRecovery(params: IdentityVerifyRecoveryParams): Promise<IdentityRecoveryVerifyResult> {
    return invoke('identity_verify_recovery', { ...params });
  }

  async identityFinalizeRecovery(): Promise<IdentityRecoveryFinalizeResult> {
    return invoke('identity_finalize_recovery');
  }

  // ═══ Import ═════════════════════════════════════════════════════

  async importBlueskyPosts(): Promise<number> {
    return invoke('import_bluesky_posts');
  }

  async importArchiveAuto(params: ImportArchiveAutoParams): Promise<ParseResult> {
    return invoke('import_archive_auto', { ...params });
  }

  async commitMemories(): Promise<number> {
    return invoke('commit_memories');
  }

  async importArchiveMessages(params: ImportArchiveMessagesParams): Promise<unknown> {
    return invoke('import_archive_messages', { ...params });
  }

  async debugZipContents(params: DebugZipContentsParams): Promise<string[]> {
    return invoke('debug_zip_contents', { ...params });
  }

  // ═══ Sync ═══════════════════════════════════════════════════════

  async createSyncDoc(): Promise<SyncDocResult> {
    return invoke('create_sync_doc');
  }

  async joinSyncDoc(params: JoinSyncDocParams): Promise<string> {
    return invoke('join_sync_doc', { ...params });
  }

  async migrateWillowPaths(): Promise<unknown> {
    return invoke('migrate_willow_paths');
  }

  // ═══ Collab ═════════════════════════════════════════════════════

  async getCollabState(params: GetCollabStateParams): Promise<CollabStateResponse> {
    return invoke('get_collab_state', { ...params });
  }

  async createCollabDoc(params: CreateCollabDocParams): Promise<CollabDocMeta> {
    return invoke('create_collab_doc', { ...params });
  }

  async applyCollabUpdate(params: ApplyCollabUpdateParams): Promise<void> {
    return invoke('apply_collab_update', { ...params });
  }

  async snapshotCollabToItem(params: SnapshotCollabToItemParams): Promise<string> {
    return invoke('snapshot_collab_to_item', { ...params });
  }

  // ═══ Relay (Phase 4.5 — stub) ══════════════════════════════════

  async relaySyncStatus(): Promise<RelaySyncStatus> {
    try {
      return await invoke('relay_sync_status');
    } catch {
      // Backend command doesn't exist yet — gracefully degrade
      return { status: 'offline', lastSync: null, error: 'Relay not configured' };
    }
  }
}
