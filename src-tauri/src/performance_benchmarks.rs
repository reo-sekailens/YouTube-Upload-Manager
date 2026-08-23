//! Ignored, release-only native performance fixtures.
//!
//! These benchmarks deliberately use fresh synthetic SQLite databases under
//! the OS temporary directory. They never open the operator profile, access
//! secure storage, call Google, or print local paths and record contents.

use super::*;
use crate::media_runtime::copy_and_digest_profiled;
use std::{hint::black_box, time::Instant};

const SCALE_FIXTURES: [usize; 4] = [0, 100, 1_000, 10_000];
const SAMPLE_RUNS: usize = 7;

struct FixtureDatabase {
    root: PathBuf,
    state: AppState,
}

impl FixtureDatabase {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-performance-{label}-{}",
            Uuid::new_v4()
        ));
        let media_directory = root.join("media");
        fs::create_dir_all(&media_directory).unwrap();
        let state = AppState {
            database_path: root.join("fixture.sqlite3"),
            media_directory,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        database(&state).unwrap();
        Self { root, state }
    }
}

impl Drop for FixtureDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Debug)]
struct SampleSummary {
    samples: usize,
    minimum_ms: f64,
    median_ms: f64,
    p95_ms: f64,
}

fn require_release() {
    assert!(
        !cfg!(debug_assertions),
        "performance benchmarks must run with `cargo test --release performance_benchmark -- --ignored --nocapture`"
    );
}

fn samples_with_count(
    sample_count: usize,
    warmup: bool,
    mut operation: impl FnMut(),
) -> SampleSummary {
    if warmup {
        operation();
    }
    let mut elapsed = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let started = Instant::now();
        operation();
        elapsed.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    summarize_milliseconds(elapsed)
}

