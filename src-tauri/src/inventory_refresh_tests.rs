// Keep fixture-only workers explicitly outside production worker budgets.
#[cfg(test)]
mod tests {
    use super::super::*;
    use serde_json::{json, Value};
    use std::sync::{atomic::AtomicUsize, Barrier};

    struct Fixture {
        root: PathBuf,
        state: AppState,
    }

    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("youtube-inventory-refresh-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("media")).unwrap();
            let state = AppState {
                database_path: root.join("queue.sqlite3"),
                media_directory: root.join("media"),
                folder_monitor_lock: Arc::new(Mutex::new(())),
                oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
            };
            let connection = initialize_database(&state).unwrap();
            connection.execute("INSERT INTO connection_settings(singleton,oauth_client_id,oauth_credential_id,oauth_project_id,active_channel,active_channel_id,connection_detail,updated_at) VALUES(1,'fixture-client','fixture-bundle','fixture-project','Channel A','channel-a','Selected channel A',?1) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id='fixture-client',oauth_credential_id='fixture-bundle',oauth_project_id='fixture-project',active_channel='Channel A',active_channel_id='channel-a',connection_detail='Selected channel A'", [now()]).unwrap();
            drop(connection);
            Self { root, state }
        }

        fn db(&self) -> Connection {
            database(&self.state).unwrap()
        }

        fn key(&self) -> String {
            SelectedYouTubeReadBinding::capture(&self.db())
                .unwrap()
                .cache_key()
                .unwrap()
        }

        fn checkpoint(&self) -> InventoryReadCheckpoint {
            youtube_read_cache::load(&self.db(), "inventory", &self.key())
                .unwrap()
                .unwrap()
        }

        fn expire_head(&self) {
            let mut saved = self.checkpoint();
            saved.checked_at = Some((Utc::now() - ChronoDuration::minutes(16)).to_rfc3339());
            youtube_read_cache::save(&self.db(), "inventory", &self.key(), &saved).unwrap();
        }

        fn seed_remote(&self, channel: &str, id: &str, title: &str) {
            let keys = title_matching::keys(title);
            self.db().execute("INSERT INTO remote_videos(video_id,channel_name,channel_id,title,normalized_title,canonical_title,has_copy_marker,numeric_title_key,title_keys_version,duration,privacy_status,upload_status,updated_at) VALUES(?1,'Fixture channel',?2,?3,?4,?5,?6,?7,?8,'PT5M','private','processed',?9)",params![id,channel,title,keys.normalized,keys.canonical,keys.has_copy_marker as i64,keys.numeric,title_matching::TITLE_KEYS_VERSION,now()]).unwrap();
        }

        fn seed_receipt(&self, id: &str, video: &str, channel: &str) {
            self.db().execute("INSERT INTO upload_items(id,title,file_name,channel_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,status,confirmed_bytes,video_id,resumable_session_uri,detail,created_at,updated_at) VALUES(?1,'Approved fixture','fixture.mp4','Fixture channel',?2,'synthetic-original','synthetic-original',100,100,'uploaded',100,?3,'synthetic-checkpoint-reference','Confirmed fixture receipt',?4,?4)",params![id,channel,video,now()]).unwrap();
        }

        fn receipt(&self, id: &str) -> String {
            self.db().query_row("SELECT json_object('status',status,'video',video_id,'confirmed',confirmed_bytes,'source',source_path,'session',resumable_session_uri,'detail',detail,'updatedAt',updated_at) FROM upload_items WHERE id=?1",[id],|row|row.get(0)).unwrap()
        }

