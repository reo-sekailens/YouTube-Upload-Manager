// Fixture-only HTTP workers remain outside production worker budgets.
#[cfg(test)]
mod tests {
    use super::super::*;

    struct Fixture {
        root: PathBuf,
        state: AppState,
    }

    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("youtube-upload-session-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("media")).unwrap();
            Self {
                state: AppState {
                    database_path: root.join("queue.sqlite3"),
                    media_directory: root.join("media"),
                    folder_monitor_lock: Arc::new(Mutex::new(())),
                    oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
                },
                root,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // This fixture owns only its freshly created UUID directory.
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn response_fixture(
        status: u16,
        headers: &str,
        body: &str,
        total_bytes: u64,
    ) -> (
        Result<UploadSessionStatus, UploadSessionFailure>,
        String,
        Vec<u8>,
    ) {
        let fixture = Fixture::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
            body.len()
        );
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            stream.write_all(response.as_bytes()).unwrap();
            (
                String::from_utf8(request[..header_end].to_vec())
                    .unwrap()
                    .to_ascii_lowercase(),
                request[header_end..].to_vec(),
            )
        });
        let result = query_upload_session(
            &fixture.state,
            &format!("http://{address}/resumable/session"),
            total_bytes,
            "fixture-only-native-access-token",
        );
        let (headers, body) = server.join().unwrap();
        (result, headers, body)
    }

    fn fixed_time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-19T01:30:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn quota_body(reasons: &[&str]) -> String {
        serde_json::json!({"error":{"errors":reasons.iter().map(|reason| {
            serde_json::json!({"reason":reason})
        }).collect::<Vec<_>>()}})
        .to_string()
    }

    #[test]
    fn status_probe_is_an_authenticated_empty_put_with_original_total() {
        let (result, headers, body) =
            response_fixture(308, "Range: bytes=0-262143\r\n", "", 1_048_576);
        assert_eq!(result.unwrap(), UploadSessionStatus::Incomplete(262_144));
        assert!(headers.starts_with("put /resumable/session http/1.1\r\n"));
        assert!(headers.contains("authorization: bearer fixture-only-native-access-token\r\n"));
        assert!(headers.contains("content-length: 0\r\n"));
        assert!(headers.contains("content-range: bytes */1048576\r\n"));
        assert!(!headers.contains("transfer-encoding:"));
        assert!(body.is_empty());
    }

    #[test]
    fn status_probe_preserves_completed_provider_receipt() {
        let (result, _, body) = response_fixture(
            201,
            "Content-Type: application/json\r\n",
            r#"{"id":"confirmed-video_01","status":{"uploadStatus":"uploaded"}}"#,
            1_048_576,
        );
        assert_eq!(
            result.unwrap(),
            UploadSessionStatus::Completed("confirmed-video_01".into())
        );
        assert!(body.is_empty());
    }

    #[test]
    fn incomplete_response_accepts_only_provider_confirmed_prefixes() {
        for (range, expected) in [
            (None, 0),
            (Some("bytes=0-0"), 1),
            (Some("bytes=0-262143"), 262_144),
            (Some("bytes=0-1048574"), 1_048_575),
        ] {
            assert_eq!(
                decode_upload_session_response(308, range, "", 1_048_576, None).unwrap(),
                UploadSessionStatus::Incomplete(expected)
            );
        }
    }

    #[test]
    fn malformed_or_out_of_file_acknowledgments_never_authorize_upload_progress() {
        for range in [
            "",
            "bytes=1-10",
            "bytes=0-",
            "bytes=0--1",
            "bytes=0-+99",
            "bytes=0- 99",
            "bytes=0-99 ",
            "bytes=0-99,200-299",
            "bytes 0-99/1048576",
            "bytes=0-1048576",
            "bytes=0-18446744073709551615",
            "bytes=0-18446744073709551616",
        ] {
            let failure =
                decode_upload_session_response(308, Some(range), "", 1_048_576, None).unwrap_err();
            assert!(failure.terminal, "unsafe range: {range}");
            assert_eq!(failure.limit, None);
            assert!(failure.detail.contains("checkpoint is retained"));
        }
        let overflow = decode_upload_session_response(
            308,
            Some("bytes=0-18446744073709551615"),
            "",
            u64::MAX,
            None,
        )
        .unwrap_err();
        assert!(overflow.terminal);
    }

    #[test]
    fn full_byte_acknowledgment_and_retry_after_wait_for_a_final_receipt() {
        let full =
            decode_upload_session_response(308, Some("bytes=0-1048575"), "", 1_048_576, None)
                .unwrap_err();
        assert!(!full.terminal);
        assert_eq!(full.limit, None);
        let deadline = fixed_time() + ChronoDuration::minutes(3);
        let delayed = decode_upload_session_response(
            308,
            Some("bytes=0-262143"),
            "",
            1_048_576,
            Some(deadline),
        )
        .unwrap_err();
        assert!(!delayed.terminal);
        assert_eq!(delayed.retry_after, Some(deadline));
    }

    #[test]
    fn terminal_session_failures_never_become_zero_offset_or_provider_receipts() {
        for status in [401, 403, 404, 410] {
            let failure = decode_upload_session_response(
                status,
                Some("bytes=0-262143"),
                r#"{"id":"not-a-success","error":{"message":"provider-private-detail"}}"#,
                1_048_576,
                None,
            )
            .unwrap_err();
            assert!(failure.terminal, "HTTP {status}");
            assert_eq!(failure.limit, None);
            assert_eq!(failure.retry_after, None);
            assert!(failure.detail.contains(&format!("HTTP {status}")));
            assert!(failure.detail.contains("protected checkpoint are retained"));
            assert!(!failure.detail.contains("provider-private-detail"));
        }
    }

    #[test]
    fn transient_status_failures_retain_retry_after_without_reporting_progress() {
        let deadline = fixed_time() + ChronoDuration::minutes(10);
        for status in [408, 429, 500, 502, 503, 504] {
            let failure = decode_upload_session_response(
                status,
                Some("bytes=0-262143"),
                "provider-private-detail",
                1_048_576,
                Some(deadline),
            )
            .unwrap_err();
            assert!(!failure.terminal, "HTTP {status}");
            assert_eq!(failure.retry_after, Some(deadline));
            assert_eq!(failure.limit, None);
            assert!(!failure.detail.contains("provider-private-detail"));
        }
    }

    #[test]
    fn status_probe_reads_provider_retry_after_header() {
        let before = Utc::now();
        let (result, _, _) = response_fixture(503, "Retry-After: 120\r\n", "", 1_048_576);
        let after = Utc::now();
        let failure = result.unwrap_err();
        assert!(!failure.terminal);
        let deadline = failure.retry_after.unwrap();
        assert!(deadline >= before + ChronoDuration::seconds(120));
        assert!(deadline <= after + ChronoDuration::seconds(120));
    }

    #[test]
    fn retry_after_accepts_future_delta_or_http_date_and_rejects_unsafe_values() {
        let time = fixed_time();
        assert_eq!(
            upload_retry_after(Some("120"), time),
            Some(time + ChronoDuration::seconds(120))
        );
        assert_eq!(
            upload_retry_after(Some("Mon, 19 Oct 2026 02:30:00 GMT"), time),
            Some(time + ChronoDuration::hours(1))
        );
        assert_eq!(upload_retry_after(None, time), None);
        for value in [
            "",
            "not-a-date",
            "-1",
            "0",
            "9223372036854775807",
            "18446744073709551616",
            "Mon, 19 Oct 2026 00:30:00 GMT",
        ] {
            assert_eq!(upload_retry_after(Some(value), time), None, "{value}");
        }
    }

    #[test]
    fn exact_quota_reasons_keep_distinct_project_and_channel_markers() {
        for (reasons, expected) in [
            (vec!["quotaExceeded"], API_QUOTA_LIMIT_MARKER),
            (vec!["dailyLimitExceeded"], API_QUOTA_LIMIT_MARKER),
            (vec!["uploadLimitExceeded"], DAILY_UPLOAD_LIMIT_MARKER),
            (
                vec!["quotaExceeded", "uploadLimitExceeded"],
                API_AND_CHANNEL_LIMIT_MARKER,
            ),
        ] {
            for status in [400, 403, 429] {
                let failure = decode_upload_session_response(
                    status,
                    None,
                    &quota_body(&reasons),
                    1_048_576,
                    None,
                )
                .unwrap_err();
                assert_eq!(failure.limit, Some(expected));
                assert_eq!(failure.detail, expected);
                assert!(!failure.terminal);
            }
        }
        for body in [
            quota_body(&["quotaExceededElsewhere"]),
            quota_body(&["pre_uploadLimitExceeded"]),
            quota_body(&["QuotaExceeded"]),
            r#"{"error":{"message":"quotaExceeded uploadLimitExceeded","errors":[{"reason":"forbidden"}]}}"#.into(),
        ] {
            let failure = decode_upload_session_response(403, None, &body, 1_048_576, None)
                .unwrap_err();
            assert_eq!(failure.limit, None);
            assert!(failure.terminal);
        }
    }

    #[test]
    fn completed_status_requires_a_readable_safe_video_id() {
        for status in [200, 201] {
            assert_eq!(
                decode_upload_session_response(
                    status,
                    None,
                    r#"{"id":"confirmed-video_01"}"#,
                    1_048_576,
                    None,
                )
                .unwrap(),
                UploadSessionStatus::Completed("confirmed-video_01".into())
            );
            for body in [
                "",
                "{}",
                r#"{"id":""}"#,
                r#"{"id":123}"#,
                r#"{"id":"unsafe/video"}"#,
            ] {
                let failure = decode_upload_session_response(status, None, body, 1_048_576, None)
                    .unwrap_err();
                assert!(failure.terminal);
                assert!(failure.detail.contains("checkpoint is retained"));
            }
        }
    }

    #[test]
    fn connection_revision_ignores_display_details_and_fences_every_credential_identity() {
        let fixture = Fixture::new();
        let connection = initialize_database(&fixture.state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,oauth_client_id,oauth_credential_id,oauth_project_id,active_channel,active_channel_id,connection_detail,updated_at) VALUES(1,'fixture-client','fixture-bundle','fixture-project','Fixture channel','fixture-channel','Connected',?1) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id='fixture-client',oauth_credential_id='fixture-bundle',oauth_project_id='fixture-project',active_channel='Fixture channel',active_channel_id='fixture-channel',connection_detail='Connected'", [now()]).unwrap();
        let revision = connection_settings(&connection)
            .unwrap()
            .connection_revision
            .unwrap();
        assert!(!revision.is_empty());
        connection.execute("UPDATE connection_settings SET active_channel='Renamed fixture channel',connection_detail='A different status message',updated_at='2030-01-01T00:00:00Z',deletion_authorized=1,deletion_sudo_until='2030-01-01T00:00:00Z' WHERE singleton=1", []).unwrap();
        assert_eq!(
            connection_settings(&connection)
                .unwrap()
                .connection_revision,
            Some(revision.clone())
        );
        for (column, original, changed) in [
            ("active_channel_id", "fixture-channel", "other-channel"),
            ("oauth_client_id", "fixture-client", "other-client"),
            ("oauth_project_id", "fixture-project", "other-project"),
            ("oauth_credential_id", "fixture-bundle", "other-bundle"),
        ] {
            connection
                .execute(
                    &format!("UPDATE connection_settings SET {column}=?1 WHERE singleton=1"),
                    [changed],
                )
                .unwrap();
            assert_ne!(
                connection_settings(&connection)
                    .unwrap()
                    .connection_revision,
                Some(revision.clone()),
                "{column}"
            );
            connection
                .execute(
                    &format!("UPDATE connection_settings SET {column}=?1 WHERE singleton=1"),
                    [original],
                )
                .unwrap();
            assert_eq!(
                connection_settings(&connection)
                    .unwrap()
                    .connection_revision,
                Some(revision.clone())
            );
        }
        connection
            .execute(
                "UPDATE connection_settings SET oauth_client_id=NULL WHERE singleton=1",
                [],
            )
            .unwrap();
        assert_eq!(
            connection_settings(&connection)
                .unwrap()
                .connection_revision,
            None
        );
    }
    fn queued_fixture() -> (Fixture, Connection) {
        let fixture = Fixture::new();
        let connection = initialize_database(&fixture.state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,oauth_client_id,oauth_project_id,oauth_credential_id,active_channel,active_channel_id,updated_at) VALUES(1,'fixture.apps.googleusercontent.com','project-a','bundle-a','Fixture','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id=excluded.oauth_client_id,oauth_project_id=excluded.oauth_project_id,oauth_credential_id=excluded.oauth_credential_id,active_channel=excluded.active_channel,active_channel_id=excluded.active_channel_id",[now()]).unwrap();
        connection.execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,status,confirmed_bytes,resumable_session_uri,created_at,updated_at) VALUES('interrupted','Existing','existing.mp4','channel-a','original-locator','original-locator',100,100,'dispatching',40,'protected-checkpoint',?1,?1),('fresh','Next','next.mp4','channel-a','next-locator','next-locator',100,100,'queued',0,NULL,?1,?1)",[now()]).unwrap();
        (fixture, connection)
    }

    #[test]
    fn expired_session_releases_capacity_and_preserves_original_checkpoint_and_identity() {
        let (fixture, mut connection) = queued_fixture();
        let snapshot: (String,String,i64,String) = connection.query_row("SELECT source_path,workspace_path,confirmed_bytes,resumable_session_uri FROM upload_items WHERE id='interrupted'",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
        let failure =
            decode_upload_session_response(404, None, "private provider payload", 100, None)
                .unwrap_err();
        assert!(!retain_upload_session_failure(
            &fixture.state,
            "interrupted",
            "channel-a",
            failure
        )
        .unwrap());
        let revision = connection_settings(&connection)
            .unwrap()
            .connection_revision
            .unwrap();
        assert!(!upload_recovery::eligible(
            &connection,
            "interrupted",
            "channel-a",
            &revision,
            Utc::now()
        )
        .unwrap());
        assert!(upload_recovery::eligible(
            &connection,
            "fresh",
            "channel-a",
            &revision,
            Utc::now()
        )
        .unwrap());
        assert_eq!(
            upload_resume_plan(&fixture.state, Utc::now()).unwrap(),
            (true, true, false, None)
        );
        assert_eq!(
            claim_uploads_with_controls(&mut connection, "channel-a", vec!["fresh".into()])
                .unwrap(),
            vec!["fresh".to_string()]
        );
        assert_eq!(connection.query_row("SELECT source_path,workspace_path,confirmed_bytes,resumable_session_uri FROM upload_items WHERE id='interrupted'",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap(),snapshot);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        let (status, detail): (String, String) = connection
            .query_row(
                "SELECT status,detail FROM upload_items WHERE id='interrupted'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "needs_reconciliation");
        assert!(detail.contains("HTTP 404"));
        assert!(!detail.contains("private provider payload"));
    }

    #[test]
    fn delayed_interrupted_session_rearms_worker_only_at_its_saved_deadline() {
        let (fixture, connection) = queued_fixture();
        connection
            .execute(
                "UPDATE upload_items SET status='cancelled' WHERE id='fresh'",
                [],
            )
            .unwrap();
        let failure = decode_upload_session_response(503, None, "", 100, None).unwrap_err();
        assert!(!retain_upload_session_failure(
            &fixture.state,
            "interrupted",
            "channel-a",
            failure
        )
        .unwrap());
        let current = Utc::now();
        let plan = upload_resume_plan(&fixture.state, current).unwrap();
        assert_eq!((plan.0, plan.1, plan.2), (false, false, false));
        let deadline = plan.3.expect("durable session retry must schedule a wake");
        assert!(deadline > current);
        let due =
            upload_resume_plan(&fixture.state, deadline + ChronoDuration::milliseconds(1)).unwrap();
        assert_eq!(due, (true, true, false, None));
        let reopened = Connection::open(&fixture.state.database_path).unwrap();
        let revision = connection_settings(&reopened)
            .unwrap()
            .connection_revision
            .unwrap();
        assert!(!upload_recovery::eligible(
            &reopened,
            "interrupted",
            "channel-a",
            &revision,
            current
        )
        .unwrap());
    }

    #[test]
    fn stale_or_cancelled_upload_failure_cannot_persist_a_recovery_hold() {
        let (fixture, connection) = queued_fixture();
        connection
            .execute(
                "UPDATE upload_items SET status='cancelled' WHERE id='interrupted'",
                [],
            )
            .unwrap();
        let failure = decode_upload_session_response(503, None, "", 100, None).unwrap_err();
        assert!(
            retain_upload_session_failure(&fixture.state, "interrupted", "channel-a", failure)
                .is_err()
        );
        let revision = connection_settings(&connection)
            .unwrap()
            .connection_revision
            .unwrap();
        assert!(
            !upload_recovery::recorded(&connection, "interrupted", "channel-a", &revision).unwrap()
        );
    }

    #[test]
    fn recovery_reaches_a_due_session_after_256_held_items_and_caps_only_promotions() {
        let (fixture, mut connection) = queued_fixture();
        let revision = connection_settings(&connection)
            .unwrap()
            .connection_revision
            .unwrap();
        let current = Utc::now();
        {
            let transaction = connection.transaction().unwrap();
            transaction.execute("UPDATE upload_items SET status='needs_reconciliation',updated_at='2026-10-19T01:00:00Z' WHERE id='interrupted'", []).unwrap();
            transaction.execute("UPDATE upload_items SET status='needs_reconciliation',channel_id='channel-b' WHERE id='fresh'", []).unwrap();
            for index in 0..256 {
                let item = format!("held-{index:03}");
                transaction.execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,status,confirmed_bytes,resumable_session_uri,created_at,updated_at) VALUES(?1,'Held','held.mp4','channel-a','retained-original','retained-original',100,100,'needs_reconciliation',40,'retained-checkpoint','2026-10-18T00:00:00Z','2026-10-18T00:00:00Z')", [&item]).unwrap();
                upload_recovery::record_failure(
                    &transaction,
                    &item,
                    "channel-a",
                    &revision,
                    index % 2 == 0,
                    current + ChronoDuration::hours(1),
                )
                .unwrap();
            }
            upload_recovery::record_failure(
                &transaction,
                "interrupted",
                "channel-a",
                &revision,
                false,
                current - ChronoDuration::minutes(2),
            )
            .unwrap();
            transaction.execute("INSERT INTO upload_items(id,title,file_name,channel_id,workspace_path,size_bytes,total_bytes,status,confirmed_bytes,video_id,created_at,updated_at) VALUES('receipt','Confirmed','receipt.mp4','channel-a','retained-original',100,100,'needs_reconciliation',100,'known-video','2026-10-18T00:00:00Z','2026-10-18T00:00:00Z')", []).unwrap();
            transaction.commit().unwrap();
        }
        assert_eq!(upload_resume_plan(&fixture.state, current).unwrap().0, true);
        let mut looked_up = Vec::new();
        assert_eq!(
            recover_resumable_uploads_with_checkpoint_lookup(
                &mut connection,
                "channel-a",
                |item| {
                    looked_up.push(item.to_owned());
                    Ok(true)
                }
            )
            .unwrap(),
            1
        );
        assert_eq!(looked_up, ["interrupted"]);
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM upload_items WHERE id LIKE 'held-%' AND status='needs_reconciliation' AND confirmed_bytes=40 AND source_path='retained-original' AND workspace_path='retained-original' AND resumable_session_uri='retained-checkpoint'", [], |row| row.get::<_, i64>(0)).unwrap(), 256);
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id='interrupted'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "queued"
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM upload_items WHERE id IN ('fresh','receipt') AND status='needs_reconciliation'", [], |row| row.get::<_,i64>(0)).unwrap(), 2);

        // A replacement permits another protected-session check for the same
        // channel. The cap applies to accepted identities, not scanned rows.
        connection
            .execute(
                "UPDATE connection_settings SET oauth_credential_id='new-bundle' WHERE singleton=1",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE upload_items SET status='needs_reconciliation' WHERE id='interrupted'",
                [],
            )
            .unwrap();
        let mut protected_reads = 0;
        assert_eq!(
            recover_resumable_uploads_with_checkpoint_lookup(&mut connection, "channel-a", |_| {
                protected_reads += 1;
                Ok(true)
            })
            .unwrap(),
            256
        );
        assert_eq!(protected_reads, 256);
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM upload_items WHERE channel_id='channel-a' AND video_id IS NULL AND status='needs_reconciliation'", [], |row| row.get::<_,i64>(0)).unwrap(), 1);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            259
        );
    }
}
