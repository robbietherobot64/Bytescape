    use super::*;
    use crate::ap::testutil::{connect_pool, enable_ap_flags, make_local_actor, test_state, unique};
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    fn authed(req: axum::http::request::Builder, token: &str) -> axum::http::request::Builder {
        req.header("Authorization", format!("Bearer {token}"))
    }

    async fn get_json(app: &axum::Router, path: &str, token: Option<&str>) -> serde_json::Value {
        let mut req = Request::builder().method("GET").uri(path).header("host", "test");
        if let Some(t) = token {
            req = authed(req, t);
        }
        let resp = app
            .clone()
            .oneshot(req.body(Body::empty()).expect("request"))
            .await
            .expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "GET {path}");
        let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024).await.expect("body");
        serde_json::from_slice(&bytes).expect("json")
    }

    /// Merged followers: local + inbound AP rows, provenance tags, viewer
    /// follow-state, and remove affordance on one's own profile. The
    /// home-REST source is fail-open (dead proxy here) — rows still serve.
    #[tokio::test]
    async fn merged_followers_unions_sources_with_provenance() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::api::profile::routes().with_state(state.clone());

        let owner = make_local_actor(&pool, &domain).await;
        let local_fan = make_local_actor(&pool, &domain).await;
        let s = unique();

        // (a) local follower row (local_fan follows the owner's actor URL).
        let owner_actor = format!("https://{}/@{}", domain, owner.username);
        crate::db::follows::FollowRow::create(
            &pool,
            local_fan.user.id,
            &owner_actor,
            "native",
            None,
        )
        .await
        .expect("local fan row");
        crate::db::follows::FollowRow::set_accepted(&pool, local_fan.user.id, &owner_actor)
            .await
            .expect("accept");

        // (b) inbound AP follower with cached display data.
        let actor_id = format!("https://probe.ap.test/users/mergefan_{s}");
        sqlx::query(
            "INSERT INTO ap_remote_actors (actor_id, inbox_url, public_key_pem, preferred_username, display_name, fetched_at, updated_at) \
             VALUES ($1, $2, 'PEM', $3, 'Merge Fan', now(), now())",
        )
        .bind(&actor_id)
        .bind(format!("{actor_id}/inbox"))
        .bind(format!("mergefan_{s}"))
        .execute(&pool)
        .await
        .expect("seed actor");
        sqlx::query(
            "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) VALUES ($1, $2, $3, true)",
        )
        .bind(&actor_id)
        .bind(format!("{actor_id}/inbox"))
        .bind(owner.user.id)
        .execute(&pool)
        .await
        .expect("inbound follower");

        // (d) fail-open: a Mastodon link whose home instance is dead.
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            owner.user.id,
            "mastodon",
            &format!("deadhome_{}", unique()),
            "deadhome",
            "tok",
            Some("refresh"),
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read"),
            &serde_json::json!({ "instance": "http://127.0.0.1:1" }),
        )
        .await
        .expect("dead home link");

        let token = crate::auth::create_token(owner.user.id, &owner.username, &state.config.jwt_secret)
            .expect("token");
        let rows = get_json(
            &app,
            &format!("/api/profile/{}/followers", owner.username),
            Some(&token),
        )
        .await;
        let rows = rows["rows"].as_array().expect("rows");

        let local = rows
            .iter()
            .find(|r| r["username"] == local_fan.username.as_str())
            .expect("local follower present");
        assert_eq!(local["network"], "local");
        assert_eq!(local["can_remove"], true, "own profile can remove");

        let inbound = rows
            .iter()
            .find(|r| r["handle"] == actor_id.as_str())
            .expect("inbound AP follower present");
        assert_eq!(inbound["network"], "activitypub");
        assert_eq!(inbound["identity_used"], actor_id);
        assert_eq!(inbound["display_name"], "Merge Fan", "ap_remote_actors display join");
        assert_eq!(inbound["can_remove"], true);

        // Fail-open: the dead home proxy must not error the endpoint (rows
        // above arrived) and must not fabricate rows.
        assert!(
            !rows.iter().any(|r| r["username"] == "deadhome"),
            "dead proxy contributes nothing but never errors"
        );

        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(vec![owner.user.id, local_fan.user.id])
            .execute(&pool)
            .await;
    }

    /// Merged following: rows tagged with network + identity provenance.
    #[tokio::test]
    async fn merged_following_tags_network_and_identity() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::api::profile::routes().with_state(state.clone());

        let owner = make_local_actor(&pool, &domain).await;
        let pal = make_local_actor(&pool, &domain).await;
        let s = unique();
        let remote = format!("https://probe.ap.test/users/pal_{s}");

        let pal_actor = format!("https://{}/@{}", domain, pal.username);
        crate::db::follows::FollowRow::create(&pool, owner.user.id, &pal_actor, "native", None)
            .await
            .expect("local follow");
        crate::db::follows::FollowRow::set_accepted(&pool, owner.user.id, &pal_actor)
            .await
            .expect("accept");
        crate::db::follows::FollowRow::create(&pool, owner.user.id, &remote, "ap", None)
            .await
            .expect("ap follow");
        crate::db::follows::FollowRow::set_identity(&pool, owner.user.id, &remote, &owner.username)
            .await
            .expect("identity");

        let token = crate::auth::create_token(owner.user.id, &owner.username, &state.config.jwt_secret)
            .expect("token");
        let rows = get_json(
            &app,
            &format!("/api/profile/{}/following", owner.username),
            Some(&token),
        )
        .await;
        let rows = rows["rows"].as_array().expect("rows");

        let local = rows
            .iter()
            .find(|r| r["handle"] == pal_actor.as_str())
            .expect("local row");
        assert_eq!(local["network"], "local");

        let ap = rows
            .iter()
            .find(|r| r["handle"] == remote.as_str())
            .expect("ap row");
        assert_eq!(ap["network"], "activitypub");
        assert_eq!(ap["identity_used"], owner.username, "provenance recorded");
        assert_eq!(ap["following"], true, "viewer state from the shared matcher");

        let _ = sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(vec![owner.user.id, pal.user.id])
            .execute(&pool)
            .await;
    }

    /// Counts include inbound followers (the profile used to ignore them).
    #[tokio::test]
    async fn profile_count_includes_inbound_followers() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::api::profile::routes().with_state(state.clone());

        let owner = make_local_actor(&pool, &domain).await;
        let s = unique();
        let actor_id = format!("https://probe.ap.test/users/countfan_{s}");
        sqlx::query(
            "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) VALUES ($1, $2, $3, true)",
        )
        .bind(&actor_id)
        .bind(format!("{actor_id}/inbox"))
        .bind(owner.user.id)
        .execute(&pool)
        .await
        .expect("inbound follower");

        let doc = get_json(&app, &format!("/api/profile/{}", owner.username), None).await;
        assert!(
            doc["followers_count"].as_i64().unwrap_or(0) >= 1,
            "inbound followers counted: {doc}"
        );

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.user.id)
            .execute(&pool)
            .await;
    }

    /// Removing an inbound AP follower retracts ON-NETWORK: an Undo of the
    /// stored Follow is stored + queued to the follower's inbox before the
    /// row goes (tasks.md 4.2).
    #[tokio::test]
    async fn remove_ap_follower_retracts_on_network() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::api::profile::routes().with_state(state.clone());

        let owner = make_local_actor(&pool, &domain).await;
        let s = unique();
        let actor_id = format!("https://probe.ap.test/users/gonefan_{s}");
        let inbox = format!("{actor_id}/inbox");
        sqlx::query(
            "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) VALUES ($1, $2, $3, true)",
        )
        .bind(&actor_id)
        .bind(&inbox)
        .bind(owner.user.id)
        .execute(&pool)
        .await
        .expect("inbound follower");
        // The original Follow activity the Undo will reference.
        let follow_id = format!("{actor_id}/activities/follow-{s}");
        sqlx::query(
            "INSERT INTO ap_activities (id, actor_id, activity_type, object_id, direction, payload, user_id) \
             VALUES ($1, $2, 'Follow', $3, 'in', '{}'::jsonb, $4)",
        )
        .bind(&follow_id)
        .bind(&actor_id)
        .bind(format!("https://{}/@{}", domain, owner.username))
        .bind(owner.user.id)
        .execute(&pool)
        .await
        .expect("original follow");

        let token = crate::auth::create_token(owner.user.id, &owner.username, &state.config.jwt_secret)
            .expect("token");
        let req = authed(
            Request::builder()
                .method("POST")
                .uri(format!("/api/profile/{}/followers/remove", owner.username))
                .header("host", "test")
                .header("Content-Type", "application/json"),
            &token,
        )
        .body(Body::from(serde_json::json!({ "actor_id": actor_id }).to_string()))
        .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "remove accepted");

        // Row gone.
        let gone: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM followers WHERE user_id = $1 AND actor_id = $2",
        )
        .bind(owner.user.id)
        .bind(&actor_id)
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(gone, 0, "follower row removed");

        // On-network retraction: an Undo of the stored Follow is queued to
        // the follower's inbox.
        let undo_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ap_delivery_queue q JOIN ap_activities a ON a.id = q.activity_id \
             WHERE a.activity_type = 'Undo' AND a.object_id = $1 AND q.inbox_url = $2",
        )
        .bind(&follow_id)
        .bind(&inbox)
        .fetch_one(&pool)
        .await
        .expect("undo queue count");
        assert!(undo_rows >= 1, "the Undo is queued to the follower's inbox");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.user.id)
            .execute(&pool)
            .await;
    }

    /// Phase 6: a hidden home graph degrades the merged read to the STORED
    /// rows and the response discloses the hidden state — the live home proxy
    /// is never called.
    #[tokio::test]
    async fn hidden_home_graph_degrades_to_stored_rows() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::api::profile::routes().with_state(state.clone());

        let owner = make_local_actor(&pool, &domain).await;
        // Mock home that reports hide_collections=true (verify_credentials).
        let (base, heads, thread) = crate::oauth::mastodon::stub_http(
            "HTTP/1.1 200 OK",
            r#"{"id":"9","username":"o","acct":"o","hide_collections":true}"#,
            1,
        );
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            owner.user.id,
            "mastodon",
            &format!("hiddenhome_{}", unique()),
            "hiddenhome",
            "tok",
            Some("refresh"),
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read"),
            &serde_json::json!({ "instance": base }),
        )
        .await
        .expect("link hidden home");
        // The stored mirror row (the degraded source).
        let s = unique();
        let stored = format!("https://probe.ap.test/users/stored_{s}");
        crate::db::follows::FollowRow::create(&pool, owner.user.id, &stored, "mastodon_import", None)
            .await
            .expect("stored row");
        crate::db::follows::FollowRow::set_accepted(&pool, owner.user.id, &stored)
            .await
            .expect("accept");

        let rows = get_json(
            &app,
            &format!("/api/profile/{}/following", owner.username),
            None,
        )
        .await;
        assert_eq!(rows["home_graph_hidden"], true, "the hidden state is disclosed");
        assert!(rows["home_instance"].as_str().is_some(), "the managing instance is named");
        let list = rows["rows"].as_array().expect("rows");
        assert!(
            list.iter().any(|r| r["handle"] == stored.as_str()),
            "the stored mirror row survives the degrade: {list:?}"
        );
        let head = heads.lock().clone();
        assert!(
            head.contains("verify_credentials"),
            "the home state was read: {head}"
        );
        assert!(
            !head.contains("/following"),
            "the live home graph walk is skipped when hidden: {head}"
        );
        let _ = thread.join();

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.user.id)
            .execute(&pool)
            .await;
    }

    /// Phase 6: the graph_visibility toggle is enforced on the merged read —
    /// a private graph withholds rows from everyone but its owner.
    #[tokio::test]
    async fn private_graph_withholds_rows_from_viewers() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::api::profile::routes().with_state(state.clone());

        let owner = make_local_actor(&pool, &domain).await;
        let s = unique();
        let actor_id = format!("https://probe.ap.test/users/privfan_{s}");
        sqlx::query(
            "INSERT INTO followers (actor_id, inbox_url, user_id, accepted) VALUES ($1, $2, $3, true)",
        )
        .bind(&actor_id)
        .bind(format!("{actor_id}/inbox"))
        .bind(owner.user.id)
        .execute(&pool)
        .await
        .expect("follower");
        crate::db::users::UserRow::set_graph_visibility(&pool, owner.user.id, "private")
            .await
            .expect("private");

        // A viewer sees the hidden state and no rows.
        let doc = get_json(
            &app,
            &format!("/api/profile/{}/followers", owner.username),
            None,
        )
        .await;
        assert_eq!(doc["hidden"], true);
        assert!(doc["rows"].as_array().unwrap().is_empty(), "no rows leak: {doc}");

        // The owner still sees their own list.
        let token = crate::auth::create_token(owner.user.id, &owner.username, &state.config.jwt_secret)
            .expect("token");
        let doc = get_json(
            &app,
            &format!("/api/profile/{}/followers", owner.username),
            Some(&token),
        )
        .await;
        assert_eq!(doc["hidden"], false);
        assert!(!doc["rows"].as_array().unwrap().is_empty(), "the owner sees their list");

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.user.id)
            .execute(&pool)
            .await;
    }

    /// Phase 6: the visibility settings — the local toggle round-trips, the
    /// home proxy is gated on write:accounts (stale grants demand the
    /// re-link, never a silent failure), and the home's answer is
    /// authoritative on read-back.
    #[tokio::test]
    async fn settings_visibility_round_trip_and_scope_gate() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let app = crate::api::settings::routes().with_state(state.clone());
        let owner = make_local_actor(&pool, &domain).await;
        let token = crate::auth::create_token(owner.user.id, &owner.username, &state.config.jwt_secret)
            .expect("token");

        // Local toggle round trip.
        let req = authed(
            Request::builder()
                .method("PATCH")
                .uri("/api/settings/visibility")
                .header("host", "test")
                .header("Content-Type", "application/json"),
            &token,
        )
        .body(Body::from(r#"{"graph_visibility":"private"}"#))
        .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "local toggle accepted");
        let stored: String = sqlx::query_scalar("SELECT graph_visibility FROM users WHERE id = $1")
            .bind(owner.user.id)
            .fetch_one(&pool)
            .await
            .expect("read");
        assert_eq!(stored, "private", "the column flipped");

        // Stale grant → the instruction-focused re-link error, no silent write.
        let s = unique();
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            owner.user.id,
            "mastodon",
            &format!("stalevis_{}", s),
            "stalevis",
            "tok",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:follows"), // no write:accounts
            &serde_json::json!({ "instance": "https://m.example" }),
        )
        .await
        .expect("link stale");
        let req = authed(
            Request::builder()
                .method("PATCH")
                .uri("/api/settings/visibility")
                .header("host", "test")
                .header("Content-Type", "application/json"),
            &token,
        )
        .body(Body::from(r#"{"hide_collections":true}"#))
        .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 403, "stale grant refused visibly");
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.expect("body");
        let msg = String::from_utf8_lossy(&bytes).to_string();
        assert!(msg.contains("re-link"), "the error names the fix: {msg}");

        // Positive proxy against a stub home: PATCH update_credentials then
        // the authoritative read-back.
        let (base, heads, thread) = crate::oauth::mastodon::stub_http(
            "HTTP/1.1 200 OK",
            r#"{"id":"9","username":"o","acct":"o","hide_collections":false}"#,
            2,
        );
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            owner.user.id,
            "mastodon",
            &format!("goodvis_{}", s),
            "goodvis",
            "tok",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:accounts"),
            &serde_json::json!({ "instance": base }),
        )
        .await
        .expect("link good");
        let req = authed(
            Request::builder()
                .method("PATCH")
                .uri("/api/settings/visibility")
                .header("host", "test")
                .header("Content-Type", "application/json"),
            &token,
        )
        .body(Body::from(r#"{"hide_collections":true}"#))
        .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "proxy accepted");
        let head = heads.lock().clone();
        assert!(
            head.contains("PATCH /api/v1/accounts/update_credentials"),
            "the write proxied to the home instance: {head}"
        );
        assert!(
            head.contains("hide_collections=true"),
            "the form field is sent: {head}"
        );
        assert!(
            head.contains("verify_credentials"),
            "the home answer is read back: {head}"
        );
        let _ = thread.join();

        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(owner.user.id)
            .execute(&pool)
            .await;
    }