        fn pause_api(&self) {
            self.db().execute("INSERT INTO api_quota_pauses(quota_scope,channel_id,pause_until,updated_at) VALUES('project:fixture-project','channel-a',?1,?2)",params![(Utc::now()+ChronoDuration::hours(1)).to_rfc3339(),now()]).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // This is the UUID directory created by this fixture, never app data.
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn channel() -> Value {
        json!({"items":[{"id":"channel-a","snippet":{"title":"Channel A"},"contentDetails":{"relatedPlaylists":{"uploads":"uploads-a"}}}]})
    }

    fn playlist_item(id: &str, title: &str) -> Value {
        json!({"contentDetails":{"videoId":id},"snippet":{"title":title,"channelId":"channel-a","videoOwnerChannelId":"channel-a"}})
    }

    fn video(id: &str, title: &str) -> Value {
        json!({"id":id,"snippet":{"channelId":"channel-a","title":title},"contentDetails":{"duration":"PT5M"},"status":{"privacyStatus":"private","uploadStatus":"processed"},"processingDetails":{"processingStatus":"succeeded"}})
    }

    fn baseline(endpoint: &str, _: &[(&str, &str)]) -> Result<Value, String> {
        Ok(match endpoint {
            "channels" => channel(),
            "playlistItems" => json!({"items":[playlist_item("video-a","Video A")]}),
            "videos" => json!({"items":[video("video-a","Video A")]}),
            _ => panic!("Unexpected synthetic inventory endpoint: {endpoint}"),
        })
    }

    #[test]
    fn warm_inventory_reviews_a_new_confirmed_upload_title_without_another_provider_read() {
        let fixture = Fixture::new();
        run_inventory_refresh(&fixture.state, false, Some("channel-a"), baseline).unwrap();
        fixture.seed_receipt("completed-a", "new-video-a", "channel-a");
        fixture.db().execute("UPDATE upload_items SET status='uploading',confirmed_bytes=0,title='Recently uploaded clip',digest='synthetic-digest-a' WHERE id='completed-a'", []).unwrap();
        persist_provider_success(
            &fixture.state,
            "completed-a",
            "channel-a",
            100,
            "new-video-a",
        )
        .unwrap();
        fixture.db().execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,digest,size_bytes,total_bytes,status,confirmed_bytes,created_at,updated_at) VALUES('queued-b','Recently uploaded clip','different-original.mp4','channel-a','synthetic-original-b','synthetic-original-b','synthetic-digest-b',100,100,'queued',0,?1,?1)", [now()]).unwrap();
        let completed_before = fixture.receipt("completed-a");
        let source_identity = || {
            fixture.db().query_row("SELECT json_object('id',id,'file',file_name,'source',source_path,'workspace',workspace_path,'digest',digest,'total',total_bytes,'confirmed',confirmed_bytes,'video',video_id,'checkpoint',resumable_session_uri) FROM upload_items WHERE id='queued-b'", [], |row| row.get::<_,String>(0)).unwrap()
        };
        let original_identity = source_identity();
        let observations_before: i64 = fixture
            .db()
            .query_row("SELECT COUNT(*) FROM quota_observations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let matches =
            check_upload_title_duplicates_impl(&fixture.state, &["queued-b".into()]).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].item_id, "queued-b");
        assert_eq!(matches[0].matched_titles, ["Recently uploaded clip"]);
        assert_eq!(matches[0].match_scope, "youtube");
        assert_eq!(
            fixture
                .db()
                .query_row(
                    "SELECT status,duplicate_decision FROM upload_items WHERE id='queued-b'",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                )
                .unwrap(),
            ("draft".into(), "pending".into())
        );
        assert_eq!(source_identity(), original_identity);
        assert_eq!(fixture.receipt("completed-a"), completed_before);
        assert_eq!(inventory_count(&fixture.db(), "channel-a").unwrap(), 1);
        assert_eq!(
            fixture
                .db()
                .query_row("SELECT COUNT(*) FROM quota_observations", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            observations_before
        );
        assert_eq!(
            run_inventory_refresh(&fixture.state, false, Some("channel-a"), |_, _| panic!(
                "The local receipt must not invalidate or reread the warm provider snapshot"
            ))
            .unwrap(),
            1
        );

        let approved =
            resolve_upload_title_duplicates_impl(&fixture.state, &["queued-b".into()], "ignore")
                .unwrap();
        assert_eq!(approved.len(), 1);
        assert!(
            check_upload_title_duplicates_impl(&fixture.state, &["queued-b".into()])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fixture
                .db()
                .query_row(
                    "SELECT duplicate_decision FROM upload_items WHERE id='queued-b'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "ignore"
        );
        assert_eq!(source_identity(), original_identity);
        assert_eq!(fixture.receipt("completed-a"), completed_before);
    }

    #[test]
    fn local_uploaded_title_evidence_excludes_unconfirmed_missing_and_other_channel_receipts() {
        let fixture = Fixture::new();
        for (id, channel) in [
            ("confirmed", "channel-a"),
            ("reconciliation", "channel-a"),
            ("queued", "channel-a"),
            ("missing", "channel-a"),
            ("empty", "channel-a"),
            ("invalid", "channel-a"),
            ("incomplete", "channel-a"),
            ("zero", "channel-a"),
            ("foreign", "channel-b"),
        ] {
            fixture.seed_receipt(id, &format!("video-{id}"), channel);
            fixture
                .db()
                .execute(
                    "UPDATE upload_items SET title=?1 WHERE id=?2",
                    params![format!("Fixture title {id}"), id],
                )
                .unwrap();
        }
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET status='needs_reconciliation' WHERE id='reconciliation'",
                [],
            )
            .unwrap();
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET status='queued' WHERE id='queued'",
                [],
            )
            .unwrap();
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET video_id=NULL WHERE id='missing'",
                [],
            )
            .unwrap();
        fixture
            .db()
            .execute("UPDATE upload_items SET video_id='' WHERE id='empty'", [])
            .unwrap();
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET video_id='invalid video id' WHERE id='invalid'",
                [],
            )
            .unwrap();
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET confirmed_bytes=99 WHERE id='incomplete'",
                [],
            )
            .unwrap();
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET confirmed_bytes=0,total_bytes=0 WHERE id='zero'",
                [],
            )
            .unwrap();
        let index = RemoteTitleIndex::load(&fixture.db(), "channel-a").unwrap();
        assert_eq!(index.titles.len(), 1);
        assert_eq!(index.titles[0].video_id, "video-confirmed");
        assert_eq!(index.matches("Fixture title confirmed").len(), 1);
        for rejected in [
            "reconciliation",
            "queued",
            "missing",
            "empty",
            "invalid",
            "incomplete",
            "zero",
            "foreign",
        ] {
            assert!(
                index
                    .matches(&format!("Fixture title {rejected}"))
                    .is_empty(),
                "{rejected}"
            );
        }
    }

    #[test]
    fn processed_remote_title_takes_precedence_over_the_same_local_uploaded_receipt() {
        let fixture = Fixture::new();
        fixture.seed_remote("channel-a", "shared-video", "Current provider title");
        fixture.seed_receipt("older-local-receipt", "shared-video", "channel-a");
        fixture
        .db()
        .execute(
            "UPDATE upload_items SET title='Original local title' WHERE id='older-local-receipt'",
            [],
        )
        .unwrap();
        let before = fixture.receipt("older-local-receipt");
        let index = RemoteTitleIndex::load(&fixture.db(), "channel-a").unwrap();
        assert_eq!(index.titles.len(), 1);
        assert_eq!(index.matches("Current provider title").len(), 1);
        assert!(index.matches("Original local title").is_empty());
        assert_eq!(fixture.receipt("older-local-receipt"), before);
    }

    #[test]
    fn source_cleanup_reads_only_the_reviewed_video_and_retains_other_pending_sources() {
        let fixture = Fixture::new();
        fixture.seed_receipt("target", "video-a", "channel-a");
        fixture.seed_receipt("other", "video-b", "channel-a");
        let original = fixture.root.join("fixture-original.mp4");
        fs::write(&original, b"synthetic verified source").unwrap();
        let (_, digest) = digest_file(&original).unwrap();
        fixture
        .db()
        .execute(
            "UPDATE upload_items SET delete_source_after_upload=1,source_delete_status='pending'",
            [],
        )
        .unwrap();
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET source_path=?1,digest=?2 WHERE id='target'",
                params![original.to_string_lossy(), digest],
            )
            .unwrap();
        let binding = SelectedYouTubeReadBinding::capture(&fixture.db()).unwrap();
        let mut calls = 0;
        verify_source_cleanup_processing_with(&fixture.state, "target", &binding, |path, query| {
            calls += 1;
            assert_eq!(path, "videos");
            assert!(query.contains(&("id", "video-a")));
            Ok(json!({"items":[video("video-a","Video A")]}))
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert!(!original.exists());
        let status = |id: &str| {
            fixture
                .db()
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id=?1",
                    [id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap()
        };
        assert_eq!(status("target"), "deleted");
        assert_eq!(status("other"), "pending");
    }

    #[test]
    fn source_cleanup_rejects_foreign_missing_and_changed_binding_evidence() {
        let fixture = Fixture::new();
        fixture.seed_receipt("target", "video-a", "channel-a");
        fixture.db().execute("UPDATE upload_items SET delete_source_after_upload=1,source_delete_status='pending' WHERE id='target'", []).unwrap();
        let binding = SelectedYouTubeReadBinding::capture(&fixture.db()).unwrap();
        for payload in [
            json!({"items":[]}),
            json!({"items":[video("other-video","Other")]}),
            json!({"items":[{"id":"video-a","snippet":{"channelId":"foreign"}}]}),
        ] {
            assert!(verify_source_cleanup_processing_with(
                &fixture.state,
                "target",
                &binding,
                |_, _| Ok(payload.clone())
            )
            .is_err());
        }
        assert!(verify_source_cleanup_processing_with(
            &fixture.state,
            "target",
            &binding,
            |_, _| {
                fixture
                    .db()
                    .execute(
                        "UPDATE connection_settings SET oauth_credential_id='changed'",
                        [],
                    )
                    .unwrap();
                Ok(json!({"items":[video("video-a","Video A")]}))
            }
        )
        .is_err());
        assert_eq!(
            fixture
                .db()
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id='target'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "pending"
        );
    }

    #[test]
    fn source_cleanup_saved_quota_hold_and_foreign_item_make_no_provider_request() {
        let fixture = Fixture::new();
        fixture.seed_receipt("foreign", "video-b", "channel-b");
        let binding = SelectedYouTubeReadBinding::capture(&fixture.db()).unwrap();
        assert!(verify_source_cleanup_processing_with(
            &fixture.state,
            "foreign",
            &binding,
            |_, _| panic!("Foreign item cannot request processing")
        )
        .is_err());
        fixture.seed_receipt("target", "video-a", "channel-a");
        fixture.pause_api();
        assert!(verify_source_cleanup_processing_with(
            &fixture.state,
            "target",
            &binding,
            |_, _| panic!("Saved quota hold must prevent a request")
        )
        .is_err());
    }

    #[test]
    fn inventory_cold_baseline_three_reads_then_one_hundred_warm_consumers_use_zero() {
        let fixture = Fixture::new();
        let mut calls = Vec::new();
        assert_eq!(
            run_inventory_refresh(
                &fixture.state,
                false,
                Some("channel-a"),
                |endpoint, query| {
                    calls.push(endpoint.to_owned());
                    baseline(endpoint, query)
                }
            )
            .unwrap(),
            1
        );
        assert_eq!(calls, ["channels", "playlistItems", "videos"]);
        for _ in 0..100 {
            assert_eq!(
                run_inventory_refresh(&fixture.state, false, Some("channel-a"), |_, _| {
                    panic!("A warm automatic inventory consumer must not issue a provider request")
                })
                .unwrap(),
                1
            );
        }
        assert_eq!(fixture.checkpoint().failures, 0);
    }

    #[test]
    fn inventory_stale_head_checks_one_page_without_rechecking_identity_or_known_details() {
        let fixture = Fixture::new();
        run_inventory_refresh(&fixture.state, false, Some("channel-a"), baseline).unwrap();
        fixture.expire_head();
        let full_at = fixture.checkpoint().full_synced_at;
        let mut calls = 0;
        assert_eq!(
        run_inventory_refresh(&fixture.state, false, Some("channel-a"), |endpoint, _| {
            calls += 1;
            assert_eq!(endpoint, "playlistItems");
            Ok(json!({"items":[playlist_item("video-a","Video A")],"nextPageToken":"older-page"}))
        })
        .unwrap(),
        1
    );
        assert_eq!(calls, 1);
        assert_eq!(fixture.checkpoint().full_synced_at, full_at);
        assert!(recent_read(
            fixture.checkpoint().checked_at.as_deref(),
            Utc::now(),
            ChronoDuration::minutes(1)
        ));
    }

    #[test]
    fn inventory_empty_success_uses_two_reads_and_remains_warm_after_database_reopen() {
        let fixture = Fixture::new();
        let mut calls = Vec::new();
        assert_eq!(
            run_inventory_refresh(&fixture.state, false, Some("channel-a"), |endpoint, _| {
                calls.push(endpoint.to_owned());
                Ok(match endpoint {
                    "channels" => channel(),
                    "playlistItems" => json!({"items":[]}),
                    _ => panic!("An empty library must not request video details"),
                })
            })
            .unwrap(),
            0
        );
        assert_eq!(calls, ["channels", "playlistItems"]);
        let reopened = fixture.state.clone();
        drop(database(&reopened).unwrap());
        assert_eq!(
            run_inventory_refresh(&reopened, false, Some("channel-a"), |_, _| {
                panic!("An explicitly completed empty inventory must remain cached after reopen")
            })
            .unwrap(),
            0
        );
        assert!(fixture.checkpoint().full_synced_at.is_some());
    }

    #[test]
    fn inventory_one_hundred_quota_blocked_consumers_send_zero_provider_requests() {
        let fixture = Fixture::new();
        fixture.pause_api();
        let mut calls = 0;
        for _ in 0..100 {
            let error = run_inventory_refresh(&fixture.state, false, Some("channel-a"), |_, _| {
                calls += 1;
                Err("The quota admission gate should have stopped this call".into())
            })
            .unwrap_err();
            assert!(error.contains("Google Cloud"));
        }
        assert_eq!(calls, 0);
        assert!(youtube_read_cache::load::<InventoryReadCheckpoint>(
            &fixture.db(),
            "inventory",
            &fixture.key()
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn inventory_failed_refresh_keeps_old_freshness_and_backoff_prevents_repeated_reads() {
        let fixture = Fixture::new();
        run_inventory_refresh(&fixture.state, false, Some("channel-a"), baseline).unwrap();
        fixture.expire_head();
        let before = fixture.checkpoint();
        let mut calls = 0;
        let first =
            run_inventory_refresh(&fixture.state, false, Some("channel-a"), |endpoint, _| {
                calls += 1;
                assert_eq!(endpoint, "playlistItems");
                Err("Synthetic network failure".into())
            })
            .unwrap_err();
        assert_eq!(first, "Synthetic network failure");
        for _ in 0..100 {
            assert!(
                run_inventory_refresh(&fixture.state, false, Some("channel-a"), |_, _| {
                    calls += 1;
                    Err("Backoff was bypassed".into())
                })
                .unwrap_err()
                .contains("backed off")
            );
        }
        assert_eq!(calls, 1);
        let saved = fixture.checkpoint();
        assert_eq!(saved.checked_at, before.checked_at);
        assert_eq!(saved.full_synced_at, before.full_synced_at);
        assert_eq!(saved.failures, 1);
        assert!(saved.retry_after.is_some());
        assert_eq!(inventory_count(&fixture.db(), "channel-a").unwrap(), 1);
    }

    #[test]
    fn failed_initial_inventory_read_rearms_queue_at_retry_deadline_without_network() {
        let fixture = Fixture::new();
        fixture.db().execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,status,confirmed_bytes,created_at,updated_at) VALUES('queued-a','Approved fixture','fixture.mp4','channel-a','synthetic-original','synthetic-original',100,100,'queued',0,?1,?1)", [now()]).unwrap();
        let mut calls = 0;
        assert!(
            run_inventory_refresh(&fixture.state, false, Some("channel-a"), |_, _| {
                calls += 1;
                Err("Synthetic initial inventory read failure".into())
            })
            .is_err()
        );
        let saved = fixture.checkpoint();
        assert!(saved.checked_at.is_none());
        assert!(saved.full_synced_at.is_none());
        let retry = DateTime::parse_from_rfc3339(saved.retry_after.as_deref().unwrap())
            .unwrap()
            .with_timezone(&Utc);
        let (queued, open, postprocess, next) =
            upload_resume_plan(&fixture.state, Utc::now()).unwrap();
        assert!(queued);
        assert!(!open);
        assert!(!postprocess);
        assert_eq!(next, Some(retry));
        assert_eq!(
            inventory_retry_deadline(
                &fixture.db(),
                "channel-a",
                retry - ChronoDuration::milliseconds(1)
            )
            .unwrap(),
            Some(retry)
        );
        assert!(inventory_retry_deadline(&fixture.db(), "channel-a", retry)
            .unwrap()
            .is_none());
        assert!(
            inventory_retry_deadline(&fixture.db(), "channel-b", Utc::now())
                .unwrap()
                .is_none()
        );
        let (queued, open, postprocess, next) = upload_resume_plan(&fixture.state, retry).unwrap();
        assert!(queued);
        assert!(open);
        assert!(!postprocess);
        assert!(next.is_none());
        assert_eq!(calls, 1, "Planning the retry must perform no provider read");

        let mut expired = fixture.checkpoint();
        expired.retry_after = Some((Utc::now() - ChronoDuration::seconds(1)).to_rfc3339());
        youtube_read_cache::save(&fixture.db(), "inventory", &fixture.key(), &expired).unwrap();
        assert_eq!(
            run_inventory_refresh(
                &fixture.state,
                false,
                Some("channel-a"),
                |endpoint, query| {
                    calls += 1;
                    baseline(endpoint, query)
                }
            )
            .unwrap(),
            1
        );
        assert_eq!(calls, 4);
        let fresh = fixture.checkpoint();
        assert_eq!(fresh.failures, 0);
        assert!(fresh.retry_after.is_none());
        assert_eq!(
            run_inventory_refresh(&fixture.state, false, Some("channel-a"), |_, _| {
                panic!("The successful retry must be shared by subsequent warm consumers")
            })
            .unwrap(),
            1
        );
        assert!(
            inventory_retry_deadline(&fixture.db(), "channel-a", Utc::now())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn inventory_concurrent_automatic_consumers_share_one_cold_refresh() {
        let fixture = Fixture::new();
        let start = Arc::new(Barrier::new(9));
        let calls = Arc::new(AtomicUsize::new(0));
        let workers = (0..8)
            .map(|_| {
                let state = fixture.state.clone();
                let start = start.clone();
                let calls = calls.clone();
                thread::spawn(move || {
                    start.wait();
                    run_inventory_refresh(&state, false, Some("channel-a"), |endpoint, query| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        baseline(endpoint, query)
                    })
                })
            })
            .collect::<Vec<_>>();
        start.wait();
        for worker in workers {
            assert_eq!(worker.join().unwrap().unwrap(), 1);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn inventory_late_client_project_or_bundle_change_cannot_commit_or_cache_result() {
        for column in ["oauth_client_id", "oauth_project_id", "oauth_credential_id"] {
            let fixture = Fixture::new();
            fixture.seed_remote("channel-a", "retained-video", "Retained video");
            let old_key = fixture.key();
            let mut calls = 0;
            assert!(
                run_inventory_refresh(
                    &fixture.state,
                    false,
                    Some("channel-a"),
                    |endpoint, query| {
                        calls += 1;
                        fixture
                            .db()
                            .execute(
                                &format!(
                                    "UPDATE connection_settings SET {column}='changed-fixture'"
                                ),
                                [],
                            )
                            .unwrap();
                        baseline(endpoint, query)
                    }
                )
                .is_err(),
                "{column}"
            );
            assert_eq!(calls, 1, "{column}");
            assert_eq!(inventory_count(&fixture.db(), "channel-a").unwrap(), 1);
            assert_eq!(
                fixture
                    .db()
                    .query_row(
                        "SELECT title FROM remote_videos WHERE video_id='retained-video'",
                        [],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                "Retained video"
            );
            assert!(youtube_read_cache::load::<InventoryReadCheckpoint>(
                &fixture.db(),
                "inventory",
                &old_key
            )
            .unwrap()
            .is_none());
        }
    }

    #[test]
    fn inventory_delta_merge_retains_older_and_foreign_rows_and_confirmed_receipts() {
        let fixture = Fixture::new();
        run_inventory_refresh(&fixture.state, false, Some("channel-a"), baseline).unwrap();
        fixture.seed_remote("channel-a", "old-video", "Older video");
        fixture.seed_remote("channel-b", "foreign-video", "Foreign video");
        fixture.seed_receipt("old-receipt", "old-video", "channel-a");
        fixture.seed_receipt("foreign-receipt", "foreign-video", "channel-b");
        let old_receipt = fixture.receipt("old-receipt");
        let foreign_receipt = fixture.receipt("foreign-receipt");
        fixture.expire_head();
        let mut calls = Vec::new();
        assert_eq!(run_inventory_refresh(&fixture.state, false, Some("channel-a"), |endpoint, _| {
        calls.push(endpoint.to_owned());
        Ok(match endpoint {
            "playlistItems" => json!({"items":[playlist_item("new-video","New video"),playlist_item("video-a","Video A")],"nextPageToken":"older-page"}),
            "videos" => json!({"items":[video("new-video","New video")]}),
            _ => panic!("A delta refresh must reuse its scoped channel identity"),
        })
    }).unwrap(), 3);
        assert_eq!(calls, ["playlistItems", "videos"]);
        assert_eq!(inventory_count(&fixture.db(), "channel-b").unwrap(), 1);
        assert_eq!(fixture.receipt("old-receipt"), old_receipt);
        assert_eq!(fixture.receipt("foreign-receipt"), foreign_receipt);
        assert_eq!(
            fixture
                .db()
                .query_row(
                    "SELECT COUNT(*) FROM remote_video_sync_staging",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn inventory_cache_never_crosses_reviewed_channel_or_selected_credential_binding() {
        let fixture = Fixture::new();
        run_inventory_refresh(&fixture.state, false, Some("channel-a"), baseline).unwrap();
        assert!(
            run_inventory_refresh(&fixture.state, false, Some("channel-b"), |_, _| {
                panic!("A stale reviewed channel cannot request inventory")
            })
            .is_err()
        );
        fixture
            .db()
            .execute(
                "UPDATE connection_settings SET oauth_credential_id='new-fixture-bundle'",
                [],
            )
            .unwrap();
        let mut calls = 0;
        assert_eq!(
            run_inventory_refresh(
                &fixture.state,
                false,
                Some("channel-a"),
                |endpoint, query| {
                    calls += 1;
                    baseline(endpoint, query)
                }
            )
            .unwrap(),
            1
        );
        assert_eq!(
            calls, 3,
            "Changing the protected bundle invalidates the authenticated inventory proof"
        );
    }

    #[test]
    fn inventory_expired_full_snapshot_and_explicit_refresh_still_traverse_provider() {
        let fixture = Fixture::new();
        run_inventory_refresh(&fixture.state, false, Some("channel-a"), baseline).unwrap();
        let mut saved = fixture.checkpoint();
        saved.full_synced_at = Some((Utc::now() - ChronoDuration::hours(25)).to_rfc3339());
        youtube_read_cache::save(&fixture.db(), "inventory", &fixture.key(), &saved).unwrap();
        let mut calls = 0;
        assert_eq!(
            run_inventory_refresh(
                &fixture.state,
                false,
                Some("channel-a"),
                |endpoint, query| {
                    calls += 1;
                    baseline(endpoint, query)
                }
            )
            .unwrap(),
            1
        );
        assert_eq!(
            calls, 3,
            "A recent head cannot substitute for an expired complete traversal"
        );
        assert_eq!(
            run_inventory_refresh(
                &fixture.state,
                true,
                Some("channel-a"),
                |endpoint, query| {
                    calls += 1;
                    baseline(endpoint, query)
                }
            )
            .unwrap(),
            1
        );
        assert_eq!(
            calls, 6,
            "An explicit refresh bypasses successful-result freshness"
        );
    }

    #[test]
    fn playlist_metadata_is_cached_for_one_hundred_loads_and_separate_credentials_miss() {
        let fixture = Fixture::new();
        let mut calls = 0;
        let first = list_youtube_playlists_with(&fixture.state, |endpoint, _| {
        calls += 1;
        assert_eq!(endpoint, "playlists");
        Ok(json!({"items":[{"id":"playlist-a","snippet":{"title":"Playlist A","channelId":"channel-a"}}]}))
    }).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, "playlist-a");
        for _ in 0..100 {
            assert_eq!(
                list_youtube_playlists_with(&fixture.state, |_, _| {
                    panic!("Warm playlist metadata must not issue provider requests")
                })
                .unwrap()
                .len(),
                1
            );
        }
        fixture
            .db()
            .execute(
                "UPDATE connection_settings SET oauth_project_id='another-fixture-project'",
                [],
            )
            .unwrap();
        assert!(list_youtube_playlists_with(&fixture.state, |endpoint, _| {
            calls += 1;
            assert_eq!(endpoint, "playlists");
            Ok(json!({"items":[]}))
        })
        .unwrap()
        .is_empty());
        assert_eq!(calls, 2);
    }

    #[test]
    fn playlist_empty_success_is_cached_and_late_binding_change_is_not_saved() {
        let fixture = Fixture::new();
        assert!(
            list_youtube_playlists_with(&fixture.state, |_, _| Ok(json!({"items":[]})))
                .unwrap()
                .is_empty()
        );
        assert!(list_youtube_playlists_with(&fixture.state, |_, _| {
            panic!("Empty playlist metadata must be a cache hit")
        })
        .unwrap()
        .is_empty());
        fixture
            .db()
            .execute(
                "UPDATE connection_settings SET oauth_client_id='second-fixture-client'",
                [],
            )
            .unwrap();
        let old_key = fixture.key();
        assert!(list_youtube_playlists_with(&fixture.state, |_, _| {
        fixture
            .db()
            .execute(
                "UPDATE connection_settings SET oauth_credential_id='changed-fixture-bundle'",
                [],
            )
            .unwrap();
        Ok(json!({"items":[{"id":"playlist-a","snippet":{"title":"Late playlist","channelId":"channel-a"}}]}))
    })
    .is_err());
        assert!(youtube_read_cache::load::<PlaylistReadCheckpoint>(
            &fixture.db(),
            "playlists",
            &old_key
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn playlist_quota_gate_blocks_a_cold_load_without_network() {
        let fixture = Fixture::new();
        fixture.pause_api();
        assert!(list_youtube_playlists_with(&fixture.state, |_, _| {
            panic!("An existing API quota hold must stop cold playlist reads")
        })
        .err()
        .unwrap()
        .contains("Google Cloud"));
    }

    #[test]
    fn playlist_foreign_owner_is_rejected_without_creating_a_cache_hit() {
        let fixture = Fixture::new();
        let mut calls = 0;
        for _ in 0..2 {
            assert!(list_youtube_playlists_with(&fixture.state, |_, _| {
            calls += 1;
            Ok(json!({"items":[{"id":"foreign-playlist","snippet":{"title":"Foreign playlist","channelId":"channel-b"}}]}))
        }).is_err());
        }
        assert_eq!(calls, 2);
        assert!(youtube_read_cache::load::<PlaylistReadCheckpoint>(
            &fixture.db(),
            "playlists",
            &fixture.key()
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn channel_identity_proof_reuses_same_token_but_refreshes_on_token_or_bundle_change() {
        let fixture = Fixture::new();
        let mut calls = 0;
        for _ in 0..100 {
            assert_eq!(
                active_channel_id_with(&fixture.state, "synthetic-access-token-a", || {
                    calls += 1;
                    Ok(channel())
                })
                .unwrap(),
                "channel-a"
            );
        }
        assert_eq!(calls, 1);
        assert_eq!(
            active_channel_id_with(&fixture.state, "synthetic-access-token-b", || {
                calls += 1;
                Ok(channel())
            })
            .unwrap(),
            "channel-a"
        );
        fixture
            .db()
            .execute(
                "UPDATE connection_settings SET oauth_credential_id='new-fixture-bundle'",
                [],
            )
            .unwrap();
        assert_eq!(
            active_channel_id_with(&fixture.state, "synthetic-access-token-b", || {
                calls += 1;
                Ok(channel())
            })
            .unwrap(),
            "channel-a"
        );
        assert_eq!(calls, 3);
    }

    #[test]
    fn channel_identity_foreign_or_late_changed_binding_is_never_cached() {
        let fixture = Fixture::new();
        let mut calls = 0;
        for _ in 0..2 {
            assert!(
                active_channel_id_with(&fixture.state, "synthetic-foreign-token", || {
                    calls += 1;
                    Ok(json!({"items":[{"id":"channel-b"}]}))
                })
                .is_err()
            );
        }
        assert_eq!(calls, 2);
        assert!(
            active_channel_id_with(&fixture.state, "synthetic-late-token", || {
                fixture
                    .db()
                    .execute(
                        "UPDATE connection_settings SET oauth_project_id='changed-fixture-project'",
                        [],
                    )
                    .unwrap();
                Ok(channel())
            })
            .is_err()
        );
        assert_eq!(
            active_channel_id_with(&fixture.state, "synthetic-late-token", || {
                calls += 1;
                Ok(channel())
            })
            .unwrap(),
            "channel-a"
        );
        assert_eq!(calls, 3);
    }
}
