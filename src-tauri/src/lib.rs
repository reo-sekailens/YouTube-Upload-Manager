use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use keyring::v1::Entry as CredentialEntry;
#[cfg(feature = "performance-harness")]
use rusqlite::trace::TraceEventCodes;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Condvar, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions as FsOpenOptions};
use uuid::Uuid;

mod media_runtime;
mod performance;
#[cfg(test)]
mod performance_benchmarks;
mod persistence;
mod provider_transport;
mod state_events;
mod title_matching;
#[cfg(test)]
mod upload_performance_benchmarks;
mod upload_scheduler;

#[cfg(test)]
use media_runtime::bundled_sidecar_path;
use media_runtime::{
    active_upload, cached_ffprobe_report, digest_file, digest_file_scheduled,
    digest_reader_for_path, iso_bmff_duration_seconds, probe_duration_from_report,
    source_volume_id, MediaIoPriority, ProbeDepth, ThrottledCancellationCheck,
    HASH_READ_BUFFER_BYTES,
};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as UnixOpenOptionsExt;
#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

const FOLDER_MONITOR_POLL_INTERVAL: Duration = Duration::from_secs(5);
const DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(30);
const CAPABILITY_CHECK_INTERVAL: ChronoDuration = ChronoDuration::days(3);
const FOLDER_INVENTORY_REFRESH_INTERVAL: ChronoDuration = ChronoDuration::hours(6);
const CAPABILITY_READ_SAMPLE_BYTES: usize = 8 * 1024 * 1024;
const MAX_CONCURRENT_UPLOADS_PER_VOLUME: usize = 4;
const MAX_CONCURRENT_UPLOAD_WORKERS: usize = 4;
const DAILY_UPLOAD_LIMIT_MARKER: &str = "youtube_daily_upload_limit";
const UPLOAD_CANCELLED_MARKER: &str = "upload_cancelled_locally";
const UPLOAD_OAUTH_SCOPES: &str =
    "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly";
const DELETION_OAUTH_SCOPES: &str =
    "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly https://www.googleapis.com/auth/youtube.force-ssl";
const DELETION_SUDO_MODE_MINUTES: i64 = 15;
const DIAGNOSTIC_REPORT_EVENT_LIMIT: i64 = 30;
const PANIC_MARKER_FILE: &str = "last-panic-marker";
const UPLOAD_CHUNK_MIN_BYTES: usize = 8 * 1024 * 1024;
const UPLOAD_CHUNK_DEFAULT_BYTES: usize = 64 * 1024 * 1024;
const UPLOAD_CHUNK_MAX_BYTES: usize = 128 * 1024 * 1024;
const WEBVIEW_ERROR_MARKER_FILE: &str = "last-webview-error-marker";
const APP_RELEASE_CHANNEL: &str = env!("APP_RELEASE_CHANNEL");
/// YouTube publishes this 256 GB maximum in decimal units.
const YOUTUBE_MAX_UPLOAD_BYTES: u64 = 256_000_000_000;
const YOUTUBE_MAX_UPLOAD_DURATION_SECONDS: f64 = 12.0 * 60.0 * 60.0;
const SUPPORTED_VIDEO_EXTENSIONS: &[&str] = &[
    "3g2", "3gp", "avi", "flv", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "webm", "wmv",
];

#[derive(Clone)]
struct AppState {
    database_path: PathBuf,
    media_directory: PathBuf,
    folder_monitor_lock: Arc<Mutex<()>>,
    oauth_attempts: Arc<Mutex<HashMap<String, OAuthAttemptKind>>>,
    #[cfg(not(test))]
    startup: Arc<StartupGate>,
    #[cfg(not(test))]
    state_events: Arc<state_events::StateEventRuntime>,
    #[cfg(not(test))]
    provider_transport: Arc<provider_transport::ProviderTransport>,
    #[cfg(not(test))]
    upload_scheduler: Arc<upload_scheduler::UploadScheduler>,
    #[cfg(not(test))]
    deletion_execution_lock: Arc<Mutex<()>>,
}

#[cfg(not(test))]
fn provider_transport(state: &AppState) -> &provider_transport::ProviderTransport {
    &state.provider_transport
}

#[cfg(test)]
fn provider_transport(_state: &AppState) -> &'static provider_transport::ProviderTransport {
    static TRANSPORT: OnceLock<provider_transport::ProviderTransport> = OnceLock::new();
    TRANSPORT.get_or_init(provider_transport::ProviderTransport::new)
}

#[cfg(not(test))]
fn upload_scheduler(state: &AppState) -> &upload_scheduler::UploadScheduler {
    &state.upload_scheduler
}

#[cfg(test)]
fn upload_scheduler(_state: &AppState) -> &'static upload_scheduler::UploadScheduler {
    static SCHEDULER: OnceLock<upload_scheduler::UploadScheduler> = OnceLock::new();
    SCHEDULER.get_or_init(|| upload_scheduler::UploadScheduler::new(MAX_CONCURRENT_UPLOAD_WORKERS))
}

#[cfg(not(test))]
fn deletion_execution_lock(state: &AppState) -> &Mutex<()> {
    &state.deletion_execution_lock
}

#[cfg(test)]
fn deletion_execution_lock(_state: &AppState) -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

const STARTUP_RECOVERY_PENDING: u8 = 0;
const STARTUP_RECOVERY_RUNNING: u8 = 1;
const STARTUP_RECOVERY_COMPLETE: u8 = 2;
const STARTUP_RECOVERY_FAILED: u8 = 3;

#[derive(Default)]
struct StartupRecoveryWork {
    interrupted_upload_ids: Vec<String>,
    reconcile_imports: bool,
    resume_watched_hashes: bool,
    resume_preflight: bool,
    start_folder_worker: bool,
    start_quota_worker: bool,
    start_postprocess_worker: bool,
}

#[derive(Default)]
struct StartupWorkerEligibility {
    folder: bool,
    quota: bool,
    postprocess: bool,
}

struct StartupGate {
    classification_complete: AtomicBool,
    safe_shell_rendered: AtomicBool,
    deferred_started: AtomicBool,
    deferred_state: AtomicU8,
    dispatch_ready: AtomicBool,
    folder_worker_started: AtomicBool,
    quota_worker_started: AtomicBool,
    folder_worker_wake: Condvar,
    quota_worker_wake: Condvar,
    worker_wake_lock: Mutex<()>,
    recovery_work: Mutex<Option<StartupRecoveryWork>>,
}

impl StartupGate {
    fn new() -> Self {
        Self {
            classification_complete: AtomicBool::new(false),
            safe_shell_rendered: AtomicBool::new(false),
            deferred_started: AtomicBool::new(false),
            deferred_state: AtomicU8::new(STARTUP_RECOVERY_PENDING),
            dispatch_ready: AtomicBool::new(false),
            folder_worker_started: AtomicBool::new(false),
            quota_worker_started: AtomicBool::new(false),
            folder_worker_wake: Condvar::new(),
            quota_worker_wake: Condvar::new(),
            worker_wake_lock: Mutex::new(()),
            recovery_work: Mutex::new(None),
        }
    }

    fn readiness(&self) -> StartupReadiness {
        let recovery_state = self.deferred_state.load(Ordering::Acquire);
        let deferred_recovery_state = match recovery_state {
            STARTUP_RECOVERY_RUNNING => "running",
            STARTUP_RECOVERY_COMPLETE => "complete",
            STARTUP_RECOVERY_FAILED => "failed",
            _ => "pending",
        };
        let queue_actions_enabled = self.dispatch_ready.load(Ordering::Acquire)
            && recovery_state == STARTUP_RECOVERY_COMPLETE;
        StartupReadiness {
            classification_complete: self.classification_complete.load(Ordering::Acquire),
            safe_shell_rendered: self.safe_shell_rendered.load(Ordering::Acquire),
            deferred_recovery_state: deferred_recovery_state.to_string(),
            queue_actions_enabled,
            detail: match recovery_state {
                STARTUP_RECOVERY_RUNNING => {
                    "Recovering saved device-local work before upload controls are enabled."
                }
                STARTUP_RECOVERY_COMPLETE => {
                    "Saved work is classified and upload controls are ready."
                }
                STARTUP_RECOVERY_FAILED => {
                    "Saved work could not be classified safely. Upload controls remain disabled."
                }
                _ => "The safe shell is ready; saved work has not started recovery yet.",
            }
            .to_string(),
        }
    }
}

#[cfg(not(test))]
fn startup_gate(state: &AppState) -> &StartupGate {
    &state.startup
}

#[cfg(test)]
fn startup_gate(_state: &AppState) -> &'static StartupGate {
    static TEST_GATE: OnceLock<StartupGate> = OnceLock::new();
    TEST_GATE.get_or_init(|| {
        let gate = StartupGate::new();
        gate.classification_complete.store(true, Ordering::Release);
        gate.safe_shell_rendered.store(true, Ordering::Release);
        gate.deferred_started.store(true, Ordering::Release);
        gate.deferred_state
            .store(STARTUP_RECOVERY_COMPLETE, Ordering::Release);
        gate.dispatch_ready.store(true, Ordering::Release);
        gate
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OAuthAttemptKind {
    Connection,
    Deletion,
}

fn cancel_connection_attempt(
    attempts: &mut HashMap<String, OAuthAttemptKind>,
    attempt_id: &str,
) -> bool {
    if attempts.get(attempt_id) == Some(&OAuthAttemptKind::Connection) {
        attempts.remove(attempt_id);
        true
    } else {
        false
    }
}

#[derive(Clone)]
struct PreflightLocalDeleteTarget {
    path: PathBuf,
    file_name: String,
    signature: (u64, String),
    digest: String,
    created_at: Instant,
}

fn preflight_local_delete_targets() -> &'static Mutex<HashMap<String, PreflightLocalDeleteTarget>> {
    static TARGETS: OnceLock<Mutex<HashMap<String, PreflightLocalDeleteTarget>>> = OnceLock::new();
    TARGETS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// All library-refresh callers share one local inventory and one staging table.
/// Serializing them prevents concurrent refreshes from racing the atomic
/// replacement of that inventory.
fn inventory_sync_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadItem {
    id: String,
    title: String,
    file_name: String,
    size_bytes: u64,
    digest: Option<String>,
    status: String,
    confirmed_bytes: u64,
    total_bytes: u64,
    video_id: Option<String>,
    detail: Option<String>,
    visibility: String,
    made_for_kids: bool,
    playlist_id: Option<String>,
    playlist_title: Option<String>,
    upload_started_at: Option<String>,
    transfer_bytes_per_second: Option<f64>,
    delete_source_after_upload: bool,
    source_delete_status: Option<String>,
    updated_at: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManualUploadSettings {
    made_for_kids: bool,
    visibility: String,
    playlist_id: Option<String>,
    playlist_title: Option<String>,
    delete_source_after_upload: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchImportItemReceipt {
    ordinal: u64,
    file_name: String,
    status: String,
    item: Option<UploadItem>,
    detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchImportReceipt {
    requested_count: u64,
    imported_count: u64,
    queued_count: u64,
    duplicate_count: u64,
    failed_count: u64,
    items: Vec<BatchImportItemReceipt>,
    detail: Option<String>,
}

#[derive(Serialize)]
struct YouTubePlaylist {
    id: String,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateCandidate {
    id: String,
    confidence: String,
    left_title: String,
    right_title: String,
    left_video_id: Option<String>,
    right_video_id: Option<String>,
    evidence: String,
    decision: Option<String>,
}

/// A locally persisted upload that needs an explicit decision because a
/// normalized light-title match was found in the active YouTube library, the
/// current local batch/queue, or both.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadTitleDuplicate {
    item_id: String,
    title: String,
    matched_titles: Vec<String>,
    match_scope: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestDuplicateScan {
    id: String,
    mode: String,
    status: String,
    total_files: u64,
    completed_files: u64,
    current_file_name: Option<String>,
    pending_metadata_files: u64,
    matched_files: u64,
    file_offset: u64,
    file_limit: u64,
    activity_offset: u64,
    activity_limit: u64,
    activity_total: u64,
    files: Vec<PreIngestDuplicateFile>,
    activity_log: Vec<PreIngestActivityLogEntry>,
    youtube_title_checked: bool,
    youtube_check_detail: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestDuplicateScanStatus {
    id: String,
    mode: String,
    status: String,
    total_files: u64,
    completed_files: u64,
    current_file_name: Option<String>,
    pending_metadata_files: u64,
    matched_files: u64,
    activity_total: u64,
    youtube_title_checked: bool,
    youtube_check_detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestActivityLogEntry {
    file_name: Option<String>,
    message: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestDuplicateFile {
    local_delete_token: Option<String>,
    can_delete_local_duplicate: bool,
    ordinal: u64,
    file_name: String,
    size_bytes: u64,
    local_metadata: PreIngestLocalMetadata,
    local_matches: Vec<PreIngestLocalMatch>,
    dropped_duplicate_file_names: Vec<String>,
    uploaded_title_matches: Vec<PreIngestUploadedTitleMatch>,
    local_match_count: u64,
    dropped_duplicate_count: u64,
    uploaded_title_match_count: u64,
    error: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestLocalMetadata {
    file_type: Option<String>,
    modified_at: Option<String>,
    duration_seconds: Option<f64>,
    size_bytes: Option<u64>,
    container_format: Option<String>,
    bit_rate: Option<String>,
    streams: Vec<PreIngestMetadataStream>,
    metadata_fields: Vec<PreIngestMetadataField>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestMetadataField {
    label: String,
    value: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestMetadataStream {
    kind: String,
    label: String,
    fields: Vec<PreIngestMetadataField>,
}

fn unavailable_preflight_local_metadata(file_type: Option<String>) -> PreIngestLocalMetadata {
    PreIngestLocalMetadata {
        file_type,
        modified_at: None,
        duration_seconds: None,
        size_bytes: None,
        container_format: None,
        bit_rate: None,
        streams: Vec::new(),
        metadata_fields: Vec::new(),
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestLocalMatch {
    title: String,
    file_name: String,
    status: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreIngestUploadedTitleMatch {
    title: String,
    duration: Option<String>,
    privacy_status: Option<String>,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardSnapshot {
    active_channel: Option<String>,
    active_channel_id: Option<String>,
    revision: u64,
    items: Vec<UploadItem>,
    duplicates: Vec<DuplicateCandidate>,
    pending_title_duplicates: Vec<UploadTitleDuplicate>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct StartupReadiness {
    classification_complete: bool,
    safe_shell_rendered: bool,
    deferred_recovery_state: String,
    queue_actions_enabled: bool,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupBootstrap {
    crash_recovery: CrashRecoveryStatus,
    connection: ConnectionSettings,
    snapshot: DashboardSnapshot,
    readiness: StartupReadiness,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionSettings {
    oauth_configured: bool,
    active_channel: Option<String>,
    active_channel_id: Option<String>,
    connected: bool,
    secure_store_available: bool,
    deletion_authorized: bool,
    deletion_sudo_active: bool,
    deletion_sudo_expires_at: Option<String>,
    detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManualUploadDefaults {
    made_for_kids: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthStart {
    authorization_url: String,
    attempt_id: String,
}

#[derive(Deserialize)]
struct DesktopOAuthClientFile {
    installed: DesktopOAuthClient,
}

#[derive(Deserialize)]
struct DesktopOAuthClient {
    client_id: String,
    #[serde(default)]
    client_secret: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteVideo {
    video_id: String,
    title: String,
    duration: Option<String>,
    privacy_status: Option<String>,
    upload_status: Option<String>,
    updated_at: String,
}

const PORTABLE_ARCHIVE_VERSION: u8 = 1;
const PORTABLE_ARCHIVE_MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortableArchive {
    version: u8,
    created_at: String,
    uploads: Vec<PortableUpload>,
    remote_videos: Vec<PortableRemoteVideo>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortableUpload {
    id: String,
    title: String,
    file_name: String,
    size_bytes: u64,
    digest: Option<String>,
    video_id: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortableRemoteVideo {
    video_id: String,
    channel_name: String,
    #[serde(default)]
    channel_id: String,
    title: String,
    duration: Option<String>,
    privacy_status: Option<String>,
    #[serde(default)]
    upload_status: Option<String>,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PortableArchiveReceipt {
    upload_count: usize,
    remote_video_count: usize,
    bytes: u64,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeletionRequest {
    id: String,
    video_id: String,
    title: String,
    status: String,
    detail: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderMonitorSettings {
    enabled: bool,
    folder_path: Option<String>,
    channel_name: Option<String>,
    channel_id: Option<String>,
    visibility: String,
    made_for_kids: bool,
    delete_source_after_upload: bool,
    playlist_id: Option<String>,
    playlist_title: Option<String>,
    status: String,
    detail: String,
    last_scan_at: Option<String>,
    last_file_name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderMonitorFileActivity {
    file_name: String,
    observation_state: String,
    size_bytes: u64,
    updated_at: String,
    upload_title: Option<String>,
    upload_status: Option<String>,
    confirmed_bytes: Option<u64>,
    total_bytes: Option<u64>,
    detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderMonitorLogEntry {
    kind: String,
    detail: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FolderMonitorOverview {
    settings: FolderMonitorSettings,
    files: Vec<FolderMonitorFileActivity>,
    logs: Vec<FolderMonitorLogEntry>,
}

fn user_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn spawn_worker<F, T>(worker: F) -> thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    performance::record_worker_thread();
    thread::spawn(worker)
}

#[cfg(feature = "performance-harness")]
fn record_database_statement(event: rusqlite::trace::TraceEvent<'_>) {
    #[cfg(test)]
    persistence::record_test_database_statement(&event);
    performance::record_database_statement(event);
}

fn database(state: &AppState) -> Result<Connection, String> {
    let needs_initialization = state
        .database_path
        .metadata()
        .map(|metadata| metadata.len() == 0)
        .unwrap_or(true);
    let mut connection = Connection::open(&state.database_path).map_err(user_error)?;
    performance::record_database_open();
    #[cfg(feature = "performance-harness")]
    connection.trace_v2(
        TraceEventCodes::SQLITE_TRACE_STMT,
        Some(record_database_statement),
    );
    persistence::configure_connection(&connection, DATABASE_BUSY_TIMEOUT)
        .map_err(|error| format!("The local database connection could not be prepared: {error}"))?;
    #[cfg(not(test))]
    state.state_events.attach_commit_hook(&connection);
    if needs_initialization
        && persistence::migrate_database(&mut connection).map_err(|error| {
            format!("The local database schema could not be upgraded safely: {error}")
        })?
    {
        performance::record_database_schema_batch();
    }
    Ok(connection)
}

fn initialize_database(state: &AppState) -> Result<Connection, String> {
    let mut connection = Connection::open(&state.database_path).map_err(user_error)?;
    performance::record_database_open();
    #[cfg(feature = "performance-harness")]
    connection.trace_v2(
        TraceEventCodes::SQLITE_TRACE_STMT,
        Some(record_database_statement),
    );
    persistence::configure_connection(&connection, DATABASE_BUSY_TIMEOUT)
        .map_err(|error| format!("The local database connection could not be prepared: {error}"))?;
    #[cfg(not(test))]
    state.state_events.attach_commit_hook(&connection);
    if persistence::migrate_database(&mut connection).map_err(|error| {
        format!("The local database schema could not be upgraded safely: {error}")
    })? {
        performance::record_database_schema_batch();
    }
    Ok(connection)
}

fn export_portable_archive_impl(
    state: &AppState,
    destination: &Path,
) -> Result<PortableArchiveReceipt, String> {
    let connection = database(state)?;
    let uploads = connection
        .prepare("SELECT id, title, file_name, size_bytes, digest, video_id, created_at, updated_at FROM upload_items WHERE digest IS NOT NULL ORDER BY id ASC")
        .map_err(user_error)?
        .query_map([], |row| Ok(PortableUpload {
            id: row.get(0)?, title: row.get(1)?, file_name: row.get(2)?, size_bytes: row.get::<_, i64>(3)? as u64,
            digest: row.get(4)?, video_id: row.get(5)?, created_at: row.get(6)?, updated_at: row.get(7)?,
        }))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let remote_videos = connection
        .prepare("SELECT video_id, channel_name, channel_id, title, duration, privacy_status, upload_status, updated_at FROM remote_videos ORDER BY video_id ASC")
        .map_err(user_error)?
        .query_map([], |row| Ok(PortableRemoteVideo {
            video_id: row.get(0)?, channel_name: row.get(1)?, channel_id: row.get(2)?, title: row.get(3)?, duration: row.get(4)?, privacy_status: row.get(5)?, upload_status: row.get(6)?, updated_at: row.get(7)?,
        }))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let archive = PortableArchive {
        version: PORTABLE_ARCHIVE_VERSION,
        created_at: now(),
        uploads,
        remote_videos,
    };
    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Choose a valid archive destination.".to_string())?;
    let temporary = destination.with_file_name(format!(".{file_name}.{}.partial", Uuid::new_v4()));
    let file = File::create(&temporary).map_err(user_error)?;
    let mut encoder = GzEncoder::new(file, Compression::best());
    if let Err(error) = serde_json::to_writer(&mut encoder, &archive).map_err(user_error) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let completed = encoder.finish().map_err(user_error)?;
    completed.sync_all().map_err(user_error)?;
    if destination.exists() {
        let _ = fs::remove_file(&temporary);
        return Err(
            "Choose a new archive filename; an existing archive was left unchanged.".into(),
        );
    }
    fs::rename(&temporary, destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        user_error(error)
    })?;
    let bytes = fs::metadata(destination).map_err(user_error)?.len();
    Ok(PortableArchiveReceipt {
        upload_count: archive.uploads.len(), remote_video_count: archive.remote_videos.len(), bytes,
        detail: "Compact metadata archive created. It excludes media, source paths, refresh tokens, OAuth client secrets, and resumable upload sessions.".into(),
    })
}

fn import_portable_archive_impl(
    state: &AppState,
    source: &Path,
) -> Result<PortableArchiveReceipt, String> {
    if fs::metadata(source).map_err(user_error)?.len() > PORTABLE_ARCHIVE_MAX_BYTES {
        return Err("This portable archive is too large to import safely.".into());
    }
    let mut decoder = GzDecoder::new(File::open(source).map_err(user_error)?);
    let mut bytes = Vec::new();
    Read::by_ref(&mut decoder)
        .take(PORTABLE_ARCHIVE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(user_error)?;
    if bytes.len() as u64 > PORTABLE_ARCHIVE_MAX_BYTES {
        return Err("This portable archive expands beyond the safe import limit.".into());
    }
    let archive: PortableArchive = serde_json::from_slice(&bytes)
        .map_err(|_| "This is not a valid YouTube Upload Manager portable archive.".to_string())?;
    if archive.version != PORTABLE_ARCHIVE_VERSION {
        return Err("This portable archive uses an unsupported format version.".into());
    }
    let upload_count = archive.uploads.len();
    let remote_video_count = archive.remote_videos.len();
    let mut connection = database(state)?;
    let transaction = connection.transaction().map_err(user_error)?;
    for upload in archive.uploads {
        if upload.id.trim().is_empty()
            || upload.title.trim().is_empty()
            || upload.file_name.trim().is_empty()
            || upload.digest.as_deref().is_some_and(str::is_empty)
        {
            return Err("The portable archive contains invalid upload metadata.".into());
        }
        let id = format!("portable-{}", upload.id);
        let title_keys = title_matching::keys(&upload.title);
        transaction.execute(
            "INSERT INTO upload_items (id, title, normalized_title, canonical_title, has_copy_marker, numeric_title_key, title_keys_version, file_name, workspace_path, size_bytes, digest, status, total_bytes, video_id, detail, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'metadata_only', ?10, ?12, 'Imported portable dedupe metadata. Re-import the original file before upload.', ?13, ?14) ON CONFLICT(id) DO UPDATE SET title = excluded.title, normalized_title = excluded.normalized_title, canonical_title = excluded.canonical_title, has_copy_marker = excluded.has_copy_marker, numeric_title_key = excluded.numeric_title_key, title_keys_version = excluded.title_keys_version, file_name = excluded.file_name, size_bytes = excluded.size_bytes, digest = excluded.digest, video_id = excluded.video_id, updated_at = excluded.updated_at",
            params![id, upload.title, title_keys.normalized, title_keys.canonical, title_keys.has_copy_marker as i64, title_keys.numeric, title_matching::TITLE_KEYS_VERSION, upload.file_name, "portable-metadata", upload.size_bytes as i64, upload.digest, upload.video_id, upload.created_at, upload.updated_at],
        ).map_err(user_error)?;
    }
    let mut imported_channels = HashSet::new();
    for video in archive.remote_videos {
        if video.video_id.trim().is_empty()
            || video.channel_name.trim().is_empty()
            || video.channel_id.trim().is_empty()
            || video.title.trim().is_empty()
        {
            return Err("The portable archive contains invalid YouTube inventory metadata.".into());
        }
        imported_channels.insert(video.channel_id.clone());
        let title_keys = title_matching::keys(&video.title);
        transaction.execute(
            "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, normalized_title, canonical_title, has_copy_marker, numeric_title_key, title_keys_version, duration, privacy_status, upload_status, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) ON CONFLICT(video_id) DO UPDATE SET channel_name = excluded.channel_name, channel_id = excluded.channel_id, title = excluded.title, normalized_title = excluded.normalized_title, canonical_title = excluded.canonical_title, has_copy_marker = excluded.has_copy_marker, numeric_title_key = excluded.numeric_title_key, title_keys_version = excluded.title_keys_version, duration = excluded.duration, privacy_status = excluded.privacy_status, upload_status = excluded.upload_status, updated_at = excluded.updated_at",
            params![video.video_id, video.channel_name, video.channel_id, video.title, title_keys.normalized, title_keys.canonical, title_keys.has_copy_marker as i64, title_keys.numeric, title_matching::TITLE_KEYS_VERSION, video.duration, video.privacy_status, video.upload_status, video.updated_at],
        ).map_err(user_error)?;
    }
    for channel_id in imported_channels {
        transaction
            .execute(
                "INSERT INTO channel_generations(channel_id, inventory_generation, upload_generation, updated_at) VALUES (?1, 1, 0, ?2) ON CONFLICT(channel_id) DO UPDATE SET inventory_generation = inventory_generation + 1, updated_at = excluded.updated_at",
                params![channel_id, now()],
            )
            .map_err(user_error)?;
    }
    transaction.commit().map_err(user_error)?;
    audit_global(&connection, "portable_metadata_imported", "Operator imported compact cross-device duplicate metadata; OAuth credentials and media were excluded")?;
    Ok(PortableArchiveReceipt { upload_count, remote_video_count, bytes: fs::metadata(source).map_err(user_error)?.len(), detail: "Imported compact hash and YouTube inventory metadata. Import your Desktop OAuth JSON and connect YouTube separately on this device.".into() })
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn capability_is_current(checked_at: &str) -> bool {
    DateTime::parse_from_rfc3339(checked_at)
        .ok()
        .is_some_and(|timestamp| {
            timestamp.with_timezone(&Utc) + CAPABILITY_CHECK_INTERVAL > Utc::now()
        })
}

fn sampled_read_bytes_per_second(path: &Path) -> Result<f64, String> {
    let mut file = File::open(path).map_err(user_error)?;
    let mut remaining = CAPABILITY_READ_SAMPLE_BYTES;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let started = Instant::now();
    let mut bytes = 0_usize;
    while remaining > 0 {
        let chunk = remaining.min(buffer.len());
        let read = file.read(&mut buffer[..chunk]).map_err(user_error)?;
        if read == 0 {
            break;
        }
        bytes += read;
        remaining -= read;
    }
    let seconds = started.elapsed().as_secs_f64();
    if bytes == 0 || seconds <= 0.0 {
        return Err("The source disk capability check could not read this upload file.".into());
    }
    Ok(bytes as f64 / seconds)
}

fn source_volume_concurrency_limit(
    connection: &Connection,
    path: &Path,
) -> Result<(String, usize), String> {
    let volume_id = source_volume_id(path);
    let cached: Option<(String, f64)> = connection
        .query_row(
            "SELECT checked_at, read_bytes_per_second FROM upload_disk_capabilities WHERE volume_id = ?1",
            [&volume_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(user_error)?;
    let disk_rate = match cached {
        Some((checked_at, rate)) if capability_is_current(&checked_at) => rate,
        _ => {
            let rate = sampled_read_bytes_per_second(path)?;
            connection.execute(
                "INSERT INTO upload_disk_capabilities (volume_id, checked_at, read_bytes_per_second) VALUES (?1, ?2, ?3) ON CONFLICT(volume_id) DO UPDATE SET checked_at = excluded.checked_at, read_bytes_per_second = excluded.read_bytes_per_second",
                params![&volume_id, now(), rate],
            ).map_err(user_error)?;
            rate
        }
    };
    let network_rate: Option<f64> = connection
        .query_row(
            "SELECT upload_bytes_per_second FROM upload_connection_capability WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?;
    let limit = network_rate
        .filter(|rate| *rate > 0.0)
        .map(|rate| {
            // A measured YouTube stream represents the connection throughput
            // that one worker can sustain. A volume that can read several
            // such streams concurrently may receive more than one worker.
            // Keep an explicit ceiling so a stale optimistic sample cannot
            // turn a large folder intake into an unbounded fan-out.
            ((disk_rate / rate).floor() as usize).clamp(1, MAX_CONCURRENT_UPLOADS_PER_VOLUME)
        })
        .unwrap_or(1);
    Ok((volume_id, limit))
}

fn record_connection_capability(
    connection: &Connection,
    bytes_per_second: Option<f64>,
) -> Result<(), String> {
    let Some(rate) = bytes_per_second.filter(|rate| *rate > 0.0) else {
        return Ok(());
    };
    let existing: Option<String> = connection
        .query_row(
            "SELECT checked_at FROM upload_connection_capability WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?;
    if existing.as_deref().is_some_and(capability_is_current) {
        return Ok(());
    }
    connection.execute(
        "INSERT INTO upload_connection_capability (singleton, checked_at, upload_bytes_per_second) VALUES (1, ?1, ?2) ON CONFLICT(singleton) DO UPDATE SET checked_at = excluded.checked_at, upload_bytes_per_second = excluded.upload_bytes_per_second",
        params![now(), rate],
    ).map_err(user_error)?;
    Ok(())
}

/// YouTube requires a stable chunk size for a resumable session (apart from
/// its final chunk). Select it from the latest measured upstream capacity when
/// a session begins, keeping the safe 8 MiB floor for poor connections.
fn adaptive_upload_chunk_size(connection: &Connection) -> Result<usize, String> {
    let measured: Option<f64> = connection
        .query_row(
            "SELECT upload_bytes_per_second FROM upload_connection_capability WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?
        .flatten();
    Ok(match measured.unwrap_or(0.0) {
        rate if rate >= 25.0 * 1024.0 * 1024.0 => UPLOAD_CHUNK_MAX_BYTES,
        rate if rate >= 8.0 * 1024.0 * 1024.0 => UPLOAD_CHUNK_DEFAULT_BYTES,
        rate if rate >= 4.0 * 1024.0 * 1024.0 => 32 * 1024 * 1024,
        rate if rate >= 1.5 * 1024.0 * 1024.0 => 16 * 1024 * 1024,
        _ => UPLOAD_CHUNK_MIN_BYTES,
    })
}

fn panic_marker_path() -> &'static OnceLock<PathBuf> {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    &PATH
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CrashRecoveryStatus {
    crash_detected: bool,
    detected_at: Option<String>,
    failure_kind: Option<String>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AppReleaseIdentity {
    version: String,
    channel: String,
    build_profile: String,
}

fn release_identity() -> AppReleaseIdentity {
    AppReleaseIdentity {
        version: env!("CARGO_PKG_VERSION").to_string(),
        channel: APP_RELEASE_CHANNEL.to_string(),
        build_profile: if cfg!(debug_assertions) {
            "debug".to_string()
        } else {
            "release".to_string()
        },
    }
}

fn marker_timestamp(path: &Path, key: &str) -> Option<String> {
    // Marker files are intentionally tiny and contain no error or provider
    // details. Parse strictly so a corrupted local file cannot surface data in
    // the webview or a GitHub report.
    let mut marker = String::new();
    File::open(path)
        .ok()?
        .take(256)
        .read_to_string(&mut marker)
        .ok()?;
    let timestamp = marker.trim().strip_prefix(key)?;
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc).to_rfc3339())
}

fn crash_recovery_status_for_paths(
    panic_path: &Path,
    webview_error_path: &Path,
) -> CrashRecoveryStatus {
    let markers = [
        (marker_timestamp(panic_path, "panic_at="), "Native panic"),
        (
            marker_timestamp(webview_error_path, "webview_error_at="),
            "Webview error",
        ),
    ];
    let latest_marker = markers
        .into_iter()
        .filter_map(|(detected_at, failure_kind)| {
            detected_at.map(|detected_at| (detected_at, failure_kind))
        })
        .max_by(|left, right| left.0.cmp(&right.0));
    CrashRecoveryStatus {
        crash_detected: latest_marker.is_some(),
        detected_at: latest_marker
            .as_ref()
            .map(|(detected_at, _)| detected_at.clone()),
        failure_kind: latest_marker.map(|(_, failure_kind)| failure_kind.to_string()),
    }
}

fn crash_recovery_status(state: &AppState) -> CrashRecoveryStatus {
    crash_recovery_status_for_paths(
        &state.database_path.with_file_name(PANIC_MARKER_FILE),
        &state
            .database_path
            .with_file_name(WEBVIEW_ERROR_MARKER_FILE),
    )
}

fn record_webview_error_impl(state: &AppState) -> Result<(), String> {
    // Do not accept an error message from the webview: it might contain
    // account data, local paths, OAuth material, or provider responses.
    fs::write(
        state
            .database_path
            .with_file_name(WEBVIEW_ERROR_MARKER_FILE),
        format!("webview_error_at={}\n", now()),
    )
    .map_err(|_| "Unable to persist the local crash recovery marker.".to_string())
}

fn acknowledge_crash_recovery_impl(state: &AppState) -> Result<(), String> {
    for path in [
        state.database_path.with_file_name(PANIC_MARKER_FILE),
        state
            .database_path
            .with_file_name(WEBVIEW_ERROR_MARKER_FILE),
    ] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Unable to clear the local crash recovery marker.".into()),
        }
    }
    Ok(())
}

/// Persist only that a panic occurred and when it happened. Panic payloads can
/// contain credentials, paths, or provider responses, so they are never saved.
fn install_panic_marker(path: PathBuf) {
    if panic_marker_path().set(path).is_err() {
        return;
    }
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(marker_path) = panic_marker_path().get() {
            let _ = fs::write(marker_path, format!("panic_at={}\n", now()));
        }
        previous_hook(info);
    }));
}

fn diagnostic_detail(value: &str) -> String {
    let normalized = value.to_ascii_lowercase();
    let contains_sensitive_value = [
        "token",
        "secret",
        "password",
        "authorization",
        "bearer ",
        "cookie",
        "verifier",
        "client_id",
        "refresh_",
        "access_",
        "http://",
        "https://",
        "\\\\",
        ":\\",
        "/users/",
        "/home/",
        "/var/",
        "/private/",
    ]
    .iter()
    .any(|needle| normalized.contains(needle));
    let contains_channel_id = value.split_whitespace().any(|word| {
        let word = word.trim_matches(|character: char| {
            !character.is_ascii_alphanumeric() && character != '-' && character != '_'
        });
        word.starts_with("UC") && word.len() >= 20
    });
    if contains_sensitive_value || contains_channel_id {
        "[redacted sensitive detail]".into()
    } else {
        value.chars().take(500).collect()
    }
}

fn safe_diagnostic_issue_name(kind: &str) -> String {
    let is_safe = !kind.is_empty()
        && kind.len() <= 80
        && kind
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    is_safe
        .then(|| kind.to_string())
        .unwrap_or_else(|| "unclassified_local_event".into())
}

fn is_diagnostic_issue_name(kind: &str) -> bool {
    [
        "error",
        "failed",
        "warning",
        "crash",
        "interrupted",
        "cancelled",
    ]
    .iter()
    .any(|needle| kind.contains(needle))
}

fn diagnostic_report_impl(state: &AppState) -> Result<String, String> {
    let connection =
        database(state).map_err(|_| "Unable to read the local diagnostic data.".to_string())?;
    let release_identity = release_identity();
    let mut report = format!(
        "# YouTube Upload Manager diagnostic report\n\n## App and system\n\n- App version: {}\n- Release channel: {}\n- Build profile: {}\n- Operating system: {} ({})\n- CPU architecture: {}\n- Generated at: {}\n- Local-only diagnostic report: yes\n\n## Connection and queue\n\n",
        release_identity.version,
        release_identity.channel,
        release_identity.build_profile,
        std::env::consts::OS,
        std::env::consts::FAMILY,
        std::env::consts::ARCH,
        now(),
    );
    let (oauth_client_imported, youtube_connected): (bool, bool) = connection
        .query_row(
            "SELECT oauth_client_id IS NOT NULL, active_channel IS NOT NULL FROM connection_settings WHERE singleton = 1",
            [],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, i64>(1)? != 0)),
        )
        .optional()
        .map_err(|_| "Unable to read the local diagnostic data.".to_string())?
        .unwrap_or((false, false));
    report.push_str(&format!(
        "- Desktop OAuth client imported: {}\n- YouTube connection active: {}\n",
        if oauth_client_imported { "yes" } else { "no" },
        if youtube_connected { "yes" } else { "no" },
    ));
    let mut status_statement = connection
        .prepare("SELECT status, COUNT(*) FROM upload_items GROUP BY status ORDER BY status")
        .map_err(|_| "Unable to read the local diagnostic data.".to_string())?;
    let statuses = status_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|_| "Unable to read the local diagnostic data.".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Unable to read the local diagnostic data.".to_string())?;
    if statuses.is_empty() {
        report.push_str("- Queue: empty\n");
    } else {
        report.push_str("- Queue counts:\n");
        for (status, count) in statuses {
            report.push_str(&format!("  - `{status}`: {count}\n"));
        }
    }

    report.push_str("\n## Crash marker\n\n");
    let crash_recovery = crash_recovery_status(state);
    if crash_recovery.crash_detected {
        report.push_str(&format!(
            "- `{}` was detected at {}.\n",
            crash_recovery
                .failure_kind
                .as_deref()
                .unwrap_or("Unclassified crash"),
            crash_recovery
                .detected_at
                .as_deref()
                .unwrap_or("an unknown time")
        ));
    } else {
        report.push_str("- No unacknowledged app crash or webview error recorded.\n");
    }

    report.push_str("\n## Recent warnings, errors, and audit events\n\n");
    let mut events_statement = connection
        .prepare(
            "SELECT kind, detail, created_at FROM audit_events ORDER BY created_at DESC LIMIT ?1",
        )
        .map_err(|_| "Unable to read the local diagnostic data.".to_string())?;
    let events = events_statement
        .query_map([DIAGNOSTIC_REPORT_EVENT_LIMIT], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|_| "Unable to read the local diagnostic data.".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Unable to read the local diagnostic data.".to_string())?;
    let mut issue_names = events
        .iter()
        .filter_map(|(kind, _, _)| {
            is_diagnostic_issue_name(kind).then(|| safe_diagnostic_issue_name(kind))
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(failure_kind) = crash_recovery.failure_kind.as_deref() {
        issue_names.push(failure_kind.to_string());
    }
    issue_names.sort();
    issue_names.dedup();
    report.push_str("\n## Detected crash, error, and warning names\n\n");
    if issue_names.is_empty() {
        report.push_str("- No persisted crash, error, or warning names.\n");
    } else {
        for issue_name in issue_names {
            report.push_str(&format!("- `{issue_name}`\n"));
        }
    }
    if events.is_empty() {
        report.push_str("- No persisted audit events.\n");
    } else {
        for (kind, detail, created_at) in events {
            let detail = detail
                .map(|value| diagnostic_detail(&value))
                .unwrap_or_else(|| "No detail recorded.".into());
            report.push_str(&format!("- {created_at} — `{kind}` — {detail}\n"));
        }
    }
    report.push_str("\n## Reproduction steps\n\n1. Describe what you were doing when the problem occurred.\n2. Include the expected result and the actual result.\n3. Attach screenshots only after checking that they do not expose account or video information.\n");
    Ok(report)
}

fn deletion_sudo_until() -> String {
    (Utc::now() + ChronoDuration::minutes(DELETION_SUDO_MODE_MINUTES)).to_rfc3339()
}

fn deletion_sudo_is_active(value: Option<&str>) -> bool {
    value
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .is_some_and(|timestamp| timestamp.with_timezone(&Utc) > Utc::now())
}

fn clear_expired_deletion_authorization(connection: &Connection) -> Result<bool, String> {
    let (authorized, expires_at): (bool, Option<String>) = connection
        .query_row(
            "SELECT deletion_authorized, deletion_sudo_until FROM connection_settings WHERE singleton = 1",
            [],
            |row| Ok((row.get::<_, i64>(0)? != 0, row.get(1)?)),
        )
        .optional()
        .map_err(user_error)?
        .unwrap_or((false, None));
    if !authorized || expires_at.is_none() || deletion_sudo_is_active(expires_at.as_deref()) {
        return Ok(false);
    }
    let _ = clear_refresh_token(deletion_refresh_token_entry());
    connection
        .execute(
            "UPDATE connection_settings SET deletion_authorized = 0, deletion_sudo_until = NULL, connection_detail = 'Deletion permission expired and was removed from this device.', updated_at = ?1 WHERE singleton = 1",
            [now()],
        )
        .map_err(user_error)?;
    Ok(true)
}

fn active_upload_quota_pause(connection: &Connection) -> Result<Option<String>, String> {
    let channel_id = connection_settings(connection)?
        .active_channel_id
        .unwrap_or_default();
    if channel_id.is_empty() {
        return Ok(None);
    }
    let pause_until = connection
        .query_row(
            "SELECT pause_until FROM upload_quota_pauses WHERE channel_id = ?1",
            [&channel_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(user_error)?;
    let Some(pause_until) = pause_until else {
        return Ok(None);
    };
    let parsed = DateTime::parse_from_rfc3339(&pause_until)
        .map_err(|_| "The saved YouTube upload-limit pause is invalid.".to_string())?
        .with_timezone(&Utc);
    if parsed > Utc::now() {
        return Ok(Some(pause_until));
    }
    connection
        .execute(
            "DELETE FROM upload_quota_pauses WHERE channel_id = ?1",
            [&channel_id],
        )
        .map_err(user_error)?;
    audit_global(
        connection,
        "youtube_daily_upload_limit_elapsed",
        "The saved 24-hour YouTube upload-limit pause elapsed; queued uploads may resume.",
    )?;
    Ok(None)
}

fn quota_pause_detail(pause_until: &str) -> String {
    format!(
        "YouTube's daily upload limit was reached. This item stays safely queued and will resume after {pause_until}."
    )
}

fn record_upload_quota_pause(state: &AppState, item_id: &str) -> Result<String, String> {
    let pause_until = (Utc::now() + ChronoDuration::hours(24)).to_rfc3339();
    let detail = quota_pause_detail(&pause_until);
    let connection = database(state)?;
    let channel_id = connection
        .query_row(
            "SELECT COALESCE(channel_id, '') FROM upload_items WHERE id = ?1",
            [item_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(user_error)?;
    if channel_id.is_empty() {
        return Err("The queued upload has no immutable YouTube channel binding.".into());
    }
    connection
        .execute(
            "INSERT INTO upload_quota_pauses (channel_id, pause_until, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(channel_id) DO UPDATE SET pause_until = excluded.pause_until, updated_at = excluded.updated_at",
            params![channel_id, pause_until, now()],
        )
        .map_err(user_error)?;
    connection
        .execute(
            "UPDATE upload_items SET status = 'queued', detail = ?1, updated_at = ?2 WHERE id = ?3",
            params![detail, now(), item_id],
        )
        .map_err(user_error)?;
    audit(
        &connection,
        item_id,
        "youtube_daily_upload_limit_paused",
        "YouTube reported a daily upload limit; this device paused queued uploads for 24 hours.",
    )?;
    drop(connection);
    // Unit tests exercise the deadline loop with an explicitly joined worker;
    // production starts the detached worker only after the durable pause exists.
    #[cfg(not(test))]
    ensure_quota_resume_worker(state.clone())?;
    Ok(pause_until)
}

fn defer_item_for_active_quota_pause(
    state: &AppState,
    item_id: &str,
    pause_until: &str,
) -> Result<(), String> {
    let confirmed_bytes = database(state)?
        .query_row(
            "SELECT confirmed_bytes FROM upload_items WHERE id = ?1",
            [item_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)? as u64;
    mark_upload_state(
        state,
        item_id,
        "queued",
        confirmed_bytes,
        &quota_pause_detail(pause_until),
        None,
    )
}

fn connection_settings(connection: &Connection) -> Result<ConnectionSettings, String> {
    connection
        .query_row(
            "SELECT oauth_client_id, active_channel, active_channel_id, connection_detail, deletion_authorized, deletion_sudo_until FROM connection_settings WHERE singleton = 1",
            [],
            |row| {
                let configured_client_id: Option<String> = row.get(0)?;
                let active_channel_id: Option<String> = row.get(2)?;
                let deletion_sudo_expires_at: Option<String> = row.get(5)?;
                let deletion_sudo_active = deletion_sudo_is_active(deletion_sudo_expires_at.as_deref());
                Ok(ConnectionSettings {
                    oauth_configured: configured_client_id.is_some(),
                    active_channel: row.get(1)?,
                    active_channel_id: active_channel_id.clone(),
                    connected: active_channel_id.is_some(),
                    secure_store_available: secure_store_available(),
                    detail: row.get(3)?,
                    deletion_authorized: row.get::<_, i64>(4)? != 0,
                    deletion_sudo_active,
                    deletion_sudo_expires_at: if deletion_sudo_active { deletion_sudo_expires_at } else { None },
                })
            },
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(ConnectionSettings {
                oauth_configured: false,
                active_channel: None,
                active_channel_id: None,
                connected: false,
                secure_store_available: secure_store_available(),
                detail: None,
                deletion_authorized: false,
                deletion_sudo_active: false,
                deletion_sudo_expires_at: None,
            }),
            other => Err(other),
        })
        .map_err(user_error)
}

fn secure_store_available() -> bool {
    CredentialEntry::store_status().is_ok()
}

#[cfg(not(feature = "performance-harness"))]
const SECURE_STORE_SERVICE: &str = "com.sekailens.youtube-upload-manager";
#[cfg(feature = "performance-harness")]
const SECURE_STORE_SERVICE: &str = "com.sekailens.youtube-upload-manager.performance-harness";
const UPLOAD_REFRESH_TOKEN_KEY: &str = "youtube-refresh-token";
const DELETION_REFRESH_TOKEN_KEY: &str = "youtube-deletion-refresh-token";

fn refresh_token_entry() -> Result<CredentialEntry, String> {
    CredentialEntry::new(SECURE_STORE_SERVICE, UPLOAD_REFRESH_TOKEN_KEY).map_err(user_error)
}

fn deletion_refresh_token_entry() -> Result<CredentialEntry, String> {
    CredentialEntry::new(SECURE_STORE_SERVICE, DELETION_REFRESH_TOKEN_KEY).map_err(user_error)
}

fn oauth_client_secret_entry() -> Result<CredentialEntry, String> {
    CredentialEntry::new(SECURE_STORE_SERVICE, "youtube-oauth-client-secret").map_err(user_error)
}

fn configured_oauth_client_secret() -> Result<Option<String>, String> {
    match oauth_client_secret_entry()?.get_password() {
        Ok(secret) if !secret.is_empty() => Ok(Some(secret)),
        Ok(_) | Err(_) => Ok(None),
    }
}

fn clear_oauth_client_secret() -> Result<(), String> {
    match oauth_client_secret_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(user_error(error)),
    }
}

fn desktop_oauth_client_from_file(contents: &str) -> Result<(String, String), String> {
    let parsed: DesktopOAuthClientFile = serde_json::from_str(contents)
        .map_err(|_| "Choose the downloaded JSON for a Google Desktop OAuth client.".to_string())?;
    let client_id = parsed.installed.client_id.trim().to_string();
    if !valid_google_client_id(&client_id) {
        return Err("The JSON does not contain a valid Google Desktop OAuth client ID.".into());
    }
    Ok((client_id, parsed.installed.client_secret.trim().to_string()))
}

fn configured_oauth_client_id(connection: &Connection) -> Result<String, String> {
    connection
        .query_row(
            "SELECT oauth_client_id FROM connection_settings WHERE singleton = 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(user_error)?
        .flatten()
        .ok_or_else(|| "Import a Google Desktop OAuth JSON before connecting YouTube.".to_string())
}

// OAuth callbacks use this boundary so credentials stay out of the database and webview.
fn persist_refresh_token(
    refresh_token: &str,
    entry: Result<CredentialEntry, String>,
) -> Result<(), String> {
    if refresh_token.is_empty() {
        return Err("Refusing to store an empty refresh token.".into());
    }
    entry?.set_password(refresh_token).map_err(user_error)
}

fn clear_refresh_token(entry: Result<CredentialEntry, String>) -> Result<(), String> {
    match entry?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(user_error(error)),
    }
}

fn upload_session_entry(item_id: &str) -> Result<CredentialEntry, String> {
    CredentialEntry::new(
        SECURE_STORE_SERVICE,
        &format!("youtube-resumable-session-{item_id}"),
    )
    .map_err(user_error)
}

fn save_upload_session(item_id: &str, session_uri: &str) -> Result<(), String> {
    let url = url::Url::parse(session_uri).map_err(user_error)?;
    if url.scheme() != "https" {
        return Err("Refusing to persist a non-HTTPS YouTube upload session.".into());
    }
    upload_session_entry(item_id)?
        .set_password(session_uri)
        .map_err(user_error)
}

fn confirmed_offset_from_range(range: Option<&str>) -> Result<u64, String> {
    let Some(range) = range else { return Ok(0) };
    range
        .strip_prefix("bytes=0-")
        .ok_or_else(|| "YouTube returned an invalid resumable range.".to_string())?
        .parse::<u64>()
        .map(|value| value.saturating_add(1))
        .map_err(|_| "YouTube returned an invalid resumable range.".into())
}

fn oauth_verifier_entry(state: &str) -> Result<CredentialEntry, String> {
    CredentialEntry::new(SECURE_STORE_SERVICE, &format!("youtube-oauth-pkce-{state}"))
        .map_err(user_error)
}

fn set_connection_detail(
    state: &AppState,
    detail: &str,
    active_channel: Option<(&str, &str)>,
) -> Result<(), String> {
    let connection = database(state)?;
    connection
        .execute(
            "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, connection_detail, updated_at) VALUES (1, ?1, ?2, ?3, ?4) ON CONFLICT(singleton) DO UPDATE SET active_channel = excluded.active_channel, active_channel_id = excluded.active_channel_id, connection_detail = excluded.connection_detail, updated_at = excluded.updated_at",
            params![active_channel.map(|value| value.0), active_channel.map(|value| value.1), detail, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn set_connection_failure_detail(state: &AppState, detail: &str) -> Result<(), String> {
    let connection = database(state)?;
    connection
        .execute(
            "INSERT INTO connection_settings (singleton, connection_detail, updated_at) VALUES (1, ?1, ?2) ON CONFLICT(singleton) DO UPDATE SET connection_detail = excluded.connection_detail, updated_at = excluded.updated_at",
            params![detail, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn oauth_error_message(error: &str) -> String {
    if error == "access_denied" {
        "Google authorization was cancelled.".into()
    } else {
        "Google authorization did not complete. Try connecting again.".into()
    }
}

fn oauth_token_error_message(response: &serde_json::Value, refreshing: bool) -> String {
    let error = response
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    match error {
        "invalid_grant" if refreshing => {
            "Google rejected the saved authorization (invalid_grant). Connect YouTube again."
                .into()
        }
        "invalid_grant" => "Google rejected the authorization code (invalid_grant). It may have expired or the PKCE callback did not match; connect again.".into(),
        "invalid_client" | "deleted_client" => "Google rejected this OAuth desktop client (invalid_client). Verify that the client ID still exists in the selected Google Cloud project.".into(),
        "unauthorized_client" => "Google does not allow this OAuth client to use the installed-app authorization flow (unauthorized_client). Verify that it is a Desktop app client.".into(),
        "invalid_request" => "Google rejected the token request (invalid_request). Verify the OAuth desktop-client configuration and try again.".into(),
        "unsupported_grant_type" => "Google rejected the token request type (unsupported_grant_type). The OAuth client configuration is incompatible with this app.".into(),
        "access_denied" => "Google denied the token exchange (access_denied). Confirm the Google account is an allowed test user and authorize the requested access.".into(),
        _ if refreshing => {
            "Google rejected the saved YouTube authorization. Connect YouTube again.".into()
        }
        _ => "Google rejected the authorization response with an unrecognized OAuth error. Verify the Google Auth Platform configuration and try again.".into(),
    }
}

fn callback_value(target: &str, key: &str) -> Option<String> {
    let callback = url::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    callback
        .query_pairs()
        .find_map(|(name, value)| (name == key).then(|| value.into_owned()))
}

fn valid_oauth_callback_request(method: Option<&str>, target: &str, expected_state: &str) -> bool {
    method == Some("GET")
        && url::Url::parse(&format!("http://127.0.0.1{target}"))
            .ok()
            .is_some_and(|url| url.path() == "/oauth2/callback")
        && callback_value(target, "state").as_deref() == Some(expected_state)
}

fn respond_to_callback(stream: &mut TcpStream, text: &str) {
    let body = format!("<!doctype html><title>YouTube Upload Manager</title><p>{text}</p><p>You may close this window and return to the app.</p>");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn complete_oauth_connection(
    state: &AppState,
    client_id: &str,
    redirect_uri: &str,
    verifier: &str,
    code: &str,
    attempt_kind: OAuthAttemptKind,
) -> Result<String, String> {
    let client_secret = configured_oauth_client_secret()?;
    let mut token_form = vec![
        ("code", code),
        ("client_id", client_id),
        ("code_verifier", verifier),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    if let Some(secret) = client_secret.as_deref() {
        token_form.push(("client_secret", secret));
    }
    let token_response = provider_transport(state)
        .control_client()?
        .post("https://oauth2.googleapis.com/token")
        .form(&token_form)
        .send()
        .map_err(|_| "Google token exchange could not be reached.".to_string())?;
    let token_status = token_response.status();
    let token_response: serde_json::Value = token_response.json().map_err(|_| {
        if token_status.is_success() {
            "Google returned an unreadable authorization response.".to_string()
        } else {
            "Google rejected the authorization response with an unreadable OAuth error.".to_string()
        }
    })?;
    if !token_status.is_success() {
        return Err(oauth_token_error_message(&token_response, false));
    }
    let access_token = token_response
        .get("access_token")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Google did not return an access token.".to_string())?;
    let access_token_lifetime = token_response
        .get("expires_in")
        .and_then(|value| value.as_u64())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(45 * 60));
    let refresh_token = token_response
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            "Google did not return a refresh token; revoke this app in Google and connect again."
                .to_string()
        })?;
    let channel_response = provider_transport(state)
        .control_client()?
        .get("https://www.googleapis.com/youtube/v3/channels")
        .query(&[("part", "id,snippet"), ("mine", "true")])
        .bearer_auth(access_token)
        .send()
        .map_err(|_| "YouTube channel verification could not be reached. Check your connection and try again.".to_string())?;
    if !channel_response.status().is_success() {
        return Err(youtube_inventory_http_error(
            channel_response.status().as_u16(),
        ));
    }
    let channel_response: serde_json::Value = channel_response
        .json()
        .map_err(|_| "YouTube returned an unreadable channel response.".to_string())?;
    let channel_item = channel_response
        .get("items")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .ok_or_else(|| "No YouTube channel was returned for this Google account.".to_string())?;
    let channel = channel_item
        .pointer("/snippet/title")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "No YouTube channel was returned for this Google account.".to_string())?
        .to_string();
    let channel_id = channel_item
        .get("id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "YouTube did not return an immutable channel identity.".to_string())?
        .to_string();
    let connection = database(state)?;
    match attempt_kind {
        OAuthAttemptKind::Connection => {
            provider_transport(state)
                .invalidate_access_token(provider_transport::AccessTokenKind::Upload);
            provider_transport(state)
                .invalidate_access_token(provider_transport::AccessTokenKind::Deletion);
            persist_refresh_token(refresh_token, refresh_token_entry())?;
            provider_transport(state).cache_access_token(
                provider_transport::AccessTokenKind::Upload,
                access_token,
                access_token_lifetime,
            );
            // A new ordinary connection may represent another account; revoke any
            // separately granted destructive credential rather than carrying it
            // across channel changes.
            let _ = clear_refresh_token(deletion_refresh_token_entry());
            set_connection_detail(
                state,
                "Connected to YouTube on this device.",
                Some((&channel, &channel_id)),
            )?;
            connection
                .execute(
                    "UPDATE connection_settings SET deletion_authorized = 0, deletion_sudo_until = NULL, updated_at = ?1 WHERE singleton = 1",
                    [now()],
                )
                .map_err(user_error)?;
        }
        OAuthAttemptKind::Deletion => {
            let settings = connection_settings(&connection)?;
            if settings.active_channel_id.as_deref() != Some(channel_id.as_str()) {
                return Err(
                    "Deletion authorization must use the currently connected YouTube channel."
                        .into(),
                );
            }
            provider_transport(state)
                .invalidate_access_token(provider_transport::AccessTokenKind::Deletion);
            persist_refresh_token(refresh_token, deletion_refresh_token_entry())?;
            provider_transport(state).cache_access_token(
                provider_transport::AccessTokenKind::Deletion,
                access_token,
                access_token_lifetime,
            );
            connection
                .execute(
                    "UPDATE connection_settings SET deletion_authorized = 1, deletion_sudo_until = ?1, connection_detail = 'Deletion permission granted for the active YouTube channel.', updated_at = ?2 WHERE singleton = 1",
                    params![deletion_sudo_until(), now()],
                )
                .map_err(user_error)?;
        }
    }
    audit_global(
        &connection,
        "youtube_connected",
        "YouTube channel connection verified",
    )?;
    let _ = ensure_upload_postprocess_worker(state.clone());
    Ok(channel)
}

fn refreshed_access_token_for(
    state: &AppState,
    kind: provider_transport::AccessTokenKind,
    entry: Result<CredentialEntry, String>,
    missing_credential_message: &str,
) -> Result<String, String> {
    provider_transport(state).access_token(kind, || {
        let connection = database(state)?;
        let client_id = configured_oauth_client_id(&connection)?;
        let refresh_token = entry?
            .get_password()
            .map_err(|_| missing_credential_message.to_string())?;
        let client_secret = configured_oauth_client_secret()?;
        let mut token_form = vec![
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];
        if let Some(secret) = client_secret.as_deref() {
            token_form.push(("client_secret", secret));
        }
        let token_response = provider_transport(state)
            .control_client()?
            .post("https://oauth2.googleapis.com/token")
            .form(&token_form)
            .send()
            .map_err(|_| "Google token refresh could not be reached.".to_string())?;
        let token_status = token_response.status();
        let token_response: serde_json::Value = token_response.json().map_err(|_| {
            if token_status.is_success() {
                "Google returned an unreadable token refresh response.".to_string()
            } else {
                "Google rejected the saved YouTube authorization with an unreadable OAuth error."
                    .to_string()
            }
        })?;
        if !token_status.is_success() {
            return Err(oauth_token_error_message(&token_response, true));
        }
        let token = token_response
            .get("access_token")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .ok_or_else(|| "Google did not return a refreshed access token.".to_string())?;
        let lifetime = token_response
            .get("expires_in")
            .and_then(|value| value.as_u64())
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(45 * 60));
        Ok((token, lifetime))
    })
}

fn refreshed_access_token(state: &AppState) -> Result<String, String> {
    refreshed_access_token_for(
        state,
        provider_transport::AccessTokenKind::Upload,
        refresh_token_entry(),
        "This device no longer has a YouTube credential; connect YouTube again.",
    )
}

fn refreshed_deletion_access_token(state: &AppState) -> Result<String, String> {
    refreshed_access_token_for(
        state,
        provider_transport::AccessTokenKind::Deletion,
        deletion_refresh_token_entry(),
        "This device no longer has a deletion credential; grant deletion permission again.",
    )
}

fn stored_upload_session(item_id: &str) -> Result<Option<String>, String> {
    match upload_session_entry(item_id)?.get_password() {
        Ok(uri) => Ok(Some(uri)),
        Err(_) => Ok(None),
    }
}

fn clear_upload_session(item_id: &str) -> Result<(), String> {
    match upload_session_entry(item_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(user_error(error)),
    }
}

fn mark_upload_state(
    state: &AppState,
    item_id: &str,
    status: &str,
    confirmed_bytes: u64,
    detail: &str,
    video_id: Option<&str>,
) -> Result<(), String> {
    let connection = database(state)?;
    connection
        .execute(
            "UPDATE upload_items SET status = ?1, confirmed_bytes = ?2, detail = ?3, video_id = COALESCE(?4, video_id), source_delete_status = CASE WHEN ?1 = 'uploaded' AND delete_source_after_upload = 1 THEN 'pending' ELSE source_delete_status END, updated_at = ?5 WHERE id = ?6 AND status != 'cancelled'",
            params![status, confirmed_bytes as i64, detail, video_id, now(), item_id],
        )
        .map_err(user_error)?;
    Ok(())
}

fn persist_provider_success(
    state: &AppState,
    item_id: &str,
    expected_channel_id: &str,
    total_bytes: u64,
    video_id: &str,
) -> Result<(), String> {
    if video_id.trim().is_empty() || video_id.len() > 128 {
        return Err("YouTube completed the upload without a usable video ID.".into());
    }
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let changed = transaction
        .execute(
            "UPDATE upload_items SET status = 'uploaded', confirmed_bytes = ?1, video_id = ?2, detail = CASE WHEN playlist_id IS NOT NULL OR delete_source_after_upload = 1 THEN 'Uploaded to YouTube. Optional post-upload work is pending locally.' ELSE 'Uploaded to YouTube; processing status can be checked from inventory.' END, playlist_status = CASE WHEN playlist_id IS NOT NULL THEN 'pending' ELSE 'not_requested' END, source_delete_status = CASE WHEN delete_source_after_upload = 1 THEN 'pending' ELSE source_delete_status END, updated_at = ?3 WHERE id = ?4 AND channel_id = ?5 AND status IN ('uploading', 'cancelled')",
            params![total_bytes as i64, video_id, now(), item_id, expected_channel_id],
        )
        .map_err(user_error)?;
    if changed != 1 {
        return Err(
            "The provider success could not be bound to its immutable local upload claim.".into(),
        );
    }
    audit(
        &transaction,
        item_id,
        "upload_completed",
        "YouTube returned a video ID; the provider receipt was committed before optional post-upload work",
    )?;
    transaction.commit().map_err(user_error)
}

fn record_source_delete_outcome(
    connection: &Connection,
    item_id: &str,
    status: &str,
    audit_kind: &str,
    audit_detail: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE upload_items SET source_delete_status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now(), item_id],
        )
        .map_err(user_error)?;
    audit(connection, item_id, audit_kind, audit_detail)
}

/// Reject local links at destructive boundaries. Canonicalizing a link first
/// would turn an operator-reviewed directory entry into its target and make a
/// later cleanup capable of deleting that target instead.
fn is_link_or_reparse_point(path: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(path).map_err(user_error)?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(windows)]
    {
        return Ok(metadata.file_attributes() & 0x400 != 0);
    }
    #[cfg(not(windows))]
    Ok(false)
}

/// The reviewed name can also traverse a linked parent directory. Check every
/// existing path component before canonicalization so a junction or symlink is
/// never silently converted into an external deletion target.
fn has_link_or_reparse_component(path: &Path) -> Result<bool, String> {
    for component in path.ancestors() {
        if is_link_or_reparse_point(component)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Move a reviewed external file to a unique sibling name before final
/// validation and deletion. This prevents a replacement at the reviewed path
/// from being removed by a later path-based delete.
fn stage_file_for_deletion(path: &Path) -> Result<PathBuf, String> {
    if has_link_or_reparse_component(path)? {
        return Err("A linked local file cannot be deleted from this application.".into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "The reviewed local file has no parent folder.".to_string())?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "The reviewed local file has an unsupported name.".to_string())?;
    for _ in 0..4 {
        let staged = parent.join(format!(
            ".{file_name}.youtube-upload-manager-{}.pending",
            Uuid::new_v4()
        ));
        if staged.exists() {
            continue;
        }
        fs::rename(path, &staged).map_err(user_error)?;
        return Ok(staged);
    }
    Err("The reviewed local file could not be staged safely for deletion.".into())
}

fn restore_staged_file(staged: &Path, original: &Path) {
    if !original.exists() {
        let _ = fs::rename(staged, original);
    }
}

/// Delete only an unchanged external source after a persisted YouTube success.
/// The managed workspace copy is never a cleanup target. A pending cleanup is
/// intentionally resumable after a crash or a transient file lock.
fn finalize_confirmed_source_cleanup(state: &AppState, item_id: &str) -> Result<(), String> {
    let connection = database(state)?;
    let cleanup = connection
        .query_row(
            "SELECT source_path, digest FROM upload_items WHERE id = ?1 AND status = 'uploaded' AND source_delete_status = 'pending'",
            [item_id],
            |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(user_error)?;
    let Some((source_path, expected_digest)) = cleanup else {
        return Ok(());
    };
    let (Some(source_path), Some(expected_digest)) = (source_path, expected_digest) else {
        return record_source_delete_outcome(
            &connection,
            item_id,
            "retained",
            "source_cleanup_retained",
            "Original source was retained because the verified source path or digest is unavailable.",
        );
    };
    if has_link_or_reparse_component(Path::new(&source_path)).unwrap_or(true) {
        return record_source_delete_outcome(
            &connection,
            item_id,
            "retained",
            "source_cleanup_retained",
            "Original source was retained because a linked path cannot be cleaned up automatically.",
        );
    }
    let managed_directory = match state.media_directory.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    let source = match Path::new(&source_path).canonicalize() {
        Ok(path) => path,
        Err(_) => return record_source_delete_outcome(
            &connection,
            item_id,
            "retained",
            "source_cleanup_retained",
            "Original source was retained because it is no longer available at the saved location.",
        ),
    };
    if !source.is_file() || source.starts_with(&managed_directory) {
        return record_source_delete_outcome(
            &connection,
            item_id,
            "retained",
            "source_cleanup_retained",
            "Original source was retained because the cleanup target was not a verified external file.",
        );
    }
    let staged = match stage_file_for_deletion(&source) {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    let current_digest = match digest_file(&staged) {
        Ok((_, digest)) => digest,
        Err(_) => {
            restore_staged_file(&staged, &source);
            return Ok(());
        }
    };
    if current_digest != expected_digest {
        restore_staged_file(&staged, &source);
        return record_source_delete_outcome(
            &connection,
            item_id,
            "retained",
            "source_cleanup_retained",
            "Original source was retained because it changed after import.",
        );
    }
    match fs::remove_file(&staged) {
        Ok(()) => {
            connection
                .execute(
                    "UPDATE upload_items SET source_path = NULL, source_delete_status = 'deleted', updated_at = ?1 WHERE id = ?2",
                    params![now(), item_id],
                )
                .map_err(user_error)?;
            audit(
                &connection,
                item_id,
                "source_cleanup_deleted",
                "Original source was deleted after YouTube confirmed the upload and its SHA-256 still matched.",
            )
        }
        Err(_) => audit(
            &connection,
            item_id,
            "source_cleanup_retry_pending",
            "Original source deletion is pending retry after a local filesystem error.",
        ),
    }
}

/// An explicit, typed post-upload cleanup request. The native boundary verifies
/// that YouTube has already confirmed the upload before it can mark the source
/// for the same guarded cleanup used by the opt-in automatic mode.
fn delete_uploaded_source_impl(
    state: &AppState,
    item_id: &str,
    confirmation: &str,
) -> Result<(), String> {
    let connection = database(state)?;
    let (status, file_name): (String, String) = connection
        .query_row(
            "SELECT status, file_name FROM upload_items WHERE id = ?1",
            [item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(user_error)?
        .ok_or_else(|| {
            "This uploaded video is no longer available for source cleanup.".to_string()
        })?;
    if status != "uploaded" {
        return Err(
            "Wait for YouTube to confirm this upload before deleting its original file.".into(),
        );
    }
    if confirmation.trim() != file_name {
        return Err("Type the exact original filename before deleting it.".into());
    }
    connection
        .execute(
            "UPDATE upload_items SET source_delete_status = 'pending', updated_at = ?1 WHERE id = ?2 AND status = 'uploaded'",
            params![now(), item_id],
        )
        .map_err(user_error)?;
    audit(
        &connection,
        item_id,
        "source_cleanup_confirmed",
        "Operator confirmed original-source cleanup after YouTube confirmed the upload.",
    )?;
    drop(connection);
    finalize_confirmed_source_cleanup(state, item_id)
}

fn begin_upload_transfer(
    state: &AppState,
    item_id: &str,
    confirmed_bytes: u64,
    detail: &str,
) -> Result<Instant, String> {
    let started_at = now();
    let connection = database(state)?;
    connection
        .execute(
            "UPDATE upload_items SET status = 'uploading', confirmed_bytes = ?1, detail = ?2, upload_started_at = ?3, transfer_bytes_per_second = NULL, updated_at = ?3 WHERE id = ?4",
            params![confirmed_bytes as i64, detail, started_at, item_id],
        )
        .map_err(user_error)?;
    Ok(Instant::now())
}

fn record_upload_progress(
    state: &AppState,
    item_id: &str,
    confirmed_bytes: u64,
    initial_offset: u64,
    transfer_started: Instant,
) -> Result<(), String> {
    let elapsed_seconds = transfer_started.elapsed().as_secs_f64();
    let transfer_bytes_per_second = if elapsed_seconds > 0.0 && confirmed_bytes > initial_offset {
        Some((confirmed_bytes - initial_offset) as f64 / elapsed_seconds)
    } else {
        None
    };
    let connection = database(state)?;
    connection
        .execute(
            "UPDATE upload_items SET status = 'uploading', confirmed_bytes = ?1, detail = 'YouTube confirmed upload bytes.', transfer_bytes_per_second = ?2, updated_at = ?3 WHERE id = ?4",
            params![confirmed_bytes as i64, transfer_bytes_per_second, now(), item_id],
        )
        .map_err(user_error)?;
    record_connection_capability(&connection, transfer_bytes_per_second)?;
    Ok(())
}

fn valid_upload_visibility(visibility: &str) -> Result<&str, String> {
    match visibility {
        "private" | "unlisted" | "public" => Ok(visibility),
        _ => Err("Choose private, unlisted, or public visibility.".into()),
    }
}

fn valid_folder_monitor_visibility(visibility: &str) -> Result<&str, String> {
    match visibility {
        "private" | "unlisted" => Ok(visibility),
        _ => Err("Watched-folder uploads can be private or unlisted only.".into()),
    }
}

fn valid_playlist_selection(
    playlist_id: Option<String>,
    playlist_title: Option<String>,
) -> Result<(Option<String>, Option<String>), String> {
    match (playlist_id, playlist_title) {
        (None, None) => Ok((None, None)),
        (Some(id), Some(title))
            if !id.is_empty()
                && id.len() <= 128
                && id.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '_' || character == '-'
                })
                && !title.trim().is_empty()
                && title.chars().count() <= 150 =>
        {
            Ok((Some(id), Some(title.trim().to_string())))
        }
        _ => Err("Choose a valid YouTube playlist or select no playlist.".into()),
    }
}

fn valid_new_playlist_title(title: &str) -> Result<String, String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 150 {
        return Err("Enter a playlist name between 1 and 150 characters.".into());
    }
    Ok(title.to_string())
}

fn establish_upload_session(
    state: &AppState,
    item_id: &str,
    title: &str,
    total_bytes: u64,
    visibility: &str,
    made_for_kids: bool,
    access_token: &str,
) -> Result<String, String> {
    let response = provider_transport(state)
        .upload_client()?
        .post("https://www.googleapis.com/upload/youtube/v3/videos")
        .query(&[("uploadType", "resumable"), ("part", "snippet,status")])
        .bearer_auth(access_token)
        .header("X-Upload-Content-Length", total_bytes)
        .header("X-Upload-Content-Type", "application/octet-stream")
        .json(&serde_json::json!({
            "snippet": { "title": title },
            "status": { "privacyStatus": visibility, "selfDeclaredMadeForKids": made_for_kids }
        }))
        .send()
        .map_err(|_| "YouTube could not start the resumable upload.".to_string())?;
    if !response.status().is_success() {
        if youtube_daily_upload_limit_response(response) {
            return Err(DAILY_UPLOAD_LIMIT_MARKER.into());
        }
        return Err("YouTube rejected the upload setup request.".into());
    }
    let session_uri = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "YouTube did not return an upload session.".to_string())?;
    save_upload_session(item_id, session_uri)?;
    begin_upload_transfer(state, item_id, 0, "YouTube resumable session started.")?;
    let connection = database(state)?;
    audit(
        &connection,
        item_id,
        "youtube_upload_session_started",
        &format!("YouTube resumable session started with {visibility} visibility and made-for-kids declaration {made_for_kids}."),
    )?;
    Ok(session_uri.to_string())
}

fn youtube_daily_upload_limit_response(response: reqwest::blocking::Response) -> bool {
    let status = response.status().as_u16();
    if status != 403 && status != 429 {
        return false;
    }
    let body = response.text().unwrap_or_default();
    ["quotaExceeded", "dailyLimitExceeded", "uploadLimitExceeded"]
        .iter()
        .any(|reason| body.contains(reason))
}

fn add_video_to_playlist(
    state: &AppState,
    access_token: &str,
    playlist_id: &str,
    video_id: &str,
) -> Result<(), String> {
    provider_transport(state)
        .control_client()?
        .post("https://www.googleapis.com/youtube/v3/playlistItems")
        .query(&[("part", "snippet")])
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "snippet": {
                "playlistId": playlist_id,
                "resourceId": { "kind": "youtube#video", "videoId": video_id }
            }
        }))
        .send()
        .map_err(|_| {
            "YouTube could not add the uploaded video to the selected playlist.".to_string()
        })?
        .error_for_status()
        .map_err(|_| {
            "YouTube did not authorize adding this video to the selected playlist.".to_string()
        })?;
    Ok(())
}

fn query_upload_session(
    state: &AppState,
    session_uri: &str,
    total_bytes: u64,
) -> Result<Option<u64>, String> {
    let response = provider_transport(state)
        .upload_client()?
        .put(session_uri)
        .header("Content-Length", "0")
        .header("Content-Range", format!("bytes */{total_bytes}"))
        .send()
        .map_err(|_| "YouTube upload-session reconciliation could not be reached.".to_string())?;
    if response.status().as_u16() == 308 {
        return confirmed_offset_from_range(
            response
                .headers()
                .get("range")
                .and_then(|value| value.to_str().ok()),
        )
        .map(Some);
    }
    if response.status().is_success() {
        return Ok(None);
    }
    Err("YouTube upload session expired or could not be safely reconciled.".into())
}

fn upload_item(state: &AppState, item_id: &str) -> Result<bool, String> {
    let connection = database(state)?;
    let (title, source_path, workspace_path, total_bytes, expected_digest, _channel_name, channel_id, source_modified_key, requested_visibility, made_for_kids): (
        String,
        Option<String>,
        String,
        u64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        bool,
    ) = connection
        .query_row(
            "SELECT title, source_path, workspace_path, total_bytes, digest, channel_name, channel_id, source_modified_key, visibility, made_for_kids FROM upload_items WHERE id = ?1 AND status IN ('dispatching', 'needs_reconciliation')",
            [item_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get::<_, i64>(3)? as u64,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get::<_, i64>(9)? != 0,
                ))
            },
        )
        .map_err(|_| "This upload is no longer eligible to run.".to_string())?;
    let watched_folder_item = source_modified_key.is_some();
    let is_direct_reference = source_path.as_deref() == Some(workspace_path.as_str());
    if is_direct_reference {
        let expected_signature = source_modified_key
            .as_ref()
            .map(|modified_key| (total_bytes, modified_key.clone()));
        let source_result =
            verify_reference_source(Path::new(&workspace_path), expected_signature.as_ref());
        let valid = source_result.is_ok_and(|(signature, digest)| {
            signature.0 == total_bytes && expected_digest.as_deref() == Some(digest.as_str())
        });
        if !valid {
            let detail = if watched_folder_item {
                "Watched source changed or disappeared after its stability check; it was not uploaded."
            } else {
                "Original source changed or disappeared after it was reviewed; it was not uploaded."
            };
            connection
                .execute(
                    "UPDATE upload_items SET status = 'failed', background_hash_status = CASE WHEN source_modified_key IS NULL THEN background_hash_status ELSE 'failed' END, detail = ?1, updated_at = ?2 WHERE id = ?3 AND status IN ('dispatching', 'needs_reconciliation')",
                    params![detail, now(), item_id],
                )
                .map_err(user_error)?;
            audit(
                &connection,
                item_id,
                "reference_source_final_integrity_failed",
                "Original source verification failed before provider dispatch; upload was withheld",
            )?;
            return Ok(false);
        }
    } else if !Path::new(&workspace_path).is_file() {
        return Err("The upload media file is missing; it cannot be sent to YouTube.".into());
    }
    let active_channel_id = connection_settings(&connection)?.active_channel_id;
    if channel_id.is_none() || active_channel_id.as_deref() != channel_id.as_deref() {
        connection
            .execute(
                "UPDATE upload_items SET status = 'queued', detail = 'Upload paused until its reviewed YouTube channel is active.', updated_at = ?1 WHERE id = ?2 AND status = 'dispatching'",
                params![now(), item_id],
            )
            .map_err(user_error)?;
        return Ok(false);
    }
    // Watched-folder automation is limited to the operator's persisted private
    // or unlisted choice; public visibility remains available only for manual intake.
    let visibility = if watched_folder_item {
        valid_folder_monitor_visibility(&requested_visibility)?
    } else {
        valid_upload_visibility(&requested_visibility)?
    };
    let access_token = refreshed_access_token(state)?;
    let session_uri = match stored_upload_session(item_id)? {
        Some(uri) => match query_upload_session(state, &uri, total_bytes)? {
            Some(offset) => {
                begin_upload_transfer(
                    state,
                    item_id,
                    offset,
                    "Resuming from YouTube-confirmed byte range.",
                )?;
                uri
            }
            None => {
                // This session has reached a terminal provider state. Retain
                // the local item for explicit review, but remove the session
                // from the recovery lane so it cannot repeatedly preempt a
                // genuinely resumable upload.
                clear_upload_session(item_id)?;
                mark_upload_state(
                    state,
                    item_id,
                    "needs_reconciliation",
                    total_bytes,
                    "YouTube reports a completed upload; review the channel before any retry.",
                    None,
                )?;
                return Ok(false);
            }
        },
        None => establish_upload_session(
            state,
            item_id,
            &title,
            total_bytes,
            visibility,
            made_for_kids,
            &access_token,
        )?,
    };
    let start_offset = connection
        .query_row(
            "SELECT confirmed_bytes FROM upload_items WHERE id = ?1",
            [item_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)? as u64;
    let chunk_bytes = adaptive_upload_chunk_size(&connection)?;
    let workspace_path = Path::new(&workspace_path);
    let _media_upload = active_upload(workspace_path)?;
    let upload_client = provider_transport(state).upload_client()?;
    let mut offset = start_offset;
    let transfer_started = Instant::now();
    let mut cancellation_status = connection
        .prepare_cached("SELECT status FROM upload_items WHERE id = ?1")
        .map_err(user_error)?;
    while offset < total_bytes {
        let active: String = cancellation_status
            .query_row([item_id], |row| row.get(0))
            .map_err(user_error)?;
        if active == "cancelled" {
            return Err(UPLOAD_CANCELLED_MARKER.into());
        }
        let bytes = (total_bytes - offset).min(chunk_bytes as u64);
        let response = provider_transport(state).put_upload_chunk_with_client(
            &upload_client,
            &session_uri,
            workspace_path,
            offset,
            bytes,
            total_bytes,
        )?;
        if response.status().as_u16() == 308 {
            offset = confirmed_offset_from_range(
                response
                    .headers()
                    .get("range")
                    .and_then(|value| value.to_str().ok()),
            )?;
            record_upload_progress(state, item_id, offset, start_offset, transfer_started)?;
            continue;
        }
        if response.status().is_success() {
            let payload: serde_json::Value = response.json().map_err(|_| {
                "YouTube completed the upload but returned an unreadable result.".to_string()
            })?;
            let video_id = payload
                .get("id")
                .and_then(|value| value.as_str())
                .ok_or_else(|| "YouTube completed the upload without a video ID.".to_string())?;
            persist_provider_success(
                state,
                item_id,
                channel_id.as_deref().expect("upload channel was validated"),
                total_bytes,
                video_id,
            )?;
            return Ok(true);
        }
        if youtube_daily_upload_limit_response(response) {
            return Err(DAILY_UPLOAD_LIMIT_MARKER.into());
        }
        return Err(
            "YouTube rejected an upload chunk; the session was preserved for reconciliation."
                .into(),
        );
    }
    Err("Upload finished without a final YouTube result; reconciliation is required.".into())
}

fn pending_upload_postprocess_ids(state: &AppState) -> Result<Vec<String>, String> {
    let connection = database(state)?;
    let Some(channel_id) = connection_settings(&connection)?.active_channel_id else {
        return Ok(Vec::new());
    };
    let ids = connection
        .prepare(
            "SELECT id FROM upload_items WHERE channel_id = ?1 AND status = 'uploaded' AND ((playlist_id IS NOT NULL AND playlist_status = 'pending') OR (delete_source_after_upload = 1 AND source_delete_status = 'pending')) ORDER BY updated_at ASC, id ASC",
        )
        .map_err(user_error)?
        .query_map([channel_id], |row| row.get::<_, String>(0))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(ids)
}

fn persist_playlist_outcome(
    state: &AppState,
    item_id: &str,
    result: Result<String, String>,
) -> Result<(), String> {
    let (status, detail, kind, audit_detail) = match result {
        Ok(playlist_name) => (
            "complete",
            "Uploaded to YouTube and added to the selected playlist.",
            "playlist_item_added",
            format!("Uploaded video added to {playlist_name}."),
        ),
        Err(error) => (
            "failed",
            "Uploaded to YouTube, but the optional playlist step failed. The upload receipt remains confirmed.",
            "playlist_item_failed",
            error,
        ),
    };
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let changed = transaction
        .execute(
            "UPDATE upload_items SET playlist_status = ?1, detail = ?2, updated_at = ?3 WHERE id = ?4 AND playlist_status = 'pending' AND status = 'uploaded'",
            params![status, detail, now(), item_id],
        )
        .map_err(user_error)?;
    if changed != 1 {
        return Err(
            "The optional playlist outcome no longer matches a pending uploaded item.".into(),
        );
    }
    audit(&transaction, item_id, kind, &audit_detail)?;
    transaction.commit().map_err(user_error)
}

fn process_upload_postprocess_item(state: &AppState, item_id: &str) -> Result<(), String> {
    let connection = database(state)?;
    let active_channel_id = connection_settings(&connection)?.active_channel_id;
    let job = connection
        .query_row(
            "SELECT channel_id, video_id, playlist_id, playlist_title, playlist_status, source_delete_status FROM upload_items WHERE id = ?1 AND status = 'uploaded'",
            [item_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()
        .map_err(user_error)?;
    let Some((channel_id, video_id, playlist_id, playlist_title, playlist_status, cleanup_status)) =
        job
    else {
        return Ok(());
    };
    if channel_id.is_none() || channel_id != active_channel_id {
        return Ok(());
    }

    if playlist_status == "pending" {
        let playlist_result = match (playlist_id.as_deref(), video_id.as_deref()) {
            (Some(playlist_id), Some(video_id)) => refreshed_access_token(state)
                .and_then(|token| add_video_to_playlist(state, &token, playlist_id, video_id))
                .map(|_| {
                    playlist_title
                        .clone()
                        .unwrap_or_else(|| "selected playlist".into())
                }),
            _ => Err("The saved playlist job is missing its provider identity.".into()),
        };
        persist_playlist_outcome(state, item_id, playlist_result)?;
    }

    let _ = upload_session_entry(item_id)
        .and_then(|entry| entry.delete_credential().map_err(user_error));
    if cleanup_status.as_deref() == Some("pending") {
        // This may leave the status pending after a transient filesystem error;
        // the single-pass worker will retry on a later wake or relaunch without
        // occupying an upload transfer permit.
        finalize_confirmed_source_cleanup(state, item_id)?;
    }
    Ok(())
}

fn run_upload_postprocess_worker(state: AppState) {
    let mut attempted = HashSet::<String>::new();
    loop {
        let pending = pending_upload_postprocess_ids(&state).unwrap_or_default();
        let next = pending
            .into_iter()
            .find(|item_id| !attempted.contains(item_id));
        let Some(item_id) = next else { break };
        attempted.insert(item_id.clone());
        let _ = process_upload_postprocess_item(&state, &item_id);
    }
    upload_scheduler(&state).end_postprocess_worker();
    // Close the enqueue-vs-exit race without retry-spinning a cleanup that was
    // deliberately left pending after one filesystem attempt.
    let has_unattempted = pending_upload_postprocess_ids(&state)
        .unwrap_or_default()
        .into_iter()
        .any(|item_id| !attempted.contains(&item_id));
    if has_unattempted && upload_scheduler(&state).begin_postprocess_worker() {
        run_upload_postprocess_worker(state);
    }
}

fn ensure_upload_postprocess_worker(state: AppState) -> Result<(), String> {
    if pending_upload_postprocess_ids(&state)?.is_empty() {
        return Ok(());
    }
    start_upload_postprocess_worker(state)
}

fn start_upload_postprocess_worker(state: AppState) -> Result<(), String> {
    if upload_scheduler(&state).begin_postprocess_worker() {
        spawn_worker(move || run_upload_postprocess_worker(state));
    }
    Ok(())
}

fn run_queued_upload(state: AppState, item_id: String) {
    let mut provider_succeeded = false;
    let pause_until =
        database(&state).and_then(|connection| active_upload_quota_pause(&connection));
    match pause_until {
        Ok(Some(pause_until)) => {
            let _ = defer_item_for_active_quota_pause(&state, &item_id, &pause_until);
        }
        Err(_) => {}
        Ok(None) => match upload_item(&state, &item_id) {
            Ok(succeeded) => provider_succeeded = succeeded,
            Err(error) => {
                if error == DAILY_UPLOAD_LIMIT_MARKER {
                    let _ = record_upload_quota_pause(&state, &item_id);
                } else if error != UPLOAD_CANCELLED_MARKER {
                    let confirmed_bytes = database(&state)
                        .and_then(|connection| {
                            connection
                                .query_row(
                                    "SELECT confirmed_bytes FROM upload_items WHERE id = ?1",
                                    [&item_id],
                                    |row| row.get::<_, i64>(0),
                                )
                                .map(|value| value as u64)
                                .map_err(user_error)
                        })
                        .unwrap_or(0);
                    let _ = mark_upload_state(
                        &state,
                        &item_id,
                        "needs_reconciliation",
                        confirmed_bytes,
                        &error,
                        None,
                    );
                    if let Ok(connection) = database(&state) {
                        let _ = audit(
                            &connection,
                            &item_id,
                            "upload_interrupted",
                            "Upload requires provider reconciliation",
                        );
                    }
                }
            }
        },
    }
    upload_scheduler(&state).release(&item_id);
    // Release the durable/in-memory permit before the next fair handoff.
    let _ = start_queued_uploads_impl(&state);
    if provider_succeeded {
        let _ = ensure_upload_postprocess_worker(state);
    }
}

fn youtube_json(
    state: &AppState,
    access_token: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<serde_json::Value, String> {
    let response = provider_transport(state)
        .control_client()?
        .get(format!("https://www.googleapis.com/youtube/v3/{path}"))
        .bearer_auth(access_token)
        .query(query)
        .send()
        .map_err(|_| {
            "YouTube inventory sync could not be reached. Check your connection and try again."
                .to_string()
        })?;
    if !response.status().is_success() {
        return Err(youtube_inventory_http_error(response.status().as_u16()));
    }
    response
        .json()
        .map_err(|_| "YouTube returned an unreadable inventory response.".to_string())
}

fn youtube_inventory_http_error(status: u16) -> String {
    match status {
        401 => "YouTube access expired or was revoked. Connect YouTube again, then refresh the library.".into(),
        403 => "Google denied library access. Confirm the YouTube Data API is enabled for this OAuth project, then reconnect YouTube and try again.".into(),
        429 => "YouTube is rate-limiting library refreshes. Wait a little, then try again.".into(),
        500..=599 => "YouTube is temporarily unavailable. Your last complete local library was kept; try refreshing again shortly.".into(),
        _ => "YouTube rejected the library refresh. Your last complete local library was kept.".into(),
    }
}

/// Inventory refresh errors are shown in the watched-folder panel and copied
/// into diagnostics. Keep their classifications actionable but never persist a
/// raw credential-store, filesystem, or provider response error.
fn safe_folder_monitor_inventory_failure(error: &str) -> &'static str {
    if error.contains("no longer has a YouTube credential") {
        "This device no longer has a saved YouTube credential. Reconnect YouTube, then retry the scan."
    } else if error.contains("invalid_grant") {
        "Google rejected the saved sign-in (invalid_grant). Reconnect YouTube, then retry the scan."
    } else if error.contains("invalid_client") {
        "Google rejected this OAuth desktop client (invalid_client). Verify the client in Google Cloud, reconnect YouTube, then retry."
    } else if error.contains("Google token refresh could not be reached") {
        "Google token refresh could not be reached. Check your connection, then retry the scan."
    } else if error.contains("YouTube authorization expired or was revoked") {
        "YouTube access expired or was revoked. Reconnect YouTube, then retry the scan."
    } else if error.contains("Google denied library access") {
        "Google denied YouTube library access. Enable the YouTube Data API, reconnect YouTube, then retry."
    } else if error.contains("rate-limiting") {
        "YouTube is rate-limiting library refreshes. Wait a little, then retry the scan."
    } else if error.contains("temporarily unavailable") {
        "YouTube is temporarily unavailable. Retry the scan shortly."
    } else if error.contains("database is locked") || error.contains("database is busy") {
        "Another local operation is briefly using the database. Wait a moment, then retry the scan."
    } else if error.contains("inventory sync could not be reached") {
        "YouTube inventory sync could not be reached. Check your connection, then retry the scan."
    } else {
        "The YouTube inventory refresh failed. Reconnect YouTube, then retry the scan."
    }
}

fn safe_inventory_sync_failure(error: &str) -> String {
    let detail = diagnostic_detail(error);
    if detail != "[redacted sensitive detail]" {
        detail
    } else if error.contains("database is locked") || error.contains("database is busy") {
        "The local library could not be saved because another local operation held the database. Retry the refresh.".into()
    } else {
        safe_folder_monitor_inventory_failure(error).into()
    }
}

fn youtube_playlist_creation_http_error(status: u16) -> String {
    match status {
        401 => "YouTube authorization expired or was revoked. Connect YouTube again, then create the playlist.".into(),
        403 => "Google denied playlist creation. Reconnect YouTube to grant playlist access, then try again.".into(),
        429 => "YouTube is rate-limiting playlist creation. Wait a little, then try again.".into(),
        500..=599 => "YouTube is temporarily unavailable. No playlist was confirmed; try again shortly.".into(),
        _ => "YouTube could not create the playlist. No playlist was confirmed.".into(),
    }
}

fn clear_stale_inventory_staging(connection: &Connection, channel_id: &str) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM remote_video_sync_staging WHERE channel_id = ?1",
            [channel_id],
        )
        .map_err(user_error)?;
    Ok(())
}

fn replace_inventory_from_staging(
    state: &AppState,
    channel_id: &str,
    sync_id: &str,
) -> Result<(), String> {
    // The long staging phase performs hundreds of autocommit inserts. Reopen
    // before the short atomic replacement so it cannot inherit any connection
    // state from that phase.
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| format!("The local library commit could not start: {error}"))?;
    transaction
        .execute(
            "DELETE FROM remote_videos WHERE channel_id = ?1",
            [channel_id],
        )
        .map_err(|error| {
            format!("The local library commit could not clear the prior snapshot: {error}")
        })?;
    transaction
        .execute(
            "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, normalized_title, canonical_title, has_copy_marker, numeric_title_key, title_keys_version, duration, privacy_status, upload_status, updated_at) SELECT video_id, channel_name, channel_id, title, normalized_title, canonical_title, has_copy_marker, numeric_title_key, title_keys_version, duration, privacy_status, upload_status, updated_at FROM remote_video_sync_staging WHERE sync_id = ?1 AND channel_id = ?2",
            params![sync_id, channel_id],
        )
        .map_err(|error| format!("The local library commit could not save the refreshed snapshot: {error}"))?;
    transaction
        .execute(
            "DELETE FROM remote_video_sync_staging WHERE sync_id = ?1 AND channel_id = ?2",
            params![sync_id, channel_id],
        )
        .map_err(|error| {
            format!("The local library commit could not clear staged records: {error}")
        })?;
    transaction
        .execute(
            "INSERT INTO channel_generations(channel_id, inventory_generation, upload_generation, updated_at) VALUES (?1, 1, 0, ?2) ON CONFLICT(channel_id) DO UPDATE SET inventory_generation = inventory_generation + 1, updated_at = excluded.updated_at",
            params![channel_id, now()],
        )
        .map_err(|error| format!("The local library generation could not advance: {error}"))?;
    state_events::record_invalidation(
        &transaction,
        channel_id,
        "inventory",
        channel_id,
        "replace",
    )?;
    state_events::record_invalidation(
        &transaction,
        channel_id,
        "dedupe",
        channel_id,
        "invalidate",
    )?;
    transaction
        .commit()
        .map_err(|error| format!("The local library commit could not finish: {error}"))?;
    Ok(())
}

/// Folder intake uses the last complete channel snapshot between scheduled
/// refreshes. A five-second local folder poll must never become a five-second
/// full YouTube library crawl.
fn folder_inventory_refresh_due(connection: &Connection, channel_id: &str) -> Result<bool, String> {
    let newest: Option<String> = connection
        .query_row(
            "SELECT MAX(updated_at) FROM remote_videos WHERE channel_id = ?1",
            [channel_id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    Ok(newest
        .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
        .is_none_or(|timestamp| {
            timestamp.with_timezone(&Utc) + FOLDER_INVENTORY_REFRESH_INTERVAL <= Utc::now()
        }))
}

fn sync_channel_inventory_worker(state: &AppState) -> Result<usize, String> {
    let _guard = inventory_sync_lock()
        .lock()
        .map_err(|_| "The local YouTube library refresh lock is unavailable.".to_string())?;
    let access_token = refreshed_access_token(state)?;
    let channel_response = youtube_json(
        state,
        &access_token,
        "channels",
        &[("part", "contentDetails,snippet"), ("mine", "true")],
    )?;
    let channel = channel_response
        .get("items")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .ok_or_else(|| "No YouTube channel was returned for this account.".to_string())?;
    let channel_name = channel
        .pointer("/snippet/title")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "YouTube did not return a channel name.".to_string())?;
    let channel_id = channel
        .get("id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "YouTube did not return an immutable channel identity.".to_string())?;
    let uploads_playlist = channel
        .pointer("/contentDetails/relatedPlaylists/uploads")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "YouTube did not return an uploads playlist.".to_string())?;
    let mut connection = database(state)?;
    let sync_id = Uuid::new_v4().to_string();
    clear_stale_inventory_staging(&connection, channel_id)?;
    let mut next_page: Option<String> = None;
    loop {
        let mut query = vec![
            ("part", "contentDetails"),
            ("playlistId", uploads_playlist),
            ("maxResults", "50"),
        ];
        if let Some(page) = next_page.as_deref() {
            query.push(("pageToken", page));
        }
        let playlist = youtube_json(state, &access_token, "playlistItems", &query)?;
        let ids = playlist
            .get("items")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| {
                item.pointer("/contentDetails/videoId")
                    .and_then(|value| value.as_str())
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
        for ids in ids.chunks(50) {
            let joined = ids.join(",");
            let videos = youtube_json(
                state,
                &access_token,
                "videos",
                &[
                    ("part", "snippet,contentDetails,status"),
                    ("id", joined.as_str()),
                ],
            )?;
            let staged_videos = videos
                .get("items")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
                .filter_map(|video| {
                    Some((
                        video.get("id")?.as_str()?.to_string(),
                        video.pointer("/snippet/title")?.as_str()?.to_string(),
                        video
                            .pointer("/contentDetails/duration")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        video
                            .pointer("/status/privacyStatus")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        video
                            .pointer("/status/uploadStatus")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                    ))
                })
                .collect::<Vec<_>>();
            let transaction = connection.transaction().map_err(user_error)?;
            {
                let mut insert = transaction.prepare_cached("INSERT INTO remote_video_sync_staging (sync_id, video_id, channel_name, channel_id, title, normalized_title, canonical_title, has_copy_marker, numeric_title_key, title_keys_version, duration, privacy_status, upload_status, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) ON CONFLICT(sync_id, video_id) DO UPDATE SET channel_name = excluded.channel_name, channel_id = excluded.channel_id, title = excluded.title, normalized_title = excluded.normalized_title, canonical_title = excluded.canonical_title, has_copy_marker = excluded.has_copy_marker, numeric_title_key = excluded.numeric_title_key, title_keys_version = excluded.title_keys_version, duration = excluded.duration, privacy_status = excluded.privacy_status, upload_status = excluded.upload_status, updated_at = excluded.updated_at").map_err(user_error)?;
                let timestamp = now();
                for (video_id, title, duration, privacy, upload_status) in staged_videos {
                    let keys = title_matching::keys(&title);
                    insert
                        .execute(params![
                            &sync_id,
                            video_id,
                            channel_name,
                            channel_id,
                            title,
                            keys.normalized,
                            keys.canonical,
                            keys.has_copy_marker as i64,
                            keys.numeric,
                            title_matching::TITLE_KEYS_VERSION,
                            duration,
                            privacy,
                            upload_status,
                            &timestamp,
                        ])
                        .map_err(user_error)?;
                }
            }
            transaction.commit().map_err(user_error)?;
        }
        next_page = playlist
            .get("nextPageToken")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        if next_page.is_none() {
            break;
        }
    }
    let count = connection
        .query_row(
            "SELECT COUNT(*) FROM remote_video_sync_staging WHERE sync_id = ?1 AND channel_id = ?2",
            params![&sync_id, channel_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)? as usize;
    let processed_count = connection
        .query_row(
            "SELECT COUNT(*) FROM remote_video_sync_staging WHERE sync_id = ?1 AND channel_id = ?2 AND upload_status = 'processed'",
            params![&sync_id, channel_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)? as usize;
    // Keep the last complete inventory until every remote page has been read.
    // A crash or network failure therefore leaves a usable, coherent snapshot.
    drop(connection);
    replace_inventory_from_staging(state, channel_id, &sync_id)?;
    set_connection_detail(
        state,
        &format!("Synced {count} YouTube video records locally; {processed_count} fully processed videos are eligible for duplicate checks."),
        Some((channel_name, channel_id)),
    )?;
    let connection = database(state)?;
    audit_global(
        &connection,
        "youtube_inventory_synced",
        "Channel upload inventory synced locally",
    )?;
    Ok(count)
}

fn await_oauth_callback(
    state: AppState,
    listener: TcpListener,
    expected_state: String,
    client_id: String,
    redirect_uri: String,
    attempt_kind: OAuthAttemptKind,
) {
    let deadline = Instant::now() + Duration::from_secs(600);
    let _ = listener.set_nonblocking(true);
    while Instant::now() < deadline {
        let attempt_active = state
            .oauth_attempts
            .lock()
            .map(|attempts| attempts.contains_key(&expected_state))
            .unwrap_or(false);
        if !attempt_active {
            let _ = oauth_verifier_entry(&expected_state)
                .and_then(|entry| entry.delete_credential().map_err(user_error));
            return;
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let attempt_active = state
                    .oauth_attempts
                    .lock()
                    .map(|attempts| attempts.contains_key(&expected_state))
                    .unwrap_or(false);
                if !attempt_active {
                    respond_to_callback(&mut stream, "Connection was cancelled.");
                    return;
                }
                let mut request = [0_u8; 8192];
                let bytes = match stream.read(&mut request) {
                    Ok(bytes) => bytes,
                    Err(_) => 0,
                };
                let request_line = String::from_utf8_lossy(&request[..bytes])
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                let mut request_parts = request_line.split_whitespace();
                let method = request_parts.next();
                let target = request_parts.next().unwrap_or("/");
                if !valid_oauth_callback_request(method, target, &expected_state) {
                    respond_to_callback(&mut stream, "Connection did not complete.");
                    continue;
                }
                let code = callback_value(target, "code");
                let result = if let Some(error) = callback_value(target, "error") {
                    Err(oauth_error_message(&error))
                } else if let Some(code) = code {
                    let verifier = oauth_verifier_entry(&expected_state)
                        .and_then(|entry| entry.get_password().map_err(user_error));
                    verifier.and_then(|verifier| {
                        complete_oauth_connection(
                            &state,
                            &client_id,
                            &redirect_uri,
                            &verifier,
                            &code,
                            attempt_kind,
                        )
                    })
                } else {
                    Err("The Google authorization response did not include a code.".to_string())
                };
                let _ = oauth_verifier_entry(&expected_state)
                    .and_then(|entry| entry.delete_credential().map_err(user_error));
                let _ = state
                    .oauth_attempts
                    .lock()
                    .map(|mut attempts| attempts.remove(&expected_state));
                match result {
                    Ok(_) => {
                        // Startup may have recovered queued items before a
                        // channel was connected. A completed connection must
                        // retry those items without a separate Start action.
                        kick_automatic_upload_dispatch(state.clone());
                        respond_to_callback(&mut stream, "YouTube is connected.")
                    }
                    Err(error) => {
                        let _ = set_connection_failure_detail(&state, &error);
                        if let Ok(connection) = database(&state) {
                            let _ = audit_global(
                                &connection,
                                "youtube_connection_failed",
                                &diagnostic_detail(&error),
                            );
                        }
                        respond_to_callback(&mut stream, "Connection did not complete.");
                    }
                }
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(125))
            }
            Err(_) => break,
        }
    }
    let attempt_active = state
        .oauth_attempts
        .lock()
        .map(|mut attempts| attempts.remove(&expected_state).is_some())
        .unwrap_or(false);
    if attempt_active {
        let _ = set_connection_detail(
            &state,
            "Google authorization timed out. Connect again when ready.",
            None,
        );
    }
}

fn valid_google_client_id(client_id: &str) -> bool {
    let candidate = client_id.trim();
    candidate.len() > ".apps.googleusercontent.com".len()
        && candidate.ends_with(".apps.googleusercontent.com")
        && candidate.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '.'
        })
}

fn row_to_upload_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<UploadItem> {
    Ok(UploadItem {
        id: row.get("id")?,
        title: row.get("title")?,
        file_name: row.get("file_name")?,
        size_bytes: row.get::<_, i64>("size_bytes")? as u64,
        digest: row.get("digest")?,
        status: row.get("status")?,
        confirmed_bytes: row.get::<_, i64>("confirmed_bytes")? as u64,
        total_bytes: row.get::<_, i64>("total_bytes")? as u64,
        video_id: row.get("video_id")?,
        detail: row.get("detail")?,
        visibility: row.get("visibility")?,
        made_for_kids: row.get::<_, i64>("made_for_kids")? != 0,
        playlist_id: row.get("playlist_id")?,
        playlist_title: row.get("playlist_title")?,
        upload_started_at: row.get("upload_started_at")?,
        transfer_bytes_per_second: row.get("transfer_bytes_per_second")?,
        delete_source_after_upload: row.get::<_, i64>("delete_source_after_upload")? != 0,
        source_delete_status: row.get("source_delete_status")?,
        updated_at: row.get("updated_at")?,
    })
}

fn find_item(connection: &Connection, id: &str) -> Result<UploadItem, String> {
    connection
        .query_row(
            "SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, visibility, made_for_kids, playlist_id, playlist_title, upload_started_at, transfer_bytes_per_second, delete_source_after_upload, source_delete_status, updated_at FROM upload_items WHERE id = ?1",
            [id],
            row_to_upload_item,
        )
        .map_err(user_error)
}

fn exact_local_duplicates(
    connection: &Connection,
    channel_id: Option<&str>,
) -> Result<Vec<DuplicateCandidate>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, digest FROM upload_items WHERE digest IS NOT NULL AND status = 'uploaded' AND ((?1 IS NOT NULL AND channel_id = ?1) OR (?1 IS NULL AND (channel_id IS NULL OR channel_id = ''))) ORDER BY created_at ASC",
        )
        .map_err(user_error)?;
    let rows = statement
        .query_map([channel_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut by_digest: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (id, title, digest) in rows {
        by_digest.entry(digest).or_default().push((id, title));
    }
    let mut candidates = Vec::new();
    for (digest, items) in by_digest {
        if items.len() < 2 {
            continue;
        }
        let (left_id, left_title) = &items[0];
        for (right_id, right_title) in items.iter().skip(1) {
            candidates.push(DuplicateCandidate {
                id: format!("local:{left_id}:{right_id}"),
                confidence: "exact_local".into(),
                left_title: left_title.clone(),
                right_title: right_title.clone(),
                left_video_id: None,
                right_video_id: None,
                evidence: format!("Matching managed-media SHA-256: {digest}"),
                decision: None,
            });
        }
    }
    Ok(candidates)
}

fn ignored_duplicate_candidate_ids(connection: &Connection) -> Result<HashSet<String>, String> {
    let channel_id = connection_settings(connection)?
        .active_channel_id
        .unwrap_or_default();
    let mut statement = connection
        .prepare("SELECT candidate_id FROM ignored_duplicate_candidates WHERE channel_id = ?1")
        .map_err(user_error)?;
    let ignored_ids = statement
        .query_map([channel_id], |row| row.get::<_, String>(0))
        .map_err(user_error)?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(user_error)?;
    Ok(ignored_ids)
}

fn current_duplicate_candidates(
    connection: &Connection,
) -> Result<Vec<DuplicateCandidate>, String> {
    let settings = connection_settings(connection)?;
    duplicate_candidates_for_channel(connection, settings.active_channel_id.as_deref())
}

fn duplicate_candidates_for_channel(
    connection: &Connection,
    active_channel_id: Option<&str>,
) -> Result<Vec<DuplicateCandidate>, String> {
    let channel_id = active_channel_id.unwrap_or_default();
    let (inventory_generation, upload_generation) = connection
        .query_row(
            "SELECT inventory_generation, upload_generation FROM channel_generations WHERE channel_id = ?1",
            [channel_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(user_error)?
        .unwrap_or((0, 0));
    let projection_current = connection
        .query_row(
            "SELECT inventory_generation = ?2 AND upload_generation = ?3 FROM duplicate_projection_state WHERE channel_id = ?1",
            params![channel_id, inventory_generation, upload_generation],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(user_error)?
        .is_some_and(|current| current != 0);
    if !projection_current {
        let mut candidates = exact_local_duplicates(connection, active_channel_id)?;
        if let Some(active_channel_id) = active_channel_id {
            candidates.extend(uploaded_title_duplicates(connection, active_channel_id)?);
        }
        let transaction = connection.unchecked_transaction().map_err(user_error)?;
        transaction
            .execute(
                "DELETE FROM duplicate_candidate_projection WHERE channel_id = ?1",
                [channel_id],
            )
            .map_err(user_error)?;
        {
            let mut insert = transaction
                .prepare_cached(
                    "INSERT INTO duplicate_candidate_projection(channel_id, candidate_id, confidence, left_title, right_title, left_video_id, right_video_id, evidence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .map_err(user_error)?;
            for candidate in candidates {
                insert
                    .execute(params![
                        channel_id,
                        candidate.id,
                        candidate.confidence,
                        candidate.left_title,
                        candidate.right_title,
                        candidate.left_video_id,
                        candidate.right_video_id,
                        candidate.evidence
                    ])
                    .map_err(user_error)?;
            }
        }
        transaction
            .execute(
                "INSERT INTO duplicate_projection_state(channel_id, inventory_generation, upload_generation, rebuilt_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(channel_id) DO UPDATE SET inventory_generation = excluded.inventory_generation, upload_generation = excluded.upload_generation, rebuilt_at = excluded.rebuilt_at",
                params![channel_id, inventory_generation, upload_generation, now()],
            )
            .map_err(user_error)?;
        transaction.commit().map_err(user_error)?;
    }
    connection
        .prepare(
            "SELECT candidate_id, confidence, left_title, right_title, left_video_id, right_video_id, evidence FROM duplicate_candidate_projection WHERE channel_id = ?1 ORDER BY candidate_id ASC",
        )
        .map_err(user_error)?
        .query_map([channel_id], |row| {
            Ok(DuplicateCandidate {
                id: row.get(0)?,
                confidence: row.get(1)?,
                left_title: row.get(2)?,
                right_title: row.get(3)?,
                left_video_id: row.get(4)?,
                right_video_id: row.get(5)?,
                evidence: row.get(6)?,
                decision: None,
            })
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)
}

fn ignore_duplicate_candidate_impl(state: &AppState, candidate_id: &str) -> Result<(), String> {
    if candidate_id.trim().is_empty() {
        return Err("Choose a duplicate candidate to ignore.".into());
    }
    let connection = database(state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .unwrap_or_default();
    if !current_duplicate_candidates(&connection)?
        .iter()
        .any(|candidate| candidate.id == candidate_id)
    {
        return Err("That duplicate candidate is no longer available for review.".into());
    }
    connection
        .execute(
            "INSERT INTO ignored_duplicate_candidates (channel_id, candidate_id, ignored_at) VALUES (?1, ?2, ?3) ON CONFLICT(channel_id, candidate_id) DO UPDATE SET ignored_at = excluded.ignored_at",
            params![channel_id, candidate_id, now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "duplicate_candidate_ignored",
        "Operator marked a duplicate candidate as a false positive",
    )
}

fn re_audit_ignored_duplicate_candidates_impl(state: &AppState) -> Result<usize, String> {
    let connection = database(state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .unwrap_or_default();
    let cleared = connection
        .execute(
            "DELETE FROM ignored_duplicate_candidates WHERE channel_id = ?1",
            [channel_id],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "ignored_duplicate_candidates_reaudited",
        &format!("Operator restored {cleared} ignored duplicate candidate(s) for re-audit"),
    )?;
    Ok(cleared)
}

#[cfg(test)]
fn normalized_uploaded_title(title: &str) -> String {
    title_matching::normalized(title)
}

#[cfg(test)]
fn canonical_uploaded_title(title: &str) -> (String, bool) {
    title_matching::canonical(title)
}

#[cfg(test)]
fn title_number_sequences(title: &str) -> Vec<String> {
    title_matching::number_sequences(title)
}

/// Returns review-only title evidence. Numeric-sequence matching is deliberately
/// conservative: it needs multiple ordered sequences and at least six digits,
/// so short incidental numbers do not become duplicate candidates.
fn uploaded_title_match_evidence(left: &str, right: &str) -> Option<&'static str> {
    title_matching::evidence(left, right)
}

fn uploaded_titles_match(left: &str, right: &str) -> bool {
    uploaded_title_match_evidence(left, right).is_some()
}

fn ensure_remote_title_keys(connection: &Connection, channel_id: &str) -> Result<(), String> {
    let stale = connection
        .prepare(
            "SELECT video_id, title FROM remote_videos WHERE channel_id = ?1 AND title_keys_version != ?2",
        )
        .map_err(user_error)?
        .query_map(
            params![channel_id, title_matching::TITLE_KEYS_VERSION],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    if stale.is_empty() {
        return Ok(());
    }
    let mut update = connection
        .prepare_cached(
            "UPDATE remote_videos SET normalized_title = ?1, canonical_title = ?2, has_copy_marker = ?3, numeric_title_key = ?4, title_keys_version = ?5 WHERE video_id = ?6 AND channel_id = ?7",
        )
        .map_err(user_error)?;
    for (video_id, title) in stale {
        let keys = title_matching::keys(&title);
        update
            .execute(params![
                keys.normalized,
                keys.canonical,
                keys.has_copy_marker as i64,
                keys.numeric,
                title_matching::TITLE_KEYS_VERSION,
                video_id,
                channel_id
            ])
            .map_err(user_error)?;
    }
    Ok(())
}

fn ensure_upload_title_keys(connection: &Connection, channel_id: &str) -> Result<(), String> {
    let stale = connection
        .prepare(
            "SELECT id, title FROM upload_items WHERE (channel_id = ?1 OR channel_id IS NULL OR channel_id = '') AND title_keys_version != ?2",
        )
        .map_err(user_error)?
        .query_map(
            params![channel_id, title_matching::TITLE_KEYS_VERSION],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    if stale.is_empty() {
        return Ok(());
    }
    let mut update = connection
        .prepare_cached(
            "UPDATE upload_items SET normalized_title = ?1, canonical_title = ?2, has_copy_marker = ?3, numeric_title_key = ?4, title_keys_version = ?5 WHERE id = ?6",
        )
        .map_err(user_error)?;
    for (item_id, title) in stale {
        let keys = title_matching::keys(&title);
        update
            .execute(params![
                keys.normalized,
                keys.canonical,
                keys.has_copy_marker as i64,
                keys.numeric,
                title_matching::TITLE_KEYS_VERSION,
                item_id
            ])
            .map_err(user_error)?;
    }
    Ok(())
}

#[derive(Clone)]
struct IndexedRemoteTitle {
    title: String,
    duration: Option<String>,
    privacy_status: Option<String>,
    updated_at: String,
    normalized: String,
    canonical: String,
    numeric: String,
}

struct RemoteTitleIndex {
    titles: Vec<IndexedRemoteTitle>,
    normalized: HashMap<String, Vec<usize>>,
    canonical: HashMap<String, Vec<usize>>,
    numeric: HashMap<String, Vec<usize>>,
}

impl RemoteTitleIndex {
    fn load(connection: &Connection, channel_id: &str) -> Result<Self, String> {
        ensure_remote_title_keys(connection, channel_id)?;
        let titles = connection
            .prepare(
                "SELECT title, duration, privacy_status, updated_at, normalized_title, canonical_title, has_copy_marker, numeric_title_key FROM remote_videos WHERE channel_id = ?1 AND upload_status = 'processed' ORDER BY video_id ASC",
            )
            .map_err(user_error)?
            .query_map([channel_id], |row| {
                Ok(IndexedRemoteTitle {
                    title: row.get(0)?,
                    duration: row.get(1)?,
                    privacy_status: row.get(2)?,
                    updated_at: row.get(3)?,
                    normalized: row.get(4)?,
                    canonical: row.get(5)?,
                    numeric: row.get(7)?,
                })
            })
            .map_err(user_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(user_error)?;
        let mut index = Self {
            titles,
            normalized: HashMap::new(),
            canonical: HashMap::new(),
            numeric: HashMap::new(),
        };
        for (position, title) in index.titles.iter().enumerate() {
            index
                .normalized
                .entry(title.normalized.clone())
                .or_default()
                .push(position);
            index
                .canonical
                .entry(title.canonical.clone())
                .or_default()
                .push(position);
            if !title.numeric.is_empty() {
                index
                    .numeric
                    .entry(title.numeric.clone())
                    .or_default()
                    .push(position);
            }
        }
        Ok(index)
    }

    fn matches(&self, title: &str) -> Vec<&IndexedRemoteTitle> {
        let keys = title_matching::keys(title);
        let mut positions = BTreeSet::<usize>::new();
        if let Some(values) = self.normalized.get(&keys.normalized) {
            positions.extend(values.iter().copied());
        }
        if let Some(values) = self.canonical.get(&keys.canonical) {
            positions.extend(values.iter().copied());
        }
        if !keys.numeric.is_empty() {
            if let Some(values) = self.numeric.get(&keys.numeric) {
                positions.extend(values.iter().copied());
            }
        }
        positions
            .into_iter()
            .filter_map(|position| self.titles.get(position))
            .filter(|candidate| {
                // Persisted keys generate a small candidate set; the existing
                // evidence function remains the final explainable contract.
                uploaded_titles_match(title, &candidate.title)
            })
            .collect()
    }
}

fn matching_uploaded_titles(
    connection: &Connection,
    channel_id: &str,
    title: &str,
) -> Result<Vec<String>, String> {
    Ok(RemoteTitleIndex::load(connection, channel_id)?
        .matches(title)
        .into_iter()
        .map(|candidate| candidate.title.clone())
        .collect())
}

/// Lightweight local-title evidence for the active queue. Unscoped records are
/// considered only when they belong to the current operator-selected batch, so
/// one device's unqueued drafts cannot leak into another channel's review.
fn matching_local_upload_titles(
    connection: &Connection,
    channel_id: &str,
    title: &str,
    excluded_item_id: &str,
    batch_item_ids: &HashSet<String>,
) -> Result<Vec<String>, String> {
    ensure_upload_title_keys(connection, channel_id)?;
    let keys = title_matching::keys(title);
    let mut statement = connection
        .prepare(
            "SELECT id, title, channel_id FROM upload_items WHERE id != ?1 AND status IN ('draft', 'queued', 'dispatching', 'uploading', 'needs_reconciliation', 'uploaded', 'failed') AND (channel_id = ?2 OR channel_id IS NULL OR channel_id = '') AND (normalized_title = ?3 OR (canonical_title = ?4 AND (has_copy_marker = 1 OR ?5 = 1)) OR (?6 != '' AND numeric_title_key = ?6)) ORDER BY created_at ASC",
        )
        .map_err(user_error)?;
    let items = statement
        .query_map(
            params![
                excluded_item_id,
                channel_id,
                keys.normalized,
                keys.canonical,
                keys.has_copy_marker as i64,
                keys.numeric
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(items
        .into_iter()
        .filter(|(item_id, _, item_channel)| {
            item_channel.as_deref() == Some(channel_id) || batch_item_ids.contains(item_id)
        })
        .map(|(_, candidate_title, _)| candidate_title)
        .filter(|candidate_title| uploaded_titles_match(title, candidate_title))
        .collect())
}

fn light_dedupe_match_scope(remote_match: bool, local_match: bool) -> String {
    match (remote_match, local_match) {
        (true, true) => "youtube_and_local".into(),
        (true, false) => "youtube".into(),
        (false, true) => "local_queue".into(),
        (false, false) => "none".into(),
    }
}

fn light_dedupe_title_match(
    connection: &Connection,
    channel_id: &str,
    title: &str,
    item_id: &str,
    batch_item_ids: &HashSet<String>,
) -> Result<Option<(Vec<String>, String)>, String> {
    let remote_index = RemoteTitleIndex::load(connection, channel_id)?;
    light_dedupe_title_match_indexed(
        connection,
        channel_id,
        title,
        item_id,
        batch_item_ids,
        &remote_index,
    )
}

fn light_dedupe_title_match_indexed(
    connection: &Connection,
    channel_id: &str,
    title: &str,
    item_id: &str,
    batch_item_ids: &HashSet<String>,
    remote_index: &RemoteTitleIndex,
) -> Result<Option<(Vec<String>, String)>, String> {
    let mut remote_titles = remote_index
        .matches(title)
        .into_iter()
        .map(|candidate| candidate.title.clone())
        .collect::<Vec<_>>();
    let mut local_titles =
        matching_local_upload_titles(connection, channel_id, title, item_id, batch_item_ids)?;
    let scope = light_dedupe_match_scope(!remote_titles.is_empty(), !local_titles.is_empty());
    if scope == "none" {
        return Ok(None);
    }
    remote_titles.append(&mut local_titles);
    remote_titles.sort();
    remote_titles.dedup();
    Ok(Some((remote_titles, scope)))
}

fn pending_upload_title_duplicates(
    connection: &Connection,
    channel_id: &str,
) -> Result<Vec<UploadTitleDuplicate>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, title FROM upload_items WHERE status = 'draft' AND duplicate_decision = 'pending' AND (channel_id = ?1 OR channel_id IS NULL OR channel_id = '') ORDER BY updated_at ASC",
        )
        .map_err(user_error)?;
    let rows = statement
        .query_map([channel_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let pending_item_ids = rows
        .iter()
        .map(|(item_id, _)| item_id.clone())
        .collect::<HashSet<_>>();
    let remote_index = RemoteTitleIndex::load(connection, channel_id)?;
    rows.into_iter()
        .filter_map(|(item_id, title)| {
            match light_dedupe_title_match_indexed(
                connection,
                channel_id,
                &title,
                &item_id,
                &pending_item_ids,
                &remote_index,
            ) {
                Ok(Some((matched_titles, match_scope))) => Some(Ok(UploadTitleDuplicate {
                    item_id,
                    title,
                    matched_titles,
                    match_scope,
                })),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect()
}

/// Synchronizes the operator-authorized inventory before each new dispatch and
/// moves unresolved matches back to a safe local draft for review.
fn check_upload_title_duplicates_impl(
    state: &AppState,
    item_ids: &[String],
) -> Result<Vec<UploadTitleDuplicate>, String> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    let settings = connection_settings(&database(state)?)?;
    if !settings.connected {
        return Err("Connect a YouTube channel before checking uploaded titles.".into());
    }
    sync_channel_inventory_worker(state)?;
    let connection = database(state)?;
    let active_channel_id = connection_settings(&connection)?
        .active_channel_id
        .ok_or_else(|| "No active YouTube channel is available for the title check.".to_string())?;
    let batch_item_ids = item_ids.iter().cloned().collect::<HashSet<_>>();
    let remote_index = RemoteTitleIndex::load(&connection, &active_channel_id)?;
    let mut candidates = Vec::new();
    for item_id in item_ids {
        let item = connection
            .query_row(
                "SELECT title, channel_id, duplicate_decision FROM upload_items WHERE id = ?1",
                [item_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(user_error)?;
        let Some((title, item_channel, decision)) = item else {
            continue;
        };
        if item_channel
            .as_deref()
            .is_some_and(|value| value != active_channel_id)
            || decision.as_deref() == Some("ignore")
        {
            continue;
        }
        let Some((matched_titles, match_scope)) = light_dedupe_title_match_indexed(
            &connection,
            &active_channel_id,
            &title,
            item_id,
            &batch_item_ids,
            &remote_index,
        )?
        else {
            continue;
        };
        connection
            .execute(
                "UPDATE upload_items SET status = 'draft', duplicate_decision = 'pending', detail = 'A light duplicate title match was found. Choose whether to upload anyway or skip this local file.', updated_at = ?1 WHERE id = ?2",
                params![now(), item_id],
            )
            .map_err(user_error)?;
        audit(
            &connection,
            item_id,
            "upload_light_duplicate_detected",
            "Light duplicate title match requires an operator decision",
        )?;
        candidates.push(UploadTitleDuplicate {
            item_id: item_id.clone(),
            title,
            matched_titles,
            match_scope,
        });
    }
    Ok(candidates)
}

/// Creates a persistent pre-ingest job. Light matching only compares names; deep
/// matching streams SHA-256 one source at a time and checkpoints after each file.
fn create_preflight_scan_job(
    state: &AppState,
    paths: &[FilePath],
    mode: &str,
) -> Result<String, String> {
    if paths.is_empty() {
        return Err("Choose at least one file to check.".into());
    }
    if !matches!(mode, "light" | "deep") {
        return Err("Choose either light or deep matching.".into());
    }
    let id = Uuid::new_v4().to_string();
    let mut connection = database(state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .unwrap_or_default();
    let transaction = connection.transaction().map_err(user_error)?;
    let timestamp = now();
    transaction.execute(
        "INSERT INTO preflight_scan_jobs (id, channel_id, mode, status, total_files, completed_files, inventory_status, created_at, updated_at) VALUES (?1, ?2, ?3, 'queued', ?4, 0, ?5, ?6, ?6)",
        params![&id, &channel_id, mode, paths.len() as i64, if mode == "deep" { "pending" } else { "not_requested" }, &timestamp],
    ).map_err(user_error)?;
    {
        let mut insert = transaction.prepare_cached(
                "INSERT INTO preflight_scan_files (job_id, ordinal, source_locator, file_name, status) VALUES (?1, ?2, ?3, ?4, 'queued')",
            ).map_err(user_error)?;
        for (ordinal, path) in paths.iter().enumerate() {
            let locator = serde_json::to_string(path).map_err(user_error)?;
            insert
                .execute(params![
                    &id,
                    ordinal as i64,
                    locator,
                    preflight_file_name(path)
                ])
                .map_err(user_error)?;
        }
    }
    audit_global(
        &transaction,
        "preflight_scan_created",
        &format!("Created persistent {mode} pre-ingest duplicate scan"),
    )?;
    record_preflight_scan_event(
        &transaction,
        &id,
        None,
        if mode == "deep" {
            "Deep SHA-256 duplicate check created."
        } else {
            "Light filename duplicate check created."
        },
    )?;
    transaction.commit().map_err(user_error)?;
    Ok(id)
}

fn record_preflight_scan_event(
    connection: &Connection,
    job_id: &str,
    file_name: Option<&str>,
    message: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO preflight_scan_events (job_id, file_name, message, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![job_id, file_name, message, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn run_preflight_scan_job(
    state: &AppState,
    app: &AppHandle,
    job_id: &str,
    queue_metadata: bool,
) -> Result<(), String> {
    let connection = database(state)?;
    let (mode, job_channel_id): (String, String) = connection
        .query_row(
            "SELECT mode, channel_id FROM preflight_scan_jobs WHERE id = ?1",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(user_error)?;
    connection.execute("UPDATE preflight_scan_jobs SET status = 'running', detail = NULL, updated_at = ?1 WHERE id = ?2", params![now(), job_id]).map_err(user_error)?;
    loop {
        let active: String = connection
            .query_row(
                "SELECT status FROM preflight_scan_jobs WHERE id = ?1",
                [job_id],
                |row| row.get(0),
            )
            .map_err(user_error)?;
        if active == "cancelled" {
            return Ok(());
        }
        let next = connection.query_row(
            "SELECT ordinal, source_locator, file_name FROM preflight_scan_files WHERE job_id = ?1 AND status = 'queued' ORDER BY ordinal ASC LIMIT 1",
            [job_id], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        ).optional().map_err(user_error)?;
        let Some((ordinal, locator, file_name)) = next else {
            break;
        };
        if connection.execute("UPDATE preflight_scan_files SET status = 'running' WHERE job_id = ?1 AND ordinal = ?2 AND status = 'queued'", params![job_id, ordinal]).map_err(user_error)? == 0 {
            continue;
        }
        record_preflight_scan_event(
            &connection,
            job_id,
            Some(&file_name),
            if mode == "deep" {
                "Hashing file."
            } else {
                "Matching filename."
            },
        )?;
        let outcome = if mode == "light" {
            Ok((0_u64, None))
        } else {
            serde_json::from_str::<FilePath>(&locator)
                .map_err(|_| "This saved file reference is invalid.".to_string())
                .and_then(|path| {
                    digest_preflight_file(app, &path, &|| {
                        connection
                            .query_row(
                                "SELECT status = 'cancelled' FROM preflight_scan_jobs WHERE id = ?1",
                                [job_id],
                                |row| row.get::<_, i64>(0),
                            )
                            .map(|value| value != 0)
                            .unwrap_or(true)
                    })
                    .map(|(bytes, digest)| (bytes, Some(digest)))
                })
        };
        match outcome {
            Ok((size_bytes, _digest)) if mode == "deep" && size_bytes == 0 => {
                connection.execute("UPDATE preflight_scan_files SET status = 'error', size_bytes = 0, error = 'Empty files cannot be compared.' WHERE job_id = ?1 AND ordinal = ?2", params![job_id, ordinal]).map_err(user_error)?;
                record_preflight_scan_event(
                    &connection,
                    job_id,
                    Some(&file_name),
                    "Skipped: empty files cannot be compared.",
                )?;
            }
            Ok((size_bytes, digest)) => {
                connection.execute("UPDATE preflight_scan_files SET status = 'complete', size_bytes = ?1, digest = ?2, error = NULL WHERE job_id = ?3 AND ordinal = ?4", params![size_bytes as i64, digest, job_id, ordinal]).map_err(user_error)?;
                record_preflight_scan_event(
                    &connection,
                    job_id,
                    Some(&file_name),
                    "Duplicate check completed.",
                )?;
            }
            Err(error) => {
                connection.execute("UPDATE preflight_scan_files SET status = 'error', error = ?1 WHERE job_id = ?2 AND ordinal = ?3", params![error, job_id, ordinal]).map_err(user_error)?;
                record_preflight_scan_event(
                    &connection,
                    job_id,
                    Some(&file_name),
                    "Duplicate check could not complete.",
                )?;
            }
        }
        connection.execute("UPDATE preflight_scan_jobs SET completed_files = completed_files + 1, updated_at = ?2 WHERE id = ?1", params![job_id, now()]).map_err(user_error)?;
    }
    let active: String = connection
        .query_row(
            "SELECT status FROM preflight_scan_jobs WHERE id = ?1",
            [job_id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if active == "cancelled" {
        return Ok(());
    }
    let active_channel_id = connection_settings(&connection)?
        .active_channel_id
        .unwrap_or_default();
    if mode == "deep" && !job_channel_id.is_empty() && active_channel_id != job_channel_id {
        connection.execute("UPDATE preflight_scan_jobs SET status = 'materializing', inventory_status = 'failed', detail = 'The active YouTube channel changed during this check. Local results were retained; restart the channel-specific check.', updated_at = ?1 WHERE id = ?2", params![now(), job_id]).map_err(user_error)?;
    } else if mode == "deep" && connection_settings(&connection)?.connected {
        connection.execute("UPDATE preflight_scan_jobs SET status = 'syncing', inventory_status = 'syncing', updated_at = ?1 WHERE id = ?2", params![now(), job_id]).map_err(user_error)?;
        drop(connection);
        match sync_channel_inventory_worker(state) {
            Ok(_) => database(state)?.execute("UPDATE preflight_scan_jobs SET status = 'materializing', inventory_status = 'complete', updated_at = ?1 WHERE id = ?2", params![now(), job_id]).map_err(user_error)?,
            Err(error) => database(state)?.execute("UPDATE preflight_scan_jobs SET status = 'materializing', inventory_status = 'failed', detail = ?1, updated_at = ?2 WHERE id = ?3", params![format!("YouTube titles were not refreshed: {error}"), now(), job_id]).map_err(user_error)?,
        };
    } else {
        let detail = if mode == "deep" {
            "Connect YouTube to refresh uploaded titles; local deep matching completed."
        } else {
            "Fast filename match completed. Deep matching is available on demand."
        };
        connection.execute("UPDATE preflight_scan_jobs SET status = 'materializing', detail = ?1, updated_at = ?2 WHERE id = ?3", params![detail, now(), job_id]).map_err(user_error)?;
    }
    materialize_preflight_match_evidence(state, job_id)?;
    database(state)?
        .execute(
            "UPDATE preflight_scan_jobs SET status = 'complete', updated_at = ?1 WHERE id = ?2",
            params![now(), job_id],
        )
        .map_err(user_error)?;
    audit_global(
        &database(state)?,
        "preflight_scan_completed",
        &format!("Completed persistent {mode} pre-ingest duplicate scan"),
    )?;
    if queue_metadata {
        queue_preflight_metadata_collection(state.clone(), job_id.to_string());
    }
    Ok(())
}

/// FFprobe enrichment is intentionally separate from duplicate matching. The
/// scan's filename or hash result is ready immediately; each source's optional
/// container metadata is collected once on a native worker and retained with
/// the resumable local job for later UI reads.
fn queue_preflight_metadata_collection(state: AppState, job_id: String) {
    spawn_worker(move || {
        let _ = run_preflight_metadata_collection(&state, &job_id);
    });
}

fn run_preflight_metadata_collection(state: &AppState, job_id: &str) -> Result<(), String> {
    let connection = database(state)?;
    loop {
        let next = connection
            .query_row(
                "SELECT ordinal, source_locator, file_name FROM preflight_scan_files WHERE job_id = ?1 AND status = 'complete' AND metadata_status = 'pending' ORDER BY ordinal ASC LIMIT 1",
                [job_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()
            .map_err(user_error)?;
        let Some((ordinal, locator, file_name)) = next else {
            break;
        };
        if connection
            .execute(
                "UPDATE preflight_scan_files SET metadata_status = 'running' WHERE job_id = ?1 AND ordinal = ?2 AND metadata_status = 'pending'",
                params![job_id, ordinal],
            )
            .map_err(user_error)?
            == 0
        {
            continue;
        }
        record_preflight_scan_event(
            &connection,
            job_id,
            Some(&file_name),
            "Reading media metadata in the background.",
        )?;
        let metadata = serde_json::from_str::<FilePath>(&locator)
            .map(|path| {
                preflight_local_metadata_with_cancel(&path, &file_name, true, &|| {
                    connection
                        .query_row(
                            "SELECT status = 'cancelled' FROM preflight_scan_jobs WHERE id = ?1",
                            [job_id],
                            |row| row.get::<_, i64>(0),
                        )
                        .map(|value| value != 0)
                        .unwrap_or(true)
                })
            })
            .unwrap_or_else(|_| unavailable_preflight_local_metadata(None));
        let cancelled = connection
            .query_row(
                "SELECT status = 'cancelled' FROM preflight_scan_jobs WHERE id = ?1",
                [job_id],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value != 0)
            .unwrap_or(true);
        if cancelled {
            connection
                .execute(
                    "UPDATE preflight_scan_files SET metadata_status = 'pending' WHERE job_id = ?1 AND ordinal = ?2 AND metadata_status = 'running'",
                    params![job_id, ordinal],
                )
                .map_err(user_error)?;
            return Ok(());
        }
        let metadata_json = serde_json::to_string(&metadata).map_err(user_error)?;
        connection
            .execute(
                "UPDATE preflight_scan_files SET metadata_json = ?1, metadata_status = 'complete' WHERE job_id = ?2 AND ordinal = ?3",
                params![metadata_json, job_id, ordinal],
            )
            .map_err(user_error)?;
        record_preflight_scan_event(
            &connection,
            job_id,
            Some(&file_name),
            "Media metadata recorded.",
        )?;
    }
    Ok(())
}

const PREFLIGHT_EVIDENCE_PREVIEW_LIMIT: usize = 4;
const PREFLIGHT_PAGE_LIMIT: u64 = 64;

fn materialize_preflight_match_evidence(state: &AppState, job_id: &str) -> Result<(), String> {
    let mut connection = database(state)?;
    let (mode, channel_id): (String, String) = connection
        .query_row(
            "SELECT mode, channel_id FROM preflight_scan_jobs WHERE id = ?1",
            [job_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(user_error)?;
    let remote_index = if channel_id.is_empty() {
        None
    } else {
        Some(RemoteTitleIndex::load(&connection, &channel_id)?)
    };
    let saved_upload_rows = connection
        .prepare(
            "SELECT title, file_name, status, digest FROM upload_items WHERE status = 'uploaded' AND COALESCE(channel_id, '') = ?1 ORDER BY created_at ASC",
        )
        .map_err(user_error)?
        .query_map([&channel_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut local_matches_by_key = HashMap::<String, Vec<PreIngestLocalMatch>>::new();
    for (title, file_name, status, digest) in saved_upload_rows {
        let key = if mode == "deep" {
            digest.map(|value| format!("digest:{value}"))
        } else {
            Some(format!("name:{}", file_name.to_ascii_lowercase()))
        };
        if let Some(key) = key {
            local_matches_by_key
                .entry(key)
                .or_default()
                .push(PreIngestLocalMatch {
                    title,
                    file_name,
                    status,
                });
        }
    }
    let rows = connection
        .prepare(
            "SELECT ordinal, source_locator, file_name, digest, error FROM preflight_scan_files WHERE job_id = ?1 ORDER BY ordinal ASC",
        )
        .map_err(user_error)?
        .query_map([job_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut dropped_names_by_key = HashMap::<String, Vec<(i64, String)>>::new();
    for (ordinal, _, file_name, digest, _) in &rows {
        let key = if mode == "deep" {
            digest.as_ref().map(|value| format!("digest:{value}"))
        } else {
            Some(format!("name:{}", file_name.to_ascii_lowercase()))
        };
        if let Some(key) = key {
            dropped_names_by_key
                .entry(key)
                .or_default()
                .push((*ordinal, file_name.clone()));
        }
    }

    let transaction = connection.transaction().map_err(user_error)?;
    let mut matched_files = 0_i64;
    {
        let mut update = transaction
            .prepare_cached(
                "UPDATE preflight_scan_files SET local_matches_json = ?1, dropped_duplicate_names_json = ?2, uploaded_title_matches_json = ?3, local_match_count = ?4, dropped_duplicate_count = ?5, uploaded_title_match_count = ?6, can_delete_local_duplicate = ?7 WHERE job_id = ?8 AND ordinal = ?9",
            )
            .map_err(user_error)?;
        for (ordinal, locator, file_name, digest, error) in rows {
            let match_key = if mode == "deep" {
                digest.as_ref().map(|value| format!("digest:{value}"))
            } else {
                Some(format!("name:{}", file_name.to_ascii_lowercase()))
            };
            let all_local_matches = match_key
                .as_ref()
                .and_then(|key| local_matches_by_key.get(key))
                .cloned()
                .unwrap_or_default();
            let all_dropped_names = match_key
                .as_ref()
                .and_then(|key| dropped_names_by_key.get(key))
                .map(|candidates| {
                    candidates
                        .iter()
                        .filter(|(other_ordinal, _)| *other_ordinal != ordinal)
                        .map(|(_, name)| name.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let title = Path::new(&file_name)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(&file_name);
            let all_uploaded_matches = remote_index
                .as_ref()
                .map(|index| {
                    index
                        .matches(title)
                        .into_iter()
                        .map(|candidate| PreIngestUploadedTitleMatch {
                            title: candidate.title.clone(),
                            duration: candidate.duration.clone(),
                            privacy_status: candidate.privacy_status.clone(),
                            updated_at: candidate.updated_at.clone(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let has_match = !all_local_matches.is_empty()
                || !all_dropped_names.is_empty()
                || !all_uploaded_matches.is_empty();
            if has_match {
                matched_files += 1;
            }
            let source_is_desktop_path = serde_json::from_str::<FilePath>(&locator)
                .ok()
                .and_then(|value| value.as_path().map(Path::to_path_buf))
                .is_some();
            let can_delete = error.is_none() && source_is_desktop_path && has_match;
            update
                .execute(params![
                    serde_json::to_string(
                        &all_local_matches
                            .iter()
                            .take(PREFLIGHT_EVIDENCE_PREVIEW_LIMIT)
                            .cloned()
                            .collect::<Vec<_>>()
                    )
                    .map_err(user_error)?,
                    serde_json::to_string(
                        &all_dropped_names
                            .iter()
                            .take(PREFLIGHT_EVIDENCE_PREVIEW_LIMIT)
                            .cloned()
                            .collect::<Vec<_>>()
                    )
                    .map_err(user_error)?,
                    serde_json::to_string(
                        &all_uploaded_matches
                            .iter()
                            .take(PREFLIGHT_EVIDENCE_PREVIEW_LIMIT)
                            .cloned()
                            .collect::<Vec<_>>()
                    )
                    .map_err(user_error)?,
                    all_local_matches.len() as i64,
                    all_dropped_names.len() as i64,
                    all_uploaded_matches.len() as i64,
                    can_delete as i64,
                    job_id,
                    ordinal
                ])
                .map_err(user_error)?;
        }
    }
    transaction
        .execute(
            "UPDATE preflight_scan_jobs SET matched_files = ?1, evidence_materialized_at = ?2, updated_at = ?2 WHERE id = ?3",
            params![matched_files, now(), job_id],
        )
        .map_err(user_error)?;
    transaction.commit().map_err(user_error)
}

fn load_preflight_scan_status(
    state: &AppState,
    job_id: &str,
) -> Result<PreIngestDuplicateScanStatus, String> {
    let connection = database(state)?;
    let (
        job_channel_id,
        mode,
        status,
        total_files,
        completed_files,
        inventory_status,
        detail,
        matched_files,
    ): (String, String, String, i64, i64, String, Option<String>, i64) = connection
        .query_row(
            "SELECT channel_id, mode, status, total_files, completed_files, inventory_status, detail, matched_files FROM preflight_scan_jobs WHERE id = ?1",
            [job_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .map_err(|error| {
            if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
                "This duplicate scan is no longer available.".to_string()
            } else {
                user_error(error)
            }
        })?;
    let active_channel_id = connection_settings(&connection)?
        .active_channel_id
        .unwrap_or_default();
    if active_channel_id != job_channel_id {
        return Err("This duplicate scan belongs to a different YouTube channel session. Start a new check for the active channel.".into());
    }
    let current_file_name = connection
        .query_row(
            "SELECT file_name FROM preflight_scan_files WHERE job_id = ?1 AND status = 'running' ORDER BY ordinal ASC LIMIT 1",
            [job_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(user_error)?;
    let pending_metadata_files = connection
        .query_row(
            "SELECT COUNT(*) FROM preflight_scan_files WHERE job_id = ?1 AND status = 'complete' AND metadata_status != 'complete'",
            [job_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)?;
    let activity_total = connection
        .query_row(
            "SELECT COUNT(*) FROM preflight_scan_events WHERE job_id = ?1",
            [job_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)?;
    Ok(PreIngestDuplicateScanStatus {
        id: job_id.to_string(),
        mode,
        status,
        total_files: total_files as u64,
        completed_files: completed_files as u64,
        current_file_name,
        pending_metadata_files: pending_metadata_files as u64,
        matched_files: matched_files as u64,
        activity_total: activity_total as u64,
        youtube_title_checked: inventory_status == "complete",
        youtube_check_detail: detail,
    })
}

fn load_preflight_scan_page(
    state: &AppState,
    job_id: &str,
    file_offset: u64,
    file_limit: u64,
    activity_offset: u64,
    activity_limit: u64,
) -> Result<PreIngestDuplicateScan, String> {
    let mut status = load_preflight_scan_status(state, job_id)?;
    if status.status == "complete" {
        let materialized = database(state)?
            .query_row(
                "SELECT evidence_materialized_at IS NOT NULL FROM preflight_scan_jobs WHERE id = ?1",
                [job_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(user_error)?
            != 0;
        if !materialized {
            materialize_preflight_match_evidence(state, job_id)?;
            status = load_preflight_scan_status(state, job_id)?;
        }
    }
    let connection = database(state)?;
    let file_limit = file_limit.clamp(1, PREFLIGHT_PAGE_LIMIT);
    let activity_limit = activity_limit.clamp(1, PREFLIGHT_PAGE_LIMIT);
    let files = connection
        .prepare(
            "SELECT ordinal, file_name, size_bytes, error, local_matches_json, dropped_duplicate_names_json, uploaded_title_matches_json, local_match_count, dropped_duplicate_count, uploaded_title_match_count, can_delete_local_duplicate FROM preflight_scan_files WHERE job_id = ?1 ORDER BY ordinal ASC LIMIT ?2 OFFSET ?3",
        )
        .map_err(user_error)?
        .query_map(params![job_id, file_limit as i64, file_offset as i64], |row| {
            let file_name = row.get::<_, String>(1)?;
            let local_matches_json = row.get::<_, String>(4)?;
            let dropped_names_json = row.get::<_, String>(5)?;
            let uploaded_matches_json = row.get::<_, String>(6)?;
            Ok(PreIngestDuplicateFile {
                local_delete_token: None,
                can_delete_local_duplicate: row.get::<_, i64>(10)? != 0,
                ordinal: row.get::<_, i64>(0)? as u64,
                file_name: file_name.clone(),
                size_bytes: row.get::<_, i64>(2)? as u64,
                local_metadata: unavailable_preflight_local_metadata(
                    Path::new(&file_name)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .map(|extension| extension.to_ascii_uppercase()),
                ),
                local_matches: serde_json::from_str(&local_matches_json).unwrap_or_default(),
                dropped_duplicate_file_names: serde_json::from_str(&dropped_names_json)
                    .unwrap_or_default(),
                uploaded_title_matches: serde_json::from_str(&uploaded_matches_json)
                    .unwrap_or_default(),
                local_match_count: row.get::<_, i64>(7)? as u64,
                dropped_duplicate_count: row.get::<_, i64>(8)? as u64,
                uploaded_title_match_count: row.get::<_, i64>(9)? as u64,
                error: row.get(3)?,
            })
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let activity_log = connection
        .prepare(
            "SELECT file_name, message, created_at FROM preflight_scan_events WHERE job_id = ?1 ORDER BY id ASC LIMIT ?2 OFFSET ?3",
        )
        .map_err(user_error)?
        .query_map(
            params![job_id, activity_limit as i64, activity_offset as i64],
            |row| {
                Ok(PreIngestActivityLogEntry {
                    file_name: row.get(0)?,
                    message: row.get(1)?,
                    created_at: row.get(2)?,
                })
            },
        )
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(PreIngestDuplicateScan {
        id: status.id,
        mode: status.mode,
        status: status.status,
        total_files: status.total_files,
        completed_files: status.completed_files,
        current_file_name: status.current_file_name,
        pending_metadata_files: status.pending_metadata_files,
        matched_files: status.matched_files,
        file_offset,
        file_limit,
        activity_offset,
        activity_limit,
        activity_total: status.activity_total,
        files,
        activity_log,
        youtube_title_checked: status.youtube_title_checked,
        youtube_check_detail: status.youtube_check_detail,
    })
}

fn load_preflight_file_metadata(
    state: &AppState,
    job_id: &str,
    ordinal: u64,
) -> Result<PreIngestLocalMetadata, String> {
    load_preflight_scan_status(state, job_id)?;
    let connection = database(state)?;
    let (file_name, metadata_json): (String, Option<String>) = connection
        .query_row(
            "SELECT file_name, metadata_json FROM preflight_scan_files WHERE job_id = ?1 AND ordinal = ?2",
            params![job_id, ordinal as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "This pre-ingest metadata entry is no longer available.".to_string())?;
    Ok(metadata_json
        .as_deref()
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_else(|| {
            unavailable_preflight_local_metadata(
                Path::new(&file_name)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_ascii_uppercase()),
            )
        }))
}

fn load_preflight_scan(state: &AppState, job_id: &str) -> Result<PreIngestDuplicateScan, String> {
    load_preflight_scan_page(state, job_id, 0, 48, 0, 48)
}

#[cfg(any())]
fn load_preflight_scan_legacy(
    state: &AppState,
    job_id: &str,
) -> Result<PreIngestDuplicateScan, String> {
    let connection = database(state)?;
    let (job_channel_id, mode, status, total_files, completed_files, inventory_status, detail): (String, String, String, i64, i64, String, Option<String>) = connection.query_row(
        "SELECT channel_id, mode, status, total_files, completed_files, inventory_status, detail FROM preflight_scan_jobs WHERE id = ?1", [job_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
    ).map_err(|error| if matches!(error, rusqlite::Error::QueryReturnedNoRows) { "This duplicate scan is no longer available.".to_string() } else { user_error(error) })?;
    let channel_id = connection_settings(&connection)?.active_channel_id;
    if channel_id.as_deref().unwrap_or_default() != job_channel_id {
        return Err("This duplicate scan belongs to a different YouTube channel session. Start a new check for the active channel.".into());
    }
    let rows = connection.prepare("SELECT ordinal, source_locator, file_name, size_bytes, digest, status, error, metadata_json FROM preflight_scan_files WHERE job_id = ?1 ORDER BY ordinal ASC").map_err(user_error)?
        .query_map([job_id], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, String>(5)?, row.get::<_, Option<String>>(6)?, row.get::<_, Option<String>>(7)?))).map_err(user_error)?
        .collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    let current_file_name = rows
        .iter()
        .find_map(|(_, _, file_name, _, _, status, _, _)| {
            (status == "running").then(|| file_name.clone())
        });
    let pending_metadata_files = connection
        .query_row(
            "SELECT COUNT(*) FROM preflight_scan_files WHERE job_id = ?1 AND status = 'complete' AND metadata_status != 'complete'",
            [job_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)? as u64;
    let activity_log = connection.prepare("SELECT file_name, message, created_at FROM preflight_scan_events WHERE job_id = ?1 ORDER BY id ASC LIMIT 512").map_err(user_error)?
        .query_map([job_id], |row| Ok(PreIngestActivityLogEntry { file_name: row.get(0)?, message: row.get(1)?, created_at: row.get(2)? })).map_err(user_error)?
        .collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    // A poll must be a bounded SQLite read. Preload the relevant local and
    // remote evidence once instead of issuing one database query per selected
    // file every 450ms.
    let saved_upload_rows = connection
        .prepare(
            "SELECT title, file_name, status, digest FROM upload_items WHERE status = 'uploaded'",
        )
        .map_err(user_error)?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut local_matches_by_key = HashMap::<String, Vec<PreIngestLocalMatch>>::new();
    for (title, file_name, status, digest) in saved_upload_rows {
        let key = if mode == "deep" {
            digest.map(|value| format!("digest:{value}"))
        } else {
            Some(format!("name:{}", file_name.to_ascii_lowercase()))
        };
        if let Some(key) = key {
            local_matches_by_key
                .entry(key)
                .or_default()
                .push(PreIngestLocalMatch {
                    title,
                    file_name,
                    status,
                });
        }
    }
    let uploaded_videos = channel_id
        .as_deref()
        .map(|channel| {
            connection
                .prepare("SELECT title, duration, privacy_status, updated_at FROM remote_videos WHERE channel_id = ?1 AND upload_status = 'processed' ORDER BY video_id ASC")
                .map_err(user_error)?
                .query_map([channel], |row| {
                    Ok(PreIngestUploadedTitleMatch {
                        title: row.get(0)?,
                        duration: row.get(1)?,
                        privacy_status: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                })
                .map_err(user_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(user_error)
        })
        .transpose()?
        .unwrap_or_default();
    let mut dropped_names_by_key = HashMap::<String, Vec<(i64, String)>>::new();
    for (ordinal, _, file_name, _, digest, _, _, _) in &rows {
        let key = if mode == "deep" {
            digest.as_ref().map(|value| format!("digest:{value}"))
        } else {
            Some(format!("name:{}", file_name.to_ascii_lowercase()))
        };
        if let Some(key) = key {
            dropped_names_by_key
                .entry(key)
                .or_default()
                .push((*ordinal, file_name.clone()));
        }
    }
    let mut files = Vec::new();
    for (ordinal, locator, file_name, size_bytes, digest, _file_status, error, metadata_json) in
        &rows
    {
        let title = Path::new(file_name)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(file_name);
        let local_metadata = metadata_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<PreIngestLocalMetadata>(value).ok())
            // Metadata probing can involve a large ISO-BMFF traversal. It is
            // deliberately performed by the separate metadata worker, never
            // by this frequently-polled UI snapshot command.
            .unwrap_or_else(|| {
                unavailable_preflight_local_metadata(
                    Path::new(file_name)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .map(|extension| extension.to_ascii_uppercase()),
                )
            });
        let match_key = if mode == "deep" {
            digest.as_ref().map(|value| format!("digest:{value}"))
        } else {
            Some(format!("name:{}", file_name.to_ascii_lowercase()))
        };
        let local_matches = match_key
            .as_ref()
            .and_then(|key| local_matches_by_key.get(key))
            .cloned()
            .unwrap_or_default();
        let dropped_duplicate_file_names = match_key
            .as_ref()
            .and_then(|key| dropped_names_by_key.get(key))
            .map(|candidates| {
                candidates
                    .iter()
                    .filter(|(other_ordinal, _)| other_ordinal != ordinal)
                    .map(|(_, other_name)| other_name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let uploaded_title_matches: Vec<PreIngestUploadedTitleMatch> = uploaded_videos
            .iter()
            .filter(|candidate| uploaded_titles_match(title, &candidate.title))
            .cloned()
            .collect();
        let source_is_desktop_path = serde_json::from_str::<FilePath>(locator)
            .ok()
            .and_then(|value| value.as_path().map(Path::to_path_buf));
        let can_delete_local_duplicate = source_is_desktop_path.is_some()
            && (!local_matches.is_empty() || !uploaded_title_matches.is_empty());
        files.push(PreIngestDuplicateFile {
            // Creating a deletion token hashes the whole source so the later
            // destructive action has a fresh content binding. Do that only
            // after the operator explicitly chooses Delete local file.
            local_delete_token: None,
            can_delete_local_duplicate,
            ordinal: *ordinal as u64,
            file_name: file_name.clone(),
            size_bytes: *size_bytes as u64,
            local_metadata,
            local_matches,
            dropped_duplicate_file_names,
            uploaded_title_matches,
            error: error.clone(),
        });
    }
    Ok(PreIngestDuplicateScan {
        id: job_id.to_string(),
        mode,
        status,
        total_files: total_files as u64,
        completed_files: completed_files as u64,
        current_file_name,
        pending_metadata_files,
        files,
        activity_log,
        youtube_title_checked: inventory_status == "complete",
        youtube_check_detail: detail,
    })
}

fn resume_preflight_scan_jobs(state: AppState, app: AppHandle) {
    let (jobs, metadata_jobs) = database(&state).and_then(|connection| {
        let channel_id = connection_settings(&connection)?.active_channel_id.unwrap_or_default();
        connection.execute("UPDATE preflight_scan_jobs SET status = 'queued', completed_files = (SELECT COUNT(*) FROM preflight_scan_files WHERE job_id = preflight_scan_jobs.id AND status IN ('complete', 'error')), inventory_status = CASE WHEN inventory_status = 'syncing' THEN 'pending' ELSE inventory_status END, updated_at = ?1 WHERE status IN ('queued', 'running', 'syncing') AND channel_id = ?2", params![now(), &channel_id]).map_err(user_error)?;
        connection.execute("UPDATE preflight_scan_files SET metadata_status = 'pending' WHERE metadata_status = 'running'", []).map_err(user_error)?;
        let jobs = connection.prepare("SELECT id FROM preflight_scan_jobs WHERE status = 'queued' AND channel_id = ?1 ORDER BY created_at ASC").map_err(user_error)?
            .query_map([&channel_id], |row| row.get::<_, String>(0)).map_err(user_error)?
            .collect::<Result<Vec<_>, _>>().map_err(user_error)?;
        let metadata_jobs = connection.prepare("SELECT DISTINCT files.job_id FROM preflight_scan_files AS files JOIN preflight_scan_jobs AS jobs ON jobs.id = files.job_id WHERE files.status = 'complete' AND files.metadata_status = 'pending' AND jobs.channel_id = ?1 ORDER BY files.job_id ASC").map_err(user_error)?
            .query_map([&channel_id], |row| row.get::<_, String>(0)).map_err(user_error)?
            .collect::<Result<Vec<_>, _>>().map_err(user_error)?;
        Ok((jobs, metadata_jobs))
    }).unwrap_or_default();
    if !jobs.is_empty() || !metadata_jobs.is_empty() {
        let worker_state = state;
        spawn_worker(move || {
            for job_id in jobs {
                let _ = run_preflight_scan_job(&worker_state, &app, &job_id, false);
            }
            let mut bounded_metadata_jobs = metadata_jobs.into_iter().collect::<BTreeSet<_>>();
            if let Ok(pending) = database(&worker_state).and_then(|connection| {
                connection
                    .prepare("SELECT DISTINCT job_id FROM preflight_scan_files WHERE status = 'complete' AND metadata_status = 'pending' ORDER BY job_id ASC")
                    .map_err(user_error)?
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(user_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(user_error)
            }) {
                bounded_metadata_jobs.extend(pending);
            }
            for job_id in bounded_metadata_jobs {
                let _ = run_preflight_metadata_collection(&worker_state, &job_id);
            }
        });
    }
}

/// Opens and streams one picker or drag-drop file through the platform-aware
/// filesystem layer. On iOS, release the temporary security-scoped picker handle
/// as soon as this non-ingesting comparison finishes.
fn digest_preflight_file(
    app: &AppHandle,
    path: &FilePath,
    cancelled: &impl Fn() -> bool,
) -> Result<(u64, String), String> {
    if let Some(source) = path.as_path() {
        return digest_file_scheduled(source, MediaIoPriority::Background, cancelled)
            .map_err(|_| "This file could not be read completely.".to_string());
    }
    let mut options = FsOpenOptions::new();
    options.read(true);
    let mut file = app
        .fs()
        .open(path.clone(), options)
        .map_err(|_| "This file could not be opened for local comparison.".to_string())?;
    let result = digest_reader_for_path(
        &mut file,
        Path::new("platform-picker"),
        MediaIoPriority::Background,
        cancelled,
    )
    .map_err(|_| "This file could not be read completely.".to_string());
    drop(file);
    #[cfg(target_os = "ios")]
    if matches!(path, FilePath::Url(url) if url.scheme() == "file") {
        let _ = app
            .fs()
            .stop_accessing_security_scoped_resource(path.clone());
    }
    result
}

fn register_preflight_local_delete_target(
    media_directory: &Path,
    source_path: Option<&Path>,
    file_name: &str,
) -> Option<String> {
    let source_path = source_path?;
    if has_link_or_reparse_component(source_path).ok()? {
        return None;
    }
    let source_path = source_path.canonicalize().ok()?;
    let managed_directory = media_directory.canonicalize().ok()?;
    if source_path.starts_with(&managed_directory) {
        return None;
    }
    let signature = monitored_file_signature(&fs::metadata(&source_path).ok()?).ok()?;
    let (_, digest) = digest_file(&source_path).ok()?;
    let token = Uuid::new_v4().to_string();
    preflight_local_delete_targets().lock().ok()?.insert(
        token.clone(),
        PreflightLocalDeleteTarget {
            path: source_path,
            file_name: file_name.to_string(),
            signature,
            digest,
            created_at: Instant::now(),
        },
    );
    Some(token)
}

/// Creates a short-lived deletion target after re-reading the persisted opt-in
/// duplicate review. The token records a fresh local content binding before
/// any later permanent deletion.
fn prepare_preflight_local_delete_target(
    state: &AppState,
    job_id: &str,
    ordinal: u64,
) -> Result<String, String> {
    let connection = database(state)?;
    let (mode, locator, file_name, digest, error): (String, String, String, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT jobs.mode, files.source_locator, files.file_name, files.digest, files.error FROM preflight_scan_files AS files JOIN preflight_scan_jobs AS jobs ON jobs.id = files.job_id WHERE files.job_id = ?1 AND files.ordinal = ?2",
            params![job_id, ordinal as i64],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|_| "This duplicate review entry is no longer available. Run the check again.".to_string())?;
    if error.is_some() {
        return Err(
            "This file could not be checked, so it cannot be deleted from duplicate review.".into(),
        );
    }
    let has_local_match = if mode == "deep" {
        digest
            .as_deref()
            .map(|value| {
                connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM upload_items WHERE digest = ?1 AND status = 'uploaded')",
                        [value],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|value| value != 0)
                    .map_err(user_error)
            })
            .transpose()?
            .unwrap_or(false)
    } else {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM upload_items WHERE lower(file_name) = lower(?1) AND status = 'uploaded')",
                [&file_name],
                |row| row.get::<_, i64>(0),
            )
            .map(|value| value != 0)
            .map_err(user_error)?
    };
    let title = Path::new(&file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(&file_name);
    let has_uploaded_title_match = connection_settings(&connection)?
        .active_channel_id
        .as_deref()
        .map(|channel| matching_uploaded_titles(&connection, channel, title))
        .transpose()?
        .is_some_and(|matches| !matches.is_empty());
    if !has_local_match && !has_uploaded_title_match {
        return Err(
            "This selected file no longer has a saved local or uploaded-title match to review."
                .into(),
        );
    }
    let path = serde_json::from_str::<FilePath>(&locator)
        .ok()
        .and_then(|value| value.as_path().map(Path::to_path_buf))
        .ok_or_else(|| {
            "Local duplicate deletion is available only for a desktop filesystem source."
                .to_string()
        })?;
    register_preflight_local_delete_target(&state.media_directory, Some(&path), &file_name)
        .ok_or_else(|| "This source cannot be safely deleted from duplicate review.".to_string())
}

fn delete_preflight_duplicate_file_impl(
    state: &AppState,
    token: &str,
    confirmation: &str,
) -> Result<(), String> {
    let target = preflight_local_delete_targets()
        .lock()
        .map_err(|_| {
            "The local duplicate review is unavailable; scan the files again.".to_string()
        })?
        .get(token)
        .cloned()
        .ok_or_else(|| {
            "This local duplicate review has expired. Drop the files again before deleting."
                .to_string()
        })?;
    if target.created_at.elapsed() > Duration::from_secs(15 * 60) {
        preflight_local_delete_targets()
            .lock()
            .ok()
            .and_then(|mut targets| targets.remove(token));
        return Err(
            "This local duplicate review has expired. Drop the files again before deleting.".into(),
        );
    }
    if confirmation != target.file_name {
        return Err(
            "Type the exact file name before permanently deleting this local duplicate.".into(),
        );
    }
    let managed_directory = state.media_directory.canonicalize().map_err(user_error)?;
    if has_link_or_reparse_component(&target.path).unwrap_or(true) {
        return Err("The selected local file is linked or unavailable; it was not deleted.".into());
    }
    let current_path = target.path.canonicalize().map_err(|_| {
        "The selected local file is no longer available; it was not deleted.".to_string()
    })?;
    if current_path.starts_with(&managed_directory) {
        return Err("Managed upload copies cannot be deleted from duplicate review.".into());
    }
    let current_signature =
        monitored_file_signature(&fs::metadata(&current_path).map_err(user_error)?)?;
    if current_signature != target.signature {
        return Err("The selected local file changed after review; it was not deleted.".into());
    }
    let staged = stage_file_for_deletion(&current_path)?;
    let staged_signature = match fs::metadata(&staged)
        .map_err(user_error)
        .and_then(|metadata| monitored_file_signature(&metadata))
    {
        Ok(signature) => signature,
        Err(error) => {
            restore_staged_file(&staged, &current_path);
            return Err(error);
        }
    };
    if staged_signature != target.signature {
        restore_staged_file(&staged, &current_path);
        return Err("The selected local file changed while deletion was being prepared; it was not deleted.".into());
    }
    let staged_digest = match digest_file(&staged) {
        Ok((_, digest)) => digest,
        Err(error) => {
            restore_staged_file(&staged, &current_path);
            return Err(error);
        }
    };
    if staged_digest != target.digest {
        restore_staged_file(&staged, &current_path);
        return Err(
            "The selected local file no longer matches the reviewed content; it was not deleted."
                .into(),
        );
    }
    fs::remove_file(&staged).map_err(user_error)?;
    preflight_local_delete_targets()
        .lock()
        .ok()
        .and_then(|mut targets| targets.remove(token));
    audit_global(
        &database(state)?,
        "preflight_local_duplicate_deleted",
        "Operator permanently deleted a locally reviewed duplicate after filename confirmation",
    )?;
    Ok(())
}

fn resolve_upload_title_duplicates_impl(
    state: &AppState,
    item_ids: &[String],
    action: &str,
) -> Result<Vec<UploadItem>, String> {
    if item_ids.is_empty() {
        return Err("Choose at least one duplicate title to resolve.".into());
    }
    if !matches!(action, "ignore" | "skip") {
        return Err("Duplicate handling must be either ignore or skip.".into());
    }
    let connection = database(state)?;
    let mut resolved = Vec::new();
    for item_id in item_ids {
        let changed = if action == "ignore" {
            connection.execute(
                "UPDATE upload_items SET duplicate_decision = 'ignore', status = 'draft', detail = 'Operator chose to upload despite a matching YouTube title.', updated_at = ?1 WHERE id = ?2 AND duplicate_decision = 'pending'",
                params![now(), item_id],
            )
        } else {
            connection.execute(
                "UPDATE upload_items SET duplicate_decision = 'skip', status = 'cancelled', detail = 'Skipped because its title already exists on YouTube.', updated_at = ?1 WHERE id = ?2 AND duplicate_decision = 'pending'",
                params![now(), item_id],
            )
        }
        .map_err(user_error)?;
        if changed == 0 {
            continue;
        }
        audit(
            &connection,
            item_id,
            if action == "ignore" {
                "upload_title_duplicate_ignored"
            } else {
                "upload_title_duplicate_skipped"
            },
            if action == "ignore" {
                "Operator approved upload despite matching title"
            } else {
                "Operator skipped matching title"
            },
        )?;
        resolved.push(find_item(&connection, item_id)?);
    }
    Ok(resolved)
}

fn uploaded_title_duplicates(
    connection: &Connection,
    channel_id: &str,
) -> Result<Vec<DuplicateCandidate>, String> {
    ensure_remote_title_keys(connection, channel_id)?;
    let mut statement = connection
        .prepare(
            "SELECT video_id, title, normalized_title, canonical_title, has_copy_marker, numeric_title_key FROM remote_videos WHERE channel_id = ?1 AND upload_status = 'processed' ORDER BY video_id ASC",
        )
        .map_err(user_error)?;
    let videos = statement
        .query_map([channel_id], |row| {
            let video_id = row.get::<_, String>(0)?;
            let title = row.get::<_, String>(1)?;
            Ok((
                video_id,
                title,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)? != 0,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;

    // Generate only pairs which share one of the exact matching signatures.
    // The prior all-pairs loop compared every video to every later video, which
    // made a unique 10,000-video library take minutes even though it produced
    // almost no candidates. This remains O(n + candidate_count), with the same
    // deterministic video-id ordering and the same final evidence function.
    let mut exact_groups = HashMap::<String, Vec<usize>>::new();
    let mut canonical_groups = HashMap::<String, Vec<(usize, bool)>>::new();
    let mut number_groups = HashMap::<Vec<String>, Vec<usize>>::new();
    for (index, (_, _, normalized, canonical, has_duplicate_suffix, numeric)) in
        videos.iter().enumerate()
    {
        exact_groups
            .entry(normalized.clone())
            .or_default()
            .push(index);
        canonical_groups
            .entry(canonical.clone())
            .or_default()
            .push((index, *has_duplicate_suffix));
        if !numeric.is_empty() {
            number_groups
                .entry(numeric.split('\u{1f}').map(str::to_string).collect())
                .or_default()
                .push(index);
        }
    }

    let mut matching_pairs = BTreeSet::<(usize, usize)>::new();
    for indices in exact_groups.values() {
        for (offset, left) in indices.iter().enumerate() {
            for right in indices.iter().skip(offset + 1) {
                matching_pairs.insert((*left, *right));
            }
        }
    }
    for indices in canonical_groups.values() {
        for (offset, (left, left_has_suffix)) in indices.iter().enumerate() {
            for (right, right_has_suffix) in indices.iter().skip(offset + 1) {
                if *left_has_suffix || *right_has_suffix {
                    matching_pairs.insert((*left, *right));
                }
            }
        }
    }
    for indices in number_groups.values() {
        for (offset, left) in indices.iter().enumerate() {
            for right in indices.iter().skip(offset + 1) {
                matching_pairs.insert((*left, *right));
            }
        }
    }

    let mut candidates = Vec::with_capacity(matching_pairs.len());
    for (left_index, right_index) in matching_pairs {
        let (left_id, left_title, ..) = &videos[left_index];
        let (right_id, right_title, ..) = &videos[right_index];
        let Some(evidence) = uploaded_title_match_evidence(left_title, right_title) else {
            continue;
        };
        candidates.push(DuplicateCandidate {
            id: format!("remote:{left_id}:{right_id}"),
            confidence: "metadata".into(),
            left_title: left_title.clone(),
            right_title: right_title.clone(),
            left_video_id: Some(left_id.clone()),
            right_video_id: Some(right_id.clone()),
            evidence: evidence.into(),
            decision: None,
        });
    }
    Ok(candidates)
}

fn audit(connection: &Connection, item_id: &str, kind: &str, detail: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO audit_events (id, item_id, channel_name, channel_id, kind, detail, created_at) VALUES (?1, ?2, (SELECT channel_name FROM upload_items WHERE id = ?2), COALESCE((SELECT channel_id FROM upload_items WHERE id = ?2), ''), ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), item_id, kind, detail, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn audit_global(connection: &Connection, kind: &str, detail: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO audit_events (id, item_id, kind, detail, created_at) VALUES (?1, NULL, ?2, ?3, ?4)",
            params![Uuid::new_v4().to_string(), kind, detail, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn audit_global_scoped(
    connection: &Connection,
    channel_name: &str,
    kind: &str,
    detail: &str,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO audit_events (id, item_id, channel_name, channel_id, kind, detail, created_at) VALUES (?1, NULL, ?2, COALESCE((SELECT channel_id FROM folder_monitor_settings WHERE channel_name = ?2), (SELECT active_channel_id FROM connection_settings WHERE active_channel = ?2), ''), ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), channel_name, kind, detail, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn digest_file_with_upload_priority(
    state: &AppState,
    path: &Path,
) -> Result<(u64, String), String> {
    let connection = database(state)?;
    let source_path = path.to_string_lossy().to_string();
    let cancellation = ThrottledCancellationCheck::new(Duration::from_millis(250), || {
        connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM upload_items WHERE (source_path = ?1 OR workspace_path = ?1) AND status = 'cancelled')",
                [&source_path],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(1)
            != 0
    });
    digest_file_scheduled(path, MediaIoPriority::Background, &|| {
        cancellation.is_cancelled()
    })
}

fn preflight_file_name(path: &FilePath) -> String {
    match path {
        FilePath::Path(path) => path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Unnamed file")
            .to_string(),
        FilePath::Url(url) => url
            .path_segments()
            .and_then(|segments| segments.last())
            .filter(|value| !value.is_empty())
            .unwrap_or("Selected file")
            .to_string(),
    }
}

fn metadata_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        value => serde_json::to_string(value).ok(),
    }
}

fn metadata_fields(value: &serde_json::Value) -> Vec<PreIngestMetadataField> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    for (label, value) in object {
        if label == "tags" {
            if let Some(tags) = value.as_object() {
                for (tag, value) in tags {
                    if let Some(value) = metadata_value(value) {
                        fields.push(PreIngestMetadataField {
                            label: format!("Tag · {tag}"),
                            value,
                        });
                    }
                }
            }
        } else if let Some(value) = metadata_value(value) {
            fields.push(PreIngestMetadataField {
                label: label.replace('_', " "),
                value,
            });
        }
    }
    fields
}

fn ffprobe_metadata(
    path: &Path,
) -> Option<(
    Option<f64>,
    Option<String>,
    Option<String>,
    Vec<PreIngestMetadataStream>,
    Vec<PreIngestMetadataField>,
)> {
    ffprobe_metadata_with_cancel(path, &|| false)
}

fn ffprobe_metadata_with_cancel(
    path: &Path,
    cancelled: &impl Fn() -> bool,
) -> Option<(
    Option<f64>,
    Option<String>,
    Option<String>,
    Vec<PreIngestMetadataStream>,
    Vec<PreIngestMetadataField>,
)> {
    let report = cached_ffprobe_report(path, ProbeDepth::Rich, cancelled)?.rich_report?;
    let format = report
        .get("format")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let duration_seconds = probe_duration_from_report(&report);
    let container_format = format
        .get("format_long_name")
        .or_else(|| format.get("format_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let bit_rate = format
        .get("bit_rate")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let streams = report
        .get("streams")
        .and_then(serde_json::Value::as_array)
        .map(|streams| {
            streams
                .iter()
                .take(64)
                .enumerate()
                .map(|(index, stream)| {
                    let kind = stream
                        .get("codec_type")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("data")
                        .to_string();
                    let codec = stream
                        .get("codec_long_name")
                        .or_else(|| stream.get("codec_name"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Unknown codec");
                    PreIngestMetadataStream {
                        kind: kind.clone(),
                        label: format!(
                            "{} stream {} · {}",
                            kind.to_ascii_uppercase(),
                            index + 1,
                            codec
                        ),
                        fields: metadata_fields(stream),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Some((
        duration_seconds,
        container_format,
        bit_rate,
        streams,
        metadata_fields(&format),
    ))
}

/// Resolves the Tauri-bundled FFprobe sidecar before consulting a developer's
/// PATH. This keeps metadata inspection available on clean desktop installs
/// without accepting a path from the webview.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn preflight_local_metadata(
    path: &FilePath,
    file_name: &str,
    include_deep_metadata: bool,
) -> PreIngestLocalMetadata {
    preflight_local_metadata_with_cancel(path, file_name, include_deep_metadata, &|| false)
}

fn preflight_local_metadata_with_cancel(
    path: &FilePath,
    file_name: &str,
    include_deep_metadata: bool,
    cancelled: &impl Fn() -> bool,
) -> PreIngestLocalMetadata {
    let file_type = Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_uppercase());
    let Some(source) = path.as_path() else {
        return unavailable_preflight_local_metadata(file_type);
    };
    let Ok(metadata) = fs::metadata(source) else {
        return unavailable_preflight_local_metadata(file_type);
    };
    let modified_at = metadata
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
        .map(|value| value.to_rfc3339());
    let container_duration = matches!(
        file_type.as_deref(),
        Some("MP4") | Some("M4V") | Some("MOV") | Some("INSV") | Some("LRV")
    )
    .then(|| iso_bmff_duration_seconds(source))
    .flatten();
    if !include_deep_metadata {
        return PreIngestLocalMetadata {
            file_type,
            modified_at,
            duration_seconds: container_duration,
            size_bytes: Some(metadata.len()),
            container_format: None,
            bit_rate: None,
            streams: Vec::new(),
            metadata_fields: Vec::new(),
        };
    }
    let (probe_duration, container_format, bit_rate, streams, metadata_fields) =
        ffprobe_metadata_with_cancel(source, cancelled).unwrap_or((
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ));
    let duration_seconds = probe_duration.or(container_duration);
    PreIngestLocalMetadata {
        file_type,
        modified_at,
        duration_seconds,
        size_bytes: Some(metadata.len()),
        container_format,
        bit_rate,
        streams,
        metadata_fields,
    }
}

/// Desktop uses the bundled FFprobe sidecar for broad container coverage;
/// mobile uses ISO-BMFF metadata where it is available without a sidecar.
fn upload_duration_seconds(path: &Path) -> Option<f64> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_uppercase);
    let container_duration = matches!(
        extension.as_deref(),
        Some("MP4") | Some("M4V") | Some("MOV") | Some("INSV") | Some("LRV")
    )
    .then(|| iso_bmff_duration_seconds(path))
    .flatten();
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        container_duration
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        container_duration.or_else(|| {
            cached_ffprobe_report(path, ProbeDepth::Duration, &|| false)
                .and_then(|cached| cached.duration_seconds)
                .filter(|duration| duration.is_finite() && *duration >= 0.0)
        })
    }
}

fn youtube_upload_size_limit_error(size_bytes: u64) -> Option<String> {
    (size_bytes > YOUTUBE_MAX_UPLOAD_BYTES).then(|| {
        "This video is too large for YouTube. YouTube accepts files up to 256 GB; compress or split it before importing."
            .to_string()
    })
}

fn youtube_upload_duration_limit_error(duration_seconds: Option<f64>) -> Option<String> {
    duration_seconds
        .filter(|duration| duration.is_finite() && *duration > YOUTUBE_MAX_UPLOAD_DURATION_SECONDS)
        .map(|_| {
            "This video is too long for YouTube. YouTube accepts videos up to 12 hours; split it before importing."
                .to_string()
        })
}

/// Rejects provider-impossible media before a managed copy, queue record, or
/// watched-folder dispatch is created. The native boundary covers every UI.
fn validate_youtube_upload_limits(path: &Path, size_bytes: u64) -> Result<(), String> {
    if let Some(error) = youtube_upload_size_limit_error(size_bytes) {
        return Err(error);
    }
    if let Some(error) = youtube_upload_duration_limit_error(upload_duration_seconds(path)) {
        return Err(error);
    }
    Ok(())
}

/// Opens an operator-owned source without following a final link. Uploads keep
/// this original file as their only media copy.
fn open_reference_source(path: &Path) -> Result<File, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    options.custom_flags(0x0020_0000 | 0x0800_0000); // OPEN_REPARSE_POINT + SEQUENTIAL_SCAN
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let source = options.open(path).map_err(|_| {
        "The original upload source is unavailable or cannot be opened safely.".to_string()
    })?;
    let metadata = source.metadata().map_err(user_error)?;
    #[cfg(windows)]
    if metadata.file_attributes() & 0x400 != 0 {
        return Err("A linked upload source cannot be used directly.".into());
    }
    if !metadata.is_file() {
        return Err("The selected upload source is not a regular file.".into());
    }
    Ok(source)
}

fn digest_open_reference_source(source: &mut File) -> Result<(u64, String), String> {
    source.seek(SeekFrom::Start(0)).map_err(user_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; HASH_READ_BUFFER_BYTES];
    loop {
        let read = source.read(&mut buffer).map_err(user_error)?;
        if read == 0 {
            break;
        }
        bytes = bytes.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    source.seek(SeekFrom::Start(0)).map_err(user_error)?;
    Ok((bytes, hasher.finalize().to_hex().to_string()))
}

fn verify_reference_source(
    path: &Path,
    expected_signature: Option<&(u64, String)>,
) -> Result<((u64, String), String), String> {
    let mut source = open_reference_source(path)?;
    let signature = monitored_file_signature(&source.metadata().map_err(user_error)?)?;
    if expected_signature.is_some_and(|expected| expected != &signature) {
        return Err("The original upload source changed after it was reviewed.".into());
    }
    let (bytes, digest) = digest_open_reference_source(&mut source)?;
    let final_signature = monitored_file_signature(&source.metadata().map_err(user_error)?)?;
    if signature != final_signature || bytes != signature.0 {
        return Err("The original upload source changed while it was being verified.".into());
    }
    Ok((signature, digest))
}

struct MonitoredFile {
    path: PathBuf,
    file_name: String,
    size_bytes: u64,
    modified_key: String,
}

enum MonitorDisposition {
    Queued,
    Skipped,
    Waiting,
    Paused,
}

struct MonitorFileOutcome {
    disposition: MonitorDisposition,
    dispatch_item_id: Option<String>,
}

fn folder_monitor_settings(connection: &Connection) -> Result<FolderMonitorSettings, String> {
    connection
        .query_row(
            "SELECT enabled, folder_path, channel_name, channel_id, visibility, made_for_kids, delete_source_after_upload, playlist_id, playlist_title, status, detail, last_scan_at, last_file_name FROM folder_monitor_settings WHERE singleton = 1",
            [],
            |row| {
                Ok(FolderMonitorSettings {
                    enabled: row.get::<_, i64>(0)? != 0,
                    folder_path: row.get(1)?,
                    channel_name: row.get(2)?,
                    channel_id: row.get(3)?,
                    visibility: row.get(4)?,
                    made_for_kids: row.get::<_, i64>(5)? != 0,
                    delete_source_after_upload: row.get::<_, i64>(6)? != 0,
                    playlist_id: row.get(7)?,
                    playlist_title: row.get(8)?,
                    status: row.get(9)?,
                    detail: row.get(10)?,
                    last_scan_at: row.get(11)?,
                    last_file_name: row.get(12)?,
                })
            },
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(FolderMonitorSettings {
                enabled: false,
                folder_path: None,
                channel_name: None,
                channel_id: None,
                visibility: "private".into(),
                made_for_kids: false,
                delete_source_after_upload: false,
                playlist_id: None,
                playlist_title: None,
                status: "disabled".into(),
                detail: "Folder monitoring is disabled.".into(),
                last_scan_at: None,
                last_file_name: None,
            }),
            other => Err(other),
        })
        .map_err(user_error)
}

fn folder_monitor_overview(connection: &Connection) -> Result<FolderMonitorOverview, String> {
    let settings = folder_monitor_settings(connection)?;
    let Some(channel_id) = settings.channel_id.as_deref() else {
        return Ok(FolderMonitorOverview {
            settings,
            files: Vec::new(),
            logs: Vec::new(),
        });
    };
    let files = connection
        .prepare("SELECT observations.file_path, observations.state, observations.size_bytes, observations.updated_at, uploads.title, uploads.status, uploads.confirmed_bytes, uploads.total_bytes, uploads.detail FROM folder_monitor_observations AS observations LEFT JOIN upload_items AS uploads ON uploads.id = observations.upload_item_id WHERE observations.channel_id = ?1 ORDER BY observations.updated_at DESC LIMIT 200")
        .map_err(user_error)?
        .query_map([channel_id], |row| {
            let file_path = row.get::<_, String>(0)?;
            Ok(FolderMonitorFileActivity {
                file_name: Path::new(&file_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("Unnamed file")
                    .to_string(),
                observation_state: row.get(1)?,
                size_bytes: row.get::<_, i64>(2)? as u64,
                updated_at: row.get(3)?,
                upload_title: row.get(4)?,
                upload_status: row.get(5)?,
                confirmed_bytes: row.get::<_, Option<i64>>(6)?.map(|value| value as u64),
                total_bytes: row.get::<_, Option<i64>>(7)?.map(|value| value as u64),
                detail: row.get(8)?,
            })
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let logs = connection
        .prepare("SELECT kind, detail, created_at FROM audit_events WHERE channel_id = ?1 AND kind LIKE 'folder_monitor_%' ORDER BY created_at DESC LIMIT 200")
        .map_err(user_error)?
        .query_map([channel_id], |row| {
            Ok(FolderMonitorLogEntry {
                kind: row.get(0)?,
                detail: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(FolderMonitorOverview {
        settings,
        files,
        logs,
    })
}

fn monitored_file_signature(metadata: &fs::Metadata) -> Result<(u64, String), String> {
    let modified = metadata.modified().map_err(user_error)?;
    let duration = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "A watched file has an unsupported modification time.".to_string())?;
    Ok((
        metadata.len(),
        format!("{}:{}", duration.as_secs(), duration.subsec_nanos()),
    ))
}

fn is_hidden_monitored_file(path: &Path, metadata: &fs::Metadata) -> bool {
    if path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.starts_with('.'))
    {
        return true;
    }
    #[cfg(windows)]
    if metadata.file_attributes() & 0x2 != 0 {
        return true;
    }
    false
}

fn is_temporary_monitored_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    if lower.starts_with('~') || lower.ends_with('~') {
        return true;
    }
    let mut segments = lower.split('.').collect::<Vec<_>>();
    let _ = segments.pop();
    segments.iter().any(|segment| {
        matches!(
            *segment,
            "tmp" | "temp" | "part" | "partial" | "download" | "crdownload"
        )
    })
}

fn is_supported_monitored_video(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| SUPPORTED_VIDEO_EXTENSIONS.contains(&extension.as_str()))
}

fn monitored_files(folder: &Path) -> Result<Vec<MonitoredFile>, String> {
    let mut files = Vec::new();
    for result in fs::read_dir(folder).map_err(user_error)? {
        let entry = result.map_err(user_error)?;
        let file_type = entry.file_type().map_err(user_error)?;
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let file_name = file_name.to_string();
        let metadata = entry.metadata().map_err(user_error)?;
        if is_hidden_monitored_file(&path, &metadata)
            || is_temporary_monitored_name(&file_name)
            || !is_supported_monitored_video(&path)
        {
            continue;
        }
        let (size_bytes, modified_key) = monitored_file_signature(&metadata)?;
        if size_bytes == 0 {
            continue;
        }
        files.push(MonitoredFile {
            path,
            file_name,
            size_bytes,
            modified_key,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

/// Validate a watched source through a no-follow handle and retain its path as
/// the upload source. This intentionally creates no app-managed media copy.
fn reference_watched_source(
    source_path: &Path,
    expected_signature: &(u64, String),
) -> Result<(PathBuf, (u64, String), String), String> {
    let (signature, digest) = verify_reference_source(source_path, Some(expected_signature))?;
    Ok((source_path.to_path_buf(), signature, digest))
}

fn monitor_authorized(
    connection: &Connection,
    folder_path: &str,
    channel_name: &str,
) -> Result<bool, String> {
    let monitor = folder_monitor_settings(connection)?;
    let connection_settings = connection_settings(connection)?;
    Ok(monitor.enabled
        && monitor.folder_path.as_deref() == Some(folder_path)
        && monitor.channel_name.as_deref() == Some(channel_name)
        && monitor.channel_id.is_some()
        && connection_settings.active_channel_id == monitor.channel_id)
}

fn monitored_channel_id(
    connection: &Connection,
    folder_path: &str,
    channel_name: &str,
) -> Result<String, String> {
    let monitor = folder_monitor_settings(connection)?;
    if !monitor_authorized(connection, folder_path, channel_name)? {
        return Err("The watched-folder channel is no longer active.".into());
    }
    monitor.channel_id.ok_or_else(|| {
        "This watched-folder binding predates immutable channel IDs. Reconnect YouTube and enable the monitor again."
            .to_string()
    })
}

fn set_observation_state(
    connection: &Connection,
    file: &MonitoredFile,
    channel_name: &str,
    state: &str,
    digest: Option<&str>,
    item_id: Option<&str>,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE folder_monitor_observations SET size_bytes = ?1, modified_key = ?2, state = ?3, digest = ?4, upload_item_id = ?5, updated_at = ?6 WHERE channel_name = ?7 AND file_path = ?8",
            params![
                file.size_bytes as i64,
                file.modified_key,
                state,
                digest,
                item_id,
                now(),
                channel_name,
                file.path.to_string_lossy()
            ],
        )
        .map_err(user_error)?;
    Ok(())
}

fn queue_monitored_item(
    connection: &Connection,
    item_id: &str,
    folder_path: &str,
    channel_name: &str,
    visibility: &str,
) -> Result<MonitorFileOutcome, String> {
    if !monitor_authorized(connection, folder_path, channel_name)? {
        connection
            .execute(
                "UPDATE upload_items SET detail = 'Automatic private upload paused because the watched-folder channel is no longer active.', updated_at = ?1 WHERE id = ?2 AND status = 'draft'",
                params![now(), item_id],
            )
            .map_err(user_error)?;
        return Ok(MonitorFileOutcome {
            disposition: MonitorDisposition::Paused,
            dispatch_item_id: None,
        });
    }
    let status: String = connection
        .query_row(
            "SELECT status FROM upload_items WHERE id = ?1",
            [item_id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if status == "draft" {
        let channel_id = monitored_channel_id(connection, folder_path, channel_name)?;
        connection
            .execute(
                "UPDATE upload_items SET channel_name = ?1, channel_id = ?2, visibility = ?3, status = 'queued', detail = ?4, updated_at = ?5 WHERE id = ?6",
                params![channel_name, channel_id, visibility, format!("Watched-folder file verified and queued for automatic {visibility} upload."), now(), item_id],
            )
            .map_err(user_error)?;
        audit(
            connection,
            item_id,
            "folder_monitor_queued",
            &format!("Stable managed asset queued for automatic {visibility} upload"),
        )?;
        return Ok(MonitorFileOutcome {
            disposition: MonitorDisposition::Queued,
            dispatch_item_id: Some(item_id.to_string()),
        });
    }
    if status == "queued" {
        return Ok(MonitorFileOutcome {
            disposition: MonitorDisposition::Queued,
            dispatch_item_id: Some(item_id.to_string()),
        });
    }
    Ok(MonitorFileOutcome {
        disposition: MonitorDisposition::Skipped,
        dispatch_item_id: None,
    })
}

fn ingest_stable_monitored_file(
    _state: &AppState,
    connection: &Connection,
    folder_path: &str,
    channel_name: &str,
    visibility: &str,
    made_for_kids: bool,
    delete_source_after_upload: bool,
    playlist_id: Option<&str>,
    playlist_title: Option<&str>,
    file: &MonitoredFile,
    start_deep_verification: bool,
) -> Result<MonitorFileOutcome, String> {
    let title = file
        .path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled video");
    let channel_id = monitored_channel_id(connection, folder_path, channel_name)?;
    if light_dedupe_title_match(connection, &channel_id, title, "", &HashSet::new())?.is_some() {
        set_observation_state(
            connection,
            file,
            channel_name,
            "duplicate_title",
            None,
            None,
        )?;
        audit_global_scoped(
            connection,
            channel_name,
            "folder_monitor_light_duplicate_detected",
            "Watched file matched a normalized title in the local queue or current YouTube library before copy",
        )?;
        return Ok(MonitorFileOutcome {
            disposition: MonitorDisposition::Skipped,
            dispatch_item_id: None,
        });
    }
    let expected_signature = (file.size_bytes, file.modified_key.clone());
    let (workspace_path, workspace_signature, digest) =
        reference_watched_source(&file.path, &expected_signature)?;
    if let Err(error) = validate_youtube_upload_limits(&workspace_path, file.size_bytes) {
        return Err(error);
    }
    let exact_duplicate: Option<String> = connection
        .query_row(
            "SELECT id FROM upload_items WHERE digest = ?1 AND status = 'uploaded' AND (channel_id = ?2 OR channel_id IS NULL) ORDER BY created_at ASC LIMIT 1",
            params![digest, channel_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?;
    if let Some(existing_id) = exact_duplicate {
        set_observation_state(
            connection,
            file,
            channel_name,
            "duplicate",
            Some(&digest),
            None,
        )?;
        audit_global_scoped(
            connection,
            channel_name,
            "folder_monitor_snapshot_duplicate_stopped",
            &format!("Managed watched-source snapshot matched completed local upload {existing_id}; no provider dispatch was created"),
        )?;
        return Ok(MonitorFileOutcome {
            disposition: MonitorDisposition::Skipped,
            dispatch_item_id: None,
        });
    }
    let item_id = Uuid::new_v4().to_string();
    set_observation_state(
        connection,
        file,
        channel_name,
        "processing",
        None,
        Some(&item_id),
    )?;
    let timestamp = now();
    connection
        .execute(
            "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, source_path, workspace_path, size_bytes, digest, background_hash_status, source_modified_key, imported_bytes, status, total_bytes, visibility, made_for_kids, delete_source_after_upload, playlist_id, playlist_title, created_at, updated_at, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'complete', ?10, ?8, 'draft', ?8, ?11, ?12, ?13, ?14, ?15, ?16, ?16, 'Stable watched source was verified in place with BLAKE3 and queued without a managed copy')",
            params![
                item_id,
                title,
                file.file_name,
                channel_name,
                channel_id,
                file.path.to_string_lossy(),
                workspace_path.to_string_lossy(),
                file.size_bytes as i64,
                digest,
                workspace_signature.1,
                visibility,
                made_for_kids as i64,
                delete_source_after_upload as i64,
                playlist_id,
                playlist_title,
                timestamp
            ],
        )
        .map_err(user_error)?;
    audit(
        connection,
        &item_id,
        "folder_monitor_source_referenced",
        "Stable watched file was verified in place before upload dispatch",
    )?;
    let outcome =
        queue_monitored_item(connection, &item_id, folder_path, channel_name, visibility)?;
    let state_name = if matches!(outcome.disposition, MonitorDisposition::Paused) {
        "paused"
    } else {
        "queued"
    };
    set_observation_state(
        connection,
        file,
        channel_name,
        state_name,
        None,
        Some(&item_id),
    )?;
    let _ = start_deep_verification;
    Ok(outcome)
}

fn fail_watched_hash_verification(
    state: &AppState,
    item_id: &str,
    message: &str,
) -> Result<(), String> {
    let connection = database(state)?;
    connection
        .execute(
            "UPDATE upload_items SET background_hash_status = 'failed', status = CASE WHEN status IN ('draft', 'queued', 'dispatching', 'uploading', 'needs_reconciliation') THEN 'cancelled' ELSE status END, detail = ?1, updated_at = ?2 WHERE id = ?3",
            params![message, now(), item_id],
        )
        .map_err(user_error)?;
    connection
        .execute(
            "UPDATE folder_monitor_observations SET state = 'hash_failed', updated_at = ?1 WHERE upload_item_id = ?2 AND state NOT IN ('complete', 'duplicate')",
            params![now(), item_id],
        )
        .map_err(user_error)?;
    audit(
        &connection,
        item_id,
        "folder_monitor_background_hash_failed",
        "Deep watched-source verification could not safely complete; any unfinished upload was cancelled",
    )
}

fn verify_watched_hash_in_background(state: &AppState, item_id: &str) -> Result<(), String> {
    let connection = database(state)?;
    let job = connection
        .query_row(
            "SELECT source_path, channel_id, size_bytes, source_modified_key FROM upload_items WHERE id = ?1 AND digest IS NULL AND background_hash_status IN ('pending', 'running')",
            [item_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(user_error)?;
    let Some((source_path, channel_id, expected_size, expected_modified_key)) = job else {
        return Ok(());
    };
    let Some(source_path) = source_path else {
        return fail_watched_hash_verification(
            state,
            item_id,
            "Deep watched-source verification could not find its original source. The unfinished upload was cancelled.",
        );
    };
    if connection
        .execute(
            "UPDATE upload_items SET background_hash_status = 'running', updated_at = ?1 WHERE id = ?2 AND background_hash_status IN ('pending', 'running')",
            params![now(), item_id],
        )
        .map_err(user_error)?
        == 0
    {
        return Ok(());
    }
    drop(connection);

    let source = Path::new(&source_path);
    let result = (|| {
        let (hashed_bytes, digest) = digest_file(source)?;
        let metadata = fs::metadata(source).map_err(user_error)?;
        let (current_size, current_modified_key) = monitored_file_signature(&metadata)?;
        if hashed_bytes != expected_size
            || current_size != expected_size
            || expected_modified_key.as_deref() != Some(current_modified_key.as_str())
        {
            return Err(
                "The watched source changed while its deep verification was running.".to_string(),
            );
        }
        Ok(digest)
    })();
    let digest = match result {
        Ok(digest) => digest,
        Err(error) => {
            return fail_watched_hash_verification(
                state,
                item_id,
                &format!("Deep watched-source verification stopped safely: {error} The unfinished upload was cancelled."),
            )
        }
    };

    let connection = database(state)?;
    let duplicate = connection
        .query_row(
            "SELECT id FROM upload_items WHERE id != ?1 AND digest = ?2 AND status = 'uploaded' AND (channel_id = ?3 OR channel_id IS NULL) ORDER BY created_at ASC LIMIT 1",
            params![item_id, digest, channel_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(user_error)?;
    if let Some(existing_id) = duplicate {
        let status: String = connection
            .query_row(
                "SELECT status FROM upload_items WHERE id = ?1",
                [item_id],
                |row| row.get(0),
            )
            .map_err(user_error)?;
        if status == "uploaded" {
            connection
                .execute(
                    "UPDATE upload_items SET digest = ?1, background_hash_status = 'duplicate_after_upload', detail = 'Deep BLAKE3 verification found an exact duplicate after YouTube completed this upload. The remote video was retained for explicit deletion review.', updated_at = ?2 WHERE id = ?3",
                    params![digest, now(), item_id],
                )
                .map_err(user_error)?;
            audit(
                &connection,
                item_id,
                "folder_monitor_hash_duplicate_after_upload",
                "An exact local duplicate was found after provider completion; remote deletion remains explicitly reviewed",
            )?;
        } else {
            connection
                .execute(
                    "UPDATE upload_items SET digest = ?1, background_hash_status = 'duplicate', status = 'cancelled', detail = 'Deep BLAKE3 verification found an exact already-uploaded duplicate. The unfinished upload was stopped.', updated_at = ?2 WHERE id = ?3 AND status IN ('draft', 'queued', 'dispatching', 'uploading', 'needs_reconciliation')",
                    params![digest, now(), item_id],
                )
                .map_err(user_error)?;
            connection
                .execute(
                    "UPDATE folder_monitor_observations SET state = 'duplicate', digest = ?1, updated_at = ?2 WHERE upload_item_id = ?3",
                    params![digest, now(), item_id],
                )
                .map_err(user_error)?;
            audit(
                &connection,
                item_id,
                "folder_monitor_hash_duplicate_stopped",
                &format!("Exact BLAKE3 match against completed local upload {existing_id}; unfinished provider upload cancelled"),
            )?;
        }
        return Ok(());
    }
    connection
        .execute(
            "UPDATE upload_items SET digest = ?1, background_hash_status = 'complete', detail = CASE WHEN status IN ('draft', 'queued', 'dispatching') THEN 'Deep BLAKE3 verification completed; upload remains ready.' ELSE detail END, updated_at = ?2 WHERE id = ?3",
            params![digest, now(), item_id],
        )
        .map_err(user_error)?;
    audit(
        &connection,
        item_id,
        "folder_monitor_background_hash_completed",
        "Deep BLAKE3 verification completed after the light duplicate gate",
    )
}

fn resume_watched_hash_verifications(state: AppState) {
    let item_ids: Result<Vec<String>, String> = (|| {
        let connection = database(&state)?;
        connection
            .execute(
                "UPDATE upload_items SET background_hash_status = 'pending', updated_at = ?1 WHERE background_hash_status = 'running' AND digest IS NULL",
                [now()],
            )
            .map_err(user_error)?;
        let item_ids = connection
            .prepare("SELECT id FROM upload_items WHERE source_path IS NOT NULL AND digest IS NULL AND background_hash_status = 'pending' AND status IN ('draft', 'queued', 'dispatching', 'uploading', 'needs_reconciliation') ORDER BY created_at ASC")
            .map_err(user_error)?
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(user_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(user_error)?;
        Ok(item_ids)
    })();
    if let Ok(item_ids) = item_ids {
        if !item_ids.is_empty() {
            spawn_worker(move || {
                for item_id in item_ids {
                    let _ = verify_watched_hash_in_background(&state, &item_id);
                }
            });
        }
    }
}

fn recover_monitored_file(
    state: &AppState,
    connection: &Connection,
    folder_path: &str,
    channel_name: &str,
    visibility: &str,
    file: &MonitoredFile,
    item_id: Option<String>,
) -> Result<MonitorFileOutcome, String> {
    let Some(item_id) = item_id else {
        set_observation_state(connection, file, channel_name, "observed", None, None)?;
        return Ok(MonitorFileOutcome {
            disposition: MonitorDisposition::Waiting,
            dispatch_item_id: None,
        });
    };
    let item = connection
        .query_row(
            "SELECT source_path, workspace_path, partial_path, size_bytes, status, digest FROM upload_items WHERE id = ?1",
            [&item_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()
        .map_err(user_error)?;
    let Some((_source_path, _workspace_path, _partial_path, _size_bytes, status, digest)) = item
    else {
        if Uuid::parse_str(&item_id).is_ok() {
            let _ = fs::remove_file(state.media_directory.join(format!("{item_id}.partial")));
            let _ = fs::remove_file(state.media_directory.join(format!("{item_id}.media")));
        }
        set_observation_state(connection, file, channel_name, "observed", None, None)?;
        return Ok(MonitorFileOutcome {
            disposition: MonitorDisposition::Waiting,
            dispatch_item_id: None,
        });
    };
    if status == "importing" {
        return Err("This watched upload was created by an older managed-copy version and is being migrated during startup.".into());
    }
    let outcome =
        queue_monitored_item(connection, &item_id, folder_path, channel_name, visibility)?;
    let state_name = if matches!(outcome.disposition, MonitorDisposition::Paused) {
        "paused"
    } else if outcome.dispatch_item_id.is_some() {
        "queued"
    } else {
        "complete"
    };
    set_observation_state(
        connection,
        file,
        channel_name,
        state_name,
        digest.as_deref(),
        Some(&item_id),
    )?;
    Ok(outcome)
}

fn update_monitor_result(
    connection: &Connection,
    status: &str,
    detail: &str,
    last_file_name: Option<&str>,
) -> Result<FolderMonitorSettings, String> {
    connection
        .execute(
            "UPDATE folder_monitor_settings SET status = ?1, detail = ?2, last_scan_at = ?3, last_file_name = COALESCE(?4, last_file_name), updated_at = ?3 WHERE singleton = 1",
            params![status, detail, now(), last_file_name],
        )
        .map_err(user_error)?;
    folder_monitor_settings(connection)
}

fn scan_folder_monitor_locked(
    state: &AppState,
    dispatch_uploads: bool,
) -> Result<FolderMonitorSettings, String> {
    let connection = database(state)?;
    let settings = folder_monitor_settings(&connection)?;
    if !settings.enabled {
        return Ok(settings);
    }
    let folder_path = settings
        .folder_path
        .as_deref()
        .ok_or_else(|| "The enabled folder monitor has no folder path.".to_string())?;
    let channel_name = settings
        .channel_name
        .as_deref()
        .ok_or_else(|| "The enabled folder monitor has no channel binding.".to_string())?;
    let Some(channel_id) = settings.channel_id.as_deref() else {
        return update_monitor_result(
            &connection,
            "paused",
            "This watched-folder binding predates immutable channel IDs. Reconnect YouTube and enable the monitor again.",
            None,
        );
    };
    let visibility = valid_folder_monitor_visibility(&settings.visibility)?;
    if connection_settings(&connection)?
        .active_channel_id
        .as_deref()
        != Some(channel_id)
    {
        return update_monitor_result(
            &connection,
            "paused",
            "Monitoring is paused until its originally authorized YouTube channel is active again.",
            None,
        );
    }
    let folder = Path::new(folder_path);
    if !folder.is_dir() {
        return update_monitor_result(
            &connection,
            "error",
            "The watched folder is unavailable; no files were uploaded.",
            None,
        );
    }
    let files = match monitored_files(folder) {
        Ok(files) => files,
        Err(error) => {
            return update_monitor_result(
                &connection,
                "error",
                &format!("The watched folder could not be scanned: {error}"),
                None,
            )
        }
    };
    let mut observations = connection
        .prepare(
            "SELECT file_path, size_bytes, modified_key, state, upload_item_id FROM folder_monitor_observations WHERE channel_id = ?1",
        )
        .map_err(user_error)?
        .query_map([channel_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ),
            ))
        })
        .map_err(user_error)?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(user_error)?;
    let mut waiting = 0_usize;
    let mut queued = 0_usize;
    let mut skipped = 0_usize;
    let mut paused = false;
    let mut failures = Vec::new();
    let mut dispatch_ids = Vec::new();
    let mut last_file_name = None;
    let mut inventory_synced = false;

    for file in files {
        last_file_name = Some(file.file_name.clone());
        let file_path = file.path.to_string_lossy().to_string();
        let observation = observations.remove(&file_path);
        let outcome = match observation {
            None => {
                let timestamp = now();
                connection
                    .execute(
                        "INSERT INTO folder_monitor_observations (channel_id, channel_name, file_path, size_bytes, modified_key, state, first_seen_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'observed', ?6, ?6)",
                        params![channel_id, channel_name, file_path, file.size_bytes as i64, file.modified_key, timestamp],
                    )
                    .map_err(user_error)?;
                waiting += 1;
                continue;
            }
            Some((size_bytes, modified_key, _, _))
                if size_bytes != file.size_bytes || modified_key != file.modified_key =>
            {
                set_observation_state(&connection, &file, channel_name, "observed", None, None)?;
                waiting += 1;
                continue;
            }
            Some((_, _, state_name, item_id)) if state_name == "observed" => {
                let claimed = connection
                    .execute(
                        "UPDATE folder_monitor_observations SET state = 'processing', updated_at = ?1 WHERE channel_id = ?2 AND file_path = ?3 AND state = 'observed' AND size_bytes = ?4 AND modified_key = ?5",
                        params![now(), channel_id, file_path, file.size_bytes as i64, file.modified_key],
                    )
                    .map_err(user_error)?;
                if claimed == 0 {
                    continue;
                }
                // Do the provider inventory read only when this scan may dispatch a
                // newly stable file. Baseline and test scans remain local-only.
                if dispatch_uploads
                    && !inventory_synced
                    && folder_inventory_refresh_due(&connection, channel_id)?
                {
                    if let Err(error) = sync_channel_inventory_worker(state) {
                        let detail = format!(
                            "YouTube library refresh failed before watched-folder upload: {}",
                            safe_folder_monitor_inventory_failure(&error)
                        );
                        // Leave the source eligible for the next retry. Keeping it
                        // as `processing` would cause later scans to bypass the
                        // required fresh-inventory safety gate.
                        set_observation_state(
                            &connection,
                            &file,
                            channel_name,
                            "observed",
                            None,
                            None,
                        )?;
                        audit_global_scoped(
                            &connection,
                            channel_name,
                            "folder_monitor_inventory_sync_failed",
                            &detail,
                        )?;
                        return update_monitor_result(
                            &connection,
                            "error",
                            &detail,
                            last_file_name.as_deref(),
                        );
                    }
                    inventory_synced = true;
                }
                let _ = item_id;
                ingest_stable_monitored_file(
                    state,
                    &connection,
                    folder_path,
                    channel_name,
                    visibility,
                    settings.made_for_kids,
                    settings.delete_source_after_upload,
                    settings.playlist_id.as_deref(),
                    settings.playlist_title.as_deref(),
                    &file,
                    dispatch_uploads,
                )
            }
            Some((_, _, state_name, item_id))
                if matches!(
                    state_name.as_str(),
                    "processing" | "paused" | "queued" | "error"
                ) =>
            {
                recover_monitored_file(
                    state,
                    &connection,
                    folder_path,
                    channel_name,
                    visibility,
                    &file,
                    item_id,
                )
            }
            Some((_, _, state_name, item_id)) if state_name == "dispatched" => {
                let should_resume = item_id
                    .as_deref()
                    .and_then(|item_id| {
                        connection
                            .query_row(
                                "SELECT status FROM upload_items WHERE id = ?1",
                                [item_id],
                                |row| row.get::<_, String>(0),
                            )
                            .optional()
                            .ok()
                            .flatten()
                    })
                    .is_some_and(|status| status == "queued");
                if !should_resume {
                    continue;
                }
                recover_monitored_file(
                    state,
                    &connection,
                    folder_path,
                    channel_name,
                    visibility,
                    &file,
                    item_id,
                )
            }
            Some((_, _, state_name, _)) if state_name == "baseline" => {
                set_observation_state(&connection, &file, channel_name, "observed", None, None)?;
                waiting += 1;
                continue;
            }
            Some(_) => continue,
        };
        match outcome {
            Ok(result) => {
                match result.disposition {
                    MonitorDisposition::Queued => queued += 1,
                    MonitorDisposition::Skipped => skipped += 1,
                    MonitorDisposition::Waiting => waiting += 1,
                    MonitorDisposition::Paused => paused = true,
                }
                if let Some(item_id) = result.dispatch_item_id {
                    dispatch_ids.push(item_id);
                }
            }
            Err(error) => {
                failures.push(error);
                let state_name = if failures
                    .last()
                    .is_some_and(|failure| failure.starts_with("This video is too "))
                {
                    "rejected"
                } else {
                    "error"
                };
                connection
                    .execute(
                        "UPDATE folder_monitor_observations SET state = ?1, updated_at = ?2 WHERE channel_id = ?3 AND file_path = ?4",
                        params![state_name, now(), channel_id, file_path],
                    )
                    .map_err(user_error)?;
            }
        }
    }

    dispatch_ids.sort();
    dispatch_ids.dedup();
    let (status, detail) = if !failures.is_empty() {
        (
            "error",
            format!(
                "{} watched file(s) could not be prepared; the next scan will retry safely. {}",
                failures.len(),
                failures[0]
            ),
        )
    } else if paused {
        (
            "paused",
            "Monitoring paused before network dispatch because the bound channel changed.".into(),
        )
    } else if queued > 0 {
        (
            "watching",
            format!("Queued {queued} stable video(s) for {visibility} YouTube upload."),
        )
    } else if waiting > 0 {
        (
            "watching",
            format!("Waiting for {waiting} video(s) to remain unchanged across another scan."),
        )
    } else if skipped > 0 {
        (
            "watching",
            format!("Skipped {skipped} video(s) already represented by a local digest or synced YouTube title."),
        )
    } else {
        (
            "watching",
            "Folder scan complete; no new stable videos were found.".into(),
        )
    };
    let result = update_monitor_result(&connection, status, &detail, last_file_name.as_deref())?;
    drop(connection);
    if dispatch_uploads && !dispatch_ids.is_empty() {
        let _ = start_queued_uploads_impl(state);
    }
    Ok(result)
}

fn scan_folder_monitor_impl(
    state: &AppState,
    dispatch_uploads: bool,
) -> Result<FolderMonitorSettings, String> {
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "The folder monitor lock is unavailable.".to_string())?;
    scan_folder_monitor_locked(state, dispatch_uploads)
}

fn record_folder_monitor_scan_failure(state: &AppState) {
    let Ok(connection) = database(state) else {
        return;
    };
    let _ = update_monitor_result(
        &connection,
        "error",
        "The background folder scan stopped before completing. Your existing queue remains unchanged; use Refresh scan to retry safely.",
        None,
    );
}

/// Starts a manual scan without holding the webview's request open while
/// filesystem, OAuth, inventory, or upload work is pending.
fn request_folder_monitor_scan_impl(state: &AppState) -> Result<FolderMonitorSettings, String> {
    let connection = database(state)?;
    let settings = folder_monitor_settings(&connection)?;
    if !settings.enabled {
        return Ok(settings);
    }
    let result = update_monitor_result(
        &connection,
        "scanning",
        "Folder scan is running in the background. You can keep using the app.",
        None,
    )?;
    drop(connection);
    let worker_state = state.clone();
    spawn_worker(move || {
        if scan_folder_monitor_impl(&worker_state, true).is_err() {
            record_folder_monitor_scan_failure(&worker_state);
        }
    });
    Ok(result)
}

/// Promote only the operator-visible starting baseline into the ordinary
/// watched-file intake state. The next locked scan still verifies the bound
/// channel, refreshes inventory, copies each stable source into managed
/// storage, and uses the regular duplicate and resumable-upload flow.
fn process_existing_folder_files_impl(
    state: &AppState,
    dispatch_uploads: bool,
) -> Result<FolderMonitorSettings, String> {
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "The folder monitor lock is unavailable.".to_string())?;
    let connection = database(state)?;
    let settings = folder_monitor_settings(&connection)?;
    if !settings.enabled {
        return Err("Enable a watched folder before processing its existing files.".into());
    }
    let folder_path = settings
        .folder_path
        .as_deref()
        .ok_or_else(|| "The enabled folder monitor has no folder path.".to_string())?;
    let channel_name = settings
        .channel_name
        .as_deref()
        .ok_or_else(|| "The enabled folder monitor has no channel binding.".to_string())?;
    if !monitor_authorized(&connection, folder_path, channel_name)? {
        return Err("The watched-folder channel is no longer active. Reconnect it before processing existing files.".into());
    }
    let baseline_count = connection
        .execute(
            "UPDATE folder_monitor_observations SET state = 'observed', digest = NULL, upload_item_id = NULL, updated_at = ?1 WHERE channel_name = ?2 AND state = 'baseline'",
            params![now(), channel_name],
        )
        .map_err(user_error)?;
    if baseline_count == 0 {
        return update_monitor_result(
            &connection,
            "watching",
            "No existing baseline videos are waiting. New direct-child video files are scanned normally.",
            None,
        );
    }
    audit_global_scoped(
        &connection,
        channel_name,
        "folder_monitor_existing_files_requested",
        &format!("Operator requested safe intake for {baseline_count} existing watched-folder baseline video(s)"),
    )?;
    drop(connection);
    let scanned = scan_folder_monitor_locked(state, dispatch_uploads)?;
    let connection = database(state)?;
    update_monitor_result(
        &connection,
        &scanned.status,
        &format!(
            "Processing {baseline_count} existing baseline video(s). {}",
            scanned.detail
        ),
        scanned.last_file_name.as_deref(),
    )
}

fn enable_folder_monitor_impl(
    state: &AppState,
    path: String,
    visibility: String,
    made_for_kids: bool,
    delete_source_after_upload: bool,
    playlist_id: Option<String>,
    playlist_title: Option<String>,
) -> Result<FolderMonitorSettings, String> {
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "The folder monitor lock is unavailable.".to_string())?;
    if path.trim().is_empty() {
        return Err("Choose a folder to monitor.".into());
    }
    let folder = PathBuf::from(&path);
    if !folder.is_dir() {
        return Err("Choose an existing local folder to monitor.".into());
    }
    let visibility = valid_folder_monitor_visibility(visibility.trim())?;
    let (playlist_id, playlist_title) = valid_playlist_selection(playlist_id, playlist_title)?;
    let mut connection = database(state)?;
    let connection_settings = connection_settings(&connection)?;
    let channel_name = connection_settings.active_channel.ok_or_else(|| {
        "Connect the YouTube channel that should receive watched-folder uploads first.".to_string()
    })?;
    let channel_id = connection_settings.active_channel_id.ok_or_else(|| {
        "Reconnect YouTube before enabling a watched folder so the app can bind it to the immutable channel ID."
            .to_string()
    })?;
    let timestamp = now();
    let transaction = connection.transaction().map_err(user_error)?;
    transaction
        .execute(
            "INSERT INTO folder_monitor_settings (singleton, enabled, folder_path, channel_name, channel_id, visibility, made_for_kids, delete_source_after_upload, playlist_id, playlist_title, status, detail, updated_at) VALUES (1, 1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'watching', ?9, ?10) ON CONFLICT(singleton) DO UPDATE SET enabled = 1, folder_path = excluded.folder_path, channel_name = excluded.channel_name, channel_id = excluded.channel_id, visibility = excluded.visibility, made_for_kids = excluded.made_for_kids, delete_source_after_upload = excluded.delete_source_after_upload, playlist_id = excluded.playlist_id, playlist_title = excluded.playlist_title, status = excluded.status, detail = excluded.detail, last_scan_at = NULL, last_file_name = NULL, updated_at = excluded.updated_at",
            params![folder.to_string_lossy(), channel_name, channel_id, visibility, made_for_kids as i64, delete_source_after_upload as i64, playlist_id, playlist_title, format!("Folder monitoring enabled; supported videos must remain unchanged across two scans before {visibility} upload."), timestamp],
        )
        .map_err(user_error)?;
    audit_global_scoped(
        &transaction,
        &channel_name,
        "folder_monitor_enabled",
        &format!("Operator enabled recurring {visibility} uploads for a device-local folder"),
    )?;
    transaction.commit().map_err(user_error)?;
    folder_monitor_settings(&connection)
}

fn disable_folder_monitor_impl(state: &AppState) -> Result<FolderMonitorSettings, String> {
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "The folder monitor lock is unavailable.".to_string())?;
    let connection = database(state)?;
    let channel_name = folder_monitor_settings(&connection)?.channel_name;
    let timestamp = now();
    connection
        .execute(
            "INSERT INTO folder_monitor_settings (singleton, enabled, status, detail, updated_at) VALUES (1, 0, 'disabled', 'Folder monitoring is disabled; queued files and source media were not deleted.', ?1) ON CONFLICT(singleton) DO UPDATE SET enabled = 0, status = excluded.status, detail = excluded.detail, updated_at = excluded.updated_at",
            params![timestamp],
        )
        .map_err(user_error)?;
    if let Some(channel_name) = channel_name.as_deref() {
        audit_global_scoped(
            &connection,
            channel_name,
            "folder_monitor_disabled",
            "Operator disabled watched-folder discovery without deleting queued media",
        )?;
    } else {
        audit_global(
            &connection,
            "folder_monitor_disabled",
            "Operator disabled watched-folder discovery without deleting queued media",
        )?;
    }
    folder_monitor_settings(&connection)
}

fn folder_monitor_deadline_loop(state: AppState) {
    loop {
        if scan_folder_monitor_impl(&state, true).is_err() {
            record_folder_monitor_scan_failure(&state);
        }
        let gate = startup_gate(&state);
        let Ok(guard) = gate.worker_wake_lock.lock() else {
            gate.folder_worker_started.store(false, Ordering::Release);
            return;
        };
        let _ = gate
            .folder_worker_wake
            .wait_timeout(guard, FOLDER_MONITOR_POLL_INTERVAL);
        let enabled = database(&state)
            .and_then(|connection| {
                connection
                    .query_row(
                        "SELECT COALESCE((SELECT enabled FROM folder_monitor_settings WHERE singleton = 1), 0)",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(user_error)
            })
            .unwrap_or(0)
            != 0;
        if !enabled {
            gate.folder_worker_started.store(false, Ordering::Release);
            return;
        }
    }
}

fn require_startup_dispatch_ready(gate: &StartupGate) -> Result<(), String> {
    if !gate.classification_complete.load(Ordering::Acquire)
        || !gate.safe_shell_rendered.load(Ordering::Acquire)
        || gate.deferred_state.load(Ordering::Acquire) != STARTUP_RECOVERY_COMPLETE
        || !gate.dispatch_ready.load(Ordering::Acquire)
    {
        return Err(
            "Saved work is still being recovered; upload dispatch remains safely paused.".into(),
        );
    }
    Ok(())
}

fn start_queued_uploads_impl(state: &AppState) -> Result<usize, String> {
    require_startup_dispatch_ready(startup_gate(state))?;
    let scheduler = upload_scheduler(state);
    let _dispatch_guard = scheduler.lock_dispatch()?;
    let mut connection = database(state)?;
    let settings = connection_settings(&connection)?;
    if !settings.connected {
        return Err("Connect a YouTube channel before starting uploads.".into());
    }
    let active_channel_id = settings
        .active_channel_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "The active YouTube connection has no immutable channel identity.".to_string()
        })?;
    if let Some(pause_until) = active_upload_quota_pause(&connection)? {
        return Err(format!(
            "YouTube's daily upload limit was reached. Saved uploads will resume automatically after {pause_until}."
        ));
    }
    recover_resumable_uploads_for_active_channel(&mut connection, active_channel_id)?;
    let active_jobs = connection
        .prepare(
            "SELECT id, workspace_path FROM upload_items WHERE status IN ('dispatching', 'uploading')",
        )
        .map_err(user_error)?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    scheduler.synchronize_active(
        active_jobs
            .into_iter()
            .map(|(id, path)| (id, source_volume_id(Path::new(&path)))),
    )?;
    let queued_items = connection
        .prepare("SELECT id, workspace_path FROM upload_items WHERE status = 'queued' AND channel_id = ?1 ORDER BY created_at ASC LIMIT 256")
        .map_err(user_error)?
        .query_map([active_channel_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    if queued_items.is_empty() {
        return Err("No reviewed uploads are waiting in the local queue.".into());
    }
    let mut candidates = Vec::new();
    let mut volume_limits = HashMap::<String, usize>::new();
    for (item_id, workspace_path) in queued_items {
        let path = Path::new(&workspace_path);
        if !path.is_file() {
            continue;
        }
        let volume_id = source_volume_id(path);
        let limit = match volume_limits.get(&volume_id) {
            Some(limit) => *limit,
            None => {
                let (_, limit) = source_volume_concurrency_limit(&connection, path)?;
                volume_limits.insert(volume_id.clone(), limit);
                limit
            }
        };
        // A resumable-session URL is the only authoritative way to find a
        // partially accepted YouTube upload. YouTube does not expose
        // incomplete resumable sessions in the regular video library, so the
        // worker will query this exact provider session before any bytes are
        // sent and before newly queued uploads are allowed to take capacity.
        let resume_from_youtube_session = stored_upload_session(&item_id)?.is_some();
        candidates.push(upload_scheduler::UploadCandidate {
            item_id,
            volume_id,
            volume_limit: limit,
            resume_from_youtube_session,
        });
    }
    let selected = scheduler.select_fair(candidates)?;
    if selected.is_empty() {
        // The queue is real, but every eligible source volume is already at its
        // cached safe concurrency. The running workers will invoke this
        // scheduler again as soon as they finish or enter reconciliation.
        return Ok(0);
    }
    let item_ids = selected
        .iter()
        .map(|candidate| candidate.item_id.clone())
        .collect::<Vec<_>>();
    let blocked_ids = match check_upload_title_duplicates_impl(state, &item_ids) {
        Ok(candidates) => candidates
            .into_iter()
            .map(|candidate| candidate.item_id)
            .collect::<HashSet<_>>(),
        Err(error) => {
            for item_id in &item_ids {
                let _ = connection.execute(
                    "UPDATE upload_items SET detail = 'Waiting for the native light duplicate check before upload.', updated_at = ?1 WHERE id = ?2 AND status = 'queued'",
                    params![now(), item_id],
                );
                let _ = audit(
                    &connection,
                    item_id,
                    "upload_light_dedupe_unavailable",
                    "Native light duplicate check did not complete; upload was not started",
                );
            }
            return Err(error);
        }
    };
    let selected = selected
        .into_iter()
        .filter(|candidate| !blocked_ids.contains(&candidate.item_id))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Ok(0);
    }
    let item_ids = selected
        .iter()
        .map(|candidate| candidate.item_id.clone())
        .collect::<Vec<_>>();
    let claimed_item_ids = claim_queued_upload_items(&mut connection, item_ids)?;
    let total = claimed_item_ids.len();
    if total == 0 {
        return Err("Queued uploads were already claimed by another local worker.".into());
    }
    let selected_by_id = selected
        .into_iter()
        .map(|candidate| (candidate.item_id, candidate.volume_id))
        .collect::<HashMap<_, _>>();
    scheduler.register_claimed(claimed_item_ids.iter().filter_map(|item_id| {
        selected_by_id
            .get(item_id)
            .map(|volume_id| (item_id.clone(), volume_id.clone()))
    }))?;
    for item_id in claimed_item_ids {
        let worker_state = state.clone();
        spawn_worker(move || run_queued_upload(worker_state, item_id));
    }
    Ok(total)
}

/// Returns interrupted transfers with a persisted YouTube resumable-session
/// URL to the shared queue. Every intake path (single file, batch, folder, and
/// watched folder) uses `upload_items`, so this recovery pass deliberately has
/// no intake-specific branch.
fn recover_resumable_uploads_for_active_channel(
    connection: &mut Connection,
    active_channel_id: &str,
) -> Result<usize, String> {
    let item_ids = connection
        .prepare(
            "SELECT id FROM upload_items WHERE status = 'needs_reconciliation' AND channel_id = ?1 ORDER BY updated_at ASC LIMIT 256",
        )
        .map_err(user_error)?
        .query_map([active_channel_id], |row| row.get::<_, String>(0))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut resumable_ids = Vec::new();
    for id in item_ids {
        if stored_upload_session(&id)?.is_some() {
            resumable_ids.push(id);
        }
    }
    if resumable_ids.is_empty() {
        return Ok(0);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let timestamp = now();
    for id in &resumable_ids {
        transaction
            .execute(
                "UPDATE upload_items SET status = 'queued', detail = 'Interrupted upload is prioritized for YouTube resumable-session verification.', updated_at = ?1 WHERE id = ?2 AND status = 'needs_reconciliation'",
                params![&timestamp, id],
            )
            .map_err(user_error)?;
        audit(
            &transaction,
            id,
            "youtube_resumable_recovery_prioritized",
            "Interrupted upload returned to the priority recovery lane before new uploads",
        )?;
    }
    transaction.commit().map_err(user_error)?;
    Ok(resumable_ids.len())
}

/// A successful connection or library refresh makes recovered queue entries
/// eligible for dispatch again. Keep this asynchronous so OAuth's loopback
/// confirmation never waits for disk sampling or upload preparation.
fn kick_automatic_upload_dispatch(state: AppState) {
    spawn_worker(move || {
        let _ = start_queued_uploads_impl(&state);
    });
}

fn claim_queued_upload_items(
    connection: &mut Connection,
    item_ids: impl IntoIterator<Item = String>,
) -> Result<Vec<String>, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let timestamp = now();
    let mut claimed_item_ids = Vec::new();
    {
        let mut claim = transaction
            .prepare_cached(
                "UPDATE upload_items SET status = 'dispatching', detail = 'Upload worker claimed this item.', updated_at = ?1 WHERE id = ?2 AND status = 'queued'",
            )
            .map_err(user_error)?;
        let mut mark_observation = transaction
            .prepare_cached(
                "UPDATE folder_monitor_observations SET state = 'dispatched', updated_at = ?1 WHERE upload_item_id = ?2 AND state = 'queued'",
            )
            .map_err(user_error)?;
        for item_id in item_ids {
            if claim
                .execute(params![&timestamp, &item_id])
                .map_err(user_error)?
                == 1
            {
                mark_observation
                    .execute(params![&timestamp, &item_id])
                    .map_err(user_error)?;
                claimed_item_ids.push(item_id);
            }
        }
    }
    transaction.commit().map_err(user_error)?;
    Ok(claimed_item_ids)
}

fn quota_resume_deadline_loop(state: AppState) {
    loop {
        let pause_until = database(&state)
            .and_then(|connection| active_upload_quota_pause(&connection))
            .ok()
            .flatten();
        let Some(pause_until) = pause_until else {
            let _ = start_queued_uploads_impl(&state);
            startup_gate(&state)
                .quota_worker_started
                .store(false, Ordering::Release);
            return;
        };
        let wait = DateTime::parse_from_rfc3339(&pause_until)
            .ok()
            .and_then(|deadline| (deadline.with_timezone(&Utc) - Utc::now()).to_std().ok())
            .unwrap_or(Duration::ZERO);
        if wait.is_zero() {
            continue;
        }
        let gate = startup_gate(&state);
        let Ok(guard) = gate.worker_wake_lock.lock() else {
            gate.quota_worker_started.store(false, Ordering::Release);
            return;
        };
        let _ = gate.quota_worker_wake.wait_timeout(guard, wait);
    }
}

fn ensure_folder_monitor_worker(state: AppState) -> Result<(), String> {
    let enabled = database(&state)?
        .query_row(
            "SELECT COALESCE((SELECT enabled FROM folder_monitor_settings WHERE singleton = 1), 0)",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)?
        != 0;
    if !enabled {
        return Ok(());
    }
    start_folder_monitor_worker(state)
}

fn start_folder_monitor_worker(state: AppState) -> Result<(), String> {
    if !startup_gate(&state).dispatch_ready.load(Ordering::Acquire) {
        return Ok(());
    }
    if startup_gate(&state)
        .folder_worker_started
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        spawn_worker(move || folder_monitor_deadline_loop(state));
    } else {
        startup_gate(&state).folder_worker_wake.notify_one();
    }
    Ok(())
}

fn ensure_quota_resume_worker(state: AppState) -> Result<(), String> {
    let needed = database(&state)?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM upload_items WHERE status = 'queued') OR EXISTS(SELECT 1 FROM upload_quota_pauses)",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)?
        != 0;
    if !needed {
        return Ok(());
    }
    start_quota_resume_worker(state)
}

fn start_quota_resume_worker(state: AppState) -> Result<(), String> {
    if !startup_gate(&state).dispatch_ready.load(Ordering::Acquire) {
        return Ok(());
    }
    if startup_gate(&state)
        .quota_worker_started
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        spawn_worker(move || quota_resume_deadline_loop(state));
    } else {
        startup_gate(&state).quota_worker_wake.notify_one();
    }
    Ok(())
}

#[derive(Default)]
struct StartupRecoveryPresence {
    classification_dirty: bool,
    reconcile_imports: bool,
    resume_watched_hashes: bool,
    resume_preflight: bool,
    start_folder_worker: bool,
    start_quota_worker: bool,
    start_postprocess_worker: bool,
}

/// Captures every deferred-startup eligibility surface in one read-only
/// statement. Only rows that require fail-closed state repair cause the
/// classifier to enter a write transaction.
fn startup_recovery_presence(connection: &Connection) -> Result<StartupRecoveryPresence, String> {
    connection
        .query_row(
            "SELECT
               EXISTS(SELECT 1 FROM upload_items WHERE status IN ('importing', 'dispatching', 'uploading'))
                 OR EXISTS(SELECT 1 FROM folder_monitor_observations WHERE state = 'dispatched' AND (upload_item_id IS NULL OR NOT EXISTS (SELECT 1 FROM upload_items WHERE upload_items.id = folder_monitor_observations.upload_item_id)))
                 OR EXISTS(SELECT 1 FROM deletion_requests WHERE status = 'executing')
                 OR EXISTS(SELECT 1 FROM upload_items WHERE background_hash_status = 'running' AND digest IS NULL)
                 OR EXISTS(SELECT 1 FROM preflight_scan_jobs WHERE status IN ('running', 'syncing'))
                 OR EXISTS(SELECT 1 FROM preflight_scan_files WHERE metadata_status = 'running'),
               EXISTS(SELECT 1 FROM upload_items WHERE status = 'importing'),
               EXISTS(SELECT 1 FROM upload_items WHERE source_path IS NOT NULL AND digest IS NULL AND background_hash_status IN ('pending', 'running') AND status IN ('draft', 'queued', 'dispatching', 'uploading', 'needs_reconciliation')),
               EXISTS(SELECT 1 FROM preflight_scan_jobs WHERE status IN ('queued', 'running', 'syncing'))
                 OR EXISTS(SELECT 1 FROM preflight_scan_files WHERE status = 'complete' AND metadata_status IN ('pending', 'running')),
               COALESCE((SELECT enabled FROM folder_monitor_settings WHERE singleton = 1), 0),
               EXISTS(SELECT 1 FROM upload_items WHERE status IN ('queued', 'dispatching', 'uploading'))
                 OR EXISTS(SELECT 1 FROM upload_quota_pauses),
               EXISTS(SELECT 1 FROM upload_items WHERE channel_id = COALESCE((SELECT active_channel_id FROM connection_settings WHERE singleton = 1), '') AND status = 'uploaded' AND ((playlist_id IS NOT NULL AND playlist_status = 'pending') OR (delete_source_after_upload = 1 AND source_delete_status = 'pending')))" ,
            [],
            |row| {
                Ok(StartupRecoveryPresence {
                    classification_dirty: row.get::<_, i64>(0)? != 0,
                    reconcile_imports: row.get::<_, i64>(1)? != 0,
                    resume_watched_hashes: row.get::<_, i64>(2)? != 0,
                    resume_preflight: row.get::<_, i64>(3)? != 0,
                    start_folder_worker: row.get::<_, i64>(4)? != 0,
                    start_quota_worker: row.get::<_, i64>(5)? != 0,
                    start_postprocess_worker: row.get::<_, i64>(6)? != 0,
                })
            },
        )
        .map_err(user_error)
}

/// This startup transaction is intentionally database-only. It closes every
/// dispatch claim before the webview exists, classifies provider-ambiguous work
/// fail-closed, and resets resumable local workers without opening media,
/// secure-store entries, FFprobe, or the network.
fn classify_startup_recovery_with_connection(
    connection: &mut Connection,
) -> Result<StartupRecoveryWork, String> {
    let presence = startup_recovery_presence(connection)?;
    let mut recovery_work = StartupRecoveryWork {
        reconcile_imports: presence.reconcile_imports,
        resume_watched_hashes: presence.resume_watched_hashes,
        resume_preflight: presence.resume_preflight,
        start_folder_worker: presence.start_folder_worker,
        start_quota_worker: presence.start_quota_worker,
        start_postprocess_worker: presence.start_postprocess_worker,
        ..StartupRecoveryWork::default()
    };
    if !presence.classification_dirty {
        return Ok(recovery_work);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let interrupted = transaction
        .prepare(
            "SELECT id, status FROM upload_items WHERE status IN ('importing', 'dispatching', 'uploading') ORDER BY created_at ASC",
        )
        .map_err(user_error)?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let timestamp = now();
    for (id, status) in interrupted {
        match status.as_str() {
            "dispatching" => {
                let detail = "Recovered an automatic upload before network dispatch.";
                transaction
                    .execute(
                        "UPDATE upload_items SET status = 'queued', detail = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'dispatching'",
                        params![detail, &timestamp, &id],
                    )
                    .map_err(user_error)?;
                transaction
                    .execute(
                        "UPDATE folder_monitor_observations SET state = 'queued', updated_at = ?1 WHERE upload_item_id = ?2 AND state = 'dispatched'",
                        params![&timestamp, &id],
                    )
                    .map_err(user_error)?;
                audit(&transaction, &id, "restart_reconciliation", detail)?;
            }
            "uploading" => {
                let detail = "Interrupted upload is held for local resumable-session recovery before any provider request.";
                transaction
                    .execute(
                        "UPDATE upload_items SET status = 'needs_reconciliation', detail = ?1, updated_at = ?2 WHERE id = ?3 AND status = 'uploading'",
                        params![detail, &timestamp, &id],
                    )
                    .map_err(user_error)?;
                audit(&transaction, &id, "restart_reconciliation", detail)?;
                recovery_work.interrupted_upload_ids.push(id);
            }
            "importing" => {
                transaction
                    .execute(
                        "UPDATE upload_items SET detail = 'Interrupted local import is queued for bounded post-shell recovery.', updated_at = ?1 WHERE id = ?2 AND status = 'importing'",
                        params![&timestamp, &id],
                    )
                    .map_err(user_error)?;
            }
            _ => {}
        }
    }
    transaction
        .execute(
            "UPDATE folder_monitor_observations SET state = 'queued', updated_at = ?1 WHERE state = 'dispatched' AND (upload_item_id IS NULL OR NOT EXISTS (SELECT 1 FROM upload_items WHERE upload_items.id = folder_monitor_observations.upload_item_id))",
            [&timestamp],
        )
        .map_err(user_error)?;
    let recovered_deletions = transaction
        .execute(
            "UPDATE deletion_requests SET status = 'needs_reconciliation', detail = 'The app closed while YouTube deletion was awaiting a receipt. Confirm the video ID again to reconcile this request; no blind retry will occur.', updated_at = ?1 WHERE status = 'executing'",
            [&timestamp],
        )
        .map_err(user_error)?;
    if recovered_deletions > 0 {
        audit_global(
            &transaction,
            "youtube_deletion_recovery_required",
            "Interrupted deletion requests retained for explicit reconciliation",
        )?;
    }
    transaction
        .execute(
            "UPDATE upload_items SET background_hash_status = 'pending', updated_at = ?1 WHERE background_hash_status = 'running' AND digest IS NULL",
            [&timestamp],
        )
        .map_err(user_error)?;
    transaction
        .execute(
            "UPDATE preflight_scan_jobs SET status = 'queued', inventory_status = CASE WHEN inventory_status = 'syncing' THEN 'pending' ELSE inventory_status END, updated_at = ?1 WHERE status IN ('running', 'syncing')",
            [&timestamp],
        )
        .map_err(user_error)?;
    transaction
        .execute(
            "UPDATE preflight_scan_files SET metadata_status = 'pending' WHERE metadata_status = 'running'",
            [],
        )
        .map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    Ok(recovery_work)
}

#[cfg(test)]
fn classify_startup_recovery(state: &AppState) -> Result<StartupRecoveryWork, String> {
    let mut connection = database(state)?;
    classify_startup_recovery_with_connection(&mut connection)
}

#[cfg(test)]
fn reconcile_interrupted_deletions(state: &AppState) -> Result<(), String> {
    classify_startup_recovery(state).map(|_| ())
}

fn resolve_interrupted_upload_sessions(
    state: &AppState,
    item_ids: &[String],
) -> Result<(), String> {
    for id in item_ids {
        let has_session = stored_upload_session(id)?.is_some();
        let connection = database(state)?;
        let (status, detail) = if has_session {
            (
                "queued",
                "Recovered interrupted upload; its saved YouTube resumable session will resume from the provider-confirmed range.",
            )
        } else {
            (
                "needs_reconciliation",
                "App closed during upload before a resumable session was available; verify the YouTube channel before retrying.",
            )
        };
        connection
            .execute(
                "UPDATE upload_items SET status = ?1, detail = ?2, updated_at = ?3 WHERE id = ?4 AND status = 'needs_reconciliation'",
                params![status, detail, now(), id],
            )
            .map_err(user_error)?;
        audit(&connection, id, "restart_reconciliation", detail)?;
    }
    Ok(())
}

fn legacy_managed_file_id(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let stem = name
        .strip_suffix(".watched.partial")
        .or_else(|| name.strip_suffix(".partial"))
        .or_else(|| name.strip_suffix(".media"))?;
    Uuid::parse_str(stem).ok().map(|_| stem.to_string())
}

/// One-way local migration for releases that created `.media` copies. It runs
/// after the safe shell boundary and only removes regular UUID-named files
/// owned by this app. Queued legacy work is switched to its original source or
/// made non-dispatchable when that source no longer exists.
fn cleanup_legacy_managed_media(state: &AppState) -> Result<(), String> {
    let media_directory = match state.media_directory.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };
    let connection = database(state)?;
    let rows = connection
        .prepare("SELECT id, source_path, workspace_path, partial_path, status FROM upload_items")
        .map_err(user_error)?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut owned_paths = HashSet::<PathBuf>::new();
    let mut migrated = 0_usize;
    let mut failed = 0_usize;
    for (id, source_path, workspace_path, partial_path, _status) in rows {
        let workspace = PathBuf::from(&workspace_path);
        let owned_workspace = workspace.canonicalize().ok().is_some_and(|path| {
            path.starts_with(&media_directory)
                && legacy_managed_file_id(&path).as_deref() == Some(id.as_str())
        });
        let owned_partial = partial_path
            .as_deref()
            .and_then(|path| Path::new(path).canonicalize().ok())
            .filter(|path| path.starts_with(&media_directory))
            .filter(|path| legacy_managed_file_id(path).as_deref() == Some(id.as_str()));
        if !owned_workspace && owned_partial.is_none() {
            continue;
        }
        if owned_workspace {
            owned_paths.insert(workspace);
        }
        if let Some(partial) = owned_partial {
            owned_paths.insert(partial);
        }
        let source = source_path.filter(|path| Path::new(path).is_file());
        if let Some(source) = source {
            connection.execute(
                "UPDATE upload_items SET workspace_path = ?1, partial_path = NULL, status = CASE WHEN status = 'importing' THEN 'failed' ELSE status END, detail = CASE WHEN status = 'importing' THEN 'Legacy managed import was removed; reselect the original source to continue.' ELSE 'Legacy managed copy removed; the original source is used directly.' END, updated_at = ?2 WHERE id = ?3",
                params![source, now(), id],
            ).map_err(user_error)?;
            audit(
                &connection,
                &id,
                "legacy_managed_media_removed",
                "Legacy managed media was removed; upload now references the original source",
            )?;
            migrated += 1;
        } else {
            connection.execute(
                "UPDATE upload_items SET status = CASE WHEN status = 'uploaded' THEN status ELSE 'failed' END, partial_path = NULL, detail = CASE WHEN status = 'uploaded' THEN detail ELSE 'Legacy managed media was removed and the original source is unavailable; reselect the source to upload again.' END, updated_at = ?1 WHERE id = ?2",
                params![now(), id],
            ).map_err(user_error)?;
            audit(
                &connection,
                &id,
                "legacy_managed_media_removed",
                "Legacy managed media was removed; no original source remains for this upload",
            )?;
            failed += 1;
        }
    }
    for entry in fs::read_dir(&media_directory).map_err(user_error)? {
        let entry = entry.map_err(user_error)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(user_error)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        if legacy_managed_file_id(&path).is_some() {
            owned_paths.insert(path);
        }
    }
    let mut removed = 0_usize;
    for path in owned_paths {
        if path.starts_with(&media_directory) && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 || migrated > 0 || failed > 0 {
        audit_global(
            &connection,
            "legacy_managed_media_cleanup",
            &format!("Removed {removed} legacy managed-media file(s); migrated {migrated} source reference(s); marked {failed} unavailable upload(s) for repair"),
        )?;
    }
    Ok(())
}

fn run_deferred_startup_recovery(
    state: &AppState,
    app: &AppHandle,
) -> Result<StartupWorkerEligibility, String> {
    let recovery_work = startup_gate(state)
        .recovery_work
        .lock()
        .map_err(|_| "The startup recovery plan is unavailable.".to_string())?
        .take()
        .unwrap_or_default();
    cleanup_legacy_managed_media(state)?;
    if !recovery_work.interrupted_upload_ids.is_empty() {
        resolve_interrupted_upload_sessions(state, &recovery_work.interrupted_upload_ids)?;
    }
    // This retains the existing crash-resumable import implementation, but it
    // now runs on one post-shell worker rather than the GUI setup thread.
    if recovery_work.reconcile_imports {
        reconcile_queue_impl(state)?;
    }
    if recovery_work.resume_watched_hashes {
        resume_watched_hash_verifications(state.clone());
    }
    if recovery_work.resume_preflight {
        resume_preflight_scan_jobs(state.clone(), app.clone());
    }
    Ok(StartupWorkerEligibility {
        folder: recovery_work.start_folder_worker,
        quota: recovery_work.start_quota_worker,
        postprocess: recovery_work.start_postprocess_worker,
    })
}

fn initialize_state(app: &AppHandle) -> Result<AppState, String> {
    performance::mark_initialization_started();
    let default_root = app.path().app_data_dir().map_err(user_error)?;
    let root = performance::isolated_profile_root(default_root)?;
    let media_directory = root.join("media");
    fs::create_dir_all(&media_directory).map_err(user_error)?;
    let state = AppState {
        database_path: root.join("queue.sqlite3"),
        media_directory,
        folder_monitor_lock: Arc::new(Mutex::new(())),
        oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        #[cfg(not(test))]
        startup: Arc::new(StartupGate::new()),
        #[cfg(not(test))]
        state_events: Arc::new(state_events::StateEventRuntime::new()),
        #[cfg(not(test))]
        provider_transport: Arc::new(provider_transport::ProviderTransport::new()),
        #[cfg(not(test))]
        upload_scheduler: Arc::new(upload_scheduler::UploadScheduler::new(
            MAX_CONCURRENT_UPLOAD_WORKERS,
        )),
        #[cfg(not(test))]
        deletion_execution_lock: Arc::new(Mutex::new(())),
    };
    let mut connection = initialize_database(&state)?;
    #[cfg(feature = "performance-harness")]
    if performance::seed_requested_fixture(&state.database_path)? {
        std::process::exit(0);
    }
    let recovery_work = classify_startup_recovery_with_connection(&mut connection)?;
    *startup_gate(&state)
        .recovery_work
        .lock()
        .map_err(|_| "The startup recovery plan is unavailable.".to_string())? =
        Some(recovery_work);
    startup_gate(&state)
        .classification_complete
        .store(true, Ordering::Release);
    performance::mark_recovery_classified();
    performance::mark_native_ready();
    Ok(state)
}

#[tauri::command]
fn load_folder_monitor_settings(
    state: State<'_, AppState>,
) -> Result<FolderMonitorSettings, String> {
    folder_monitor_settings(&database(&state)?)
}

#[tauri::command]
async fn load_folder_monitor_overview(
    state: State<'_, AppState>,
) -> Result<FolderMonitorOverview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || folder_monitor_overview(&database(&state)?))
        .await
        .map_err(|_| "Folder monitor activity loading stopped unexpectedly.".to_string())?
}

#[tauri::command]
fn enable_folder_monitor(
    path: String,
    visibility: String,
    made_for_kids: bool,
    delete_source_after_upload: bool,
    playlist_id: Option<String>,
    playlist_title: Option<String>,
    state: State<'_, AppState>,
) -> Result<FolderMonitorSettings, String> {
    let settings = enable_folder_monitor_impl(
        &state,
        path,
        visibility,
        made_for_kids,
        delete_source_after_upload,
        playlist_id,
        playlist_title,
    )?;
    ensure_folder_monitor_worker(state.inner().clone())?;
    Ok(settings)
}

#[tauri::command]
fn disable_folder_monitor(state: State<'_, AppState>) -> Result<FolderMonitorSettings, String> {
    let settings = disable_folder_monitor_impl(&state)?;
    startup_gate(&state).folder_worker_wake.notify_one();
    Ok(settings)
}

#[tauri::command]
async fn scan_folder_monitor_now(
    state: State<'_, AppState>,
) -> Result<FolderMonitorSettings, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || request_folder_monitor_scan_impl(&state))
        .await
        .map_err(|_| "Folder monitoring stopped unexpectedly.".to_string())?
}

#[tauri::command]
async fn process_existing_folder_files(
    state: State<'_, AppState>,
) -> Result<FolderMonitorSettings, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || process_existing_folder_files_impl(&state, true))
        .await
        .map_err(|_| "Existing watched-folder processing stopped unexpectedly.".to_string())?
}

fn dashboard_snapshot_with_connection(
    connection: &Connection,
    settings: &ConnectionSettings,
) -> Result<DashboardSnapshot, String> {
    let items = if let Some(channel_id) = settings.active_channel_id.as_deref() {
        connection
            .prepare("SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, visibility, made_for_kids, playlist_id, playlist_title, upload_started_at, transfer_bytes_per_second, delete_source_after_upload, source_delete_status, updated_at FROM upload_items WHERE status != 'cancelled' AND channel_id = ?1 ORDER BY updated_at DESC")
            .map_err(user_error)?
            .query_map([channel_id], row_to_upload_item)
            .map_err(user_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(user_error)?
    } else {
        connection
            .prepare("SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, visibility, made_for_kids, playlist_id, playlist_title, upload_started_at, transfer_bytes_per_second, delete_source_after_upload, source_delete_status, updated_at FROM upload_items WHERE status != 'cancelled' AND (channel_id IS NULL OR channel_id = '') ORDER BY updated_at DESC")
            .map_err(user_error)?
            .query_map([], row_to_upload_item)
            .map_err(user_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(user_error)?
    };
    let ignored_candidate_ids = ignored_duplicate_candidate_ids(connection)?;
    let mut duplicates =
        duplicate_candidates_for_channel(connection, settings.active_channel_id.as_deref())?;
    duplicates.retain(|candidate| !ignored_candidate_ids.contains(&candidate.id));
    let pending_title_duplicates = settings
        .active_channel_id
        .as_deref()
        .map(|channel_id| pending_upload_title_duplicates(connection, channel_id))
        .transpose()?
        .unwrap_or_default();
    Ok(DashboardSnapshot {
        active_channel: settings.active_channel.clone(),
        active_channel_id: settings.active_channel_id.clone(),
        revision: state_events::current_revision(connection)?,
        items,
        duplicates,
        pending_title_duplicates,
    })
}

fn dashboard_snapshot_impl(state: &AppState) -> Result<DashboardSnapshot, String> {
    let connection = database(state)?;
    let settings = connection_settings(&connection)?;
    dashboard_snapshot_with_connection(&connection, &settings)
}

#[tauri::command]
fn dashboard_snapshot(state: State<'_, AppState>) -> Result<DashboardSnapshot, String> {
    dashboard_snapshot_impl(&state)
}

#[tauri::command]
fn load_state_changes(
    channel_id: String,
    after_revision: u64,
    state: State<'_, AppState>,
) -> Result<state_events::StateChangeBatch, String> {
    state_events::load_state_changes(&database(&state)?, &channel_id, after_revision)
}

fn startup_bootstrap_impl(state: &AppState) -> Result<StartupBootstrap, String> {
    let connection = database(state)?;
    let connection_summary = connection_settings(&connection)?;
    let snapshot = dashboard_snapshot_with_connection(&connection, &connection_summary)?;
    Ok(StartupBootstrap {
        crash_recovery: crash_recovery_status(state),
        connection: connection_summary,
        snapshot,
        readiness: startup_gate(state).readiness(),
    })
}

#[tauri::command]
fn startup_bootstrap(state: State<'_, AppState>) -> Result<StartupBootstrap, String> {
    startup_bootstrap_impl(&state)
}

#[tauri::command]
async fn complete_startup_after_safe_shell(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<StartupBootstrap, String> {
    let state = state.inner().clone();
    let gate = startup_gate(&state);
    gate.safe_shell_rendered.store(true, Ordering::Release);
    let should_run = gate
        .deferred_started
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok();
    if should_run {
        gate.deferred_state
            .store(STARTUP_RECOVERY_RUNNING, Ordering::Release);
        let worker_state = state.clone();
        let recovery = tauri::async_runtime::spawn_blocking(move || {
            run_deferred_startup_recovery(&worker_state, &app)
        })
        .await
        .unwrap_or_else(|_| Err("Startup recovery stopped unexpectedly.".to_string()));
        match recovery {
            Ok(worker_eligibility) => {
                let gate = startup_gate(&state);
                gate.dispatch_ready.store(true, Ordering::Release);
                gate.deferred_state
                    .store(STARTUP_RECOVERY_COMPLETE, Ordering::Release);
                let workers_started = (|| {
                    if worker_eligibility.folder {
                        start_folder_monitor_worker(state.clone())?;
                    }
                    if worker_eligibility.quota {
                        start_quota_resume_worker(state.clone())?;
                    }
                    if worker_eligibility.postprocess {
                        start_upload_postprocess_worker(state.clone())?;
                    }
                    Ok::<(), String>(())
                })();
                if workers_started.is_err() {
                    gate.dispatch_ready.store(false, Ordering::Release);
                    gate.deferred_state
                        .store(STARTUP_RECOVERY_FAILED, Ordering::Release);
                }
            }
            Err(_) => {
                let gate = startup_gate(&state);
                gate.dispatch_ready.store(false, Ordering::Release);
                gate.deferred_state
                    .store(STARTUP_RECOVERY_FAILED, Ordering::Release);
            }
        }
    }
    startup_bootstrap_impl(&state)
}

#[tauri::command]
fn load_connection_settings(state: State<'_, AppState>) -> Result<ConnectionSettings, String> {
    connection_settings(&database(&state)?)
}

#[tauri::command]
fn github_issue_diagnostic_report(state: State<'_, AppState>) -> Result<String, String> {
    diagnostic_report_impl(&state)
}

#[tauri::command]
fn native_performance_report() -> performance::NativePerformanceReport {
    performance::report()
}

#[cfg(feature = "performance-harness")]
#[tauri::command]
fn mark_performance_milestone(
    state: State<'_, AppState>,
    milestone: String,
    metrics: performance::FrontendPerformanceMetrics,
) -> Result<(), String> {
    performance::mark_frontend_milestone(&milestone, metrics)?;
    performance::write_startup_snapshot(
        &state
            .database_path
            .with_file_name("native-performance-startup.json"),
    )
}

#[tauri::command]
fn app_release_identity() -> AppReleaseIdentity {
    release_identity()
}

#[tauri::command]
fn load_crash_recovery_status(state: State<'_, AppState>) -> CrashRecoveryStatus {
    crash_recovery_status(&state)
}

#[tauri::command]
fn record_webview_error(state: State<'_, AppState>) -> Result<(), String> {
    record_webview_error_impl(&state)
}

#[tauri::command]
fn acknowledge_crash_recovery(state: State<'_, AppState>) -> Result<(), String> {
    acknowledge_crash_recovery_impl(&state)
}

fn manual_upload_defaults(connection: &Connection) -> Result<ManualUploadDefaults, String> {
    let made_for_kids = connection
        .query_row(
            "SELECT manual_made_for_kids_default FROM connection_settings WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(user_error)?
        .unwrap_or(0);
    Ok(ManualUploadDefaults {
        made_for_kids: made_for_kids != 0,
    })
}

#[tauri::command]
fn load_manual_upload_defaults(state: State<'_, AppState>) -> Result<ManualUploadDefaults, String> {
    manual_upload_defaults(&database(&state)?)
}

#[tauri::command]
fn save_manual_upload_defaults(
    made_for_kids: bool,
    state: State<'_, AppState>,
) -> Result<ManualUploadDefaults, String> {
    let connection = database(&state)?;
    connection
        .execute(
            "INSERT INTO connection_settings (singleton, manual_made_for_kids_default, updated_at) VALUES (1, ?1, ?2) ON CONFLICT(singleton) DO UPDATE SET manual_made_for_kids_default = excluded.manual_made_for_kids_default, updated_at = excluded.updated_at",
            params![made_for_kids as i64, now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "manual_made_for_kids_default_set",
        if made_for_kids {
            "Manual Made for Kids default set to yes."
        } else {
            "Manual Made for Kids default set to no."
        },
    )?;
    manual_upload_defaults(&connection)
}

#[tauri::command]
fn import_desktop_oauth_client(
    path: String,
    state: State<'_, AppState>,
) -> Result<ConnectionSettings, String> {
    let metadata = fs::metadata(&path)
        .map_err(|_| "The selected OAuth JSON file could not be read.".to_string())?;
    if !metadata.is_file() || metadata.len() > 64 * 1024 {
        return Err("Choose a small downloaded Google Desktop OAuth JSON file.".into());
    }
    let contents = fs::read_to_string(&path)
        .map_err(|_| "The selected OAuth JSON file is not readable text.".to_string())?;
    let (client_id, client_secret) = desktop_oauth_client_from_file(&contents)?;
    if client_secret.is_empty() {
        clear_oauth_client_secret()?;
    } else {
        oauth_client_secret_entry()?
            .set_password(&client_secret)
            .map_err(user_error)?;
    }
    let connection = database(&state)?;
    connection
        .execute(
            "INSERT INTO connection_settings (singleton, oauth_client_id, active_channel, active_channel_id, connection_detail, deletion_authorized, updated_at) VALUES (1, ?1, NULL, NULL, 'Custom Google Desktop OAuth client imported on this device.', 0, ?2) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id = excluded.oauth_client_id, active_channel = NULL, active_channel_id = NULL, connection_detail = excluded.connection_detail, deletion_authorized = 0, updated_at = excluded.updated_at",
            params![client_id, now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "oauth_desktop_client_imported",
        "Desktop OAuth client JSON imported into protected local credential storage",
    )?;
    connection_settings(&connection)
}

#[tauri::command]
async fn export_portable_archive(
    path: String,
    state: State<'_, AppState>,
) -> Result<PortableArchiveReceipt, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        export_portable_archive_impl(&state, Path::new(&path))
    })
    .await
    .map_err(|_| "Archive export stopped unexpectedly.".to_string())?
}

#[tauri::command]
async fn import_portable_archive(
    path: String,
    state: State<'_, AppState>,
) -> Result<PortableArchiveReceipt, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        import_portable_archive_impl(&state, Path::new(&path))
    })
    .await
    .map_err(|_| "Archive import stopped unexpectedly.".to_string())?
}

fn begin_oauth_connection(
    state: State<'_, AppState>,
    scope: &str,
    attempt_kind: OAuthAttemptKind,
) -> Result<OAuthStart, String> {
    if !secure_store_available() {
        return Err("This device's secure credential store is unavailable; YouTube tokens cannot be stored safely.".into());
    }
    let connection = database(&state)?;
    let client_id = configured_oauth_client_id(&connection)?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "A secure local OAuth callback port could not be opened.".to_string())?;
    let port = listener.local_addr().map_err(user_error)?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth2/callback");
    let state_token = Uuid::new_v4().simple().to_string();
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    oauth_verifier_entry(&state_token)?
        .set_password(&verifier)
        .map_err(user_error)?;
    state
        .oauth_attempts
        .lock()
        .map_err(user_error)?
        .insert(state_token.clone(), attempt_kind);
    let authorization_url = url::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", scope),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state_token.as_str()),
        ],
    )
    .map_err(user_error)?
    .to_string();
    set_connection_failure_detail(&state, "Waiting for Google authorization in your browser.")?;
    let callback_state = state.inner().clone();
    let callback_state_token = state_token.clone();
    spawn_worker(move || {
        await_oauth_callback(
            callback_state,
            listener,
            callback_state_token,
            client_id,
            redirect_uri,
            attempt_kind,
        )
    });
    Ok(OAuthStart {
        authorization_url,
        attempt_id: state_token,
    })
}

#[tauri::command]
fn begin_youtube_connection(state: State<'_, AppState>) -> Result<OAuthStart, String> {
    begin_oauth_connection(state, UPLOAD_OAUTH_SCOPES, OAuthAttemptKind::Connection)
}

#[tauri::command]
fn begin_deletion_authorization(state: State<'_, AppState>) -> Result<OAuthStart, String> {
    if !connection_settings(&database(&state)?)?.connected {
        return Err("Connect the YouTube channel before granting deletion permission.".into());
    }
    begin_oauth_connection(state, DELETION_OAUTH_SCOPES, OAuthAttemptKind::Deletion)
}

#[tauri::command]
fn cancel_youtube_connection(
    attempt_id: String,
    state: State<'_, AppState>,
) -> Result<ConnectionSettings, String> {
    let cancelled = {
        let mut attempts = state.oauth_attempts.lock().map_err(user_error)?;
        cancel_connection_attempt(&mut attempts, &attempt_id)
    };
    if !cancelled {
        return Err("This Google connection attempt is no longer active.".into());
    }
    let _ = oauth_verifier_entry(&attempt_id)
        .and_then(|entry| entry.delete_credential().map_err(user_error));
    set_connection_failure_detail(
        &state,
        "Google connection cancelled. You can continue using the existing local connection.",
    )?;
    let connection = database(&state)?;
    audit_global(
        &connection,
        "youtube_connection_cancelled",
        "Operator cancelled the pending Google connection",
    )?;
    connection_settings(&connection)
}

#[tauri::command]
fn enable_deletion_sudo_mode(state: State<'_, AppState>) -> Result<ConnectionSettings, String> {
    let connection = database(&state)?;
    clear_expired_deletion_authorization(&connection)?;
    if !connection_settings(&connection)?.deletion_authorized {
        return Err("Grant YouTube deletion permission before entering deletion mode.".into());
    }
    connection
        .execute(
            "UPDATE connection_settings SET deletion_sudo_until = ?1, updated_at = ?2 WHERE singleton = 1",
            params![deletion_sudo_until(), now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_deletion_sudo_enabled",
        "Operator entered temporary deletion mode",
    )?;
    connection_settings(&connection)
}

#[tauri::command]
fn disable_deletion_sudo_mode(state: State<'_, AppState>) -> Result<ConnectionSettings, String> {
    let _ = clear_refresh_token(deletion_refresh_token_entry());
    provider_transport(&state)
        .invalidate_access_token(provider_transport::AccessTokenKind::Deletion);
    let connection = database(&state)?;
    connection
        .execute(
            "UPDATE connection_settings SET deletion_authorized = 0, deletion_sudo_until = NULL, updated_at = ?1 WHERE singleton = 1",
            params![now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_deletion_sudo_disabled",
        "Operator exited temporary deletion mode",
    )?;
    connection_settings(&connection)
}

#[tauri::command]
fn disconnect_youtube(state: State<'_, AppState>) -> Result<ConnectionSettings, String> {
    // Credential deletion is idempotent from the operator's perspective: an already-cleared
    // credential must not prevent the local connection record from being removed.
    let _ = clear_refresh_token(refresh_token_entry());
    let _ = clear_refresh_token(deletion_refresh_token_entry());
    provider_transport(&state).invalidate_access_token(provider_transport::AccessTokenKind::Upload);
    provider_transport(&state)
        .invalidate_access_token(provider_transport::AccessTokenKind::Deletion);
    let connection = database(&state)?;
    connection
        .execute(
            "UPDATE connection_settings SET active_channel = NULL, active_channel_id = NULL, deletion_authorized = 0, deletion_sudo_until = NULL, connection_detail = 'YouTube disconnected on this device.', updated_at = ?1 WHERE singleton = 1",
            params![now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_disconnected",
        "Local YouTube credential removed",
    )?;
    connection_settings(&connection)
}

#[tauri::command]
async fn sync_channel_inventory(state: State<'_, AppState>) -> Result<usize, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = connection_settings(&database(&state)?)?;
        if !settings.connected {
            return Err("Connect a YouTube channel before syncing its library.".into());
        }
        match sync_channel_inventory_worker(&state) {
            Ok(count) => {
                // A successful explicit refresh proves that the recovered
                // queue can safely retry its native pre-dispatch checks.
                kick_automatic_upload_dispatch(state.clone());
                Ok(count)
            }
            Err(error) => {
                if let Ok(connection) = database(&state) {
                    let _ = audit_global(
                        &connection,
                        "youtube_inventory_sync_failed",
                        &safe_inventory_sync_failure(&error),
                    );
                }
                Err(error)
            }
        }
    })
    .await
    .map_err(|_| "YouTube inventory synchronization stopped unexpectedly.".to_string())?
}

/// Claims and starts queued uploads when capacity is available.
///
/// This is intentionally a native command because the webview uses it after
/// startup and an explicit library refresh. Keeping the claim in native code
/// preserves the durable scheduler's single-worker and per-volume limits.
#[tauri::command]
fn resume_queued_uploads(state: State<'_, AppState>) -> Result<usize, String> {
    start_queued_uploads_impl(state.inner())
}

#[tauri::command]
async fn check_upload_title_duplicates(
    item_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<UploadTitleDuplicate>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        check_upload_title_duplicates_impl(&state, &item_ids)
    })
    .await
    .map_err(|_| "The duplicate title check stopped unexpectedly.".to_string())?
}

#[tauri::command]
async fn ignore_duplicate_candidate(
    candidate_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        ignore_duplicate_candidate_impl(&state, &candidate_id)
    })
    .await
    .map_err(|_| "Saving the duplicate-review decision stopped unexpectedly.".to_string())?
}

#[tauri::command]
async fn re_audit_ignored_duplicate_candidates(
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || re_audit_ignored_duplicate_candidates_impl(&state))
        .await
        .map_err(|_| "Restoring ignored duplicate candidates stopped unexpectedly.")
        .map(|result| result)?
}

#[tauri::command]
async fn start_preflight_duplicate_files(
    paths: Vec<FilePath>,
    mode: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<PreIngestDuplicateScan, String> {
    let state = state.inner().clone();
    let job_id = create_preflight_scan_job(&state, &paths, &mode)?;
    let worker_state = state.clone();
    let worker_app = app.clone();
    let worker_job_id = job_id.clone();
    spawn_worker(move || {
        let _ = run_preflight_scan_job(&worker_state, &worker_app, &worker_job_id, true);
    });
    load_preflight_scan(&state, &job_id)
}

#[tauri::command]
fn load_preflight_duplicate_scan(
    job_id: String,
    file_offset: Option<u64>,
    file_limit: Option<u64>,
    activity_offset: Option<u64>,
    activity_limit: Option<u64>,
    state: State<'_, AppState>,
) -> Result<PreIngestDuplicateScan, String> {
    load_preflight_scan_page(
        &state,
        &job_id,
        file_offset.unwrap_or(0),
        file_limit.unwrap_or(48),
        activity_offset.unwrap_or(0),
        activity_limit.unwrap_or(48),
    )
}

#[tauri::command]
fn load_preflight_duplicate_scan_status(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<PreIngestDuplicateScanStatus, String> {
    load_preflight_scan_status(&state, &job_id)
}

#[tauri::command]
fn load_preflight_duplicate_file_metadata(
    job_id: String,
    ordinal: u64,
    state: State<'_, AppState>,
) -> Result<PreIngestLocalMetadata, String> {
    load_preflight_file_metadata(&state, &job_id, ordinal)
}

#[tauri::command]
fn cancel_preflight_duplicate_scan(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let connection = database(&state)?;
    let changed = connection.execute(
        "UPDATE preflight_scan_jobs SET status = 'cancelled', detail = 'Operator cancelled this pre-ingest check. No selected file was ingested or uploaded.', updated_at = ?1 WHERE id = ?2 AND status IN ('queued', 'running', 'syncing')",
        params![now(), job_id],
    ).map_err(user_error)?;
    if changed != 1 {
        return Err("Only an active pre-ingest duplicate check can be cancelled.".into());
    }
    audit_global(
        &connection,
        "preflight_scan_cancelled",
        "Operator cancelled a persisted pre-ingest duplicate check",
    )?;
    Ok(())
}

#[tauri::command]
async fn prepare_preflight_local_delete_file(
    job_id: String,
    ordinal: u64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        prepare_preflight_local_delete_target(&state, &job_id, ordinal)
    })
    .await
    .map_err(|_| "Preparing local duplicate deletion stopped unexpectedly.".to_string())?
}

#[tauri::command]
async fn delete_preflight_duplicate_file(
    token: String,
    confirmation: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        delete_preflight_duplicate_file_impl(&state, &token, &confirmation)
    })
    .await
    .map_err(|_| "Local duplicate deletion stopped unexpectedly.".to_string())?
}

#[tauri::command]
fn resolve_upload_title_duplicates(
    item_ids: Vec<String>,
    action: String,
    state: State<'_, AppState>,
) -> Result<Vec<UploadItem>, String> {
    resolve_upload_title_duplicates_impl(&state, &item_ids, &action)
}

fn list_youtube_playlists_impl(state: &AppState) -> Result<Vec<YouTubePlaylist>, String> {
    let settings = connection_settings(&database(state)?)?;
    if !settings.connected {
        return Ok(Vec::new());
    }
    let access_token = refreshed_access_token(state)?;
    let mut playlists = Vec::new();
    let mut next_page: Option<String> = None;
    loop {
        let mut query = vec![("part", "snippet"), ("mine", "true"), ("maxResults", "50")];
        if let Some(page) = next_page.as_deref() {
            query.push(("pageToken", page));
        }
        let response = youtube_json(state, &access_token, "playlists", &query)?;
        for playlist in response
            .get("items")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
        {
            let (Some(id), Some(title)) = (
                playlist.get("id").and_then(|value| value.as_str()),
                playlist
                    .pointer("/snippet/title")
                    .and_then(|value| value.as_str()),
            ) else {
                continue;
            };
            playlists.push(YouTubePlaylist {
                id: id.to_string(),
                title: title.to_string(),
            });
        }
        next_page = response
            .get("nextPageToken")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        if next_page.is_none() {
            break;
        }
    }
    playlists.sort_by(|left, right| left.title.to_lowercase().cmp(&right.title.to_lowercase()));
    Ok(playlists)
}

#[tauri::command]
async fn list_youtube_playlists(
    state: State<'_, AppState>,
) -> Result<Vec<YouTubePlaylist>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || list_youtube_playlists_impl(&state))
        .await
        .map_err(|_| "Playlist loading stopped unexpectedly.".to_string())?
}

fn create_youtube_playlist_impl(
    state: &AppState,
    title: String,
) -> Result<YouTubePlaylist, String> {
    let title = valid_new_playlist_title(&title)?;
    let connection = database(state)?;
    if !connection_settings(&connection)?.connected {
        return Err("Connect YouTube before creating a playlist.".into());
    }
    let access_token = refreshed_access_token(state)?;
    let response = provider_transport(state)
        .control_client()?
        .post("https://www.googleapis.com/youtube/v3/playlists")
        .query(&[("part", "id,snippet,status")])
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "snippet": { "title": title },
            "status": { "privacyStatus": "private" }
        }))
        .send()
        .map_err(|_| {
            "YouTube playlist creation could not be reached. Check your connection and try again."
                .to_string()
        })?;
    if !response.status().is_success() {
        let message = youtube_playlist_creation_http_error(response.status().as_u16());
        let _ = audit_global(
            &connection,
            "youtube_playlist_creation_failed",
            "YouTube did not confirm private playlist creation.",
        );
        return Err(message);
    }
    let response: serde_json::Value = response.json().map_err(|_| {
        "YouTube returned an unreadable playlist response. No playlist was confirmed.".to_string()
    })?;
    let id = response
        .get("id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| {
            "YouTube did not return a usable playlist identity. No playlist was confirmed."
                .to_string()
        })?;
    let playlist = YouTubePlaylist {
        id: id.to_string(),
        title,
    };
    audit_global(
        &connection,
        "youtube_playlist_created",
        "Created a private YouTube playlist for upload configuration.",
    )?;
    Ok(playlist)
}

#[tauri::command]
async fn create_youtube_playlist(
    title: String,
    state: State<'_, AppState>,
) -> Result<YouTubePlaylist, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || create_youtube_playlist_impl(&state, title))
        .await
        .map_err(|_| "Playlist creation stopped unexpectedly.".to_string())?
}

fn row_to_remote_video(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemoteVideo> {
    Ok(RemoteVideo {
        video_id: row.get("video_id")?,
        title: row.get("title")?,
        duration: row.get("duration")?,
        privacy_status: row.get("privacy_status")?,
        upload_status: row.get("upload_status")?,
        updated_at: row.get("updated_at")?,
    })
}

fn row_to_deletion_request(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeletionRequest> {
    Ok(DeletionRequest {
        id: row.get("id")?,
        video_id: row.get("video_id")?,
        title: row.get("title")?,
        status: row.get("status")?,
        detail: row.get("detail")?,
        updated_at: row.get("updated_at")?,
    })
}

#[tauri::command]
fn list_remote_videos(state: State<'_, AppState>) -> Result<Vec<RemoteVideo>, String> {
    let connection = database(&state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .ok_or_else(|| "Connect YouTube before viewing its saved library.".to_string())?;
    let mut statement = connection
        .prepare("SELECT video_id, title, duration, privacy_status, upload_status, updated_at FROM remote_videos WHERE channel_id = ?1 ORDER BY updated_at DESC, title ASC")
        .map_err(user_error)?;
    let videos = statement
        .query_map([channel_id], row_to_remote_video)
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(videos)
}

#[tauri::command]
fn list_deletion_requests(state: State<'_, AppState>) -> Result<Vec<DeletionRequest>, String> {
    let connection = database(&state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .ok_or_else(|| "Connect YouTube before viewing deletion requests.".to_string())?;
    let mut statement = connection
        .prepare("SELECT id, video_id, title, status, detail, updated_at FROM deletion_requests WHERE channel_id = ?1 ORDER BY updated_at DESC")
        .map_err(user_error)?;
    let requests = statement
        .query_map([channel_id], row_to_deletion_request)
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(requests)
}

#[tauri::command]
fn request_video_deletion(
    video_id: String,
    confirmation: String,
    state: State<'_, AppState>,
) -> Result<DeletionRequest, String> {
    if confirmation.trim() != video_id {
        return Err("Type the exact YouTube video ID to create a deletion request.".into());
    }
    let connection = database(&state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .ok_or_else(|| {
            "Connect the active YouTube channel before requesting deletion.".to_string()
        })?;
    let title: String = connection
        .query_row(
            "SELECT title FROM remote_videos WHERE video_id = ?1 AND channel_id = ?2",
            params![&video_id, &channel_id],
            |row| row.get(0),
        )
        .map_err(|_| {
            "Sync the active channel library and choose a current video before requesting deletion."
                .to_string()
        })?;
    let id = Uuid::new_v4().to_string();
    let detail =
        "Deletion requested locally. A separately re-authorized execution step is required.";
    connection
        .execute(
            "INSERT INTO deletion_requests (id, video_id, channel_id, title, status, detail, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?6) ON CONFLICT(video_id) DO UPDATE SET channel_id = excluded.channel_id, title = excluded.title, status = 'pending', detail = excluded.detail, updated_at = excluded.updated_at",
            params![id, video_id, channel_id, title, detail, now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_deletion_requested",
        "Operator created a local deletion request",
    )?;
    connection
        .query_row(
            "SELECT id, video_id, title, status, detail, updated_at FROM deletion_requests WHERE video_id = ?1 AND channel_id = ?2",
            params![&video_id, &channel_id],
            row_to_deletion_request,
        )
        .map_err(user_error)
}

#[tauri::command]
fn cancel_deletion_request(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let connection = database(&state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .ok_or_else(|| "Connect YouTube before changing deletion requests.".to_string())?;
    let affected = connection
        .execute(
            "UPDATE deletion_requests SET status = 'cancelled', detail = 'Operator cancelled this local deletion request.', updated_at = ?1 WHERE id = ?2 AND channel_id = ?3 AND status IN ('pending', 'needs_reconciliation')",
            params![now(), id, channel_id],
        )
        .map_err(user_error)?;
    if affected != 1 {
        return Err(
            "Only a pending or recoverable local deletion request can be cancelled.".into(),
        );
    }
    audit_global(
        &connection,
        "youtube_deletion_cancelled",
        "Operator cancelled a local deletion request",
    )?;
    Ok(())
}

#[tauri::command]
fn clear_deletion_requests(state: State<'_, AppState>) -> Result<usize, String> {
    let connection = database(&state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .ok_or_else(|| "Connect YouTube before clearing deletion requests.".to_string())?;
    let changed = connection.execute(
        "UPDATE deletion_requests SET status = 'cancelled', detail = 'Operator cleared this local deletion request. No YouTube video was changed.', updated_at = ?1 WHERE channel_id = ?2 AND status IN ('pending', 'needs_reconciliation')",
        params![now(), channel_id],
    ).map_err(user_error)?;
    if changed > 0 {
        audit_global(
            &connection,
            "youtube_deletion_queue_cleared",
            "Operator cleared pending local deletion requests",
        )?;
    }
    Ok(changed)
}

fn active_channel_id(state: &AppState, access_token: &str) -> Result<String, String> {
    youtube_json(
        state,
        access_token,
        "channels",
        &[("part", "id"), ("mine", "true")],
    )?
    .get("items")
    .and_then(|value| value.as_array())
    .and_then(|items| items.first())
    .and_then(|item| item.get("id"))
    .and_then(|value| value.as_str())
    .map(str::to_string)
    .ok_or_else(|| "YouTube did not return the active channel identity.".to_string())
}

fn execute_deletion_request_impl(
    id: String,
    confirmation: String,
    state: &AppState,
) -> Result<DeletionRequest, String> {
    // Destructive provider work is intentionally serialized even if multiple
    // command invocations race; each request still needs its own typed-ID and
    // fresh channel-ownership verification inside this guard.
    let _execution_guard = deletion_execution_lock(state)
        .lock()
        .map_err(|_| "YouTube deletion execution is unavailable.".to_string())?;
    let connection = database(state)?;
    clear_expired_deletion_authorization(&connection)?;
    let settings = connection_settings(&connection)?;
    if !settings.deletion_authorized {
        return Err(
            "Grant YouTube deletion permission before executing a deletion request.".into(),
        );
    }
    if !settings.deletion_sudo_active {
        return Err("Enter temporary deletion mode before permanently deleting a video.".into());
    }
    let saved_channel_id = settings.active_channel_id.as_deref().ok_or_else(|| {
        "The active YouTube connection has no immutable channel identity.".to_string()
    })?;
    let (video_id, status): (String, String) = connection
        .query_row(
            "SELECT video_id, status FROM deletion_requests WHERE id = ?1 AND channel_id = ?2",
            params![&id, saved_channel_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "This local deletion request no longer exists.".to_string())?;
    if !matches!(status.as_str(), "pending" | "needs_reconciliation") {
        return Err("Only a pending or interrupted deletion request can be executed.".into());
    }
    if confirmation.trim() != video_id {
        return Err("Type the exact YouTube video ID again before permanent deletion.".into());
    }
    let access_token = refreshed_deletion_access_token(state)?;
    let current_channel_id = active_channel_id(state, &access_token)?;
    let video = youtube_json(
        state,
        &access_token,
        "videos",
        &[("part", "snippet"), ("id", video_id.as_str())],
    )?;
    let verified_channel_id = video
        .get("items")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.pointer("/snippet/channelId"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            "The target video is unavailable; deletion was not attempted.".to_string()
        })?;
    if verified_channel_id != current_channel_id {
        return Err("The target video is not owned by the currently authorized channel.".into());
    }
    connection
        .execute(
            "UPDATE deletion_requests SET status = 'executing', detail = 'YouTube deletion is in progress; this request will be reconciled if the app closes before a receipt.', updated_at = ?1 WHERE id = ?2 AND status IN ('pending', 'needs_reconciliation')",
            params![now(), id],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_deletion_execution_started",
        "Deletion execution checkpoint saved before contacting YouTube",
    )?;
    let response = provider_transport(state)
        .control_client()?
        .delete("https://www.googleapis.com/youtube/v3/videos")
        .query(&[("id", video_id.as_str())])
        .bearer_auth(&access_token)
        .send()
        .map_err(|_| {
            "YouTube deletion could not be reached; the saved request needs reconciliation."
                .to_string()
        })?;
    if response.status().as_u16() != 204 {
        return Err(
            "YouTube rejected the deletion; the saved request needs reconciliation.".into(),
        );
    }
    connection
        .execute(
            "UPDATE deletion_requests SET status = 'deleted', detail = 'YouTube returned HTTP 204 after fresh channel-ownership validation.', updated_at = ?1 WHERE id = ?2",
            params![now(), id],
        )
        .map_err(user_error)?;
    connection
        .execute(
            "DELETE FROM remote_videos WHERE video_id = ?1 AND channel_id = ?2",
            params![&video_id, saved_channel_id],
        )
        .map_err(user_error)?;
    connection
        .execute(
            "INSERT INTO channel_generations(channel_id, inventory_generation, upload_generation, updated_at) VALUES (?1, 1, 0, ?2) ON CONFLICT(channel_id) DO UPDATE SET inventory_generation = inventory_generation + 1, updated_at = excluded.updated_at",
            params![saved_channel_id, now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_deletion_completed",
        "YouTube returned HTTP 204 for a confirmed deletion",
    )?;
    connection
        .query_row(
            "SELECT id, video_id, title, status, detail, updated_at FROM deletion_requests WHERE id = ?1",
            [&id],
            row_to_deletion_request,
        )
        .map_err(user_error)
}

#[tauri::command]
async fn execute_deletion_request(
    id: String,
    confirmation: String,
    state: State<'_, AppState>,
) -> Result<DeletionRequest, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        execute_deletion_request_impl(id, confirmation, &state)
    })
    .await
    .map_err(|_| {
        "YouTube deletion stopped unexpectedly; the request needs reconciliation.".to_string()
    })?
}

fn import_asset_impl(
    path: String,
    settings: ManualUploadSettings,
    state: &AppState,
) -> Result<UploadItem, String> {
    let source = PathBuf::from(&path);
    let metadata = fs::metadata(&source).map_err(user_error)?;
    if !metadata.is_file() {
        return Err("Select a video file, not a directory.".into());
    }
    if !is_supported_monitored_video(&source) {
        return Err("Select a supported video file to import.".into());
    }
    validate_youtube_upload_limits(&source, metadata.len())?;
    let visibility = valid_upload_visibility(&settings.visibility)?;
    let raw_playlist_id = settings
        .playlist_id
        .filter(|value| !value.trim().is_empty());
    let raw_playlist_title = if raw_playlist_id.is_some() {
        settings
            .playlist_title
            .filter(|value| !value.trim().is_empty())
    } else {
        None
    };
    let (playlist_id, playlist_title) =
        valid_playlist_selection(raw_playlist_id, raw_playlist_title)?;

    let (signature, digest) = verify_reference_source(&source, None)?;
    if signature.0 != metadata.len() {
        return Err("The selected source changed before it could be verified.".into());
    }
    let id = Uuid::new_v4().to_string();
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "The selected file has no supported name.".to_string())?
        .to_string();
    let title = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled video")
        .to_string();
    let title_keys = title_matching::keys(&title);
    let timestamp = now();
    let connection = database(state)?;

    connection
        .execute(
            "INSERT INTO upload_items (id, title, normalized_title, canonical_title, has_copy_marker, numeric_title_key, title_keys_version, file_name, source_path, workspace_path, size_bytes, digest, imported_bytes, status, total_bytes, visibility, made_for_kids, delete_source_after_upload, playlist_id, playlist_title, created_at, updated_at, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?11, ?10, 'draft', ?10, ?12, ?13, ?14, ?15, ?16, ?17, ?17, 'Original source verified in place; ready for review')",
            params![id, title, title_keys.normalized, title_keys.canonical, title_keys.has_copy_marker as i64, title_keys.numeric, title_matching::TITLE_KEYS_VERSION, file_name, path, metadata.len() as i64, digest, visibility, settings.made_for_kids as i64, settings.delete_source_after_upload as i64, playlist_id, playlist_title, timestamp],
        )
        .map_err(user_error)?;
    audit(
        &connection,
        &id,
        "asset_reference_verified",
        "Original media was verified in place with operator-reviewed audience, visibility, and playlist settings",
    )?;
    find_item(&connection, &id)
}

#[tauri::command]
async fn import_asset(
    path: String,
    settings: ManualUploadSettings,
    state: State<'_, AppState>,
) -> Result<UploadItem, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || import_asset_impl(path, settings, &state))
        .await
        .map_err(|_| {
            "Local media import stopped unexpectedly; its checkpoint will be recovered on launch."
                .to_string()
        })?
}

fn safe_batch_file_name(path: &str, ordinal: usize) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Selected video {}", ordinal + 1))
}

fn import_and_queue_batch_impl(
    paths: Vec<String>,
    settings: ManualUploadSettings,
    state: &AppState,
) -> Result<BatchImportReceipt, String> {
    if paths.is_empty() {
        return Err("Choose at least one video to import.".into());
    }
    if paths.len() > 512 {
        return Err("Choose no more than 512 videos in one intake batch.".into());
    }
    let requested_count = paths.len() as u64;
    let mut receipts = Vec::with_capacity(paths.len());
    let mut imported_ids = Vec::new();
    let mut receipt_by_id = HashMap::<String, usize>::new();
    for (ordinal, path) in paths.into_iter().enumerate() {
        let file_name = safe_batch_file_name(&path, ordinal);
        match import_asset_impl(path, settings.clone(), state) {
            Ok(item) => {
                receipt_by_id.insert(item.id.clone(), receipts.len());
                imported_ids.push(item.id.clone());
                receipts.push(BatchImportItemReceipt {
                    ordinal: ordinal as u64,
                    file_name,
                    status: "imported".into(),
                    item: Some(item),
                    detail: None,
                });
            }
            Err(error) => receipts.push(BatchImportItemReceipt {
                ordinal: ordinal as u64,
                file_name,
                status: "failed".into(),
                item: None,
                detail: Some(diagnostic_detail(&error)),
            }),
        }
    }

    let connection_settings = connection_settings(&database(state)?)?;
    let mut batch_detail = None;
    if connection_settings.connected && !imported_ids.is_empty() {
        match check_upload_title_duplicates_impl(state, &imported_ids) {
            Ok(duplicates) => {
                let duplicate_ids = duplicates
                    .into_iter()
                    .map(|candidate| candidate.item_id)
                    .collect::<HashSet<_>>();
                let queue_ids = imported_ids
                    .iter()
                    .filter(|item_id| !duplicate_ids.contains(*item_id))
                    .cloned()
                    .collect::<Vec<_>>();
                let mut queue_results = queue_items_batch_impl(state, &queue_ids)?
                    .into_iter()
                    .collect::<HashMap<_, _>>();
                for item_id in &imported_ids {
                    let Some(receipt_index) = receipt_by_id.get(item_id).copied() else {
                        continue;
                    };
                    if duplicate_ids.contains(item_id) {
                        receipts[receipt_index].status = "duplicate_review".into();
                        receipts[receipt_index].item = find_item(&database(state)?, item_id).ok();
                        continue;
                    }
                    match queue_results
                        .remove(item_id)
                        .unwrap_or_else(|| Err("This imported item could not be queued.".into()))
                    {
                        Ok(item) => {
                            receipts[receipt_index].status = "queued".into();
                            receipts[receipt_index].item = Some(item);
                        }
                        Err(error) => {
                            receipts[receipt_index].status = "failed".into();
                            receipts[receipt_index].detail = Some(diagnostic_detail(&error));
                            receipts[receipt_index].item =
                                find_item(&database(state)?, item_id).ok();
                        }
                    }
                }
                if receipts.iter().any(|receipt| receipt.status == "queued") {
                    ensure_quota_resume_worker(state.clone())?;
                    let _ = start_queued_uploads_impl(state);
                }
            }
            Err(error) => {
                batch_detail = Some(format!(
                    "Videos were imported safely, but the channel title check did not complete: {}",
                    diagnostic_detail(&error)
                ));
            }
        }
    }
    Ok(BatchImportReceipt {
        requested_count,
        imported_count: receipts
            .iter()
            .filter(|receipt| receipt.item.is_some())
            .count() as u64,
        queued_count: receipts
            .iter()
            .filter(|receipt| receipt.status == "queued")
            .count() as u64,
        duplicate_count: receipts
            .iter()
            .filter(|receipt| receipt.status == "duplicate_review")
            .count() as u64,
        failed_count: receipts
            .iter()
            .filter(|receipt| receipt.status == "failed")
            .count() as u64,
        items: receipts,
        detail: batch_detail,
    })
}

#[tauri::command]
async fn import_and_queue_batch(
    paths: Vec<String>,
    settings: ManualUploadSettings,
    state: State<'_, AppState>,
) -> Result<BatchImportReceipt, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        import_and_queue_batch_impl(paths, settings, &state)
    })
    .await
    .map_err(|_| {
        "The local intake batch stopped unexpectedly; completed imports remain recoverable."
            .to_string()
    })?
}

fn set_item_visibility_impl(
    state: &AppState,
    id: &str,
    visibility: &str,
) -> Result<UploadItem, String> {
    let visibility = valid_upload_visibility(visibility)?;
    let connection = database(state)?;
    let channel_name: Option<String> = connection
        .query_row(
            "SELECT channel_name FROM upload_items WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(|_| "This upload item no longer exists.".to_string())?;
    if channel_name.is_some() {
        return Err("Watched-folder uploads always remain private.".into());
    }
    let changed = connection
        .execute(
            "UPDATE upload_items SET visibility = ?1, detail = 'Upload visibility saved locally.', updated_at = ?2 WHERE id = ?3 AND status IN ('draft', 'failed')",
            params![visibility, now(), id],
        )
        .map_err(user_error)?;
    if changed == 0 {
        return Err("Choose visibility before this item is queued for upload.".into());
    }
    audit(
        &connection,
        id,
        "upload_visibility_set",
        &format!("Upload visibility saved locally as {visibility}."),
    )?;
    find_item(&connection, id)
}

#[tauri::command]
fn set_item_visibility(
    id: String,
    visibility: String,
    state: State<'_, AppState>,
) -> Result<UploadItem, String> {
    set_item_visibility_impl(&state, &id, &visibility)
}

fn set_item_delete_source_after_upload_impl(
    state: &AppState,
    id: &str,
    delete_source_after_upload: bool,
) -> Result<UploadItem, String> {
    let connection = database(state)?;
    let changed = connection
        .execute(
            "UPDATE upload_items SET delete_source_after_upload = ?1, source_delete_status = NULL, updated_at = ?2 WHERE id = ?3 AND status IN ('draft', 'failed')",
            params![delete_source_after_upload as i64, now(), id],
        )
        .map_err(user_error)?;
    if changed == 0 {
        return Err("Set source cleanup before this item is queued for upload.".into());
    }
    audit(
        &connection,
        id,
        "source_cleanup_preference_set",
        if delete_source_after_upload {
            "Operator enabled original-source deletion after a confirmed YouTube upload."
        } else {
            "Operator disabled original-source deletion after a confirmed YouTube upload."
        },
    )?;
    find_item(&connection, id)
}

#[tauri::command]
async fn set_item_delete_source_after_upload(
    id: String,
    delete_source_after_upload: bool,
    state: State<'_, AppState>,
) -> Result<UploadItem, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        set_item_delete_source_after_upload_impl(&state, &id, delete_source_after_upload)
    })
    .await
    .map_err(|_| "Saving the source cleanup choice stopped unexpectedly.".to_string())?
}

#[tauri::command]
async fn delete_uploaded_source(
    state: State<'_, AppState>,
    id: String,
    confirmation: String,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        delete_uploaded_source_impl(&state, &id, &confirmation)
    })
    .await
    .map_err(|_| "Original source cleanup stopped unexpectedly.".to_string())?
}

fn queue_items_batch_impl(
    state: &AppState,
    item_ids: &[String],
) -> Result<Vec<(String, Result<UploadItem, String>)>, String> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut connection = database(state)?;
    let settings = connection_settings(&connection)?;
    let channel_name = settings
        .active_channel
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "Connect the reviewed YouTube channel before queueing this upload.".to_string()
        })?;
    let channel_id = settings
        .active_channel_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "The active YouTube connection has no immutable channel identity.".to_string()
        })?;
    let transaction = connection.transaction().map_err(user_error)?;
    let mut results = Vec::with_capacity(item_ids.len());
    for item_id in item_ids {
        let validation = transaction
            .query_row(
                "SELECT status, duplicate_decision, workspace_path FROM upload_items WHERE id = ?1",
                [item_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(user_error)?;
        let result = match validation {
            None => Err("This upload item no longer exists.".into()),
            Some((status, _, _)) if status != "draft" && status != "failed" => {
                Err("Only a reviewed draft or recoverable failed item can be queued.".into())
            }
            Some((_, Some(decision), _)) if decision == "pending" => Err(
                "A matching uploaded title needs a decision before this file can be queued.".into(),
            ),
            Some((_, _, workspace_path)) if !Path::new(&workspace_path).is_file() => Err(
                "This item has not finished its device-local import; repair it before queueing."
                    .into(),
            ),
            Some(_) => transaction
                .execute(
                    "UPDATE upload_items SET channel_name = ?1, channel_id = ?2, status = 'queued', detail = 'Saved in local queue for the reviewed YouTube channel.', updated_at = ?3 WHERE id = ?4",
                    params![&channel_name, &channel_id, now(), item_id],
                )
                .map_err(user_error)
                .and_then(|_| {
                    audit(&transaction, item_id, "item_queued", "Queue state saved locally")?;
                    find_item(&transaction, item_id)
                }),
        };
        results.push((item_id.clone(), result));
    }
    transaction.commit().map_err(user_error)?;
    Ok(results)
}

fn queue_item_impl(state: &AppState, id: &str) -> Result<UploadItem, String> {
    let connection = database(state)?;
    let item = find_item(&connection, &id)?;
    if item.status != "draft" && item.status != "failed" {
        return Err("Only a reviewed draft or recoverable failed item can be queued.".into());
    }
    let duplicate_decision: Option<String> = connection
        .query_row(
            "SELECT duplicate_decision FROM upload_items WHERE id = ?1",
            [&id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if duplicate_decision.as_deref() == Some("pending") {
        return Err(
            "A matching uploaded title needs a decision before this file can be queued.".into(),
        );
    }
    let workspace_path: String = connection
        .query_row(
            "SELECT workspace_path FROM upload_items WHERE id = ?1",
            [&id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if !Path::new(&workspace_path).is_file() {
        return Err(
            "This item has not finished its device-local import; repair it before queueing.".into(),
        );
    }
    let settings = connection_settings(&connection)?;
    let channel_name = settings
        .active_channel
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "Connect the reviewed YouTube channel before queueing this upload.".to_string()
        })?;
    let channel_id = settings
        .active_channel_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "The active YouTube connection has no immutable channel identity.".to_string()
        })?;
    connection
        .execute(
            "UPDATE upload_items SET channel_name = ?1, channel_id = ?2, status = 'queued', detail = 'Saved in local queue for the reviewed YouTube channel.', updated_at = ?3 WHERE id = ?4",
            params![channel_name, channel_id, now(), id],
        )
        .map_err(user_error)?;
    audit(&connection, &id, "item_queued", "Queue state saved locally")?;
    find_item(&connection, id)
}

#[tauri::command]
fn queue_item(id: String, state: State<'_, AppState>) -> Result<UploadItem, String> {
    let item = queue_item_impl(&state, &id)?;
    // Queueing is the operator's final upload decision. Dispatch is automatic
    // and uses the same capacity-aware scheduler as watched folders and
    // startup recovery; the webview never needs a global start control.
    ensure_quota_resume_worker(state.inner().clone())?;
    let _ = start_queued_uploads_impl(&state);
    Ok(item)
}

#[tauri::command]
fn clear_upload_queue(state: State<'_, AppState>) -> Result<usize, String> {
    let connection = database(&state)?;
    let changed = connection.execute(
        "UPDATE upload_items SET status = 'cancelled', detail = 'Operator removed this local upload job from the queue. The original media remains in its chosen location; an in-flight request stops at its next checkpoint.', updated_at = ?1 WHERE status IN ('importing', 'draft', 'queued', 'dispatching', 'uploading', 'needs_reconciliation', 'failed')",
        [now()],
    ).map_err(user_error)?;
    if changed > 0 {
        audit_global(
            &connection,
            "upload_queue_cleared",
            "Operator cleared local upload jobs without deleting original media",
        )?;
    }
    Ok(changed)
}

#[tauri::command]
fn cancel_upload_item(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let connection = database(&state)?;
    let changed = connection
        .execute(
            "UPDATE upload_items SET status = 'cancelled', detail = 'Operator removed this local upload job from the queue. The original media remains in its chosen location; an in-flight request stops at its next checkpoint.', updated_at = ?1 WHERE id = ?2 AND status IN ('importing', 'draft', 'queued', 'dispatching', 'uploading', 'needs_reconciliation', 'failed')",
            params![now(), id],
        )
        .map_err(user_error)?;
    if changed == 0 {
        return Err("This upload is already completed or no longer in the queue.".into());
    }
    audit(
        &connection,
        &id,
        "upload_item_cancelled",
        "Operator removed this individual upload from the local queue",
    )?;
    Ok(())
}

fn reconcile_queue_impl(state: &AppState) -> Result<Vec<UploadItem>, String> {
    let connection = database(state)?;
    let mut statement = connection
        .prepare("SELECT id, source_path, workspace_path, partial_path, size_bytes, status FROM upload_items WHERE status IN ('importing', 'dispatching', 'uploading')")
        .map_err(user_error)?;
    let interrupted = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)? as u64,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;

    for (id, _source_path, _workspace_path, _partial_path, _expected_bytes, status) in interrupted {
        if status == "dispatching" {
            let detail = "Recovered an automatic upload before network dispatch.";
            connection
                .execute(
                    "UPDATE upload_items SET status = 'queued', detail = ?1, updated_at = ?2 WHERE id = ?3",
                    params![detail, now(), id],
                )
                .map_err(user_error)?;
            connection
                .execute(
                    "UPDATE folder_monitor_observations SET state = 'queued', updated_at = ?1 WHERE upload_item_id = ?2 AND state = 'dispatched'",
                    params![now(), id],
                )
                .map_err(user_error)?;
            audit(&connection, &id, "restart_reconciliation", detail)?;
            continue;
        }
        if status == "importing" {
            let detail = "Interrupted managed-copy import cannot resume in no-copy mode; restore and reselect the original source.";
            connection.execute("UPDATE upload_items SET status = 'failed', detail = ?1, updated_at = ?2 WHERE id = ?3", params![detail, now(), id]).map_err(user_error)?;
            audit(&connection, &id, "restart_reconciliation", detail)?;
            continue;
        }
        let (next_status, detail) = if stored_upload_session(&id)?.is_some() {
            (
                "queued",
                "Recovered interrupted upload; its saved YouTube resumable session will resume from the provider-confirmed range.",
            )
        } else {
            (
                "needs_reconciliation",
                "App closed during upload before a resumable session was available; verify the YouTube channel before retrying.",
            )
        };
        connection
            .execute(
                "UPDATE upload_items SET status = ?1, detail = ?2, updated_at = ?3 WHERE id = ?4",
                params![next_status, detail, now(), id],
            )
            .map_err(user_error)?;
        audit(&connection, &id, "restart_reconciliation", detail)?;
    }

    // Older watched-folder releases could persist the observation claim without
    // persisting the matching item dispatch. Make those orphaned claims visible
    // to the next local scan without initiating any network work here.
    connection
        .execute(
            "UPDATE folder_monitor_observations SET state = 'queued', updated_at = ?1 WHERE state = 'dispatched' AND (upload_item_id IS NULL OR NOT EXISTS (SELECT 1 FROM upload_items WHERE upload_items.id = folder_monitor_observations.upload_item_id))",
            params![now()],
        )
        .map_err(user_error)?;

    let active_channel_id = connection_settings(&connection)?.active_channel_id;

    let items = if let Some(channel_id) = active_channel_id.as_deref() {
        connection
            .prepare("SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, visibility, made_for_kids, playlist_id, playlist_title, upload_started_at, transfer_bytes_per_second, delete_source_after_upload, source_delete_status, updated_at FROM upload_items WHERE status != 'cancelled' AND channel_id = ?1 ORDER BY updated_at DESC")
            .map_err(user_error)?
            .query_map([channel_id], row_to_upload_item)
            .map_err(user_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(user_error)?
    } else {
        connection
            .prepare("SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, visibility, made_for_kids, playlist_id, playlist_title, upload_started_at, transfer_bytes_per_second, delete_source_after_upload, source_delete_status, updated_at FROM upload_items WHERE status != 'cancelled' AND (channel_id IS NULL OR channel_id = '') ORDER BY updated_at DESC")
            .map_err(user_error)?
            .query_map([], row_to_upload_item)
            .map_err(user_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(user_error)?
    };
    Ok(items)
}

#[tauri::command]
async fn reconcile_queue(state: State<'_, AppState>) -> Result<Vec<UploadItem>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || reconcile_queue_impl(&state))
        .await
        .map_err(|_| "Queue recovery stopped unexpectedly.".to_string())?
}

/// Ends the native application after the webview has received an explicit
/// operator confirmation. This does not issue a window-close request, so the
/// ordinary close-confirmation listener cannot intercept it a second time.
#[tauri::command]
fn exit_application(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    performance::mark_process_start();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let state = initialize_state(app.handle())
                .map_err(|error| Box::<dyn std::error::Error>::from(error))?;
            #[cfg(feature = "performance-harness")]
            let performance_snapshot_path = state
                .database_path
                .with_file_name("native-performance-startup.json");
            install_panic_marker(state.database_path.with_file_name(PANIC_MARKER_FILE));
            app.manage(state.clone());
            #[cfg(not(test))]
            state
                .state_events
                .start(app.handle().clone(), state.database_path.clone())
                .map_err(|error| Box::<dyn std::error::Error>::from(error))?;
            #[cfg(feature = "performance-harness")]
            performance::write_startup_snapshot(&performance_snapshot_path)
                .map_err(|error| Box::<dyn std::error::Error>::from(error))?;
            Ok(())
        })
        .invoke_handler({
            let handler: Box<tauri::ipc::InvokeHandler<tauri::Wry>> =
                Box::new(tauri::generate_handler![
                    dashboard_snapshot,
                    load_state_changes,
                    startup_bootstrap,
                    complete_startup_after_safe_shell,
                    github_issue_diagnostic_report,
                    native_performance_report,
                    #[cfg(feature = "performance-harness")]
                    mark_performance_milestone,
                    app_release_identity,
                    load_crash_recovery_status,
                    record_webview_error,
                    acknowledge_crash_recovery,
                    load_folder_monitor_settings,
                    load_folder_monitor_overview,
                    enable_folder_monitor,
                    disable_folder_monitor,
                    scan_folder_monitor_now,
                    process_existing_folder_files,
                    load_connection_settings,
                    load_manual_upload_defaults,
                    save_manual_upload_defaults,
                    import_desktop_oauth_client,
                    export_portable_archive,
                    import_portable_archive,
                    begin_youtube_connection,
                    cancel_youtube_connection,
                    disconnect_youtube,
                    sync_channel_inventory,
                    resume_queued_uploads,
                    check_upload_title_duplicates,
                    ignore_duplicate_candidate,
                    re_audit_ignored_duplicate_candidates,
                    start_preflight_duplicate_files,
                    load_preflight_duplicate_scan,
                    load_preflight_duplicate_scan_status,
                    load_preflight_duplicate_file_metadata,
                    cancel_preflight_duplicate_scan,
                    prepare_preflight_local_delete_file,
                    delete_preflight_duplicate_file,
                    resolve_upload_title_duplicates,
                    list_remote_videos,
                    list_deletion_requests,
                    request_video_deletion,
                    cancel_deletion_request,
                    clear_deletion_requests,
                    begin_deletion_authorization,
                    enable_deletion_sudo_mode,
                    disable_deletion_sudo_mode,
                    execute_deletion_request,
                    import_asset,
                    import_and_queue_batch,
                    list_youtube_playlists,
                    create_youtube_playlist,
                    set_item_visibility,
                    set_item_delete_source_after_upload,
                    delete_uploaded_source,
                    queue_item,
                    clear_upload_queue,
                    cancel_upload_item,
                    reconcile_queue,
                    exit_application
                ]);
            move |invoke| {
                performance::record_native_invoke();
                handler(invoke)
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running local application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_upload_limits_reject_only_files_beyond_the_published_maximums() {
        assert!(youtube_upload_size_limit_error(YOUTUBE_MAX_UPLOAD_BYTES).is_none());
        assert!(
            youtube_upload_size_limit_error(YOUTUBE_MAX_UPLOAD_BYTES + 1)
                .unwrap()
                .contains("256 GB")
        );
        assert!(
            youtube_upload_duration_limit_error(Some(YOUTUBE_MAX_UPLOAD_DURATION_SECONDS,))
                .is_none()
        );
        assert!(youtube_upload_duration_limit_error(Some(
            YOUTUBE_MAX_UPLOAD_DURATION_SECONDS + 0.01,
        ))
        .unwrap()
        .contains("12 hours"));
        assert!(youtube_upload_duration_limit_error(None).is_none());
    }

    #[test]
    fn inventory_sync_errors_are_actionable_without_provider_payloads() {
        assert!(youtube_inventory_http_error(401).contains("Connect YouTube again"));
        assert!(youtube_inventory_http_error(403).contains("YouTube Data API"));
        assert!(youtube_inventory_http_error(429).contains("rate-limiting"));
        assert!(youtube_inventory_http_error(503).contains("last complete local library"));
    }

    #[test]
    fn inventory_refresh_clears_abandoned_staging_rows_before_a_new_run() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        fs::write(state.media_directory.join("active.media"), b"active").unwrap();
        fs::write(state.media_directory.join("waiting.media"), b"waiting").unwrap();
        connection
            .execute(
                "INSERT INTO remote_video_sync_staging (sync_id, video_id, channel_name, channel_id, title, updated_at) VALUES ('interrupted', 'video-a', 'Channel A', 'channel-a-id', 'Old row', ?1)",
                [now()],
            )
            .unwrap();
        clear_stale_inventory_staging(&connection, "channel-a-id").unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM remote_video_sync_staging",
                    [],
                    |row| { row.get::<_, i64>(0) }
                )
                .unwrap(),
            0
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_playlist_entries_upsert_one_staged_video_record() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        for title in ["First API copy", "Current API copy"] {
            connection
                .execute(
                    "INSERT INTO remote_video_sync_staging (sync_id, video_id, channel_name, channel_id, title, updated_at) VALUES ('sync-a', 'video-a', 'Channel A', 'channel-a-id', ?1, ?2) ON CONFLICT(sync_id, video_id) DO UPDATE SET title = excluded.title, updated_at = excluded.updated_at",
                    params![title, now()],
                )
                .unwrap();
        }
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM remote_video_sync_staging WHERE sync_id = 'sync-a'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT title FROM remote_video_sync_staging WHERE sync_id = 'sync-a' AND video_id = 'video-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "Current API copy"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inventory_commit_reopens_and_replaces_only_the_active_channel_snapshot() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, updated_at) VALUES ('other-video', 'Other channel', 'channel-b-id', 'Keep', ?1)",
                [now()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO remote_video_sync_staging (sync_id, video_id, channel_name, channel_id, title, updated_at) VALUES ('sync-a', 'fresh-video', 'Channel A', 'channel-a-id', 'Fresh', ?1)",
                [now()],
            )
            .unwrap();
        drop(connection);

        replace_inventory_from_staging(&state, "channel-a-id", "sync-a").unwrap();

        let connection = database(&state).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM remote_videos", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM remote_video_sync_staging WHERE sync_id = 'sync-a'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inventory_database_connections_use_wal_and_a_bounded_busy_wait() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        let busy_timeout: i64 = connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();

        assert_eq!(journal_mode, "wal");
        assert_eq!(busy_timeout, DATABASE_BUSY_TIMEOUT.as_millis() as i64);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(feature = "performance-harness")]
    #[test]
    fn current_schema_hot_open_executes_no_schema_or_journal_transition_sql() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        drop(database(&state).unwrap());

        persistence::begin_test_database_trace();
        drop(database(&state).unwrap());
        let statements = persistence::take_test_database_trace();
        let prohibited = statements
            .iter()
            .filter(|statement| {
                let normalized = statement.to_ascii_uppercase();
                normalized.contains("CREATE ")
                    || normalized.contains("ALTER ")
                    || normalized.contains("TABLE_INFO")
                    || normalized.contains("JOURNAL_MODE")
            })
            .collect::<Vec<_>>();
        assert!(prohibited.is_empty(), "hot-open schema SQL: {prohibited:?}");
        assert!(
            statements
                .iter()
                .any(|statement| statement.to_ascii_uppercase().contains("FOREIGN_KEYS")),
            "per-connection foreign-key enforcement must remain explicit: {statements:?}"
        );
        assert_eq!(
            statements.len(),
            1,
            "a steady-state open should execute only the required per-connection foreign-key pragma: {statements:?}"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsupported_future_schema_error_keeps_operator_context() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = Connection::open(&state.database_path).unwrap();
        connection
            .pragma_update(
                None,
                "user_version",
                persistence::DATABASE_SCHEMA_VERSION + 1,
            )
            .unwrap();
        drop(connection);

        let error = initialize_database(&state).unwrap_err();
        assert!(
            error.starts_with("The local database schema could not be upgraded safely:"),
            "{error}"
        );
        assert!(error.contains("supports up to"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_inventory_tables_receive_required_channel_columns() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let database_path = root.join("queue.sqlite3");
        let legacy = Connection::open(&database_path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE remote_videos (video_id TEXT PRIMARY KEY NOT NULL, channel_name TEXT NOT NULL, title TEXT NOT NULL, updated_at TEXT NOT NULL); CREATE TABLE remote_video_sync_staging (sync_id TEXT NOT NULL, video_id TEXT NOT NULL, channel_name TEXT NOT NULL, title TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(sync_id, video_id));",
            )
            .unwrap();
        drop(legacy);
        let state = AppState {
            database_path,
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };

        initialize_database(&state).unwrap();
        let connection = database(&state).unwrap();
        for table in ["remote_videos", "remote_video_sync_staging"] {
            let columns = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(columns.iter().any(|column| column == "channel_id"));
        }
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundled_sidecar_is_resolved_before_a_path_fallback() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let executable = root.join("app").join("bin").join("uploader.exe");
        let sidecar = executable.parent().unwrap().join("ffprobe.exe");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, []).unwrap();
        fs::write(&sidecar, []).unwrap();

        assert_eq!(
            bundled_sidecar_path(&executable, "ffprobe.exe"),
            Some(sidecar)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn diagnostic_details_redact_sensitive_values_and_paths() {
        assert_eq!(
            diagnostic_detail("OAuth failed at C:\\Users\\operator\\client.json"),
            "[redacted sensitive detail]"
        );
        assert_eq!(
            diagnostic_detail("provider returned access_token=not-for-a-report"),
            "[redacted sensitive detail]"
        );
        assert_eq!(
            diagnostic_detail("Queued upload paused for the daily limit."),
            "Queued upload paused for the daily limit."
        );
        assert_eq!(safe_diagnostic_issue_name("upload_failed"), "upload_failed");
        assert_eq!(
            safe_diagnostic_issue_name("Error at C:\\private"),
            "unclassified_local_event"
        );
    }

    #[test]
    fn diagnostic_report_is_markdown_and_excludes_sensitive_audit_details() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO audit_events (id, kind, detail, created_at) VALUES ('diagnostic-test', 'upload_failed', 'Provider response at C:\\Users\\operator\\token.txt', ?1)",
                [now()],
            )
            .unwrap();
        drop(connection);

        let report = diagnostic_report_impl(&state).unwrap();
        assert!(report.starts_with("# YouTube Upload Manager diagnostic report"));
        assert!(report.contains(&format!("- App version: {}", env!("CARGO_PKG_VERSION"))));
        assert!(report.contains(&format!("- Release channel: {APP_RELEASE_CHANNEL}")));
        assert!(report.contains("`upload_failed`"));
        assert!(report.contains("## Detected crash, error, and warning names"));
        assert!(report.contains("[redacted sensitive detail]"));
        assert!(!report.contains("C:\\Users\\operator"));
        assert!(!report.contains("token.txt"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn release_identity_reports_the_compiled_version_and_supported_channel() {
        let identity = release_identity();

        assert_eq!(identity.version, env!("CARGO_PKG_VERSION"));
        assert!(
            identity.channel == "regular"
                || identity.channel.starts_with("nightly-")
                || identity.channel.starts_with('v')
        );
        assert!(matches!(
            identity.build_profile.as_str(),
            "debug" | "release"
        ));
    }

    #[test]
    fn crash_recovery_status_uses_only_valid_timestamp_markers() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let panic_path = root.join(PANIC_MARKER_FILE);
        let webview_path = root.join(WEBVIEW_ERROR_MARKER_FILE);
        fs::write(&panic_path, "panic_at=2026-08-22T01:02:03Z\n").unwrap();
        fs::write(&webview_path, "webview_error_at=not-a-timestamp\n").unwrap();

        assert_eq!(
            crash_recovery_status_for_paths(&panic_path, &webview_path),
            CrashRecoveryStatus {
                crash_detected: true,
                detected_at: Some("2026-08-22T01:02:03+00:00".into()),
                failure_kind: Some("Native panic".into()),
            }
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recording_and_acknowledging_webview_errors_only_persists_a_timestamp() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };

        record_webview_error_impl(&state).unwrap();
        let marker = fs::read_to_string(root.join(WEBVIEW_ERROR_MARKER_FILE)).unwrap();
        assert!(marker.starts_with("webview_error_at="));
        assert!(
            marker_timestamp(&root.join(WEBVIEW_ERROR_MARKER_FILE), "webview_error_at=").is_some()
        );
        assert!(crash_recovery_status(&state).crash_detected);

        acknowledge_crash_recovery_impl(&state).unwrap();
        assert!(!crash_recovery_status(&state).crash_detected);
        assert!(!root.join(WEBVIEW_ERROR_MARKER_FILE).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fresh_connection_settings_require_an_operator_imported_oauth_client() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE connection_settings (singleton INTEGER PRIMARY KEY, oauth_client_id TEXT, active_channel TEXT, active_channel_id TEXT, connection_detail TEXT, deletion_authorized INTEGER NOT NULL DEFAULT 0, deletion_sudo_until TEXT)",
            )
            .unwrap();

        let settings = connection_settings(&connection).unwrap();
        assert!(!settings.oauth_configured);
        assert!(configured_oauth_client_id(&connection).is_err());
    }

    #[test]
    fn queueing_a_manual_upload_binds_the_active_immutable_channel() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let workspace = state.media_directory.join("manual.media");
        fs::write(&workspace, b"manual-upload").unwrap();
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, oauth_client_id, active_channel, active_channel_id, updated_at) VALUES (1, 'client.apps.googleusercontent.com', 'Reviewed channel', 'UC-reviewed', ?1)",
                [now()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES ('manual', 'Manual', 'manual.mp4', ?1, 13, 'draft', 13, ?2, ?2)",
                params![workspace.to_string_lossy(), now()],
            )
            .unwrap();
        drop(connection);

        queue_item_impl(&state, "manual").unwrap();

        let connection = database(&state).unwrap();
        let scope: (Option<String>, Option<String>, String) = connection
            .query_row(
                "SELECT channel_name, channel_id, status FROM upload_items WHERE id = 'manual'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(scope.0.as_deref(), Some("Reviewed channel"));
        assert_eq!(scope.1.as_deref(), Some("UC-reviewed"));
        assert_eq!(scope.2, "queued");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dispatching_manual_upload_pauses_when_a_different_channel_is_active() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let workspace = state.media_directory.join("manual.media");
        fs::write(&workspace, b"manual-upload").unwrap();
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, oauth_client_id, active_channel, active_channel_id, updated_at) VALUES (1, 'client.apps.googleusercontent.com', 'Channel B', 'UC-channel-b', ?1)",
                [now()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES ('manual', 'Manual', 'manual.mp4', 'Channel A', 'UC-channel-a', ?1, 13, 'dispatching', 13, ?2, ?2)",
                params![workspace.to_string_lossy(), now()],
            )
            .unwrap();
        drop(connection);

        upload_item(&state, "manual").unwrap();

        let connection = database(&state).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id = 'manual'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "queued"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_watched_source_is_withheld_before_provider_dispatch() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let watched_source = root.join("watched.mp4");
        fs::write(&watched_source, b"watched-source").unwrap();
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, oauth_client_id, active_channel, active_channel_id, updated_at) VALUES (1, 'client.apps.googleusercontent.com', 'Reviewed channel', 'UC-reviewed', ?1)",
                [now()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, source_path, workspace_path, size_bytes, digest, source_modified_key, status, total_bytes, created_at, updated_at) VALUES ('watched', 'Watched', 'watched.mp4', 'Reviewed channel', 'UC-reviewed', ?1, ?1, 14, 'expected-digest', 'stale-signature', 'dispatching', 14, ?2, ?2)",
                params![watched_source.to_string_lossy(), now()],
            )
            .unwrap();
        drop(connection);

        upload_item(&state, "watched").unwrap();

        let connection = database(&state).unwrap();
        let result: (String, String, String) = connection
            .query_row(
                "SELECT status, background_hash_status, detail FROM upload_items WHERE id = 'watched'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(result.0, "failed");
        assert_eq!(result.1, "failed");
        assert!(result.2.contains("not uploaded"));
        assert_eq!(
            connection
                .query_row(
                    "SELECT kind FROM audit_events WHERE item_id = 'watched' ORDER BY id DESC LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "reference_source_final_integrity_failed"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cleanup_failure_remains_pending_for_a_later_relaunch_retry() {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-task108-cleanup-retry-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media-not-ready"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let source = root.join("original.mp4");
        fs::write(&source, b"cleanup-retry").unwrap();
        let digest = blake3::hash(b"cleanup-retry").to_hex().to_string();
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO upload_items(id, title, file_name, source_path, workspace_path, size_bytes, digest, status, total_bytes, delete_source_after_upload, source_delete_status, created_at, updated_at) VALUES('cleanup-retry', 'Retry', 'original.mp4', ?1, 'managed', 13, ?2, 'uploaded', 13, 1, 'pending', ?3, ?3)",
                params![source.to_string_lossy(), digest, now()],
            )
            .unwrap();
        drop(connection);

        finalize_confirmed_source_cleanup(&state, "cleanup-retry").unwrap();
        assert!(source.is_file());
        let connection = database(&state).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id = 'cleanup-retry'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "pending"
        );
        drop(connection);

        fs::create_dir_all(&state.media_directory).unwrap();
        finalize_confirmed_source_cleanup(&state, "cleanup-retry").unwrap();
        assert!(!source.exists());
        let connection = database(&state).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id = 'cleanup-retry'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "deleted"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirmed_source_cleanup_deletes_only_an_unchanged_external_source() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let source = root.join("camera-original.insv");
        let managed = state.media_directory.join("uploaded.media");
        let contents = b"confirmed-upload-source";
        fs::write(&source, contents).unwrap();
        fs::write(&managed, contents).unwrap();
        let digest = blake3::hash(contents).to_hex().to_string();
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, size_bytes, digest, status, total_bytes, delete_source_after_upload, source_delete_status, created_at, updated_at) VALUES ('uploaded', 'Uploaded', 'camera-original.insv', ?1, ?2, ?3, ?4, 'uploaded', ?3, 1, 'pending', ?5, ?5)",
            params![source.to_string_lossy(), managed.to_string_lossy(), contents.len() as i64, digest, now()],
        ).unwrap();
        drop(connection);

        finalize_confirmed_source_cleanup(&state, "uploaded").unwrap();

        assert!(!source.exists());
        assert!(managed.is_file());
        let connection = database(&state).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id = 'uploaded'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "deleted"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn confirmed_source_cleanup_retains_a_source_changed_after_import() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let source = root.join("changed-source.mp4");
        let managed = state.media_directory.join("changed.media");
        fs::write(&source, b"changed-after-import").unwrap();
        fs::write(&managed, b"original-import").unwrap();
        let digest = blake3::hash(b"original-import").to_hex().to_string();
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, size_bytes, digest, status, total_bytes, delete_source_after_upload, source_delete_status, created_at, updated_at) VALUES ('changed', 'Changed', 'changed-source.mp4', ?1, ?2, 15, ?3, 'uploaded', 15, 1, 'pending', ?4, ?4)",
            params![source.to_string_lossy(), managed.to_string_lossy(), digest, now()],
        ).unwrap();
        drop(connection);

        finalize_confirmed_source_cleanup(&state, "changed").unwrap();

        assert!(source.is_file());
        let connection = database(&state).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id = 'changed'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "retained"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn confirmed_source_cleanup_retains_a_linked_source_and_its_target() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let target = root.join("outside-target.mp4");
        let linked_source = root.join("reviewed-alias.mp4");
        let contents = b"linked-source-target";
        fs::write(&target, contents).unwrap();
        std::os::windows::fs::symlink_file(&target, &linked_source).unwrap();
        let managed = state.media_directory.join("uploaded.media");
        fs::write(&managed, contents).unwrap();
        let digest = blake3::hash(contents).to_hex().to_string();
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, size_bytes, digest, status, total_bytes, delete_source_after_upload, source_delete_status, created_at, updated_at) VALUES ('linked', 'Linked', 'reviewed-alias.mp4', ?1, ?2, ?3, ?4, 'uploaded', ?3, 1, 'pending', ?5, ?5)",
            params![linked_source.to_string_lossy(), managed.to_string_lossy(), contents.len() as i64, digest, now()],
        ).unwrap();
        drop(connection);

        finalize_confirmed_source_cleanup(&state, "linked").unwrap();

        assert!(linked_source.exists());
        assert!(target.exists());
        let connection = database(&state).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id = 'linked'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "retained"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn destructive_target_rejects_a_linked_parent_directory() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let target_directory = root.join("target-directory");
        let linked_directory = root.join("reviewed-directory");
        fs::create_dir_all(&target_directory).unwrap();
        let target_file = target_directory.join("target.mp4");
        fs::write(&target_file, b"target").unwrap();
        std::os::windows::fs::symlink_dir(&target_directory, &linked_directory).unwrap();

        assert!(has_link_or_reparse_component(&linked_directory.join("target.mp4")).unwrap());
        assert!(target_file.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_one_worker_can_claim_a_queued_upload() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let mut connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES ('claimable', 'Claimable', 'claimable.mp4', ?1, 1, 'queued', 1, ?2, ?2)",
            params![state.media_directory.join("claimable.media").to_string_lossy(), now()],
        ).unwrap();
        assert_eq!(
            claim_queued_upload_items(&mut connection, vec!["claimable".into()]).unwrap(),
            vec!["claimable"]
        );
        assert!(
            claim_queued_upload_items(&mut connection, vec!["claimable".into()])
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id = 'claimable'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "dispatching"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn automatic_scheduler_leaves_the_queue_for_the_current_upload_worker() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, oauth_client_id, active_channel, active_channel_id, updated_at) VALUES (1, 'client.apps.googleusercontent.com', 'Channel A', 'UC-channel-a', ?1)",
                [now()],
            )
            .unwrap();
        for (id, status) in [("active", "uploading"), ("waiting", "queued")] {
            connection
                .execute(
                    "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES (?1, ?1, ?1, 'Channel A', 'UC-channel-a', ?2, 1, ?3, 1, ?4, ?4)",
                    params![id, state.media_directory.join(format!("{id}.media")).to_string_lossy(), status, now()],
                )
                .unwrap();
        }
        drop(connection);

        fs::write(state.media_directory.join("active.media"), b"active").unwrap();
        fs::write(state.media_directory.join("waiting.media"), b"waiting").unwrap();

        assert_eq!(start_queued_uploads_impl(&state).unwrap(), 0);
        let connection = database(&state).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id = 'waiting'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "queued"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_loopback_requests_do_not_validate_as_oauth_callbacks() {
        let expected_state = "expected-state";
        assert!(!valid_oauth_callback_request(
            Some("GET"),
            "/oauth2/callback?state=wrong",
            expected_state
        ));
        assert!(!valid_oauth_callback_request(
            Some("POST"),
            "/oauth2/callback?state=expected-state",
            expected_state
        ));
        assert!(!valid_oauth_callback_request(
            Some("GET"),
            "/oauth2/callback-other?state=expected-state",
            expected_state
        ));
        assert!(valid_oauth_callback_request(
            Some("GET"),
            "/oauth2/callback?state=expected-state&code=code",
            expected_state
        ));
    }

    #[test]
    fn cancelling_a_connection_attempt_preserves_deletion_authorization() {
        let mut attempts = HashMap::from([
            ("connection".to_string(), OAuthAttemptKind::Connection),
            ("deletion".to_string(), OAuthAttemptKind::Deletion),
        ]);

        assert!(cancel_connection_attempt(&mut attempts, "connection"));
        assert!(!cancel_connection_attempt(&mut attempts, "deletion"));
        assert_eq!(attempts.get("deletion"), Some(&OAuthAttemptKind::Deletion));
    }

    #[test]
    fn ordinary_oauth_consent_omits_the_deletion_scope() {
        assert!(!UPLOAD_OAUTH_SCOPES.contains("youtube.force-ssl"));
        assert!(DELETION_OAUTH_SCOPES.contains("youtube.force-ssl"));
        assert_ne!(UPLOAD_REFRESH_TOKEN_KEY, DELETION_REFRESH_TOKEN_KEY);
    }

    #[test]
    fn expired_deletion_authorization_is_removed_before_reentry() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, deletion_authorized, deletion_sudo_until, updated_at) VALUES (1, 1, '2000-01-01T00:00:00+00:00', ?1)",
                [now()],
            )
            .unwrap();
        assert!(clear_expired_deletion_authorization(&connection).unwrap());
        let flags: (i64, Option<String>) = connection
            .query_row(
                "SELECT deletion_authorized, deletion_sudo_until FROM connection_settings WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(flags.0, 0);
        assert_eq!(flags.1, None);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn daily_upload_limit_pause_is_persisted_and_clears_after_expiry() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, updated_at) VALUES (1, 'Limit channel', 'limit-channel-id', ?1)",
                params![now()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES ('limit-item', 'Limit item', 'limit-item.mp4', 'Limit channel', 'limit-channel-id', ?1, 1, 'dispatching', 1, ?2, ?2)",
                params![state.media_directory.join("limit-item.media").to_string_lossy(), now()],
            )
            .unwrap();
        drop(connection);

        let pause_until = record_upload_quota_pause(&state, "limit-item").unwrap();
        assert!(
            DateTime::parse_from_rfc3339(&pause_until)
                .unwrap()
                .with_timezone(&Utc)
                > Utc::now()
        );
        let connection = database(&state).unwrap();
        assert_eq!(
            active_upload_quota_pause(&connection).unwrap(),
            Some(pause_until)
        );
        assert_eq!(
            find_item(&connection, "limit-item").unwrap().status,
            "queued"
        );
        connection
            .execute(
                "UPDATE upload_quota_pauses SET pause_until = ?1 WHERE channel_id = 'limit-channel-id'",
                params![(Utc::now() - ChronoDuration::seconds(1)).to_rfc3339()],
            )
            .unwrap();
        assert_eq!(active_upload_quota_pause(&connection).unwrap(), None);
        let saved: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM upload_quota_pauses WHERE channel_id = 'limit-channel-id'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(saved, 0);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quota_deadline_worker_exits_after_an_expired_durable_pause() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, updated_at) VALUES (1, 'Limit channel', 'limit-channel-id', ?1)",
                [now()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO upload_quota_pauses (channel_id, pause_until, updated_at) VALUES ('limit-channel-id', ?1, ?2)",
                params![
                    (Utc::now() - ChronoDuration::seconds(1)).to_rfc3339(),
                    now()
                ],
            )
            .unwrap();
        drop(connection);

        startup_gate(&state)
            .quota_worker_started
            .store(true, Ordering::Release);
        let worker_state = state.clone();
        spawn_worker(move || quota_resume_deadline_loop(worker_state))
            .join()
            .unwrap();
        assert!(!startup_gate(&state)
            .quota_worker_started
            .load(Ordering::Acquire));
        assert_eq!(
            database(&state)
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM upload_quota_pauses WHERE channel_id = 'limit-channel-id'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manual_visibility_defaults_to_private_persists_and_keeps_watched_items_private() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        let timestamp = now();
        for (id, channel_name) in [("manual", None), ("watched", Some("Channel A"))] {
            connection
                .execute(
                    "INSERT INTO upload_items (id, title, file_name, channel_name, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES (?1, ?1, ?1, ?2, ?3, 1, 'draft', 1, ?4, ?4)",
                    params![id, channel_name, state.media_directory.join(format!("{id}.media")).to_string_lossy(), timestamp],
                )
                .unwrap();
        }
        drop(connection);

        assert_eq!(
            find_item(&database(&state).unwrap(), "manual")
                .unwrap()
                .visibility,
            "private"
        );
        let updated = set_item_visibility_impl(&state, "manual", "unlisted").unwrap();
        assert_eq!(updated.visibility, "unlisted");
        assert!(set_item_visibility_impl(&state, "manual", "scheduled").is_err());
        assert!(set_item_visibility_impl(&state, "watched", "public").is_err());

        let connection = database(&state).unwrap();
        let audit_detail: String = connection
            .query_row(
                "SELECT detail FROM audit_events WHERE item_id = 'manual' AND kind = 'upload_visibility_set'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(audit_detail.contains("unlisted"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_matching_local_digests_are_exact_duplicate_candidates() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        for (id, title, digest) in [
            ("first", "First copy", "matching-digest"),
            ("second", "Second copy", "matching-digest"),
            ("third", "Different source", "other-digest"),
        ] {
            connection.execute("INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES (?1, ?2, ?2, ?2, 1, ?3, 'uploaded', 1, ?4, ?4)", params![id, title, digest, now()]).unwrap();
        }

        let candidates = exact_local_duplicates(&connection, None).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, "exact_local");
        assert_eq!(candidates[0].left_title, "First copy");
        assert_eq!(candidates[0].right_title, "Second copy");
        assert_eq!(candidates[0].left_video_id, None);
        assert_eq!(candidates[0].right_video_id, None);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_duplicate_review_does_not_cross_immutable_channel_boundaries() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, updated_at) VALUES (1, 'Channel A', 'channel-a-id', ?1)",
                [now()],
            )
            .unwrap();
        for (id, title, channel_id, digest) in [
            ("a-1", "Channel A first", "channel-a-id", "digest-a"),
            ("a-2", "Channel A second", "channel-a-id", "digest-a"),
            ("b-1", "Channel B first", "channel-b-id", "digest-b"),
            ("b-2", "Channel B second", "channel-b-id", "digest-b"),
        ] {
            connection.execute("INSERT INTO upload_items (id, title, file_name, channel_id, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, ?2, 1, ?4, 'uploaded', 1, ?5, ?5)", params![id, title, channel_id, digest, now()]).unwrap();
        }

        let candidates = current_duplicate_candidates(&connection).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].left_title, "Channel A first");
        assert_eq!(candidates[0].right_title, "Channel A second");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_title_review_never_uses_another_channels_queue_as_evidence() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        for (id, channel_id) in [
            ("channel-a-item", "channel-a-id"),
            ("channel-b-item", "channel-b-id"),
        ] {
            connection.execute(
                "INSERT INTO upload_items (id, title, file_name, channel_id, workspace_path, size_bytes, status, total_bytes, duplicate_decision, created_at, updated_at) VALUES (?1, 'Same title', 'same.mp4', ?2, 'managed', 1, 'draft', 1, 'pending', ?3, ?3)",
                params![id, channel_id, now()],
            ).unwrap();
        }

        assert!(pending_upload_title_duplicates(&connection, "channel-a-id")
            .unwrap()
            .is_empty());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incomplete_uploads_never_provide_duplicate_evidence() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        for (id, status) in [("draft-copy", "draft"), ("transfer-copy", "uploading")] {
            connection.execute(
                "INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES (?1, 'Clip', 'clip.mp4', 'clip.media', 1, 'same-digest', ?2, 1, ?3, ?3)",
                params![id, status, now()],
            ).unwrap();
        }
        connection.execute(
            "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES ('processing', 'Channel A', 'channel-a-id', 'Clip', 'uploaded', ?1), ('complete', 'Channel A', 'channel-a-id', 'Clip', 'processed', ?1)",
            [now()],
        ).unwrap();

        assert!(exact_local_duplicates(&connection, None)
            .unwrap()
            .is_empty());
        assert_eq!(
            matching_uploaded_titles(&connection, "channel-a-id", "Clip").unwrap(),
            vec!["Clip"]
        );
        assert!(uploaded_title_duplicates(&connection, "channel-a-id")
            .unwrap()
            .is_empty());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignored_duplicate_candidates_stay_hidden_until_an_operator_reaudits() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        for (id, title) in [("first", "First copy"), ("second", "Second copy")] {
            connection.execute("INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES (?1, ?2, ?2, ?2, 1, 'matching-digest', 'uploaded', 1, ?3, ?3)", params![id, title, now()]).unwrap();
        }
        let candidate_id = current_duplicate_candidates(&connection).unwrap()[0]
            .id
            .clone();
        drop(connection);

        ignore_duplicate_candidate_impl(&state, &candidate_id).unwrap();
        let connection = database(&state).unwrap();
        assert!(ignored_duplicate_candidate_ids(&connection)
            .unwrap()
            .contains(&candidate_id));
        drop(connection);

        assert_eq!(
            re_audit_ignored_duplicate_candidates_impl(&state).unwrap(),
            1
        );
        assert!(ignored_duplicate_candidate_ids(&database(&state).unwrap())
            .unwrap()
            .is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn proprietary_file_extensions_are_hashable_without_ingestion() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let first = root.join("camera-original.insv");
        fs::write(&first, b"proprietary-camera-payload").unwrap();
        let digest = blake3::hash(b"proprietary-camera-payload")
            .to_hex()
            .to_string();
        let connection = database(&state).unwrap();
        connection.execute("INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES ('saved', 'Camera original', 'camera-original.insv', 'saved.media', 26, ?1, 'draft', 26, ?2, ?2)", params![digest, now()]).unwrap();
        let (size_bytes, actual_digest) = digest_file(&first).unwrap();

        assert_eq!(size_bytes, b"proprietary-camera-payload".len() as u64);
        assert_eq!(actual_digest, digest);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn light_preflight_match_can_prepare_the_guarded_local_delete_target() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let source = root.join("camera-original.insv");
        fs::write(&source, b"light-match-source").unwrap();
        let timestamp = now();
        let locator = serde_json::to_string(&FilePath::Path(source.clone())).unwrap();
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_jobs (id, mode, status, total_files, completed_files, inventory_status, created_at, updated_at) VALUES ('light-job', 'light', 'complete', 1, 1, 'not_requested', ?1, ?1)",
            [timestamp.clone()],
        ).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_files (job_id, ordinal, source_locator, file_name, status) VALUES ('light-job', 0, ?1, 'camera-original.insv', 'complete')",
            [locator],
        ).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES ('saved', 'Camera original', 'camera-original.insv', 'saved.media', 1, 'uploaded', 1, ?1, ?1)",
            [timestamp],
        ).unwrap();
        drop(connection);

        let token = prepare_preflight_local_delete_target(&state, "light-job", 0).unwrap();
        delete_preflight_duplicate_file_impl(&state, &token, "camera-original.insv").unwrap();

        assert!(!source.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loading_a_preflight_result_defers_deletion_hashing_until_requested() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let source = root.join("camera-original.insv");
        fs::write(&source, b"deep-match-source").unwrap();
        let digest = blake3::hash(b"deep-match-source").to_hex().to_string();
        let locator = serde_json::to_string(&FilePath::Path(source.clone())).unwrap();
        let timestamp = now();
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_jobs (id, mode, status, total_files, completed_files, inventory_status, created_at, updated_at) VALUES ('deep-job', 'deep', 'complete', 1, 1, 'not_requested', ?1, ?1)",
            [timestamp.clone()],
        ).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_files (job_id, ordinal, source_locator, file_name, size_bytes, digest, status) VALUES ('deep-job', 0, ?1, 'camera-original.insv', 17, ?2, 'complete')",
            params![locator, digest],
        ).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES ('saved', 'Camera original', 'camera-original.insv', 'saved.media', 17, ?1, 'uploaded', 17, ?2, ?2)",
            params![digest, timestamp],
        ).unwrap();
        drop(connection);

        let scan = load_preflight_scan(&state, "deep-job").unwrap();
        assert!(scan.files[0].can_delete_local_duplicate);
        assert!(scan.files[0].local_delete_token.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uploaded_title_match_can_prepare_the_guarded_local_delete_target() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let source = root.join("VID_20251218_195343_00_005.mp4");
        fs::write(&source, b"remote-title-match-source").unwrap();
        let timestamp = now();
        let locator = serde_json::to_string(&FilePath::Path(source.clone())).unwrap();
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, updated_at) VALUES (1, 'Channel A', 'channel-a-id', ?1)",
            [timestamp.clone()],
        ).unwrap();
        connection.execute(
            "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES ('remote-title', 'Channel A', 'channel-a-id', 'VID 20251218 195343 00 005', 'processed', ?1)",
            [timestamp.clone()],
        ).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_jobs (id, mode, status, total_files, completed_files, inventory_status, created_at, updated_at) VALUES ('remote-job', 'light', 'complete', 1, 1, 'complete', ?1, ?1)",
            [timestamp.clone()],
        ).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_files (job_id, ordinal, source_locator, file_name, status) VALUES ('remote-job', 0, ?1, 'VID_20251218_195343_00_005.mp4', 'complete')",
            [locator],
        ).unwrap();
        drop(connection);

        let token = prepare_preflight_local_delete_target(&state, "remote-job", 0).unwrap();
        delete_preflight_duplicate_file_impl(&state, &token, "VID_20251218_195343_00_005.mp4")
            .unwrap();

        assert!(!source.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preflight_uses_the_filename_from_mobile_file_uris() {
        let selected = FilePath::Url(
            url::Url::parse("file:///private/var/mobile/Media/Camera%20Original.insv").unwrap(),
        );

        assert_eq!(preflight_file_name(&selected), "Camera%20Original.insv");
    }

    #[test]
    fn local_mp4_metadata_reads_the_container_duration_without_ingestion() {
        let source = std::env::temp_dir().join(format!(
            "youtube-upload-manager-metadata-{}.mp4",
            Uuid::new_v4()
        ));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&36_u32.to_be_bytes());
        bytes.extend_from_slice(b"moov");
        bytes.extend_from_slice(&28_u32.to_be_bytes());
        bytes.extend_from_slice(b"mvhd");
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&1_000_u32.to_be_bytes());
        bytes.extend_from_slice(&8_250_u32.to_be_bytes());
        fs::write(&source, bytes).unwrap();

        let details = preflight_local_metadata(&FilePath::Path(source.clone()), "clip.mp4", true);
        assert_eq!(details.file_type.as_deref(), Some("MP4"));
        assert_eq!(details.duration_seconds, Some(8.25));
        assert_eq!(details.size_bytes, Some(36));

        fs::remove_file(source).unwrap();
    }

    #[test]
    fn light_preflight_metadata_avoids_a_media_probe_during_result_loading() {
        let source = std::env::temp_dir().join(format!(
            "youtube-upload-manager-light-metadata-{}.mp4",
            Uuid::new_v4()
        ));
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&36_u32.to_be_bytes());
        bytes.extend_from_slice(b"moov");
        bytes.extend_from_slice(&28_u32.to_be_bytes());
        bytes.extend_from_slice(b"mvhd");
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&1_000_u32.to_be_bytes());
        bytes.extend_from_slice(&8_250_u32.to_be_bytes());
        fs::write(&source, bytes).unwrap();

        let details = preflight_local_metadata(&FilePath::Path(source.clone()), "clip.mp4", false);
        assert_eq!(details.duration_seconds, Some(8.25));
        assert!(details.container_format.is_none());
        assert!(details.streams.is_empty());
        assert!(details.metadata_fields.is_empty());

        fs::remove_file(source).unwrap();
    }

    #[test]
    fn preflight_command_accepts_drag_paths_and_mobile_picker_uris() {
        let selected: Vec<FilePath> = serde_json::from_value(serde_json::json!([
            r"C:\Camera\clip.insv",
            "content://media/external/video/media/42",
        ]))
        .unwrap();

        assert!(matches!(selected[0], FilePath::Path(_)));
        assert!(matches!(selected[1], FilePath::Url(ref url) if url.scheme() == "content"));
    }

    #[test]
    fn confirmed_preflight_duplicate_deletion_reuses_the_opt_in_review_with_content_binding() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        database(&state).unwrap();
        let source = root.join("duplicate.insv");
        fs::write(&source, b"same-camera-bytes").unwrap();
        let token = register_preflight_local_delete_target(&media, Some(&source), "duplicate.insv")
            .unwrap();

        assert!(
            register_preflight_local_delete_target(&media, Some(&media), "managed.media").is_none()
        );
        assert!(delete_preflight_duplicate_file_impl(&state, &token, "wrong.insv").is_err());
        assert!(source.exists());
        fs::remove_file(&source).unwrap();
        fs::write(&source, b"changed-after-opt-in-review").unwrap();
        assert!(delete_preflight_duplicate_file_impl(&state, &token, "duplicate.insv").is_err());
        assert!(source.exists());
        assert!(media.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_deletion_rejects_replaced_content_even_with_a_matching_signature() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        database(&state).unwrap();
        let source = root.join("duplicate.insv");
        fs::write(&source, b"reviewed-content").unwrap();
        let token = register_preflight_local_delete_target(&media, Some(&source), "duplicate.insv")
            .unwrap();
        fs::write(&source, b"replaced-content").unwrap();
        let forged_signature = monitored_file_signature(&fs::metadata(&source).unwrap()).unwrap();
        preflight_local_delete_targets()
            .lock()
            .unwrap()
            .get_mut(&token)
            .unwrap()
            .signature = forged_signature;

        assert!(delete_preflight_duplicate_file_impl(&state, &token, "duplicate.insv").is_err());
        assert!(source.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_deletion_is_retained_for_explicit_reconciliation() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        connection.execute("INSERT INTO deletion_requests (id, video_id, title, status, detail, created_at, updated_at) VALUES ('request-1', 'video-1', 'Video', 'executing', 'Awaiting receipt', ?1, ?1)", [now()]).unwrap();
        drop(connection);

        reconcile_interrupted_deletions(&state).unwrap();

        assert_eq!(
            database(&state)
                .unwrap()
                .query_row(
                    "SELECT status FROM deletion_requests WHERE id = 'request-1'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "needs_reconciliation"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_export_never_overwrites_an_existing_file() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        database(&state).unwrap();
        let archive = root.join("existing.yumx.gz");
        fs::write(&archive, b"prior archive").unwrap();

        assert!(export_portable_archive_impl(&state, &archive).is_err());
        assert_eq!(fs::read(&archive).unwrap(), b"prior archive");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_archive_moves_hash_and_inventory_metadata_without_media_or_paths() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let source_media = root.join("source-media");
        let target_media = root.join("target-media");
        fs::create_dir_all(&source_media).unwrap();
        fs::create_dir_all(&target_media).unwrap();
        let source_state = AppState {
            database_path: root.join("source.sqlite3"),
            media_directory: source_media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let target_state = AppState {
            database_path: root.join("target.sqlite3"),
            media_directory: target_media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&source_state).unwrap();
        connection.execute("INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES ('asset-1', 'Camera clip', 'camera.insv', 'secret-local-path.media', 12, 'digest-1', 'draft', 12, ?1, ?1)", [now()]).unwrap();
        connection.execute("INSERT INTO remote_videos (video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES ('video-1', 'Channel', 'UC-transfer', 'Camera clip', 'processed', ?1)", [now()]).unwrap();
        let archive = root.join("portable.yumx.gz");
        let exported = export_portable_archive_impl(&source_state, &archive).unwrap();
        let imported = import_portable_archive_impl(&target_state, &archive).unwrap();
        let target = database(&target_state).unwrap();

        assert_eq!(exported.upload_count, 1);
        assert_eq!(imported.remote_video_count, 1);
        assert!(exported.bytes < 2048);
        assert_eq!(
            target
                .query_row(
                    "SELECT digest FROM upload_items WHERE id = 'portable-asset-1'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "digest-1"
        );
        assert_eq!(
            target
                .query_row(
                    "SELECT status FROM upload_items WHERE id = 'portable-asset-1'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "metadata_only"
        );
        assert_eq!(
            target
                .query_row(
                    "SELECT COUNT(*) FROM remote_videos WHERE video_id = 'video-1'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            target
                .query_row(
                    "SELECT upload_status FROM remote_videos WHERE video_id = 'video-1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "processed"
        );
        assert_eq!(
            target
                .query_row(
                    "SELECT channel_id FROM remote_videos WHERE video_id = 'video-1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "UC-transfer"
        );
        drop(target);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uploaded_title_normalization_handles_filename_separators_and_capture_sequences() {
        assert_eq!(
            normalized_uploaded_title("  Launch\tVIDEO  "),
            "launch video"
        );
        assert_eq!(
            normalized_uploaded_title("VID_20251219_204823_00_014.mp4"),
            "vid 20251219 204823 00 014"
        );
        assert_eq!(
            canonical_uploaded_title("Launch Video (2)"),
            ("launch video".into(), true)
        );
        assert_eq!(
            canonical_uploaded_title("Launch Video    (27)"),
            ("launch video".into(), true)
        );
        assert_eq!(
            canonical_uploaded_title("Launch Video (1)"),
            ("launch video 1".into(), false)
        );
        assert_eq!(
            canonical_uploaded_title("Launch Video(2)"),
            ("launch video 2".into(), false)
        );
        assert_eq!(
            canonical_uploaded_title("Launch (2) Video"),
            ("launch 2 video".into(), false)
        );
        assert!(uploaded_titles_match("Launch Video", "launch video (2)"));
        assert!(!uploaded_titles_match("Launch Video", "Launch Videos"));
        assert!(uploaded_titles_match(
            "VID_20251219_204823_00_014.mp4",
            "VID 20251219 204823 00 014"
        ));
        assert!(uploaded_titles_match(
            "VID_20251219_204823_00_014.mp4",
            "Camera import 20251219 204823 00 014"
        ));
        assert!(!uploaded_titles_match("Clip 12 34", "Other 12 34"));
        assert!(!uploaded_titles_match(
            "VID_20251219_204823_00_014.mp4",
            "VID 20251219 204824 00 014"
        ));
    }

    #[test]
    fn light_dedupe_catches_matching_titles_in_the_current_batch_and_active_queue() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        for (id, title, channel_id) in [
            ("current", "VID_20251219_204823_00_014.mp4", None),
            (
                "same-channel",
                "VID 20251219 204823 00 014",
                Some("channel-a-id"),
            ),
            ("same-batch", "Camera copy 20251219 204823 00 014", None),
            (
                "other-channel",
                "VID 20251219 204823 00 014",
                Some("channel-b-id"),
            ),
        ] {
            connection.execute(
                "INSERT INTO upload_items (id, title, file_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES (?1, ?2, ?2, ?3, ?2, 1, 'draft', 1, ?4, ?4)",
                params![id, title, channel_id, now()],
            ).unwrap();
        }
        let batch = HashSet::from(["current".to_string(), "same-batch".to_string()]);
        let (matches, scope) = light_dedupe_title_match(
            &connection,
            "channel-a-id",
            "VID_20251219_204823_00_014.mp4",
            "current",
            &batch,
        )
        .unwrap()
        .unwrap();

        assert_eq!(scope, "local_queue");
        assert_eq!(matches.len(), 2);
        assert!(matches
            .iter()
            .any(|title| title == "VID 20251219 204823 00 014"));
        assert!(matches
            .iter()
            .any(|title| title == "Camera copy 20251219 204823 00 014"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uploaded_title_candidates_are_deterministic_and_channel_scoped() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        for (video_id, channel_name, channel_id, title) in [
            ("a-base", "Channel A", "channel-a-id", "Launch Video"),
            ("a-exact", "Channel A", "channel-a-id", "  launch   VIDEO "),
            ("a-high", "Channel A", "channel-a-id", "Launch Video (17)"),
            (
                "a-internal",
                "Channel A",
                "channel-a-id",
                "Launch (2) Video",
            ),
            ("a-one", "Channel A", "channel-a-id", "Launch Video (1)"),
            ("a-partial", "Channel A", "channel-a-id", "Launch Videos"),
            ("a-suffix", "Channel A", "channel-a-id", "Launch Video (2)"),
            ("b-exact", "Channel B", "channel-b-id", "Launch Video"),
        ] {
            connection.execute("INSERT INTO remote_videos (video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES (?1, ?2, ?3, ?4, 'processed', ?5)", params![video_id, channel_name, channel_id, title, now()]).unwrap();
        }

        let candidates = uploaded_title_duplicates(&connection, "channel-a-id").unwrap();
        let candidate_ids = candidates
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            candidate_ids,
            vec![
                "remote:a-base:a-exact",
                "remote:a-base:a-high",
                "remote:a-base:a-suffix",
                "remote:a-exact:a-high",
                "remote:a-exact:a-suffix",
                "remote:a-high:a-suffix",
            ]
        );
        assert!(candidates
            .iter()
            .all(|candidate| candidate.confidence == "metadata"));
        assert!(candidates[0].evidence.contains("match exactly"));
        assert!(candidates[1].evidence.contains("duplicate-copy marker"));
        assert_eq!(candidates[0].left_video_id.as_deref(), Some("a-base"));
        assert_eq!(candidates[0].right_video_id.as_deref(), Some("a-exact"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uploaded_title_candidates_include_capture_sequence_matches() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        for (video_id, title) in [
            ("camera-file", "VID_20251219_204823_00_014"),
            ("camera-import", "Camera import 20251219 204823 00 014"),
            ("different-capture", "Camera import 20251219 204824 00 014"),
        ] {
            connection.execute(
                "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES (?1, 'Channel A', 'channel-a-id', ?2, 'processed', ?3)",
                params![video_id, title, now()],
            ).unwrap();
        }
        let candidates = uploaded_title_duplicates(&connection, "channel-a-id").unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "remote:camera-file:camera-import");
        assert!(candidates[0].evidence.contains("number sequence"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_monitor_automatically_processes_existing_stable_files() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let watched = root.join("watched");
        let media = root.join("media");
        fs::create_dir_all(&watched).unwrap();
        fs::create_dir_all(&media).unwrap();
        fs::write(watched.join("already-there.mp4"), b"baseline-video").unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, connection_detail, updated_at) VALUES (1, 'Channel A', 'channel-a-id', 'Test connection', ?1)",
                params![now()],
            )
            .unwrap();
        enable_folder_monitor_impl(
            &state,
            watched.to_string_lossy().to_string(),
            "unlisted".into(),
            false,
            false,
            None,
            None,
        )
        .unwrap();
        let first_scan = scan_folder_monitor_impl(&state, false).unwrap();
        assert_eq!(first_scan.status, "watching");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );

        let processed = scan_folder_monitor_impl(&state, false).unwrap();
        assert!(processed.detail.contains("Queued 1 stable video"));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT status FROM upload_items", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "queued"
        );
        let (source_path, workspace_path, partial_path): (String, String, Option<String>) =
            connection
                .query_row(
                    "SELECT source_path, workspace_path, partial_path FROM upload_items",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
        assert_eq!(
            source_path,
            watched.join("already-there.mp4").to_string_lossy()
        );
        assert_eq!(workspace_path, source_path);
        assert!(Path::new(&workspace_path).is_file());
        assert!(partial_path.is_none());
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE kind = 'folder_monitor_source_referenced'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        assert!(fs::read_dir(&state.media_directory)
            .unwrap()
            .next()
            .is_none());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn watched_source_reference_rejects_replacement_after_stability_check() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let source = root.join("watched.mp4");
        let verified = b"verified-watched-source";
        fs::write(&source, verified).unwrap();
        let signature = monitored_file_signature(&fs::metadata(&source).unwrap()).unwrap();

        let (workspace, _, digest) = reference_watched_source(&source, &signature).unwrap();
        fs::write(&source, b"replacement-watched-source").unwrap();

        assert_eq!(workspace, source);
        assert_eq!(digest, blake3::hash(verified).to_hex().to_string());
        assert!(reference_watched_source(&source, &signature).is_err());
        assert!(fs::read_dir(&state.media_directory)
            .unwrap()
            .next()
            .is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn watched_source_reference_rejects_a_link_swap_before_opening() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let source = root.join("watched.mp4");
        fs::write(&source, b"stable-source").unwrap();
        let signature = monitored_file_signature(&fs::metadata(&source).unwrap()).unwrap();
        let target = root.join("replacement-target.mp4");
        fs::write(&target, b"replacement-target").unwrap();
        fs::remove_file(&source).unwrap();
        std::os::windows::fs::symlink_file(&target, &source).unwrap();

        assert!(reference_watched_source(&source, &signature).is_err());
        assert!(target.exists());
        assert!(fs::read_dir(&state.media_directory)
            .unwrap()
            .next()
            .is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_legacy_media_cleanup_switches_to_original_and_removes_owned_files() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let source = root.join("original.mp4");
        fs::write(&source, b"original-video").unwrap();
        let id = Uuid::new_v4().to_string();
        let workspace = media.join(format!("{id}.media"));
        let partial = media.join(format!("{id}.partial"));
        fs::write(&workspace, b"legacy-copy").unwrap();
        fs::write(&partial, b"legacy-partial").unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, partial_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES (?1, 'Original', 'original.mp4', ?2, ?3, ?4, 14, 'queued', 14, ?5, ?5)",
            params![id, source.to_string_lossy(), workspace.to_string_lossy(), partial.to_string_lossy(), now()],
        ).unwrap();
        drop(connection);

        cleanup_legacy_managed_media(&state).unwrap();

        assert!(!workspace.exists());
        assert!(!partial.exists());
        let connection = database(&state).unwrap();
        let (saved_workspace, saved_partial, status): (String, Option<String>, String) = connection
            .query_row(
                "SELECT workspace_path, partial_path, status FROM upload_items WHERE id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(saved_workspace, source.to_string_lossy());
        assert!(saved_partial.is_none());
        assert_eq!(status, "queued");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn watched_folder_inventory_failure_keeps_the_file_retryable_and_explains_why() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let watched = root.join("watched");
        let media = root.join("media");
        fs::create_dir_all(&watched).unwrap();
        fs::create_dir_all(&media).unwrap();
        fs::write(watched.join("retryable.mp4"), b"stable-video").unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, connection_detail, updated_at) VALUES (1, 'Channel A', 'channel-a-id', 'Test connection', ?1)",
                params![now()],
            )
            .unwrap();
        enable_folder_monitor_impl(
            &state,
            watched.to_string_lossy().to_string(),
            "private".into(),
            false,
            false,
            None,
            None,
        )
        .unwrap();
        scan_folder_monitor_impl(&state, false).unwrap();
        let result = scan_folder_monitor_impl(&state, true).unwrap();

        assert_eq!(result.status, "error");
        assert!(result.detail.starts_with("YouTube library refresh failed"));
        assert_eq!(
            connection
                .query_row(
                    "SELECT state FROM folder_monitor_observations WHERE file_path = ?1",
                    [watched.join("retryable.mp4").to_string_lossy().to_string()],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "observed"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE kind = 'folder_monitor_inventory_sync_failed'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn watched_folder_inventory_failure_details_are_actionable_and_safe() {
        assert_eq!(
            safe_folder_monitor_inventory_failure(
                "Google rejected the saved authorization (invalid_grant). Connect YouTube again."
            ),
            "Google rejected the saved sign-in (invalid_grant). Reconnect YouTube, then retry the scan."
        );
        assert_eq!(
            safe_folder_monitor_inventory_failure("provider returned bearer secret-value"),
            "The YouTube inventory refresh failed. Reconnect YouTube, then retry the scan."
        );
    }

    #[test]
    fn manual_source_cleanup_requires_completed_upload_and_exact_filename() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let source = root.join("original.mp4");
        fs::write(&source, b"confirmed-upload-source").unwrap();
        let (_, digest) = digest_file(&source).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES ('cleanup', 'Completed', 'original.mp4', ?1, ?2, 23, ?3, 'uploaded', 23, ?4, ?4)",
            params![source.to_string_lossy(), media.join("cleanup.media").to_string_lossy(), digest, now()],
        ).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES ('not-ready', 'Queued', 'queued.mp4', ?1, ?2, 23, 'digest', 'queued', 23, ?3, ?3)",
            params![source.to_string_lossy(), media.join("queued.media").to_string_lossy(), now()],
        ).unwrap();
        drop(connection);

        assert!(delete_uploaded_source_impl(&state, "not-ready", "queued.mp4").is_err());
        assert!(delete_uploaded_source_impl(&state, "cleanup", "wrong.mp4").is_err());
        assert!(source.exists());
        delete_uploaded_source_impl(&state, "cleanup", "original.mp4").unwrap();
        assert!(!source.exists());
        assert_eq!(
            database(&state)
                .unwrap()
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id = 'cleanup'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "deleted"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manual_folder_scan_returns_a_background_receipt_without_waiting_for_the_worker() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let watched = root.join("watched");
        let media = root.join("media");
        fs::create_dir_all(&watched).unwrap();
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, connection_detail, updated_at) VALUES (1, 'Channel A', 'channel-a-id', 'Test connection', ?1)",
                params![now()],
            )
            .unwrap();
        enable_folder_monitor_impl(
            &state,
            watched.to_string_lossy().to_string(),
            "unlisted".into(),
            false,
            false,
            None,
            None,
        )
        .unwrap();
        let worker_gate = state.folder_monitor_lock.lock().unwrap();
        let receipt = request_folder_monitor_scan_impl(&state).unwrap();
        assert_eq!(receipt.status, "scanning");
        assert!(receipt.detail.contains("background"));
        drop(worker_gate);
        drop(connection);
        let mut complete = false;
        for _ in 0..100 {
            thread::sleep(Duration::from_millis(10));
            if folder_monitor_settings(&database(&state).unwrap())
                .unwrap()
                .status
                != "scanning"
            {
                complete = true;
                break;
            }
        }
        assert!(complete, "background scan did not finish within one second");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_monitor_overview_is_channel_scoped_bounded_and_hides_source_paths() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        let timestamp = now();
        connection.execute(
            "INSERT INTO folder_monitor_settings (singleton, enabled, folder_path, channel_name, channel_id, visibility, status, detail, updated_at) VALUES (1, 1, ?1, 'Channel A', 'channel-a', 'unlisted', 'watching', 'Watching', ?2)",
            params![root.join("private-source").to_string_lossy(), timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, status, confirmed_bytes, total_bytes, detail, created_at, updated_at) VALUES ('folder-item', 'Queued clip', 'clip.mp4', 'managed.media', 128, 'uploading', 64, 128, 'Uploading', ?1, ?1)",
            [now()],
        ).unwrap();
        connection.execute(
            "INSERT INTO folder_monitor_observations (channel_id, channel_name, file_path, size_bytes, modified_key, state, upload_item_id, first_seen_at, updated_at) VALUES ('channel-a', 'Channel A', ?1, 128, 'stable', 'dispatched', 'folder-item', ?2, ?2)",
            params![root.join("private-source").join("clip.mp4").to_string_lossy(), now()],
        ).unwrap();
        connection.execute(
            "INSERT INTO folder_monitor_observations (channel_id, channel_name, file_path, size_bytes, modified_key, state, first_seen_at, updated_at) VALUES ('channel-b', 'Channel B', 'other.mp4', 16, 'stable', 'queued', ?1, ?1)",
            [now()],
        ).unwrap();
        connection.execute(
            "INSERT INTO audit_events (id, channel_id, channel_name, kind, detail, created_at) VALUES ('folder-log', 'channel-a', 'Channel A', 'folder_monitor_queued', 'Queued safely', ?1)",
            [now()],
        ).unwrap();

        let overview = folder_monitor_overview(&connection).unwrap();
        let rendered = serde_json::to_string(&overview).unwrap();
        assert_eq!(overview.files.len(), 1);
        assert_eq!(overview.files[0].file_name, "clip.mp4");
        assert_eq!(
            overview.files[0].upload_status.as_deref(),
            Some("uploading")
        );
        assert_eq!(overview.logs.len(), 1);
        assert!(!rendered.contains("private-source\\\\clip.mp4"));
        assert!(!rendered.contains("other.mp4"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_monitor_waits_for_stability_and_reuses_the_channel_digest_record() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let watched = root.join("watched");
        let media = root.join("media");
        fs::create_dir_all(&watched).unwrap();
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, connection_detail, updated_at) VALUES (1, 'Channel A', 'channel-a-id', 'Test connection', ?1)",
                params![now()],
            )
            .unwrap();
        let preexisting = watched.join("already-there.mp4");
        fs::write(&preexisting, b"preexisting-video").unwrap();

        let enabled = enable_folder_monitor_impl(
            &state,
            watched.to_string_lossy().to_string(),
            "unlisted".into(),
            false,
            false,
            None,
            None,
        )
        .unwrap();
        assert!(enabled.enabled);
        assert_eq!(enabled.channel_name.as_deref(), Some("Channel A"));
        assert_eq!(enabled.channel_id.as_deref(), Some("channel-a-id"));
        assert_eq!(enabled.visibility, "unlisted");
        scan_folder_monitor_impl(&state, false).unwrap();
        scan_folder_monitor_impl(&state, false).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT channel_name FROM audit_events WHERE kind = 'folder_monitor_enabled'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "Channel A"
        );

        let first = watched.join("first.mp4");
        fs::write(&first, b"stable-video-payload").unwrap();

        let first_scan = scan_folder_monitor_impl(&state, false).unwrap();
        assert_eq!(first_scan.status, "watching");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );

        let second_scan = scan_folder_monitor_impl(&state, false).unwrap();
        assert_eq!(second_scan.status, "watching");
        assert!(second_scan.detail.contains("Queued"));
        let (item_id, status, channel_name, channel_id, digest): (
            String,
            String,
            String,
            String,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT id, status, channel_name, channel_id, digest FROM upload_items WHERE file_name = 'first.mp4'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(status, "queued");
        assert_eq!(channel_name, "Channel A");
        assert_eq!(channel_id, "channel-a-id");
        assert!(digest.is_some());
        let digest: String = connection
            .query_row(
                "SELECT digest FROM upload_items WHERE id = ?1",
                [&item_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!digest.is_empty());
        connection
            .execute(
                "UPDATE upload_items SET status = 'uploaded' WHERE id = ?1",
                [&item_id],
            )
            .unwrap();

        let duplicate = watched.join("second.mp4");
        fs::write(&duplicate, b"stable-video-payload").unwrap();
        scan_folder_monitor_impl(&state, false).unwrap();
        scan_folder_monitor_impl(&state, false).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT state FROM folder_monitor_observations WHERE file_path = ?1",
                    [duplicate.to_string_lossy().as_ref()],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "duplicate"
        );

        connection
            .execute(
                "INSERT INTO remote_videos (video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES ('remote-title', 'Channel A', 'channel-a-id', 'Already Uploaded', 'processed', ?1)",
                params![now()],
            )
            .unwrap();
        let title_duplicate = watched.join("Already Uploaded (2).mp4");
        fs::write(&title_duplicate, b"different-video-payload").unwrap();
        scan_folder_monitor_impl(&state, false).unwrap();
        scan_folder_monitor_impl(&state, false).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT state FROM folder_monitor_observations WHERE file_path = ?1",
                    [title_duplicate.to_string_lossy().as_ref()],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "duplicate_title"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM upload_items WHERE title = 'Already Uploaded (2)'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            0
        );

        connection
            .execute(
                "UPDATE connection_settings SET active_channel = 'Channel A', active_channel_id = 'channel-b-id', updated_at = ?1 WHERE singleton = 1",
                params![now()],
            )
            .unwrap();
        fs::write(watched.join("third.mp4"), b"new-video").unwrap();
        let paused = scan_folder_monitor_impl(&state, false).unwrap();
        assert_eq!(paused.status, "paused");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            2
        );
        connection
            .execute(
                "UPDATE upload_items SET status = 'dispatching' WHERE id = ?1",
                [&item_id],
            )
            .unwrap();
        upload_item(&state, &item_id).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id = ?1",
                    [&item_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "queued"
        );

        let disabled = disable_folder_monitor_impl(&state).unwrap();
        assert!(!disabled.enabled);
        assert_eq!(disabled.status, "disabled");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_monitor_filters_non_video_hidden_temporary_and_nested_entries() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("ready.MP4"), b"ready").unwrap();
        fs::write(root.join(".hidden.mp4"), b"hidden").unwrap();
        fs::write(root.join("copy.tmp.mp4"), b"temporary").unwrap();
        fs::write(root.join("empty.mp4"), b"").unwrap();
        fs::write(root.join("notes.txt"), b"unsupported").unwrap();
        fs::write(nested.join("nested.mp4"), b"nested").unwrap();

        let files = monitored_files(&root).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name, "ready.MP4");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_monitor_enable_requires_a_directory_and_connected_channel() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();

        assert!(enable_folder_monitor_impl(
            &state,
            root.join("missing").to_string_lossy().into(),
            "private".into(),
            false,
            false,
            None,
            None,
        )
        .is_err());
        assert!(enable_folder_monitor_impl(
            &state,
            root.to_string_lossy().into(),
            "public".into(),
            false,
            false,
            None,
            None
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_dispatch_stays_fenced_until_every_readiness_boundary_is_complete() {
        let gate = StartupGate::new();
        assert!(require_startup_dispatch_ready(&gate).is_err());

        gate.classification_complete.store(true, Ordering::Release);
        assert!(require_startup_dispatch_ready(&gate).is_err());

        gate.safe_shell_rendered.store(true, Ordering::Release);
        gate.deferred_state
            .store(STARTUP_RECOVERY_RUNNING, Ordering::Release);
        gate.dispatch_ready.store(true, Ordering::Release);
        assert!(require_startup_dispatch_ready(&gate).is_err());

        gate.deferred_state
            .store(STARTUP_RECOVERY_COMPLETE, Ordering::Release);
        gate.dispatch_ready.store(false, Ordering::Release);
        assert!(require_startup_dispatch_ready(&gate).is_err());

        gate.dispatch_ready.store(true, Ordering::Release);
        assert!(require_startup_dispatch_ready(&gate).is_ok());
    }

    #[test]
    fn startup_scheduler_never_claims_a_queue_bound_to_another_channel() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let other_media = media.join("other-channel.mp4");
        fs::write(&other_media, b"local fixture").unwrap();
        let connection = database(&state).unwrap();
        let timestamp = now();
        connection.execute(
            "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, updated_at) VALUES (1, 'Channel A', 'channel-a', ?1)",
            [&timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES ('other', 'Other', 'other.mp4', 'Channel B', 'channel-b', ?1, 13, 'queued', 13, ?2, ?2)",
            params![other_media.to_string_lossy(), &timestamp],
        ).unwrap();
        drop(connection);

        assert!(start_queued_uploads_impl(&state).is_err());
        assert_eq!(
            database(&state)
                .unwrap()
                .query_row(
                    "SELECT status FROM upload_items WHERE id = 'other'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "queued",
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(feature = "performance-harness")]
    fn clean_startup_classification_is_one_read_only_statement() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let mut connection = initialize_database(&state).unwrap();

        persistence::begin_test_database_trace();
        let work = classify_startup_recovery_with_connection(&mut connection).unwrap();
        let statements = persistence::take_test_database_trace();

        assert!(work.interrupted_upload_ids.is_empty());
        assert!(!work.reconcile_imports);
        assert!(!work.resume_watched_hashes);
        assert!(!work.resume_preflight);
        assert!(!work.start_folder_worker);
        assert!(!work.start_quota_worker);
        assert!(!work.start_postprocess_worker);
        assert_eq!(
            statements.len(),
            1,
            "clean startup should execute only the consolidated presence read: {statements:?}"
        );
        assert!(statements[0].trim_start().starts_with("SELECT"));
        assert!(!statements[0].to_ascii_uppercase().contains("UPDATE "));
        assert!(!statements[0].to_ascii_uppercase().contains("BEGIN "));

        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_presence_captures_every_deferred_eligibility_surface() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        let timestamp = now();
        connection.execute(
            "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, updated_at) VALUES (1, 'Channel A', 'channel-a', ?1)",
            [&timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO folder_monitor_settings (singleton, enabled, updated_at) VALUES (1, 1, ?1)",
            [&timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, size_bytes, background_hash_status, status, total_bytes, created_at, updated_at) VALUES ('hash', 'Hash', 'hash.mp4', 'source.mp4', 'managed.mp4', 1, 'pending', 'queued', 1, ?1, ?1)",
            [&timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES ('import', 'Import', 'import.mp4', 'managed-import.mp4', 1, 'importing', 1, ?1, ?1)",
            [&timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_jobs (id, channel_id, mode, status, total_files, inventory_status, created_at, updated_at) VALUES ('preflight', 'channel-a', 'light', 'queued', 1, 'pending', ?1, ?1)",
            [&timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_files (job_id, ordinal, source_locator, file_name, status, metadata_status) VALUES ('preflight', 0, 'source', 'source.mp4', 'complete', 'pending')",
            [],
        ).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, channel_id, workspace_path, size_bytes, status, total_bytes, playlist_id, playlist_status, created_at, updated_at) VALUES ('postprocess', 'Postprocess', 'postprocess.mp4', 'channel-a', 'managed-postprocess.mp4', 1, 'uploaded', 1, 'playlist-a', 'pending', ?1, ?1)",
            [&timestamp],
        ).unwrap();

        let presence = startup_recovery_presence(&connection).unwrap();

        assert!(presence.classification_dirty);
        assert!(presence.reconcile_imports);
        assert!(presence.resume_watched_hashes);
        assert!(presence.resume_preflight);
        assert!(presence.start_folder_worker);
        assert!(presence.start_quota_worker);
        assert!(presence.start_postprocess_worker);

        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_classification_is_transactional_and_defers_all_media_work() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        let timestamp = now();
        for (id, status, hash_status) in [
            ("importing", "importing", "running"),
            ("dispatching", "dispatching", "not_required"),
            ("uploading", "uploading", "not_required"),
        ] {
            connection.execute(
                "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, background_hash_status, status, total_bytes, created_at, updated_at) VALUES (?1, ?1, ?1, 'Channel A', 'channel-a', ?2, 999999999, ?3, ?4, 999999999, ?5, ?5)",
                params![id, root.join(format!("missing-{id}.mp4")).to_string_lossy(), hash_status, status, &timestamp],
            ).unwrap();
        }
        connection.execute(
            "INSERT INTO deletion_requests (id, video_id, channel_id, title, status, detail, created_at, updated_at) VALUES ('deletion', 'video-a', 'channel-a', 'Video A', 'executing', 'waiting', ?1, ?1)",
            [&timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_jobs (id, mode, status, total_files, inventory_status, created_at, updated_at) VALUES ('preflight', 'light', 'running', 1, 'syncing', ?1, ?1)",
            [&timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO preflight_scan_files (job_id, ordinal, source_locator, file_name, status, metadata_status) VALUES ('preflight', 0, 'missing', 'missing.mp4', 'complete', 'running')",
            [],
        ).unwrap();
        drop(connection);

        #[cfg(feature = "performance-harness")]
        persistence::begin_test_database_trace();
        let work = classify_startup_recovery(&state).unwrap();
        #[cfg(feature = "performance-harness")]
        {
            let statements = persistence::take_test_database_trace();
            assert!(statements
                .iter()
                .any(|statement| statement.trim_start().starts_with("BEGIN IMMEDIATE")));
            assert!(statements.iter().any(|statement| statement
                .to_ascii_uppercase()
                .contains("UPDATE UPLOAD_ITEMS")));
            assert!(statements
                .iter()
                .any(|statement| statement.trim_start().starts_with("COMMIT")));
        }
        assert_eq!(work.interrupted_upload_ids, vec!["uploading"]);
        assert!(work.reconcile_imports);
        assert!(work.resume_preflight);
        assert!(work.start_quota_worker);
        let connection = database(&state).unwrap();
        for (id, expected) in [
            ("importing", "importing"),
            ("dispatching", "queued"),
            ("uploading", "needs_reconciliation"),
        ] {
            assert_eq!(
                connection
                    .query_row(
                        "SELECT status FROM upload_items WHERE id = ?1",
                        [id],
                        |row| row.get::<_, String>(0),
                    )
                    .unwrap(),
                expected,
            );
        }
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM deletion_requests WHERE id = 'deletion'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "needs_reconciliation"
        );
        assert_eq!(
            connection.query_row("SELECT status || ':' || inventory_status FROM preflight_scan_jobs WHERE id = 'preflight'", [], |row| row.get::<_, String>(0)).unwrap(),
            "queued:pending"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT metadata_status FROM preflight_scan_files WHERE job_id = 'preflight'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "pending"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT background_hash_status FROM upload_items WHERE id = 'importing'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "pending"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_bootstrap_combines_crash_connection_and_channel_scoped_snapshot() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        let timestamp = now();
        connection.execute(
            "INSERT INTO connection_settings (singleton, oauth_client_id, active_channel, active_channel_id, connection_detail, updated_at) VALUES (1, 'public-client', 'Channel A', 'channel-a', 'Connected safely', ?1)",
            [&timestamp],
        ).unwrap();
        for (id, channel_id) in [
            ("channel-a-item", "channel-a"),
            ("channel-b-item", "channel-b"),
        ] {
            connection.execute(
                "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES (?1, ?1, ?1, 'Channel', ?2, 'managed', 1, 'queued', 1, ?3, ?3)",
                params![id, channel_id, &timestamp],
            ).unwrap();
        }
        drop(connection);
        fs::write(
            state.database_path.with_file_name(PANIC_MARKER_FILE),
            format!("panic_at={}\n", now()),
        )
        .unwrap();

        let bootstrap = startup_bootstrap_impl(&state).unwrap();
        assert!(bootstrap.crash_recovery.crash_detected);
        assert_eq!(
            bootstrap.connection.active_channel.as_deref(),
            Some("Channel A")
        );
        assert_eq!(
            bootstrap.connection.active_channel_id.as_deref(),
            Some("channel-a")
        );
        assert_eq!(
            bootstrap.snapshot.active_channel.as_deref(),
            Some("Channel A")
        );
        assert_eq!(
            bootstrap
                .snapshot
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["channel-a-item"],
        );
        assert!(bootstrap.snapshot.duplicates.iter().all(|candidate| {
            candidate.left_video_id.as_deref() != Some("channel-b-item")
                && candidate.right_video_id.as_deref() != Some("channel-b-item")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_reconciliation_recovers_queue_states_without_provider_calls() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        let timestamp = now();
        for (id, status) in [
            ("finished-import", "importing"),
            ("claimed-upload", "dispatching"),
            ("active-upload", "uploading"),
            ("waiting-upload", "queued"),
        ] {
            let workspace = media.join(format!("{id}.media"));
            fs::write(&workspace, b"managed-video").unwrap();
            connection.execute(
                "INSERT INTO upload_items (id, title, file_name, channel_name, workspace_path, size_bytes, status, confirmed_bytes, total_bytes, resumable_session_uri, created_at, updated_at) VALUES (?1, ?1, ?1, 'Channel A', ?2, 13, ?3, 7, 13, 'stored-session-checkpoint', ?4, ?4)",
                params![id, workspace.to_string_lossy(), status, timestamp],
            ).unwrap();
        }
        connection.execute(
            "INSERT INTO folder_monitor_observations (channel_name, file_path, size_bytes, modified_key, state, upload_item_id, first_seen_at, updated_at) VALUES ('Channel A', 'claimed.mp4', 13, 'stable', 'dispatched', 'claimed-upload', ?1, ?1)",
            params![timestamp],
        ).unwrap();
        connection.execute(
            "INSERT INTO folder_monitor_observations (channel_name, file_path, size_bytes, modified_key, state, first_seen_at, updated_at) VALUES ('Channel A', 'orphan.mp4', 13, 'stable', 'dispatched', ?1, ?1)",
            params![timestamp],
        ).unwrap();
        drop(connection);

        let recovered = reconcile_queue_impl(&state).unwrap();
        let recovered_statuses = recovered
            .iter()
            .map(|item| (item.id.as_str(), item.status.as_str()))
            .collect::<HashMap<_, _>>();

        assert_eq!(recovered_statuses["finished-import"], "failed");
        assert_eq!(recovered_statuses["claimed-upload"], "queued");
        assert_eq!(recovered_statuses["active-upload"], "needs_reconciliation");
        assert_eq!(recovered_statuses["waiting-upload"], "queued");
        assert!(recovered
            .iter()
            .find(|item| item.id == "finished-import")
            .and_then(|item| item.digest.as_deref())
            .is_none());

        let connection = database(&state).unwrap();
        let session_uri: String = connection
            .query_row(
                "SELECT resumable_session_uri FROM upload_items WHERE id = 'active-upload'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(session_uri, "stored-session-checkpoint");
        let observation_states = connection
            .prepare(
                "SELECT file_path, state FROM folder_monitor_observations ORDER BY file_path ASC",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<HashMap<_, _>, _>>()
            .unwrap();
        assert_eq!(observation_states["claimed.mp4"], "queued");
        assert_eq!(observation_states["orphan.mp4"], "queued");
        let reconciled_audits: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE kind = 'restart_reconciliation'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reconciled_audits, 3);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reconciliation_returns_only_the_active_channel_queue() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        let timestamp = now();
        connection
            .execute(
                "INSERT INTO connection_settings (singleton, active_channel, active_channel_id, updated_at) VALUES (1, 'Channel A', 'channel-a-id', ?1)",
                [timestamp.clone()],
            )
            .unwrap();
        for (id, channel_id) in [
            ("channel-a-item", "channel-a-id"),
            ("channel-b-item", "channel-b-id"),
        ] {
            let workspace = media.join(format!("{id}.media"));
            fs::write(&workspace, b"managed-video").unwrap();
            connection.execute(
                "INSERT INTO upload_items (id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at) VALUES (?1, ?1, ?1, 'Channel', ?2, ?3, 13, 'queued', 13, ?4, ?4)",
                params![id, channel_id, workspace.to_string_lossy(), timestamp],
            ).unwrap();
        }
        drop(connection);

        let recovered = reconcile_queue_impl(&state).unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].id, "channel-a-item");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_reconciliation_marks_interrupted_managed_copy_for_repair_on_a_small_native_stack() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let source = root.join("source.mp4");
        let partial = media.join("resumable.partial");
        let workspace = media.join("resumable.media");
        let contents = b"device-local-video-payload";
        fs::write(&source, contents).unwrap();
        fs::write(&partial, &contents[..8]).unwrap();
        let connection = database(&state).unwrap();
        connection.execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, partial_path, size_bytes, status, imported_bytes, total_bytes, created_at, updated_at) VALUES ('resumable', 'Resumable', 'source.mp4', ?1, ?2, ?3, ?4, 'importing', 8, ?4, ?5, ?5)",
            params![source.to_string_lossy(), workspace.to_string_lossy(), partial.to_string_lossy(), contents.len() as i64, now()],
        ).unwrap();
        drop(connection);

        // Match the constrained stack shape of the packaged Windows GUI thread.
        // Large file buffers must remain heap-backed even when release inlining
        // pulls reconciliation helpers into the startup call tree.
        let recovery_state = state.clone();
        let recovered = thread::Builder::new()
            .name("small-stack-startup-recovery".into())
            .stack_size(512 * 1024)
            .spawn(move || reconcile_queue_impl(&recovery_state))
            .unwrap()
            .join()
            .unwrap()
            .unwrap();
        let item = recovered
            .iter()
            .find(|item| item.id == "resumable")
            .unwrap();

        assert_eq!(item.status, "failed");
        assert!(partial.exists());
        assert!(item.digest.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_only_google_client_identifiers() {
        assert!(valid_google_client_id(
            "12345-example.apps.googleusercontent.com"
        ));
        assert!(!valid_google_client_id("client-secret"));
        assert!(!valid_google_client_id("https://accounts.google.com"));
    }

    #[test]
    fn new_playlist_names_are_trimmed_and_bounded() {
        assert_eq!(valid_new_playlist_title("  Uploads  ").unwrap(), "Uploads");
        assert!(valid_new_playlist_title("   ").is_err());
        assert!(valid_new_playlist_title(&"x".repeat(151)).is_err());
        assert_eq!(
            valid_new_playlist_title(&"x".repeat(150))
                .unwrap()
                .chars()
                .count(),
            150
        );
    }

    #[test]
    fn playlist_creation_errors_are_safe_and_actionable() {
        assert!(youtube_playlist_creation_http_error(403).contains("Reconnect YouTube"));
        assert!(youtube_playlist_creation_http_error(429).contains("rate-limiting"));
        assert!(youtube_playlist_creation_http_error(503).contains("temporarily unavailable"));
        assert!(!youtube_playlist_creation_http_error(418).contains("418"));
    }

    #[test]
    fn playlist_creation_requires_an_active_connection_before_any_provider_call() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();

        assert!(matches!(
            create_youtube_playlist_impl(&state, "Uploads".into()),
            Err(message) if message == "Connect YouTube before creating a playlist."
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oauth_token_errors_are_actionable_without_echoing_provider_payloads() {
        let invalid_grant = serde_json::json!({
            "error": "invalid_grant",
            "error_description": "authorization code secret-code and verifier secret-verifier"
        });
        let message = oauth_token_error_message(&invalid_grant, false);
        assert!(message.contains("invalid_grant"));
        assert!(message.contains("PKCE"));
        assert!(!message.contains("secret-code"));
        assert!(!message.contains("secret-verifier"));

        let invalid_client = serde_json::json!({
            "error": "invalid_client",
            "error_description": "client secret material"
        });
        let message = oauth_token_error_message(&invalid_client, false);
        assert!(message.contains("invalid_client"));
        assert!(message.contains("client ID"));
        assert!(!message.contains("secret material"));

        let unknown = serde_json::json!({
            "error": "provider_private_detail",
            "error_description": "raw provider payload"
        });
        let message = oauth_token_error_message(&unknown, false);
        assert!(message.contains("unrecognized OAuth error"));
        assert!(!message.contains("provider_private_detail"));
        assert!(!message.contains("raw provider payload"));
    }

    #[test]
    fn channel_verification_http_errors_explain_the_oauth_project_problem() {
        assert!(youtube_inventory_http_error(401).contains("Connect YouTube again"));
        assert!(youtube_inventory_http_error(403).contains("YouTube Data API"));
        assert!(!youtube_inventory_http_error(403).contains("403"));
    }

    #[test]
    fn desktop_oauth_json_accepts_only_installed_google_clients() {
        let parsed = desktop_oauth_client_from_file(
            r#"{"installed":{"client_id":"12345-example.apps.googleusercontent.com","client_secret":"desktop-secret"}}"#,
        )
        .unwrap();
        assert_eq!(parsed.0, "12345-example.apps.googleusercontent.com");
        assert_eq!(parsed.1, "desktop-secret");
        assert!(desktop_oauth_client_from_file(
            r#"{"web":{"client_id":"12345-example.apps.googleusercontent.com"}}"#
        )
        .is_err());
        assert!(
            desktop_oauth_client_from_file(r#"{"installed":{"client_id":"not-a-client"}}"#)
                .is_err()
        );
    }

    #[test]
    fn provider_range_uses_the_server_confirmed_next_byte() {
        assert_eq!(confirmed_offset_from_range(None).unwrap(), 0);
        assert_eq!(
            confirmed_offset_from_range(Some("bytes=0-524287")).unwrap(),
            524288
        );
        assert!(confirmed_offset_from_range(Some("bytes=2-4")).is_err());
        assert!(save_upload_session("item", "http://example.test/session").is_err());
    }

    #[test]
    fn cancellation_cannot_be_resurrected_by_a_late_interruption_update() {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-task108-cancellation-{}",
            Uuid::new_v4()
        ));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO upload_items(id, title, file_name, workspace_path, size_bytes, status, confirmed_bytes, total_bytes, detail, created_at, updated_at) VALUES('cancelled-upload', 'Cancelled', 'cancelled.mp4', 'managed', 10, 'cancelled', 5, 10, 'Operator cancelled this upload.', ?1, ?1)",
                [now()],
            )
            .unwrap();
        drop(connection);

        mark_upload_state(
            &state,
            "cancelled-upload",
            "needs_reconciliation",
            5,
            "late mock interruption",
            None,
        )
        .unwrap();
        let connection = database(&state).unwrap();
        let (status, detail): (String, String) = connection
            .query_row(
                "SELECT status, detail FROM upload_items WHERE id = 'cancelled-upload'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "cancelled");
        assert_eq!(detail, "Operator cancelled this upload.");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_success_is_durable_before_channel_scoped_optional_postprocess() {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-task108-success-{}",
            Uuid::new_v4()
        ));
        let media = root.join("media");
        fs::create_dir_all(&media).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: media,
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings(singleton, active_channel, active_channel_id, updated_at) VALUES(1, 'Channel A', 'channel-a', ?1)",
                [now()],
            )
            .unwrap();
        for (id, channel_id) in [("upload-a", "channel-a"), ("upload-b", "channel-b")] {
            connection
                .execute(
                    "INSERT INTO upload_items(id, title, file_name, channel_name, channel_id, workspace_path, size_bytes, status, confirmed_bytes, total_bytes, playlist_id, playlist_title, delete_source_after_upload, created_at, updated_at) VALUES(?1, ?1, 'fixture.mp4', ?2, ?3, 'managed', 10, 'uploading', 0, 10, 'playlist', 'Review', 1, ?4, ?4)",
                    params![id, if channel_id == "channel-a" { "Channel A" } else { "Channel B" }, channel_id, now()],
                )
                .unwrap();
        }
        drop(connection);

        persist_provider_success(&state, "upload-a", "channel-a", 10, "video-a").unwrap();
        assert!(persist_provider_success(&state, "upload-b", "channel-a", 10, "video-b").is_err());
        let connection = database(&state).unwrap();
        let receipt: (String, String, String, String) = connection
            .query_row(
                "SELECT status, video_id, playlist_status, source_delete_status FROM upload_items WHERE id = 'upload-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            receipt,
            (
                "uploaded".into(),
                "video-a".into(),
                "pending".into(),
                "pending".into()
            )
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE item_id = 'upload-a' AND kind = 'upload_completed'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
        drop(connection);

        // Re-opening the same profile models relaunch: only the immutable active
        // channel's durable lower-priority work is eligible.
        let relaunched = AppState {
            database_path: state.database_path.clone(),
            media_directory: state.media_directory.clone(),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        assert_eq!(
            pending_upload_postprocess_ids(&relaunched).unwrap(),
            vec!["upload-a"]
        );
        persist_playlist_outcome(
            &relaunched,
            "upload-a",
            Err("safe mock playlist failure".into()),
        )
        .unwrap();
        let connection = database(&relaunched).unwrap();
        let after_failure: (String, String, String) = connection
            .query_row(
                "SELECT status, video_id, playlist_status FROM upload_items WHERE id = 'upload-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            after_failure,
            ("uploaded".into(), "video-a".into(), "failed".into())
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn next_upload_permit_is_available_before_optional_cleanup_finishes() {
        let scheduler = upload_scheduler::UploadScheduler::new(1);
        scheduler
            .synchronize_active([("completed".into(), "volume-a".into())])
            .unwrap();
        scheduler.release("completed");
        let selected = scheduler
            .select_fair([upload_scheduler::UploadCandidate {
                item_id: "next".into(),
                volume_id: "volume-a".into(),
                volume_limit: 1,
                resume_from_youtube_session: false,
            }])
            .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].item_id, "next");
    }

    #[test]
    fn persisted_title_projection_is_generation_cached_and_channel_scoped() {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-title-projection-{}",
            Uuid::new_v4()
        ));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings(singleton, active_channel, active_channel_id, updated_at) VALUES (1, 'Channel A', 'channel-a', ?1)",
                [now()],
            )
            .unwrap();
        let timestamp = now();
        connection.execute("INSERT INTO remote_videos(video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES ('a-1', 'Channel A', 'channel-a', 'Launch clip', 'processed', ?1), ('a-2', 'Channel A', 'channel-a', 'Launch clip (2)', 'processed', ?1), ('b-1', 'Channel B', 'channel-b', 'Launch clip', 'processed', ?1), ('b-2', 'Channel B', 'channel-b', 'Launch clip (2)', 'processed', ?1)", [&timestamp]).unwrap();
        connection.execute("INSERT INTO channel_generations(channel_id, inventory_generation, upload_generation, updated_at) VALUES ('channel-a', 1, 0, ?1), ('channel-b', 1, 0, ?1)", [&timestamp]).unwrap();

        let candidates = current_duplicate_candidates(&connection).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].left_video_id.as_deref(), Some("a-1"));
        assert_eq!(candidates[0].right_video_id.as_deref(), Some("a-2"));
        assert!(candidates
            .iter()
            .all(|candidate| !candidate.id.contains("b-")));
        assert_eq!(
            connection
                .query_row(
                    "SELECT MIN(title_keys_version) FROM remote_videos WHERE channel_id = 'channel-a'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            title_matching::TITLE_KEYS_VERSION
        );

        connection
            .execute(
                "UPDATE duplicate_projection_state SET rebuilt_at = 'cache-hit-sentinel' WHERE channel_id = 'channel-a'",
                [],
            )
            .unwrap();
        assert_eq!(current_duplicate_candidates(&connection).unwrap().len(), 1);
        assert_eq!(
            connection
                .query_row(
                    "SELECT rebuilt_at FROM duplicate_projection_state WHERE channel_id = 'channel-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "cache-hit-sentinel"
        );
        connection.execute("UPDATE channel_generations SET inventory_generation = inventory_generation + 1 WHERE channel_id = 'channel-a'", []).unwrap();
        assert_eq!(current_duplicate_candidates(&connection).unwrap().len(), 1);
        assert_ne!(
            connection
                .query_row(
                    "SELECT rebuilt_at FROM duplicate_projection_state WHERE channel_id = 'channel-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "cache-hit-sentinel"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_intake_reports_independent_validation_failures_without_source_paths() {
        let root =
            std::env::temp_dir().join(format!("youtube-uploader-batch-intake-{}", Uuid::new_v4()));
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let valid = root.join("valid.mp4");
        let missing = root.join("missing.mp4");
        fs::write(&valid, b"small local fixture").unwrap();
        let receipt = import_and_queue_batch_impl(
            vec![
                valid.to_string_lossy().into_owned(),
                missing.to_string_lossy().into_owned(),
            ],
            ManualUploadSettings {
                made_for_kids: false,
                visibility: "private".into(),
                playlist_id: None,
                playlist_title: None,
                delete_source_after_upload: false,
            },
            &state,
        )
        .unwrap();
        assert_eq!(receipt.requested_count, 2);
        assert_eq!(receipt.imported_count, 1);
        assert_eq!(receipt.failed_count, 1);
        assert_eq!(receipt.items[0].status, "imported");
        assert_eq!(receipt.items[1].status, "failed");
        let serialized = serde_json::to_string(&receipt).unwrap();
        assert!(!serialized.contains(&root.to_string_lossy().to_string()));
        drop(receipt);
        fs::remove_dir_all(root).unwrap();
    }
}
