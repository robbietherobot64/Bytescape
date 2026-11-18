    use super::*;
    use crate::ap::testutil::{
        connect_pool, enable_ap_flags, make_local_actor, make_signer, seed_remote_actor, test_state,
        unique,
    };
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    // ── routing (pure) ────────────────────────────────────────────────────

    #[test]
    fn classify_target_routes_by_handle_form_and_source() {
        let d = "social.test";
        // Source label decides the AT side.
        assert_eq!(
            classify_target("alice.bsky.social", "bluesky", d).unwrap(),
            TargetNetwork::At("alice.bsky.social".into())
        );
        // `user@host` decides local vs remote regardless of the UI's guess.
        assert_eq!(
            classify_target("bob@remote.test", "mastodon", d).unwrap(),
            TargetNetwork::Ap("bob@remote.test".into())
        );
        assert_eq!(
            classify_target("bob@social.test", "mastodon", d).unwrap(),
            TargetNetwork::Local("bob".into())
        );
        // Bare names are local — remote resolution needs the full handle.
        assert_eq!(
            classify_target("@carol", "", d).unwrap(),
            TargetNetwork::Local("carol".into())
        );
        // A remote-labelled target without a host is an error, never a guess.
        assert!(classify_target("dave", "mastodon", d).is_err());
        assert!(classify_target("@bad@host@extra", "ap", d).is_ok()); // first @ splits -> host "host@extra"? no:
    }

    #[test]
    fn classify_target_rejects_malformed_handles() {
        let d = "social.test";
        assert!(classify_target("@", "ap", d).is_err());
        assert!(classify_target("@user@", "ap", d).is_err());
    }

    /// 3.1 acceptance: the adapter matrix — AT → createRecord, AP → home-REST
    /// for Mastodon-linked callers (no minted fallback) / minted AP otherwise,
    /// local → follows row.
    #[test]
    fn select_adapter_covers_all_three_networks() {
        let at = TargetNetwork::At("a.bsky.social".into());
        let ap = TargetNetwork::Ap("a@remote.test".into());
        let local = TargetNetwork::Local("bob".into());
        let full = CallerCaps {
            bluesky: true,
            mastodon: true,
        };
        let minted_only = CallerCaps {
            bluesky: false,
            mastodon: false,
        };

        assert_eq!(select_adapter(&at, full).unwrap(), Adapter::AtCreateRecord);
        assert_eq!(select_adapter(&ap, full).unwrap(), Adapter::ApHomeRest);
        assert_eq!(select_adapter(&ap, minted_only).unwrap(), Adapter::ApMinted);
        assert_eq!(select_adapter(&local, full).unwrap(), Adapter::LocalRow);
        assert_eq!(select_adapter(&local, minted_only).unwrap(), Adapter::LocalRow);
        // AT without a Bluesky link is an honest error, not another identity.
        let err = select_adapter(&at, minted_only).unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
    }

    // ── DB-gated round trips ──────────────────────────────────────────────

    #[tokio::test]
    async fn local_follow_writes_native_row_and_unfollow_removes_it() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let caller = make_local_actor(&pool, &domain).await;
        let target = make_local_actor(&pool, &domain).await;

        let out = follow(&state, caller.user.id, &target.username, "")
            .await
            .expect("local follow");
        assert_eq!(out.status, "following");
        assert_eq!(out.network, "local");
        assert!(!out.federated);

        let rows = db::follows::FollowRow::list_by_user(&pool, caller.user.id)
            .await
            .expect("rows");
        assert!(
            rows.iter().any(|r| r.follow_type == "native" && r.followed_actor == out.actor),
            "native row recorded: {rows:?}"
        );
        let again = follow(&state, caller.user.id, &target.username, "")
            .await
            .expect("idempotent");
        assert_eq!(again.status, "already_following");

        let gone = unfollow(&state, caller.user.id, &target.username, "")
            .await
            .expect("unfollow");
        assert_eq!(gone.status, "none");
        let rows = db::follows::FollowRow::list_by_user(&pool, caller.user.id)
            .await
            .expect("rows");
        assert!(
            !rows.iter().any(|r| r.followed_actor == out.actor),
            "row removed on undo"
        );

        let _ = sqlx::query("DELETE FROM users WHERE id = ANY($1)")
            .bind(vec![caller.user.id, target.user.id])
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn minted_ap_follow_records_ap_row_with_identity() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let caller = make_local_actor(&pool, &domain).await;
        let s = unique();
        let signer = make_signer(&format!("https://probe.ap.test/users/u{s}"));
        seed_remote_actor(
            &pool,
            &signer,
            &format!("{}/inbox", signer.actor_id),
            None,
        )
        .await;

        // No Mastodon link → the minted-actor adapter; the cached remote
        // resolves offline (WebFinger cache hit).
        let out = follow(&state, caller.user.id, &format!("u{s}@probe.ap.test"), "mastodon")
            .await
            .expect("minted AP follow");
        assert_eq!(out.network, "activitypub");
        assert_eq!(out.status, "pending");
        assert!(out.federated);
        assert_eq!(out.actor, signer.actor_id);

        let rows = db::follows::FollowRow::list_by_user(&pool, caller.user.id)
            .await
            .expect("rows");
        let row = rows
            .iter()
            .find(|r| r.followed_actor == signer.actor_id)
            .expect("ap row");
        assert_eq!(row.follow_type, "ap");
        // identity_used = the minted actor URL the Follow went out as.
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT source_platform FROM follows WHERE follower_id = $1 AND followed_actor = $2",
        )
        .bind(caller.user.id)
        .bind(&signer.actor_id)
        .fetch_one(&pool)
        .await
        .expect("identity");
        assert!(
            stored.as_deref().is_some_and(|v| v.contains(&domain)),
            "identity_used recorded the minted actor, got {stored:?}"
        );

        // Regression (2026-11-18): the minted follow MUST enqueue a delivery
        // row. ui_follow used to pass the remote's inbox URL where
        // store::audience_inboxes resolves ACTOR ids — the enqueue silently
        // produced nothing and every minted-actor UI follow was a black hole.
        let _delivery_guard = crate::ap::testutil::due_rows_lock().lock().await;
        let queued: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM ap_delivery_queue q \
             JOIN ap_activities a ON a.id = q.activity_id \
             WHERE a.actor_id = $1 AND a.object_id = $2 AND a.activity_type = 'Follow'",
        )
        .bind(&out.identity_used)
        .bind(&signer.actor_id)
        .fetch_one(&pool)
        .await
        .expect("queue count");
        assert!(queued >= 1, "the Follow is enqueued for delivery to the remote inbox");

        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(caller.user.id)
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    async fn mastodon_linked_follow_lands_on_the_home_instance() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let caller = make_local_actor(&pool, &domain).await;

        // Mock home instance on loopback: answers lookup + follow with one
        // relationship JSON. `rest_base` passes http:// URLs through.
        let (base, heads, thread) = crate::oauth::mastodon::stub_http(
            "HTTP/1.1 200 OK",
            r#"{"id":"7","username":"a","following":true,"requested":false}"#,
            2,
        );
        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            caller.user.id,
            "mastodon",
            &format!("testacct_{}", unique()),
            "testacct",
            "tok",
            Some("refresh"),
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:follows write:favourites"),
            &serde_json::json!({ "instance": base }),
        )
        .await
        .expect("link home account");

        let s = unique();
        let signer = make_signer(&format!("https://probe.ap.test/users/h{s}"));
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;

        let out = follow(&state, caller.user.id, &format!("h{s}@probe.ap.test"), "mastodon")
            .await
            .expect("home-REST follow");
        assert_eq!(out.network, "activitypub");
        assert_eq!(out.status, "following");

        let head = heads.lock().clone();
        assert!(head.contains("GET /api/v1/accounts/lookup?acct=h"), "lookup first: {head}");
        assert!(head.contains("POST /api/v1/accounts/7/follow"), "then follow: {head}");

        // The row mirrors the home-graph shape and records the home identity.
        let rows = db::follows::FollowRow::list_by_user(&pool, caller.user.id)
            .await
            .expect("rows");
        let row = rows
            .iter()
            .find(|r| r.followed_actor == signer.actor_id)
            .expect("home row");
        assert_eq!(row.follow_type, "mastodon_import");
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT source_platform FROM follows WHERE follower_id = $1 AND followed_actor = $2",
        )
        .bind(caller.user.id)
        .bind(&signer.actor_id)
        .fetch_one(&pool)
        .await
        .expect("identity");
        assert_eq!(stored.as_deref(), Some("testacct"), "identity_used = home acct");
        let _ = thread.join();

        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(caller.user.id)
            .execute(&pool)
            .await;
    }

    // ── expired-token flip-flop (auth-loop fix) ─────────────────────────

    #[test]
    fn optional_viewer_distinguishes_absent_from_invalid_tokens() {
        use axum::http::HeaderMap;
        // Absent header → anonymous.
        let headers = HeaderMap::new();
        assert!(crate::api::feed::optional_viewer(&headers, "secret")
            .expect("absent header is anonymous")
            .is_none());
        // Present but invalid/expired → 401, never a silent anonymous 200.
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer garbage".parse().unwrap(),
        );
        assert!(
            crate::api::feed::optional_viewer(&headers, "secret").is_err(),
            "a broken session must not be reclassified as anonymous"
        );
        // A real token still resolves.
        let token =
            crate::auth::create_token(uuid::Uuid::new_v4(), "u", "secret").expect("token");
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        assert!(crate::api::feed::optional_viewer(&headers, "secret")
            .expect("valid token")
            .is_some());
    }

    /// The 200/401 flip-flop fed the silent refetch loop: the same expired
    /// JWT got an anonymous 200 from optionally-authed endpoints and a 401
    /// from the rest. One semantics everywhere now.
    #[tokio::test]
    async fn expired_token_is_401_everywhere_never_anonymous_200() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let app = crate::api::feed::routes().with_state(state.clone());

        let req = Request::builder()
            .method("GET")
            .uri("/api/feed/combined")
            .header("host", "test")
            .header("Authorization", "Bearer expired-garbage")
            .body(Body::empty())
            .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(
            resp.status().as_u16(),
            401,
            "an invalid-present token is 401, never an anonymous 200"
        );

        // No header at all stays a legitimate anonymous request.
        let req = Request::builder()
            .method("GET")
            .uri("/api/feed/combined")
            .header("host", "test")
            .body(Body::empty())
            .expect("request");
        let resp = app.clone().oneshot(req).await.expect("oneshot");
        assert_eq!(resp.status().as_u16(), 200, "anonymous access is unchanged");
    }

    /// 3.3 acceptance: an unrelinked account attempting a follow sees the
    /// re-link error and NO row is created — never a silent second identity.
    #[tokio::test]
    async fn stale_scope_follow_demands_relink_and_writes_nothing() {
        let Some(pool) = connect_pool().await else { return };
        enable_ap_flags();
        let Some(state) = test_state(&pool) else { return };
        let domain = state.config.domain.clone();
        let caller = make_local_actor(&pool, &domain).await;

        crate::db::accounts::LinkedAccountRow::create(
            &pool,
            caller.user.id,
            "mastodon",
            &format!("stale_{}", unique()),
            "stale",
            "tok",
            None,
            Some(chrono::Utc::now() + chrono::Duration::hours(1)),
            None,
            Some("read write:favourites"), // pre-realignment grant: no follows write
            &serde_json::json!({ "instance": "https://m.example" }),
        )
        .await
        .expect("link stale account");

        let s = unique();
        let signer = make_signer(&format!("https://probe.ap.test/users/st{s}"));
        seed_remote_actor(&pool, &signer, &format!("{}/inbox", signer.actor_id), None).await;

        let err = follow(&state, caller.user.id, &format!("st{s}@probe.ap.test"), "mastodon")
            .await
            .expect_err("stale grant must demand re-link");
        assert!(matches!(err, AppError::Forbidden(_)), "got {err:?}");
        assert!(
            err.to_string().contains("re-link"),
            "the error names the fix: {err}"
        );

        // Nothing was written — no follow row, no second identity.
        let rows = db::follows::FollowRow::list_by_user(&pool, caller.user.id)
            .await
            .expect("rows");
        assert!(
            !rows.iter().any(|r| r.followed_actor == signer.actor_id),
            "no row on a refused write: {rows:?}"
        );

        let _ = sqlx::query("DELETE FROM ap_remote_actors WHERE actor_id = $1")
            .bind(&signer.actor_id)
            .execute(&pool)
            .await;
        let _ = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(caller.user.id)
            .execute(&pool)
            .await;
    }