fn summarize_milliseconds(mut elapsed: Vec<f64>) -> SampleSummary {
    elapsed.sort_by(f64::total_cmp);
    let p95_index = ((elapsed.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(elapsed.len() - 1);
    SampleSummary {
        samples: elapsed.len(),
        minimum_ms: elapsed[0],
        median_ms: elapsed[elapsed.len() / 2],
        p95_ms: elapsed[p95_index],
    }
}

fn samples(operation: impl FnMut()) -> SampleSummary {
    samples_with_count(SAMPLE_RUNS, true, operation)
}

fn emit_result(surface: &str, fixture_size: usize, result: &SampleSummary) {
    eprintln!(
        "PERF_RESULT {{\"schemaVersion\":1,\"localOnly\":true,\"surface\":\"{surface}\",\"fixtureSize\":{fixture_size},\"samples\":{},\"minimumMs\":{:.3},\"p50Ms\":{:.3},\"p95Ms\":{:.3}}}",
        result.samples, result.minimum_ms, result.median_ms, result.p95_ms
    );
}

fn seed_dashboard_fixture(state: &AppState, size: usize) {
    let mut connection = database(state).unwrap();
    let transaction = connection.transaction().unwrap();
    transaction
        .execute(
            "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, connection_detail, updated_at) VALUES (1, 'Synthetic benchmark channel', 'performance-channel', 'Synthetic local fixture', ?1)",
            [now()],
        )
        .unwrap();
    {
        let mut upload = transaction
            .prepare_cached(
                "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, digest, status, total_bytes, video_id, detail, created_at, updated_at) VALUES (?1, ?2, ?3, 'Synthetic benchmark channel', 'performance-channel', ?4, 1048576, ?5, 'uploaded', 1048576, ?6, 'Synthetic local fixture', ?7, ?7)",
            )
            .unwrap();
        let mut remote = transaction
            .prepare_cached(
                "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, duration, privacy_status, upload_status, updated_at) VALUES (?1, 'Synthetic benchmark channel', 'performance-channel', ?2, 'PT1M', 'private', 'processed', ?3)",
            )
            .unwrap();
        let timestamp = now();
        for index in 0..size {
            let id = format!("fixture-upload-{index:05}");
            let title = format!("Unique local fixture {index:05}");
            let file_name = format!("fixture-{index:05}.mp4");
            let workspace_path = format!("synthetic-managed-{index:05}");
            let digest = format!("{index:064x}");
            let video_id = format!("local-video-{index:05}");
            upload
                .execute(params![
                    id,
                    title,
                    file_name,
                    workspace_path,
                    digest,
                    video_id,
                    timestamp
                ])
                .unwrap();

            // Two ordered numeric sequences keep a small, deterministic amount
            // of positive duplicate evidence while most pairs remain distinct.
            let remote_title = if index % 250 == 1 {
                format!("Remote fixture 2026 {:05} (2)", index - 1)
            } else {
                format!("Remote fixture 2026 {index:05}")
            };
            remote
                .execute(params![
                    format!("remote-video-{index:05}"),
                    remote_title,
                    timestamp
                ])
                .unwrap();
        }
    }
    transaction.commit().unwrap();
}

fn read_dashboard_snapshot(state: &AppState) -> DashboardSnapshot {
    let connection = database(state).unwrap();
    let settings = connection_settings(&connection).unwrap();
    let items = connection
        .prepare("SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, visibility, made_for_kids, playlist_id, playlist_title, upload_started_at, transfer_bytes_per_second, delete_source_after_upload, source_delete_status, updated_at FROM upload_items WHERE status != 'cancelled' AND channel_id = ?1 ORDER BY updated_at DESC")
        .unwrap()
        .query_map(["performance-channel"], row_to_upload_item)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let ignored_candidate_ids = ignored_duplicate_candidate_ids(&connection).unwrap();
    let mut duplicates = current_duplicate_candidates(&connection).unwrap();
    duplicates.retain(|candidate| !ignored_candidate_ids.contains(&candidate.id));
    let pending_title_duplicates = pending_upload_title_duplicates(
        &connection,
        settings.active_channel_id.as_deref().unwrap(),
    )
    .unwrap();
    DashboardSnapshot {
        active_channel: settings.active_channel,
        active_channel_id: settings.active_channel_id,
        revision: state_events::current_revision(&connection).unwrap(),
        items,
        duplicates,
        pending_title_duplicates,
    }
}

#[test]
#[ignore = "release-only local performance evidence"]
fn performance_benchmark_dashboard_and_dedupe_scale() {
    require_release();
    for size in SCALE_FIXTURES {
        let fixture = FixtureDatabase::new(&format!("dashboard-{size}"));
        seed_dashboard_fixture(&fixture.state, size);
        let operation = || {
            let snapshot = read_dashboard_snapshot(&fixture.state);
            let serialized = serde_json::to_vec(&snapshot).unwrap();
            black_box(serialized);
        };
        let summary = samples(operation);
        emit_result("dashboard-and-dedupe", size, &summary);
    }
}

fn seed_preflight_fixture(state: &AppState, size: usize) -> String {
    let job_id = "synthetic-preflight".to_string();
    let mut connection = database(state).unwrap();
    let transaction = connection.transaction().unwrap();
    let timestamp = now();
    transaction
        .execute(
            "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, connection_detail, updated_at) VALUES (1, 'Synthetic benchmark channel', 'performance-channel', 'Synthetic local fixture', ?1)",
            [&timestamp],
        )
        .unwrap();
    transaction
        .execute(
            "INSERT INTO preflight_scan_jobs (id, channel_id, mode, status, total_files, completed_files, inventory_status, detail, matched_files, evidence_materialized_at, created_at, updated_at) VALUES (?1, 'performance-channel', 'light', 'complete', ?2, ?2, 'complete', 'Synthetic local fixture', 0, ?3, ?3, ?3)",
            params![job_id, size as i64, timestamp],
        )
        .unwrap();
    {
        let mut file = transaction
            .prepare_cached(
                "INSERT INTO preflight_scan_files (job_id, ordinal, source_locator, file_name, size_bytes, status, metadata_json, metadata_status) VALUES (?1, ?2, '\"synthetic-fixture\"', ?3, 1048576, 'complete', ?4, 'complete')",
            )
            .unwrap();
        let metadata =
            serde_json::to_string(&unavailable_preflight_local_metadata(Some("MP4".into())))
                .unwrap();
        for index in 0..size {
            file.execute(params![
                job_id,
                index as i64,
                format!("preflight-{index:05}.mp4"),
                metadata
            ])
            .unwrap();
        }
    }
    {
        let mut remote = transaction
            .prepare_cached(
                "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, normalized_title, canonical_title, has_copy_marker, numeric_title_key, title_keys_version, upload_status, updated_at) VALUES (?1, 'Synthetic benchmark channel', 'performance-channel', ?2, ?3, ?4, ?5, ?6, ?7, 'processed', ?8)",
            )
            .unwrap();
        for index in 0..10_000 {
            let title = format!("Remote fixture {index:05}");
            let keys = title_matching::keys(&title);
            remote
                .execute(params![
                    format!("remote-preflight-{index:05}"),
                    title,
                    keys.normalized,
                    keys.canonical,
                    keys.has_copy_marker as i64,
                    keys.numeric,
                    title_matching::TITLE_KEYS_VERSION,
                    timestamp
                ])
                .unwrap();
        }
    }
    transaction.commit().unwrap();
    job_id
}

#[test]
#[ignore = "release-only local performance evidence"]
fn performance_benchmark_preflight_snapshot_1000() {
    require_release();
    let fixture = FixtureDatabase::new("preflight-1000");
    let job_id = seed_preflight_fixture(&fixture.state, 1_000);
    let status_summary = samples(|| {
        let status = load_preflight_scan_status(&fixture.state, &job_id).unwrap();
        assert_eq!(status.total_files, 1_000);
        black_box(serde_json::to_vec(&status).unwrap());
    });
    emit_result("preflight-status-10k-inventory", 1_000, &status_summary);
    let mut maximum_page_payload_bytes = 0usize;
    let page_summary = samples(|| {
        let snapshot = load_preflight_scan_page(&fixture.state, &job_id, 0, 48, 0, 48).unwrap();
        assert_eq!(snapshot.files.len(), 48);
        let payload = serde_json::to_vec(&snapshot).unwrap();
        maximum_page_payload_bytes = maximum_page_payload_bytes.max(payload.len());
        assert!(
            payload.len() < 256 * 1024,
            "preflight page payload was {} bytes",
            payload.len()
        );
        black_box(payload);
    });
    emit_result("preflight-page-10k-inventory", 1_000, &page_summary);
    eprintln!(
        "PERF_EVIDENCE {{\"schemaVersion\":1,\"localOnly\":true,\"surface\":\"preflight-page-10k-inventory\",\"fixtureSize\":1000,\"metric\":\"maximumPayloadBytes\",\"value\":{maximum_page_payload_bytes},\"budgetBytes\":{}}}",
        256 * 1024
    );
}

fn seed_folder_overview_fixture(state: &AppState, size: usize) {
    let mut connection = database(state).unwrap();
    let transaction = connection.transaction().unwrap();
    let timestamp = now();
    transaction
        .execute(
            "INSERT INTO folder_monitor_settings (singleton, enabled, folder_path, channel_name, channel_id, visibility, status, detail, updated_at) VALUES (1, 1, 'synthetic-folder', 'Synthetic benchmark channel', 'performance-channel', 'private', 'watching', 'Synthetic local fixture', ?1)",
            [&timestamp],
        )
        .unwrap();
    {
        let mut observation = transaction
            .prepare_cached(
                "INSERT INTO folder_monitor_observations (channel_id, channel_name, file_path, size_bytes, modified_key, state, first_seen_at, updated_at) VALUES ('performance-channel', 'Synthetic benchmark channel', ?1, 1048576, ?2, 'observed', ?3, ?3)",
            )
            .unwrap();
        let mut event = transaction
            .prepare_cached(
                "INSERT INTO audit_events (id, channel_name, channel_id, kind, detail, created_at) VALUES (?1, 'Synthetic benchmark channel', 'performance-channel', 'folder_monitor_fixture', 'Synthetic local fixture', ?2)",
            )
            .unwrap();
        for index in 0..size {
            observation
                .execute(params![
                    format!("synthetic-{index:05}.mp4"),
                    format!("fixture-{index:05}"),
                    timestamp
                ])
                .unwrap();
            event
                .execute(params![format!("folder-event-{index:05}"), timestamp])
                .unwrap();
        }
    }
    transaction.commit().unwrap();
}

#[test]
#[ignore = "release-only local performance evidence"]
fn performance_benchmark_folder_overview_10000() {
    require_release();
    let fixture = FixtureDatabase::new("folder-10000");
    seed_folder_overview_fixture(&fixture.state, 10_000);
    let connection = database(&fixture.state).unwrap();
    let summary = samples(|| {
        let overview = folder_monitor_overview(&connection).unwrap();
        assert_eq!(overview.files.len(), 200);
        assert_eq!(overview.logs.len(), 200);
        black_box(serde_json::to_vec(&overview).unwrap());
    });
    emit_result("folder-overview", 10_000, &summary);
}

fn write_deterministic_media(path: &Path, bytes: usize) {
    let mut file = File::create(path).unwrap();
    let block = (0..1024 * 1024)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    for _ in 0..bytes / block.len() {
        file.write_all(&block).unwrap();
    }
    file.sync_all().unwrap();
}

#[test]
#[ignore = "release-only local performance evidence"]
fn performance_benchmark_copy_and_blake3_64_mib() {
    require_release();
    let fixture = FixtureDatabase::new("copy-hash-64mib");
    let source = fixture.root.join("source.bin");
    let destination = fixture.root.join("destination.partial");
    let bytes = 64 * 1024 * 1024;
    write_deterministic_media(&source, bytes);

    let digest_summary = samples(|| {
        let (read, digest) = digest_file(&source).unwrap();
        assert_eq!(read, bytes as u64);
        black_box(digest);
    });
    emit_result("blake3-read", bytes, &digest_summary);

    let mut stream_and_hash_samples = Vec::with_capacity(SAMPLE_RUNS + 1);
    let mut durable_flush_samples = Vec::with_capacity(SAMPLE_RUNS + 1);
    let copy_summary = samples(|| {
        if destination.exists() {
            fs::remove_file(&destination).unwrap();
        }
        let ((copied, digest), timings) = copy_and_digest_profiled(&source, &destination).unwrap();
        assert_eq!(copied, bytes as u64);
        assert_eq!(fs::metadata(&destination).unwrap().len(), bytes as u64);
        stream_and_hash_samples.push(timings.stream_and_hash.as_secs_f64() * 1_000.0);
        durable_flush_samples.push(timings.durable_flush.as_secs_f64() * 1_000.0);
        black_box(digest);
    });
    stream_and_hash_samples.remove(0);
    durable_flush_samples.remove(0);
    emit_result("copy-and-blake3", bytes, &copy_summary);
    emit_result(
        "copy-and-blake3-stream-hash",
        bytes,
        &summarize_milliseconds(stream_and_hash_samples),
    );
    emit_result(
        "copy-and-blake3-durable-flush",
        bytes,
        &summarize_milliseconds(durable_flush_samples),
    );
}
