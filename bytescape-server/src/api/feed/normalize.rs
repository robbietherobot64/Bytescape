//! Normalization: one function per source type, plus a shared embed parser.
//!
//! All Bluesky embed parsing goes through `parse_bluesky_embeds()`, which is
//! used by both `normalize_appview_post` (DB rows) and `normalize_bluesky_api_post` (API).

use super::{AuthorInfo, EmbedInfo, FeedPost, MediaAttachment};

/// Shared Bluesky embed parser — used by both AppView DB and API normalization.
///
/// Handles image, video, link, and quote embed types.
/// Returns `(media_attachments, embed_infos)`.
/// Classify a direct media URL by file extension. The AP ingest tags every
/// attachment `type: "image"` (even `*.mp4`), so without the sniff a Loops /
/// Pixelfed / Mastodon video arrived as `image/jpeg` and rendered as a broken
/// `<img>` instead of the video path (owner: stories rail needs real video
/// media). HLS playlists are typed video like the Bluesky playlist arm.
fn ap_media_mime(url: &str) -> &'static str {
    let path = url.split(['?', '#']).next().unwrap_or(url).to_ascii_lowercase();
    if path.ends_with(".mp4") || path.ends_with(".m4v") {
        "video/mp4"
    } else if path.ends_with(".webm") {
        "video/webm"
    } else if path.ends_with(".mov") {
        "video/quicktime"
    } else if path.ends_with(".m3u8") {
        "video/mp4"
    } else {
        "image/jpeg"
    }
}

pub fn parse_bluesky_embeds(
    embeds: &serde_json::Value,
    author_did: &str,
) -> (Vec<MediaAttachment>, Vec<EmbedInfo>) {
    let mut media = Vec::new();
    let mut embed_info = Vec::new();

    let Some(arr) = embeds.as_array() else {
        return (media, embed_info);
    };

    for e in arr {
        let etype = e.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match etype {
            "image" => {
                let cid = e.get("cid").and_then(|v| v.as_str()).unwrap_or("");
                let alt = e.get("alt").and_then(|v| v.as_str()).unwrap_or("");
                if !cid.is_empty() {
                    media.push(MediaAttachment {
                        url: format!(
                            "https://cdn.bsky.app/img/feed_fullsize/plain/{}/{}@jpeg",
                            author_did, cid
                        ),
                        thumbnail_url: Some(format!(
                            "https://cdn.bsky.app/img/feed_thumbnail/plain/{}/{}@jpeg",
                            author_did, cid
                        )),
                        mime_type: Some("image/jpeg".to_string()),
                        width: None,
                        height: None,
                        alt_text: if alt.is_empty() {
                            None
                        } else {
                            Some(alt.to_string())
                        },
                    });
                } else if let Some(raw_url) = e.get("url").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    // ActivityPub / non-Bluesky media: the embed carries a direct
                    // media URL (no cid). Use it as-is so AP post media renders —
                    // classified by extension, since the ingest tags every AP
                    // attachment "image" even for mp4 (video → video path).
                    let mime = ap_media_mime(raw_url);
                    let thumb = e
                        .get("thumbnail")
                        .or_else(|| e.get("icon"))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .or_else(|| (!mime.starts_with("video")).then(|| raw_url.to_string()));
                    media.push(MediaAttachment {
                        url: raw_url.to_string(),
                        thumbnail_url: thumb,
                        mime_type: Some(mime.to_string()),
                        width: None,
                        height: None,
                        alt_text: if alt.is_empty() {
                            None
                        } else {
                            Some(alt.to_string())
                        },
                    });
                }
            }
            "video" => {
                let vid_cid = e.get("cid").and_then(|v| v.as_str()).unwrap_or("");
                if !vid_cid.is_empty() {
                    let thumb_cid = e
                        .get("thumbnail_cid")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let thumb_url = if !thumb_cid.is_empty() {
                        Some(format!(
                            "https://cdn.bsky.app/img/feed_thumbnail/plain/{}/{}@jpeg",
                            author_did, thumb_cid
                        ))
                    } else {
                        None
                    };
                    media.push(MediaAttachment {
                        url: format!(
                            "https://video.bsky.app/watch/{}/{}/playlist.m3u8",
                            author_did, vid_cid
                        ),
                        thumbnail_url: thumb_url,
                        mime_type: Some("video/mp4".to_string()),
                        width: None,
                        height: None,
                        alt_text: None,
                    });
                } else if let Some(raw_url) = e.get("url").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    // AP Video object with a direct media URL (Loops
                    // `/ap/users/{id}/video/{id}` and friends) — no cid to
                    // derive a CDN URL from.
                    let alt = e.get("alt").and_then(|v| v.as_str()).unwrap_or("");
                    let thumb = e
                        .get("thumbnail")
                        .or_else(|| e.get("icon"))
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string());
                    media.push(MediaAttachment {
                        url: raw_url.to_string(),
                        thumbnail_url: thumb,
                        mime_type: Some(ap_media_mime(raw_url).to_string()),
                        width: None,
                        height: None,
                        alt_text: if alt.is_empty() { None } else { Some(alt.to_string()) },
                    });
                }
            }
            "link" => {
                let url = e.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let title = e.get("title").and_then(|v| v.as_str());
                let desc = e.get("description").and_then(|v| v.as_str());
                let thumb = e
                    .get("thumbnail")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty());
                if !url.is_empty() {
                    embed_info.push(EmbedInfo {
                        kind: "link".to_string(),
                        url: url.to_string(),
                        title: title.map(|s| s.to_string()),
                        description: desc.map(|s| s.to_string()),
                        thumbnail: thumb.map(|s| s.to_string()),
                    });
                }
            }
            "quote" => {
                let quote_uri = e.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                if !quote_uri.is_empty() {
                    embed_info.push(EmbedInfo {
                        kind: "quote".to_string(),
                        url: quote_uri.to_string(),
                        title: None,
                        description: None,
                        thumbnail: None,
                    });
                }
            }
            _ => {}
        }
    }

    (media, embed_info)
}

