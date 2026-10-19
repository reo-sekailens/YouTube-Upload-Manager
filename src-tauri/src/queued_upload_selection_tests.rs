use super::*;

struct Fixture {
    database: Connection,
    root: PathBuf,
    source: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("queued-upload-selection-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let source = root.join("synthetic-original.mp4");
        fs::write(&source, b"synthetic fixture source").unwrap();
        let database = Connection::open_in_memory().unwrap();
        database
            .execute_batch(
                "CREATE TABLE upload_items(
                    id TEXT PRIMARY KEY, workspace_path TEXT NOT NULL,
                    created_at TEXT NOT NULL, upload_batch_id TEXT,
                    folder_monitor_id TEXT, status TEXT NOT NULL,
                    channel_id TEXT NOT NULL, confirmed_bytes INTEGER NOT NULL,
                    detail TEXT NOT NULL
                );
                CREATE TABLE upload_priority_choices(
                    channel_id TEXT, kind TEXT, target_id TEXT, priority INTEGER
                );
                CREATE TABLE folder_monitors(id TEXT, channel_id TEXT, removed INTEGER);",
            )
            .unwrap();
        Self {
            database,
            root,
            source,
        }
    }

    fn insert(&self, item: &str, channel: &str, created_at: DateTime<Utc>, source_exists: bool) {
        let source = if source_exists {
            self.source.clone()
        } else {
            self.root.join("missing-original.mp4")
        };
        self.database.execute(
            "INSERT INTO upload_items(id,workspace_path,created_at,status,channel_id,confirmed_bytes,detail)
             VALUES(?1,?2,?3,'queued',?4,0,'Reviewed upload')",
            params![item, source.to_str().unwrap(), created_at.to_rfc3339(), channel],
        ).unwrap();
    }

    fn select(&self) -> Result<(bool, Vec<QueuedUploadRow>), String> {
        eligible_queued_upload_rows(&self.database, "channel-a", "revision-a", time())
    }

    fn changes(&self) -> i64 {
        self.database
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap()
    }

    fn assert_source_unchanged(&self) {
        assert_eq!(fs::read(&self.source).unwrap(), b"synthetic fixture source");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.source);
        let _ = fs::remove_dir(&self.root);
    }
}

fn time() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-19T01:30:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn queued_selection_skips_256_terminal_or_future_holds() {
    for terminal in [true, false] {
        let fixture = Fixture::new();
        for index in 0..256 {
            let item = format!("held-{index:03}");
            fixture.insert(
                &item,
                "channel-a",
                time() - chrono::Duration::hours(1),
                true,
            );
            upload_recovery::record_failure(
                &fixture.database,
                &item,
                "channel-a",
                "revision-a",
                terminal,
                time(),
            )
            .unwrap();
        }
        fixture.insert("ready", "channel-a", time(), true);
        fixture.insert(
            "foreign",
            "channel-b",
            time() - chrono::Duration::hours(2),
            true,
        );
        let changes = fixture.changes();
        fixture
            .database
            .execute_batch("PRAGMA query_only = ON")
            .unwrap();

        let (has_queued, rows) = fixture.select().unwrap();
        assert!(has_queued);
        assert_eq!(
            rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
            ["ready"]
        );
        assert_eq!(fixture.changes(), changes);
        assert_eq!(
            fixture.database.query_row(
                "SELECT count(*) FROM youtube_upload_recovery WHERE channel_id='channel-a' AND credential_revision='revision-a'",
                [],
                |row| row.get::<_, i64>(0),
            ).unwrap(),
            256
        );
        for index in 0..256 {
            assert!(!upload_recovery::eligible(
                &fixture.database,
                &format!("held-{index:03}"),
                "channel-a",
                "revision-a",
                time(),
            )
            .unwrap());
        }
        fixture.assert_source_unchanged();
    }
}

#[test]
fn queued_selection_skips_256_missing_sources_without_creating_recovery_schema() {
    let fixture = Fixture::new();
    for index in 0..256 {
        fixture.insert(
            &format!("missing-{index:03}"),
            "channel-a",
            time() - chrono::Duration::hours(1),
            false,
        );
    }
    fixture.insert("ready", "channel-a", time(), true);
    fixture.insert(
        "foreign",
        "channel-b",
        time() - chrono::Duration::hours(2),
        true,
    );
    let changes = fixture.changes();
    fixture
        .database
        .execute_batch("PRAGMA query_only = ON")
        .unwrap();

    let (has_queued, rows) = fixture.select().unwrap();
    assert!(has_queued);
    assert_eq!(
        rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        ["ready"]
    );
    assert_eq!(fixture.changes(), changes);
    assert_eq!(
        fixture
            .database
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='youtube_upload_recovery'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    fixture.assert_source_unchanged();
}

#[test]
fn queued_selection_caps_ready_rows_and_preserves_existing_priority_order() {
    let fixture = Fixture::new();
    for index in 0..300 {
        fixture.insert(&format!("ready-{index:03}"), "channel-a", time(), true);
    }
    fixture.insert(
        "foreign",
        "channel-b",
        time() - chrono::Duration::hours(2),
        true,
    );
    fixture
        .database
        .execute_batch(
            "UPDATE upload_items SET confirmed_bytes=1 WHERE id='ready-299';
         UPDATE upload_items SET upload_batch_id='priority-batch' WHERE id='ready-298';
         INSERT INTO upload_priority_choices(channel_id,kind,target_id,priority)
         VALUES('channel-a','batch','priority-batch',9);",
        )
        .unwrap();
    let changes = fixture.changes();
    fixture
        .database
        .execute_batch("PRAGMA query_only = ON")
        .unwrap();

    let (_, rows) = fixture.select().unwrap();
    assert_eq!(rows.len(), 256);
    assert_eq!(rows[0].0, "ready-299");
    assert_eq!(rows[1].0, "ready-298");
    assert_eq!(rows[2].0, "ready-000");
    assert!(rows.iter().all(|row| row.0 != "foreign"));
    assert_eq!(fixture.changes(), changes);
    fixture.assert_source_unchanged();
}

#[test]
fn queued_selection_fails_closed_on_malformed_recovery_evidence() {
    let fixture = Fixture::new();
    fixture.insert(
        "corrupt",
        "channel-a",
        time() - chrono::Duration::hours(1),
        true,
    );
    fixture.insert("ready", "channel-a", time(), true);
    upload_recovery::record_failure(
        &fixture.database,
        "corrupt",
        "channel-a",
        "revision-a",
        false,
        time(),
    )
    .unwrap();
    fixture
        .database
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
         UPDATE youtube_upload_recovery SET retry_at_ms='broken';
         PRAGMA query_only = ON;",
        )
        .unwrap();
    let changes = fixture.changes();

    assert!(fixture.select().is_err());
    assert_eq!(fixture.changes(), changes);
    fixture.assert_source_unchanged();
}