/// Parse an AppView "view"-shaped embed (app.bsky.embed.*#view) — the shape
/// returned by app.bsky.feed.getPostThread — into media attachments and
/// embed infos. Handles images, video, external link cards, quote records,
/// and recordWithMedia (quote + media).
pub fn parse_bluesky_view_embed(embed: &serde_json::Value) -> (Vec<MediaAttachment>, Vec<EmbedInfo>) {
    let mut media = Vec::new();
    let mut embed_info = Vec::new();
    let etype = embed.get("$type").and_then(|v| v.as_str()).unwrap_or("");

    let aspect = |v: &serde_json::Value| -> (Option<u32>, Option<u32>) {
        v.get("aspectRatio")
            .map(|ar| {
                (
                    ar.get("width").and_then(|x| x.as_u64()).map(|x| x as u32),
                    ar.get("height").and_then(|x| x.as_u64()).map(|x| x as u32),
                )
            })
            .unwrap_or((None, None))
    };

    if etype.contains("images") || etype.contains("gallery") {
        // images#view uses "images"; gallery#view uses "items" — same shape.
        let arr = embed
            .get("images")
            .or_else(|| embed.get("items"))
            .and_then(|v| v.as_array());
        if let Some(imgs) = arr {
            for img in imgs {
                let url = img.get("fullsize").and_then(|v| v.as_str()).unwrap_or("");
                if url.is_empty() {
                    continue;
                }
                let (width, height) = aspect(img);
                media.push(MediaAttachment {
                    url: url.to_string(),
                    thumbnail_url: img.get("thumb").or_else(|| img.get("thumbnail")).and_then(|v| v.as_str()).map(|s| s.to_string()),
                    mime_type: Some("image/jpeg".to_string()),
                    width,
                    height,
                    alt_text: img.get("alt").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string()),
                });
            }
        }
    } else if etype.contains("video") {
        let playlist = embed.get("playlist").and_then(|v| v.as_str()).unwrap_or("");
        if !playlist.is_empty() {
            let (width, height) = aspect(embed);
            media.push(MediaAttachment {
                url: playlist.to_string(),
                thumbnail_url: embed.get("thumbnail").and_then(|v| v.as_str()).map(|s| s.to_string()),
                mime_type: Some("video/mp4".to_string()),
                width,
                height,
                alt_text: embed.get("alt").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string()),
            });
        }
    } else if etype.contains("external") {
        if let Some(ext) = embed.get("external") {
            let uri = ext.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            if !uri.is_empty() {
                embed_info.push(EmbedInfo {
                    kind: "link".to_string(),
                    url: uri.to_string(),
                    title: ext.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    description: ext.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    thumbnail: ext.get("thumb").and_then(|v| v.as_str()).map(|s| s.to_string()),
                });
            }
        }
    } else if etype.contains("recordWithMedia") {
        if let Some(rec) = embed.get("record") {
            let (_, q) = parse_bluesky_view_embed(rec);
            embed_info.extend(q);
        }
        if let Some(med) = embed.get("media") {
            let (m, e) = parse_bluesky_view_embed(med);
            media.extend(m);
            embed_info.extend(e);
        }
    } else if etype.contains("record") {
        let rec = embed.get("record");
        let uri = rec.and_then(|r| r.get("uri")).and_then(|v| v.as_str()).unwrap_or("");
        if !uri.is_empty() {
            let text = rec
                .and_then(|r| r.get("value"))
                .and_then(|v| v.get("text"))
                .and_then(|v| v.as_str());
            let handle = rec
                .and_then(|r| r.get("author"))
                .and_then(|a| a.get("handle"))
                .and_then(|v| v.as_str());
            embed_info.push(EmbedInfo {
                kind: "quote".to_string(),
                url: uri.to_string(),
                title: handle.map(|s| s.to_string()),
                description: text.map(|s| s.to_string()),
                thumbnail: rec
                    .and_then(|r| r.get("author"))
                    .and_then(|a| a.get("avatar"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }

    (media, embed_info)
}

/// Normalize a Bluesky post from the AppView DB into a FeedPost.
///
/// Handles embeds, media, handles, engagement counts.
/// **Requires author_handle** — posts with NULL handle are skipped (no @unknown).
/// Collect moderation and self labels from a Bluesky API post item
/// (record self-labels, applied labels, author labels, and — crucially — the
/// labels of a quoted post).
///
/// A quote post is a clean shell around whatever it quotes: the outer record
/// carries no labels even when the quoted post is explicitly `porn`. Reading
/// only the outer labels let explicit quoted media into opted-out feeds
/// (2026-11-15 incident, post 3n2e57n3o422u), so the quote record's labels are
/// folded in here.
pub fn extract_post_labels(post: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(vals) = post.pointer("/record/labels/values").and_then(|v| v.as_array()) {
        out.extend(vals.iter().filter_map(|v| v.as_str()).map(|s| s.to_lowercase()));
    }
    if let Some(labels) = post.get("labels").and_then(|v| v.as_array()) {
        out.extend(labels.iter().filter_map(|l| l.get("val")).filter_map(|v| v.as_str()).map(|s| s.to_lowercase()));
    }
    if let Some(author_labels) = post.pointer("/author/labels").and_then(|v| v.as_array()) {
        out.extend(author_labels.iter().filter_map(|l| l.get("val")).filter_map(|v| v.as_str()).map(|s| s.to_lowercase()));
    }
    // Quoted post labels: view-form `labels: [{val}]` and record-form
    // `value.labels.values: [{val}]`. Both `embed.record` (getFeed/getAuthorFeed)
    // and `record.embed.record` (record-shaped embeds) are covered, including
    // the doubly-wrapped `record.record` shape some proxies return.
    for ptr in [
        "/embed/record/labels",
        "/embed/record/record/labels",
        "/record/embed/record/labels",
        "/record/embed/record/record/labels",
        "/embed/record/value/labels/values",
        "/embed/record/record/value/labels/values",
        "/record/embed/record/value/labels/values",
        "/record/embed/record/record/value/labels/values",
    ] {
        if let Some(arr) = post.pointer(ptr).and_then(|v| v.as_array()) {
            out.extend(
                arr.iter()
                    .filter_map(|l| l.as_str().or_else(|| l.get("val").and_then(|v| v.as_str())))
                    .map(|s| s.to_lowercase()),
            );
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Parse the `appview_posts.labels` column (JSON array of strings) into the
/// label list the feed policy consumes. Lowercased and deduped so the SQL,
/// ingest and in-memory checks agree even for records that self-label `NSFW`.
pub fn labels_from_json(labels: &Option<serde_json::Value>) -> Vec<String> {
    let mut out: Vec<String> = labels
        .as_ref()
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

/// Content-type bucket for the feed type filter (the FE emoji selector).
/// Posts classify by embed shape; any other record type classifies by NSID
/// keywords so custom lexicons (events, marketplace, streams, lists) surface
/// in the feed before we build dedicated modules. `uri` is at://did/collection/rkey.
///
/// ActivityPub rows are ingress-qualified by an `https://` object URL, which has
/// no collection NSID — they classify by the same embed/media shape the AT
/// branch uses, so a fediverse image/video/link post lands in the bucket the
/// type filter can select instead of the unselectable `"other"`.
pub fn classify_content(
    uri: &str,
    content: &str,
    media: &[MediaAttachment],
    embeds: &[EmbedInfo],
) -> &'static str {
    if uri.starts_with("https://") {
        return classify_embed_shape(content, media, embeds);
    }
    // Segment 3 of the at:// URI is the collection NSID.
    let collection = uri.split('/').nth(3).unwrap_or("app.bsky.feed.post");
    if collection != "app.bsky.feed.post" {
        let c = collection.to_ascii_lowercase();
        return if c.contains("calendar") || c.contains("event") || c.contains("rsvp") {
            "event"
        } else if c.contains("market") || c.contains("listing") || c.contains("classified")
            || c.contains("forsale") || c.contains("sale") || c.contains("offer")
            || c.contains("shop") || c.contains("trade") || c.contains("store") {
            "market"
        } else if c.contains("stream") || c.contains("live") || c.contains("broadcast") {
            "live"
        } else if c.contains("article") || c.contains("blog") || c.contains("longform")
            || c.contains("essay") || c.contains("newsletter") || c.contains("document") {
            "blog"
        } else if c.contains("audio") || c.contains("podcast") || c.contains("song")
            || c.contains("track") || c.contains("music") || c.contains("album")
            || c.contains("scrobble") || c.contains("sound") || c.contains("play") {
            "audio"
        } else if c.contains("poll") || c.contains("vote") {
            "poll"
        } else if c.contains("generator") || c.contains("list") || c.contains("starterpack") {
            "list"
        } else if c.contains("feed.post") || c.contains("microblog") || c.contains("note")
            || c.contains("content") {
            "text"
        } else {
            "other"
        };
    }
    // app.bsky.feed.post: embed shape wins over text length.
    classify_embed_shape(content, media, embeds)
}

/// Embed/media shape → content bucket. Shared by the AT collection branch and
/// the AP `https://` branch so the two cannot drift.
fn classify_embed_shape(
    content: &str,
    media: &[MediaAttachment],
    embeds: &[EmbedInfo],
) -> &'static str {
    if media.iter().any(|m| m.mime_type.as_deref().map_or(false, |mt| mt.starts_with("video/")))
        || embeds.iter().any(|e| e.kind == "video" || e.kind == "youtube") {
        "video"
    } else if embeds.iter().any(|e| e.kind == "soundcloud" || e.kind == "bandcamp" || e.kind == "audio") {
        "audio"
    } else if !media.is_empty() {
        "image"
    } else if embeds.iter().any(|e| e.kind == "link") {
        "link"
    } else if content.len() > 500 {
        "long"
    } else {
        "short"
    }
}

pub fn normalize_appview_post(
    uri: String,
    cid: String,
    author_did: String,
    author_handle: Option<String>,
    author_avatar: Option<String>,
    author_display_name: Option<String>,
    text: Option<String>,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    embeds: Option<serde_json::Value>,
    labels: Vec<String>,
    like_count: i32,
    repost_count: i32,
    reply_count: i32,
) -> Option<FeedPost> {
    // Handles are display data, not existence data. The handle column is
    // slowly backfilled — dropping handle-less posts shrank every index
    // feed to the ~1% of rows that happened to have one. Fall back to the
    // DID; the backfill catches up in the background.
    let handle = author_handle
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| author_did.clone());

    let content = text.unwrap_or_default();
    let (media, embed_info) = match &embeds {
        Some(emb) => parse_bluesky_embeds(emb, &author_did),
        None => (Vec::new(), Vec::new()),
    };

    // Image/external alt text is an NSFW signal — spam packs explicit
    // descriptions there while keeping text and labels clean. The API path
    // folds it at normalize time (see normalize_bluesky_api_post); the DB
    // path reads the stored embed array, so fold the same strings here.
    // Without this, the recordWithMedia posts the ingest fix started
    // storing would render their media past the policy.
    let mut check_text = content.clone();
    if let Some(entries) = embeds.as_ref().and_then(|e| e.as_array()) {
        for entry in entries {
            if let Some(alt) = entry.get("alt").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                check_text.push(' ');
                check_text.push_str(alt);
            }
        }
    }
    let nsfw = crate::api::feed::filter::is_nsfw_content(&check_text, &labels);

    let ct = classify_content(&uri, &content, &media, &embed_info).to_string();
    let source = source_from_uri(&uri);
    Some(FeedPost {
        id: uri.clone(),
        source: source.to_string(),
        author: AuthorInfo {
            handle: format_handle(&handle),
            display_name: author_display_name.filter(|s| !s.is_empty()),
            avatar: author_avatar.filter(|s| !s.is_empty()),
        },
        content,
        content_type: ct,
        media,
        embeds: embed_info,
        likes_count: like_count.max(0) as u32,
        shares_count: repost_count.max(0) as u32,
        replies_count: reply_count.max(0) as u32,
        created_at: created_at
            .map(|c| c.to_rfc3339())
            .unwrap_or_default(),
        url: Some(uri.clone()),
        thread_id: Some(format!("{}:{}", source, uri)),
        start_time: None,
        end_time: None,
        location: None,
        display_name: None,
        action_text: None,
        group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
        reaction_counts: None,
        user_reaction: None,
        cid: Some(cid),
        author_url: Some(if uri.starts_with("https://") {
            // AP row: author_did holds the AP actor URL; use it directly.
            author_did.clone()
        } else {
            format!("https://bsky.app/profile/{}", author_did)
        }),
        reposted_by: None,
        boosted_by: None,
        quoted_post_uri: None,
        is_reshare: Some(false),
        original_post: None,
        is_own: None,
        is_following: None,
        follow_pending: None,
        cross_posts: vec![],
        matched_sources: vec![],
        labels,
        nsfw,
        author_external_id: Some(author_did),
    })
}

/// Normalize a Bluesky post from the API (hot-classic feed, getFeed).
///
/// Uses the SAME embed/media parsing as `normalize_appview_post`.
pub fn normalize_bluesky_api_post(item: &serde_json::Value) -> Option<FeedPost> {
    let post = item.get("post").unwrap_or(item);

    // Skip reply posts
    if post.pointer("/record/reply").is_some() {
        return None;
    }

    // Extract repost info from reason field
    let reposted_by = item
        .get("reason")
        .and_then(|r| r.get("$type").and_then(|t| t.as_str()))
        .filter(|t| *t == "app.bsky.feed.defs#reasonRepost")
        .and_then(|_| item.pointer("/reason/by/handle"))
        .and_then(|h| h.as_str())
        .map(format_handle);

    // Extract quote post URI from embed
    let quoted_post_uri = post.get("embed").and_then(|e| {
        let t = e.get("$type").and_then(|v| v.as_str())?;
        if t == "app.bsky.embed.record" || t == "app.bsky.embed.recordWithMedia" {
            e.pointer("/record/uri")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        }
    });

    let author = post.get("author")?;
    let record = post.get("record")?;

    let raw_handle = author["handle"].as_str()?;
    let handle = format_handle(raw_handle);
    let display_name = author["displayName"].as_str();
    let avatar = author["avatar"].as_str();

    let text = record["text"].as_str().unwrap_or("");
    let created_at = record["createdAt"].as_str().unwrap_or("");

    let like_val = post["likeCount"].as_u64().unwrap_or(0) as u32;
    let repost_val = post["repostCount"].as_u64().unwrap_or(0) as u32;
    let reply_val = post["replyCount"].as_u64().unwrap_or(0) as u32;

    let uri = post["uri"].as_str();
    let cid = post["cid"].as_str().unwrap_or("");
    let author_did = author["did"].as_str().unwrap_or("");

    // Extract embeds via the shared view-form parser (images, gallery, video,
    // external link cards, quotes, recordWithMedia).
    let (media, embeds) = post
        .get("embed")
        .map(parse_bluesky_view_embed)
        .unwrap_or_default();

    let author_url = format!("https://bsky.app/profile/{}", raw_handle);

    let mut built = FeedPost {
        id: uri.unwrap_or(cid).to_string(),
        source: "bluesky".to_string(),
        author: AuthorInfo {
            handle: handle.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            avatar: avatar.map(|s| s.to_string()),
        },
        content: text.to_string(),
        content_type: classify_content(uri.unwrap_or(cid), &text, &media, &embeds).to_string(),
        media,
        embeds,
        likes_count: like_val,
        shares_count: repost_val,
        replies_count: reply_val,
        created_at: created_at.to_string(),
        url: uri.map(|s| s.to_string()),
        thread_id: Some(format!("bluesky:{}", post["uri"].as_str().unwrap_or(cid))),
        start_time: None,
        end_time: None,
        location: None,
        display_name: None,
        action_text: None,
        group_name: None, group: None, upvotes: None, downvotes: None, my_vote: None,
        reaction_counts: None,
        user_reaction: None,
        cid: Some(cid.to_string()),
        author_url: Some(author_url),
        reposted_by,
        boosted_by: None,
        quoted_post_uri,
        is_reshare: Some(false),
        original_post: None,
        is_own: None,
        is_following: None,
        follow_pending: None,
        cross_posts: vec![],
        matched_sources: vec![],
        labels: vec![],
        nsfw: false,
        author_external_id: Some(author_did.to_string()),
    };
    built.labels = extract_post_labels(post);
    // Fold image/external alt text into the NSFW check — spam packs explicit
    // descriptions there while keeping labels and tags clean.
    let mut check_text = built.content.clone();
    for ptr in [
        "/embed/images",
        "/record/embed/images",
        "/embed/items",
        "/record/embed/items",
        "/embed/media/images",
        "/embed/media/items",
        "/record/embed/media/images",
        "/record/embed/media/items",
    ] {
        if let Some(arr) = post.pointer(ptr).and_then(|v| v.as_array()) {
            for img in arr {
                if let Some(alt) = img.get("alt").and_then(|v| v.as_str()) {
                    check_text.push(' ');
                    check_text.push_str(alt);
                }
            }
        }
    }
    if let Some(ext) = post
        .pointer("/embed/external")
        .or_else(|| post.pointer("/record/embed/external"))
    {
        for k in ["title", "description"] {
            if let Some(s) = ext.get(k).and_then(|v| v.as_str()) {
                check_text.push(' ');
                check_text.push_str(s);
            }
        }
    }
    // Quoted-post media alt text ("Nsfw🔞", etc.) — the quoted record's own
    // description is the only signal on media-heavy posts with empty text.
    for ptr in [
        "/embed/record/value/embed",
        "/record/embed/record/value/embed",
        "/embed/record/record/value/embed",
        "/record/embed/record/record/value/embed",
    ] {
        if let Some(emb) = post.pointer(ptr) {
            if let Some(alt) = emb.get("alt").and_then(|v| v.as_str()) {
                check_text.push(' ');
                check_text.push_str(alt);
            }
            for arr_key in ["images", "items"] {
                if let Some(arr) = emb.get(arr_key).and_then(|v| v.as_array()) {
                    for img in arr {
                        if let Some(alt) = img.get("alt").and_then(|v| v.as_str()) {
                            check_text.push(' ');
                            check_text.push_str(alt);
                        }
                    }
                }
            }
        }
    }
    built.nsfw = crate::api::feed::filter::is_nsfw_content(&check_text, &built.labels);
    Some(built)
}

/// Ordered ActivityPub URI markers → source label. First match wins; the order
/// is load-bearing: PieFed is checked before Pixelfed's generic `/p/`, and the
/// short `/c/`/`/u/`/`/m/` shapes come after the longer ones.
///
/// This is the single source of truth for URI-derived network labels: both
/// [`source_from_uri`] and [`source_case_sql`] are generated from it, so the
/// platform counts SQL cannot drift from the label applied to feed items.
pub const AP_SOURCE_URI_RULES: &[(&str, &str)] = &[
    ("piefed", "lemmy"),
    ("/stories/", "pixelfed"),
    ("pixelfed", "pixelfed"),
    ("/p/", "pixelfed"),
    ("/post/", "lemmy"),
    ("/comment/", "lemmy"),
    ("/c/", "lemmy"),
    ("/u/", "lemmy"),
    ("/m/", "lemmy"),
    ("lemmy", "lemmy"),
];

/// Derive the feed `source` from a post's URI. AT Protocol posts use `at://`
/// URIs → "bluesky". ActivityPub posts are ingested with their object URL
/// (`https://…`) as the uri; the network is derived from the object's path shape
/// (reliable across hosts — `sh.itjust.works` is Lemmy without "lemmy" in the
/// name). Path shapes: Pixelfed `/p/`,`/stories/`; Lemmy `/post/`,`/comment/`,
/// community `/c/`,`/m/`, user `/u/`; everything else (Mastodon and other
/// Mastodon-shaped AP software) → "mastodon". Single source-of-truth for
/// `FeedPost.source`.
///
/// PieFed is checked first and labelled "lemmy": its object URLs contain
/// `/p/` (post permalinks) *and* PieFed is forum-shaped, so the Lemmy card
/// path (title line + community) is the right renderer and the wrong-network
/// "pixelfed" label would be silently applied otherwise.
pub fn source_from_uri(uri: &str) -> &'static str {
    if uri.starts_with("https://") {
        let lower = uri.to_ascii_lowercase();
        AP_SOURCE_URI_RULES
            .iter()
            .find(|(needle, _)| lower.contains(needle))
            .map(|(_, source)| *source)
            .unwrap_or("mastodon")
    } else {
        "bluesky"
    }
}

/// SQL `CASE` expression that derives the same label as [`source_from_uri`]
/// for a `uri` column. Used as a `GROUP BY` key by the platform counts query so
/// the aggregate and the per-item label share one rule table.
pub fn source_case_sql(uri_col: &str) -> String {
    let mut sql = format!("CASE WHEN lower({uri_col}) LIKE 'https://%' THEN CASE");
    for (needle, source) in AP_SOURCE_URI_RULES {
        sql.push_str(&format!(
            " WHEN lower({uri_col}) LIKE '%{needle}%' THEN '{source}'"
        ));
    }
    sql.push_str(" ELSE 'mastodon' END ELSE 'bluesky' END");
    sql
}

/// Display handle: plain `@name` with no PDS suffix. Unresolvable sentinels
/// (`handle.invalid`) map to "unknown" — FeedFilters drops those posts per the
/// long-standing "no @unknown" policy.
pub fn format_handle(raw_handle: &str) -> String {
    let bare = raw_handle.trim_start_matches('@').split('@').next().unwrap_or("");
    if bare.is_empty() || bare == "handle.invalid" {
        "@unknown".to_string()
    } else {
        format!("@{}", bare)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 2026-11-15 leak: a clean outer post quoting an explicitly labeled
    /// one. The shell's text carries no markers and its own labels are empty —
    /// only the quoted record's labels can flag it.
    #[test]
    fn quote_post_inherits_quoted_labels() {
        let item = serde_json::json!({
            "post": {
                "uri": "at://did:plc:author/app.bsky.feed.post/outer",
                "cid": "bafyouter",
                "author": { "did": "did:plc:author", "handle": "quote.example" },
                "record": {
                    "$type": "app.bsky.feed.post",
                    "text": "Omfg it's SO tight Kitten!!!",
                    "embed": {
                        "$type": "app.bsky.embed.record",
                        "record": { "uri": "at://did:plc:x/app.bsky.feed.post/inner", "cid": "bafyinner" }
                    }
                },
                "embed": {
                    "$type": "app.bsky.embed.record#view",
                    "record": {
                        "uri": "at://did:plc:x/app.bsky.feed.post/inner",
                        "cid": "bafyinner",
                        "author": { "did": "did:plc:x", "handle": "inner.example" },
                        "value": {
                            "$type": "app.bsky.feed.post",
                            "text": "",
                            "embed": { "$type": "app.bsky.embed.video", "alt": "Nsfw" },
                            "labels": {
                                "$type": "com.atproto.label.defs#selfLabels",
                                "values": [{ "val": "graphic-media" }, { "val": "porn" }]
                            }
                        },
                        "labels": [{ "val": "porn", "src": "did:plc:labeler" }]
                    }
                }
            }
        });

        let post = normalize_bluesky_api_post(&item).expect("normalizes");
        assert!(post.labels.iter().any(|l| l == "porn"), "quote labels folded: {:?}", post.labels);
        assert!(post.nsfw, "quoted label must flag the outer shell");

        let mut hidden = vec![post.clone()];
        crate::api::feed::filter::apply_nsfw_policy(&mut hidden, crate::db::users::NsfwPref::Exclude);
        assert!(hidden.is_empty(), "exclude viewer must not receive the quote shell");

        let mut kept = vec![post];
        crate::api::feed::filter::apply_nsfw_policy(&mut kept, crate::db::users::NsfwPref::Blur);
        assert_eq!(kept.len(), 1);
        assert!(kept[0].nsfw, "blur viewer keeps it, marked for blur");
    }

    /// The DB (AppView) path used to discard `appview_posts.labels` entirely.
    #[test]
    fn appview_rows_keep_their_labels() {
        let post = normalize_appview_post(
            "at://did:plc:x/app.bsky.feed.post/db1".to_string(),
            "bafydb".to_string(),
            "did:plc:x".to_string(),
            Some("db.example".to_string()),
            None,
            None,
            Some("clean text with no markers".to_string()),
            Some(chrono::Utc::now()),
            Some(serde_json::json!([{ "uri": "at://did:plc:y/app.bsky.feed.post/q", "type": "quote" }])),
            vec!["graphic-media".to_string()],
            0,
            0,
            0,
        )
        .expect("normalizes");

        assert_eq!(post.labels, vec!["graphic-media".to_string()]);
        let mut posts = vec![post];
        crate::api::feed::filter::apply_nsfw_policy(&mut posts, crate::db::users::NsfwPref::Exclude);
        assert!(posts.is_empty(), "DB labels must reach the policy");
    }

    #[test]
    fn labels_from_json_lowercases_and_dedups() {
        let labels = Some(serde_json::json!(["NSFW", "porn", "porn", "cooking"]));
        assert_eq!(
            labels_from_json(&labels),
            vec!["cooking".to_string(), "nsfw".to_string(), "porn".to_string()]
        );
        assert!(labels_from_json(&None).is_empty());
    }

    #[test]
    fn source_from_uri_detects_network_by_path_shape() {
        assert_eq!(source_from_uri("at://did:plc:abc/app.bsky.feed.post/1"), "bluesky");
        // Mastodon object shapes.
        assert_eq!(
            source_from_uri("https://mastodon.social/users/alice/statuses/1"),
            "mastodon"
        );
        assert_eq!(source_from_uri("https://example.social/@alice/1"), "mastodon");
        // Lemmy: path-shape detection works even when the host has no "lemmy".
        assert_eq!(source_from_uri("https://sh.itjust.works/post/123"), "lemmy");
        assert_eq!(source_from_uri("https://beehaw.org/c/technology"), "lemmy");
        assert_eq!(source_from_uri("https://lemmy.world/u/alice"), "lemmy");
        // Pixelfed.
        assert_eq!(source_from_uri("https://pixelfed.social/p/alice/1"), "pixelfed");
        assert_eq!(source_from_uri("https://example.social/stories/alice/1"), "pixelfed");
        // PieFed object URLs carry `/p/` too; they are forum-shaped, so they
        // must not be labelled pixelfed.
        assert_eq!(
            source_from_uri(
                "https://piefed.social/c/piefed_meta@piefed.social/p/2603915/piefed-v1-9"
            ),
            "lemmy"
        );
    }

    /// The counts SQL must encode exactly the same rules, in the same order, as
    /// `source_from_uri` — otherwise the platform mix would disagree with the
    /// labels on the posts it counts.
    #[test]
    fn source_case_sql_mirrors_uri_rules_in_order() {
        let sql = source_case_sql("uri");
        assert!(sql.starts_with("CASE WHEN lower(uri) LIKE 'https://%' THEN CASE"));
        assert!(sql.ends_with(" ELSE 'mastodon' END ELSE 'bluesky' END"));
        let mut cursor = 0;
        for (needle, source) in AP_SOURCE_URI_RULES {
            let branch = format!("WHEN lower(uri) LIKE '%{needle}%' THEN '{source}'");
            let at = sql[cursor..]
                .find(&branch)
                .unwrap_or_else(|| panic!("missing or out-of-order branch {branch}: {sql}"));
            cursor += at + branch.len();
        }
    }

    fn media(url: &str) -> MediaAttachment {
        MediaAttachment {
            url: url.to_string(),
            thumbnail_url: None,
            mime_type: Some("image/jpeg".to_string()),
            width: None,
            height: None,
            alt_text: None,
        }
    }

    fn link(url: &str) -> EmbedInfo {
        EmbedInfo {
            kind: "link".to_string(),
            url: url.to_string(),
            title: None,
            description: None,
            thumbnail: None,
        }
    }

    /// Fediverse (`https://`) rows used to fall through to `"other"`, which no
    /// type filter can select. They now classify by embed shape like AT posts.
    #[test]
    fn classify_content_buckets_fediverse_rows() {
        // Lemmy image post (Pictrs thumbnail → stored as an image embed).
        assert_eq!(
            classify_content(
                "https://lemmy.world/post/54445374",
                "Nice Redhead",
                &[media("https://lemmy.world/pictrs/image/a.png")],
                &[]
            ),
            "image"
        );
        // Lemmy link post: a link embed, never an image.
        assert_eq!(
            classify_content(
                "https://awful.systems/post/10243225",
                "Stubsack: weekly thread",
                &[],
                &[link("https://awful.systems/post/10243225/13187737")]
            ),
            "link"
        );
        // Text-only Mastodon/Pixelfed post.
        assert_eq!(
            classify_content(
                "https://mastodon.social/users/alice/statuses/1",
                "hello",
                &[],
                &[]
            ),
            "short"
        );
        // Long-form.
        let long = "x".repeat(501);
        assert_eq!(
            classify_content("https://mastodon.social/@a/1", &long, &[], &[]),
            "long"
        );
        // AT behaviour is unchanged.
        assert_eq!(
            classify_content(
                "at://did:plc:a/app.bsky.feed.post/1",
                "hi",
                &[media("https://cdn.example/a.jpg")],
                &[]
            ),
            "image"
        );
        assert_eq!(
            classify_content("at://did:plc:a/app.bsky.feed.post/1", "hi", &[], &[]),
            "short"
        );
    }
}
