#[cfg(feature = "performance-harness")]
use serde::Deserialize;
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
    time::Instant,
};

#[cfg(feature = "performance-harness")]
use rusqlite::{params, trace::TraceEvent, Connection, TransactionBehavior};
#[cfg(feature = "performance-harness")]
use std::{fs, path::Path};

#[cfg(feature = "performance-harness")]
const HARNESS_PROFILE_ENV: &str = "YUM_PERFORMANCE_PROFILE_DIR";
#[cfg(feature = "performance-harness")]
const HARNESS_PROFILE_MARKER: &str = ".youtube-upload-manager-performance-profile";
#[cfg(feature = "performance-harness")]
const HARNESS_FIXTURE_ENV: &str = "YUM_PERFORMANCE_FIXTURE";
#[cfg(feature = "performance-harness")]
const HARNESS_FIXTURE_SEED_ONLY_ENV: &str = "YUM_PERFORMANCE_FIXTURE_SEED_ONLY";
#[cfg(feature = "performance-harness")]
const HARNESS_FIXTURE_METADATA_FILE: &str = "performance-fixture.json";
#[cfg(feature = "performance-harness")]
const INTERRUPTED_256GB_FIXTURE: &str = "interrupted-256gb";
#[cfg(feature = "performance-harness")]
const INTERRUPTED_256GB_BYTES: i64 = 256_000_000_000;
const UNAVAILABLE_COUNTER: u64 = u64::MAX;
#[cfg(test)]
const FIXTURE_SIZES: [u64; 4] = [0, 100, 1_000, 10_000];

struct Counters {
    process_started_at: Instant,
    initialization_started_ms: AtomicU64,
    recovery_classified_ms: AtomicU64,
    safe_shell_paint_ms: AtomicU64,
    native_ready_ms: AtomicU64,
    first_batch_paint_ms: AtomicU64,
    first_interaction_ms: AtomicU64,
    settled_idle_ms: AtomicU64,
    idle_sample_end_ms: AtomicU64,
    idle_start_native_invokes: AtomicU64,
    idle_end_native_invokes: AtomicU64,
    idle_start_database_opens: AtomicU64,
    idle_end_database_opens: AtomicU64,
    idle_start_database_statements: AtomicU64,
    idle_end_database_statements: AtomicU64,
    idle_start_event_messages: AtomicU64,
    idle_end_event_messages: AtomicU64,
    idle_start_worker_threads: AtomicU64,
    idle_end_worker_threads: AtomicU64,
    idle_start_ffprobe_processes: AtomicU64,
    idle_end_ffprobe_processes: AtomicU64,
    react_commits: AtomicU64,
    long_tasks: AtomicU64,
    max_long_task_ms: AtomicU64,
    native_invokes: AtomicU64,
    database_opens: AtomicU64,
    database_schema_batches: AtomicU64,
    database_statements: AtomicU64,
    event_messages: AtomicU64,
    worker_threads: AtomicU64,
    ffprobe_processes: AtomicU64,
}

impl Counters {
    fn new() -> Self {
        Self {
            process_started_at: Instant::now(),
            initialization_started_ms: AtomicU64::new(0),
            recovery_classified_ms: AtomicU64::new(0),
            safe_shell_paint_ms: AtomicU64::new(0),
            native_ready_ms: AtomicU64::new(0),
            first_batch_paint_ms: AtomicU64::new(0),
            first_interaction_ms: AtomicU64::new(0),
            settled_idle_ms: AtomicU64::new(0),
            idle_sample_end_ms: AtomicU64::new(0),
            idle_start_native_invokes: AtomicU64::new(0),
            idle_end_native_invokes: AtomicU64::new(0),
            idle_start_database_opens: AtomicU64::new(0),
            idle_end_database_opens: AtomicU64::new(0),
            idle_start_database_statements: AtomicU64::new(0),
            idle_end_database_statements: AtomicU64::new(0),
            idle_start_event_messages: AtomicU64::new(0),
            idle_end_event_messages: AtomicU64::new(0),
            idle_start_worker_threads: AtomicU64::new(0),
            idle_end_worker_threads: AtomicU64::new(0),
            idle_start_ffprobe_processes: AtomicU64::new(0),
            idle_end_ffprobe_processes: AtomicU64::new(0),
            react_commits: AtomicU64::new(UNAVAILABLE_COUNTER),
            long_tasks: AtomicU64::new(0),
            max_long_task_ms: AtomicU64::new(0),
            native_invokes: AtomicU64::new(0),
            database_opens: AtomicU64::new(0),
            database_schema_batches: AtomicU64::new(0),
            database_statements: AtomicU64::new(0),
            event_messages: AtomicU64::new(0),
            worker_threads: AtomicU64::new(0),
            ffprobe_processes: AtomicU64::new(0),
        }
    }

    fn elapsed_ms(&self) -> u64 {
        self.process_started_at.elapsed().as_millis() as u64
    }

    fn mark_once(&self, target: &AtomicU64) {
        let elapsed = self.elapsed_ms().max(1);
        let _ = target.compare_exchange(0, elapsed, Ordering::Relaxed, Ordering::Relaxed);
    }
}

fn counters() -> &'static Counters {
    static COUNTERS: OnceLock<Counters> = OnceLock::new();
    COUNTERS.get_or_init(Counters::new)
}

pub(crate) fn mark_process_start() {
    let _ = counters();
}

pub(crate) fn mark_initialization_started() {
    let state = counters();
    state.mark_once(&state.initialization_started_ms);
}

pub(crate) fn mark_recovery_classified() {
    let state = counters();
    state.mark_once(&state.recovery_classified_ms);
}

pub(crate) fn mark_native_ready() {
    let state = counters();
    state.mark_once(&state.native_ready_ms);
}

#[cfg(feature = "performance-harness")]
pub(crate) fn mark_frontend_milestone(
    milestone: &str,
    metrics: FrontendPerformanceMetrics,
) -> Result<(), String> {
    record_frontend_milestone(counters(), milestone, metrics)
}

#[cfg(feature = "performance-harness")]
fn record_frontend_milestone(
    state: &Counters,
    milestone: &str,
    metrics: FrontendPerformanceMetrics,
) -> Result<(), String> {
    match milestone {
        "safe_shell_paint" => {
            let safe_shell_paint_ms = metrics
                .safe_shell_paint_ms
                .filter(|elapsed| *elapsed > 0)
                .ok_or_else(|| {
                    "The safe-shell paint milestone requires a positive safeShellPaintMs."
                        .to_string()
                })?;
            let _ = state.safe_shell_paint_ms.compare_exchange(
                0,
                safe_shell_paint_ms,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
        "first_batch_paint" => state.mark_once(&state.first_batch_paint_ms),
        "first_interaction" => state.mark_once(&state.first_interaction_ms),
        "settled_idle" => {
            if state.settled_idle_ms.load(Ordering::Acquire) == 0 {
                state.idle_start_native_invokes.store(
                    state.native_invokes.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_start_database_opens.store(
                    state.database_opens.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_start_database_statements.store(
                    state.database_statements.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_start_event_messages.store(
                    state.event_messages.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_start_worker_threads.store(
                    state.worker_threads.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_start_ffprobe_processes.store(
                    state.ffprobe_processes.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.mark_once(&state.settled_idle_ms);
            }
        }
        "idle_sample_end" => {
            if state.settled_idle_ms.load(Ordering::Acquire) == 0 {
                return Err("The settled-idle sample has not started.".into());
            }
            if state.idle_sample_end_ms.load(Ordering::Acquire) == 0 {
                state.idle_end_native_invokes.store(
                    state.native_invokes.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_end_database_opens.store(
                    state.database_opens.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_end_database_statements.store(
                    state.database_statements.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_end_event_messages.store(
                    state.event_messages.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_end_worker_threads.store(
                    state.worker_threads.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.idle_end_ffprobe_processes.store(
                    state.ffprobe_processes.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                state.mark_once(&state.idle_sample_end_ms);
            }
        }
        _ => return Err("Unknown local performance milestone.".into()),
    }
    if let Some(react_commits) = metrics.react_commits {
        state.react_commits.store(react_commits, Ordering::Relaxed);
    }
    state
        .long_tasks
        .store(metrics.long_tasks, Ordering::Relaxed);
    state
        .max_long_task_ms
        .store(metrics.max_long_task_ms, Ordering::Relaxed);
    Ok(())
}

#[cfg(feature = "performance-harness")]
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrontendPerformanceMetrics {
    #[serde(default)]
    safe_shell_paint_ms: Option<u64>,
    react_commits: Option<u64>,
    long_tasks: u64,
    max_long_task_ms: u64,
}

pub(crate) fn record_native_invoke() {
    counters().native_invokes.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_database_open() {
    counters().database_opens.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_database_schema_batch() {
    counters()
        .database_schema_batches
        .fetch_add(1, Ordering::Relaxed);
}

/// SQLite calls this only in explicit performance-harness builds. The SQL text
/// is deliberately ignored so paths, titles, credentials, and provider data
/// can never enter a benchmark artifact.
#[cfg(feature = "performance-harness")]
pub(crate) fn record_database_statement(_: TraceEvent<'_>) {
    counters()
        .database_statements
        .fetch_add(1, Ordering::Relaxed);
}

#[allow(dead_code)]
pub(crate) fn record_event_message() {
    counters().event_messages.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_worker_thread() {
    counters().worker_threads.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_ffprobe_process() {
    counters().ffprobe_processes.fetch_add(1, Ordering::Relaxed);
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativePerformanceSnapshot {
    schema_version: u32,
    local_only: bool,
    contains_sensitive_data: bool,
    app_version: &'static str,
    build_profile: &'static str,
    operating_system: &'static str,
    architecture: &'static str,
    process_uptime_ms: u64,
    initialization_started_ms: Option<u64>,
    recovery_classified_ms: Option<u64>,
    safe_shell_paint_ms: Option<u64>,
    native_ready_ms: Option<u64>,
    first_batch_paint_ms: Option<u64>,
    first_interaction_ms: Option<u64>,
    settled_idle_ms: Option<u64>,
    idle_sample_duration_ms: Option<u64>,
    settled_idle_periodic_invokes: Option<u64>,
    settled_idle_database_opens: Option<u64>,
    settled_idle_database_statements: Option<u64>,
    settled_idle_event_messages: Option<u64>,
    settled_idle_worker_threads: Option<u64>,
    settled_idle_ffprobe_processes: Option<u64>,
    react_commits: Option<u64>,
    long_tasks: u64,
    max_long_task_ms: u64,
    native_invokes: u64,
    database_opens: u64,
    database_schema_batches: u64,
    database_statements: Option<u64>,
    event_messages: u64,
    worker_threads: u64,
    ffprobe_processes: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativePerformanceReport {
    pub(crate) json: NativePerformanceSnapshot,
    pub(crate) markdown: String,
}

fn optional_milestone(value: &AtomicU64) -> Option<u64> {
    match value.load(Ordering::Relaxed) {
        0 => None,
        elapsed => Some(elapsed),
    }
}

fn idle_delta(state: &Counters, start: &AtomicU64, end: &AtomicU64) -> Option<u64> {
    (state.idle_sample_end_ms.load(Ordering::Acquire) > 0).then(|| {
        end.load(Ordering::Relaxed)
            .saturating_sub(start.load(Ordering::Relaxed))
    })
}

pub(crate) fn snapshot() -> NativePerformanceSnapshot {
    snapshot_from(counters())
}

fn snapshot_from(state: &Counters) -> NativePerformanceSnapshot {
    let idle_sample_duration_ms = optional_milestone(&state.idle_sample_end_ms).and_then(|end| {
        optional_milestone(&state.settled_idle_ms).map(|start| end.saturating_sub(start))
    });
    // The closing `idle_sample_end` invoke is instrumentation, not periodic app
    // work. Remove exactly that one command from the measured invoke delta.
    let settled_idle_periodic_invokes = idle_delta(
        state,
        &state.idle_start_native_invokes,
        &state.idle_end_native_invokes,
    )
    .map(|commands| commands.saturating_sub(1));
    NativePerformanceSnapshot {
        schema_version: 1,
        local_only: true,
        contains_sensitive_data: false,
        app_version: env!("CARGO_PKG_VERSION"),
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        operating_system: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        process_uptime_ms: state.elapsed_ms(),
        initialization_started_ms: optional_milestone(&state.initialization_started_ms),
        recovery_classified_ms: optional_milestone(&state.recovery_classified_ms),
        safe_shell_paint_ms: optional_milestone(&state.safe_shell_paint_ms),
        native_ready_ms: optional_milestone(&state.native_ready_ms),
        first_batch_paint_ms: optional_milestone(&state.first_batch_paint_ms),
        first_interaction_ms: optional_milestone(&state.first_interaction_ms),
        settled_idle_ms: optional_milestone(&state.settled_idle_ms),
        idle_sample_duration_ms,
        settled_idle_periodic_invokes,
        settled_idle_database_opens: idle_delta(
            state,
            &state.idle_start_database_opens,
            &state.idle_end_database_opens,
        ),
        settled_idle_database_statements: idle_delta(
            state,
            &state.idle_start_database_statements,
            &state.idle_end_database_statements,
        ),
        settled_idle_event_messages: idle_delta(
            state,
            &state.idle_start_event_messages,
            &state.idle_end_event_messages,
        ),
        settled_idle_worker_threads: idle_delta(
            state,
            &state.idle_start_worker_threads,
            &state.idle_end_worker_threads,
        ),
        settled_idle_ffprobe_processes: idle_delta(
            state,
            &state.idle_start_ffprobe_processes,
            &state.idle_end_ffprobe_processes,
        ),
        react_commits: match state.react_commits.load(Ordering::Relaxed) {
            UNAVAILABLE_COUNTER => None,
            value => Some(value),
        },
        long_tasks: state.long_tasks.load(Ordering::Relaxed),
        max_long_task_ms: state.max_long_task_ms.load(Ordering::Relaxed),
        native_invokes: state.native_invokes.load(Ordering::Relaxed),
        database_opens: state.database_opens.load(Ordering::Relaxed),
        database_schema_batches: state.database_schema_batches.load(Ordering::Relaxed),
        database_statements: cfg!(feature = "performance-harness")
            .then(|| state.database_statements.load(Ordering::Relaxed)),
        event_messages: state.event_messages.load(Ordering::Relaxed),
        worker_threads: state.worker_threads.load(Ordering::Relaxed),
        ffprobe_processes: state.ffprobe_processes.load(Ordering::Relaxed),
    }
}

fn render_markdown(value: &NativePerformanceSnapshot) -> String {
    let statement_count = value
        .database_statements
        .map(|count| count.to_string())
        .unwrap_or_else(|| "disabled (requires performance-harness build)".into());
    format!(
        "# Native performance snapshot\n\n- Local-only: yes\n- Sensitive data included: no\n- App version: {}\n- Build profile: {}\n- Platform: {} ({})\n- Process uptime: {} ms\n- Initialization started: {}\n- Recovery classified: {}\n- Safe shell paint: {}\n- Native ready: {}\n- First Batch paint: {}\n- First interaction: {}\n- Settled idle marker: {}\n- Settled idle sample: {}\n- Periodic webview invokes during settled idle: {}\n- Database opens during settled idle: {}\n- SQLite statements during settled idle: {}\n- Event messages during settled idle: {}\n- Worker threads created during settled idle: {}\n- FFprobe processes created during settled idle: {}\n- React commits: {}\n- Long tasks: {}\n- Maximum long task: {} ms\n- Native invokes: {}\n- Database opens: {}\n- Database schema batches: {}\n- Database statements: {}\n- Event messages: {}\n- Worker threads: {}\n- FFprobe processes: {}\n",
        value.app_version,
        value.build_profile,
        value.operating_system,
        value.architecture,
        value.process_uptime_ms,
        value
            .initialization_started_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
        value
            .recovery_classified_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
        value
            .safe_shell_paint_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
        value
            .native_ready_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
        value
            .first_batch_paint_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
        value
            .first_interaction_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
        value
            .settled_idle_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
        value
            .idle_sample_duration_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
        value.settled_idle_periodic_invokes.map_or_else(
            || "not recorded".into(),
            |count| count.to_string(),
        ),
        value.settled_idle_database_opens.map_or_else(
            || "not recorded".into(),
            |count| count.to_string(),
        ),
        value.settled_idle_database_statements.map_or_else(
            || "not recorded".into(),
            |count| count.to_string(),
        ),
        value.settled_idle_event_messages.map_or_else(
            || "not recorded".into(),
            |count| count.to_string(),
        ),
        value.settled_idle_worker_threads.map_or_else(
            || "not recorded".into(),
            |count| count.to_string(),
        ),
        value.settled_idle_ffprobe_processes.map_or_else(
            || "not recorded".into(),
            |count| count.to_string(),
        ),
        value
            .react_commits
            .map(|count| count.to_string())
            .unwrap_or_else(|| "unavailable in the production React build".into()),
        value.long_tasks,
        value.max_long_task_ms,
        value.native_invokes,
        value.database_opens,
        value.database_schema_batches,
        statement_count,
        value.event_messages,
        value.worker_threads,
        value.ffprobe_processes,
    )
}

pub(crate) fn report() -> NativePerformanceReport {
    let json = snapshot();
    let markdown = render_markdown(&json);
    NativePerformanceReport { json, markdown }
}

#[cfg(feature = "performance-harness")]
pub(crate) fn write_startup_snapshot(path: &Path) -> Result<(), String> {
    let report = report();
    let payload = serde_json::to_vec_pretty(&report.json)
        .map_err(|error| format!("The local performance snapshot could not be encoded: {error}"))?;
    fs::write(path, payload)
        .map_err(|error| format!("The local performance snapshot could not be written: {error}"))
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PerformanceFixtureProfile {
    pub(crate) fixture_id: String,
    pub(crate) upload_items: u64,
    pub(crate) remote_videos: u64,
    pub(crate) preflight_files: u64,
    pub(crate) interrupted_imports: u64,
    pub(crate) interrupted_uploads: u64,
}

/// Describes deterministic synthetic fixture cardinalities. Fixture IDs and
/// counts are the only exported data; the harness generates all records inside
/// its isolated profile and never copies the operator's database or media.
#[cfg(test)]
pub(crate) fn deterministic_fixture_profiles() -> Vec<PerformanceFixtureProfile> {
    let mut fixtures = FIXTURE_SIZES
        .into_iter()
        .map(|size| PerformanceFixtureProfile {
            fixture_id: format!("records-{size}"),
            upload_items: size,
            remote_videos: size,
            preflight_files: size,
            interrupted_imports: 0,
            interrupted_uploads: 0,
        })
        .collect::<Vec<_>>();
    fixtures.push(PerformanceFixtureProfile {
        fixture_id: "interrupted-large".into(),
        upload_items: 10_000,
        remote_videos: 10_000,
        preflight_files: 10_000,
        interrupted_imports: 1_000,
        interrupted_uploads: 1_000,
    });
    fixtures
}

#[cfg(feature = "performance-harness")]
fn validate_harness_profile_root(default_root: &Path, candidate: &Path) -> Result<(), String> {
    if !candidate.is_absolute() {
        return Err(format!("{HARNESS_PROFILE_ENV} must be an absolute path."));
    }
    if candidate == default_root {
        return Err("The performance harness refuses to use the live app profile.".into());
    }
    Ok(())
}

/// The override exists only in a compile-time harness build. Regular builds do
/// not read the environment variable, so an operator's app-data and keyring
/// boundaries cannot be redirected accidentally.
#[cfg(feature = "performance-harness")]
pub(crate) fn isolated_profile_root(default_root: PathBuf) -> Result<PathBuf, String> {
    let candidate = std::env::var_os(HARNESS_PROFILE_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            format!("A performance-harness build requires an isolated {HARNESS_PROFILE_ENV}.")
        })?;
    validate_harness_profile_root(&default_root, &candidate)?;
    fs::create_dir_all(&candidate).map_err(|error| {
        format!("The isolated performance profile could not be created: {error}")
    })?;
    let marker = candidate.join(HARNESS_PROFILE_MARKER);
    let mut entries = fs::read_dir(&candidate)
        .map_err(|error| format!("The isolated performance profile could not be read: {error}"))?;
    let is_empty = entries.next().is_none();
    if !marker.is_file() && !is_empty {
        return Err(
            "The performance profile is non-empty and lacks the isolation marker; refusing to use it."
                .into(),
        );
    }
    if !marker.is_file() {
        fs::write(&marker, b"local synthetic performance fixtures only\n").map_err(|error| {
            format!("The isolated performance profile could not be marked: {error}")
        })?;
    }
    Ok(candidate)
}

#[cfg(feature = "performance-harness")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HarnessFixtureMetadata<'a> {
    schema_version: u32,
    local_only: bool,
    contains_sensitive_data: bool,
    fixture: &'a str,
    upload_items: u32,
    interrupted_uploads: u32,
    declared_total_bytes: i64,
    media_bytes_written: u32,
    persisted_status: &'a str,
}

/// Seeds only an explicitly requested synthetic fixture into the already
/// initialized, marker-protected harness profile. The runner invokes this once
/// before timing and then clones the closed template. Measured launches remove
/// the fixture environment variable, so this path cannot add timed setup work.
#[cfg(feature = "performance-harness")]
pub(crate) fn seed_requested_fixture(database_path: &Path) -> Result<bool, String> {
    let seed_only = std::env::var(HARNESS_FIXTURE_SEED_ONLY_ENV).ok();
    let fixture = std::env::var(HARNESS_FIXTURE_ENV).ok();
    if seed_only.as_deref() != Some("1") {
        if fixture.as_deref().is_some_and(|value| !value.is_empty()) {
            return Err(format!(
                "{HARNESS_FIXTURE_ENV} is accepted only by the untimed synthetic fixture seeder."
            ));
        }
        return Ok(false);
    }
    let fixture = fixture
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{HARNESS_FIXTURE_ENV} is required in seed-only mode."))?;
    if fixture != INTERRUPTED_256GB_FIXTURE {
        return Err(format!(
            "Unknown synthetic performance fixture requested through {HARNESS_FIXTURE_ENV}."
        ));
    }

    let profile_root = database_path
        .parent()
        .ok_or_else(|| "The isolated performance database has no profile root.".to_string())?;
    if !profile_root.join(HARNESS_PROFILE_MARKER).is_file() {
        return Err("The synthetic fixture requires a marker-protected harness profile.".into());
    }

    let mut connection = Connection::open(database_path)
        .map_err(|error| format!("The synthetic fixture database could not be opened: {error}"))?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| format!("The synthetic fixture transaction could not start: {error}"))?;
    let unexpected_uploads: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM upload_items WHERE id != ?1",
            ["performance-fixture-interrupted-256gb"],
            |row| row.get(0),
        )
        .map_err(|error| format!("The synthetic fixture profile could not be verified: {error}"))?;
    if unexpected_uploads != 0 {
        return Err("The synthetic fixture profile already contains unrelated upload rows.".into());
    }
    transaction
        .execute(
            "INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, status, total_bytes, detail, created_at, updated_at) VALUES (?1, 'Synthetic interrupted upload', '', '', ?2, 'uploading', ?2, 'Synthetic interrupted upload fixture; no media or provider session exists.', '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z') ON CONFLICT(id) DO UPDATE SET title = excluded.title, file_name = '', workspace_path = '', size_bytes = excluded.size_bytes, status = excluded.status, confirmed_bytes = 0, imported_bytes = 0, total_bytes = excluded.total_bytes, resumable_session_uri = NULL, video_id = NULL, detail = excluded.detail, channel_name = NULL, channel_id = NULL, source_path = NULL, partial_path = NULL, digest = NULL, updated_at = excluded.updated_at",
            params!["performance-fixture-interrupted-256gb", INTERRUPTED_256GB_BYTES],
        )
        .map_err(|error| format!("The synthetic interrupted upload could not be seeded: {error}"))?;
    transaction
        .commit()
        .map_err(|error| format!("The synthetic fixture transaction could not commit: {error}"))?;

    let metadata = HarnessFixtureMetadata {
        schema_version: 1,
        local_only: true,
        contains_sensitive_data: false,
        fixture: INTERRUPTED_256GB_FIXTURE,
        upload_items: 1,
        interrupted_uploads: 1,
        declared_total_bytes: INTERRUPTED_256GB_BYTES,
        media_bytes_written: 0,
        persisted_status: "uploading",
    };
    let payload = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| format!("The synthetic fixture metadata could not be encoded: {error}"))?;
    fs::write(profile_root.join(HARNESS_FIXTURE_METADATA_FILE), payload)
        .map_err(|error| format!("The synthetic fixture metadata could not be written: {error}"))?;
    Ok(true)
}

#[cfg(not(feature = "performance-harness"))]
pub(crate) fn isolated_profile_root(default_root: PathBuf) -> Result<PathBuf, String> {
    Ok(default_root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "performance-harness")]
    fn frontend_metrics(safe_shell_paint_ms: Option<u64>) -> FrontendPerformanceMetrics {
        FrontendPerformanceMetrics {
            safe_shell_paint_ms,
            react_commits: None,
            long_tasks: 0,
            max_long_task_ms: 0,
        }
    }

    #[test]
    fn performance_fixture_profiles_are_deterministic_and_cover_scale_boundaries() {
        let first = deterministic_fixture_profiles();
        let second = deterministic_fixture_profiles();
        assert_eq!(first, second);
        assert_eq!(
            first
                .iter()
                .take(4)
                .map(|fixture| fixture.upload_items)
                .collect::<Vec<_>>(),
            FIXTURE_SIZES
        );
        let interrupted = first.last().unwrap();
        assert_eq!(interrupted.fixture_id, "interrupted-large");
        assert!(interrupted.interrupted_imports > 0);
        assert!(interrupted.interrupted_uploads > 0);
    }

    #[test]
    fn performance_report_is_local_only_and_has_no_sensitive_dimensions() {
        let report = report();
        let json = serde_json::to_string(&report).unwrap();
        assert!(report.json.local_only);
        assert!(!report.json.contains_sensitive_data);
        assert!(report.markdown.contains("- Local-only: yes"));
        for forbidden in [
            "accessToken",
            "refreshToken",
            "oauth",
            "channelId",
            "sourcePath",
            "fileName",
            "mediaBytes",
            "providerPayload",
        ] {
            assert!(!json.contains(forbidden), "report exposed {forbidden}");
        }
    }

    #[cfg(feature = "performance-harness")]
    #[test]
    fn safe_shell_paint_accepts_camel_case_metric_once_and_is_idempotent() {
        let state = Counters::new();
        let first: FrontendPerformanceMetrics = serde_json::from_value(serde_json::json!({
            "safeShellPaintMs": 417,
            "reactCommits": null,
            "longTasks": 0,
            "maxLongTaskMs": 0
        }))
        .unwrap();
        record_frontend_milestone(&state, "safe_shell_paint", first).unwrap();
        record_frontend_milestone(&state, "safe_shell_paint", frontend_metrics(Some(999))).unwrap();

        assert_eq!(optional_milestone(&state.safe_shell_paint_ms), Some(417));
    }

    #[cfg(feature = "performance-harness")]
    #[test]
    fn safe_shell_paint_is_exposed_in_json_and_markdown_reports() {
        let state = Counters::new();
        record_frontend_milestone(&state, "safe_shell_paint", frontend_metrics(Some(321))).unwrap();

        let snapshot = snapshot_from(&state);
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["safeShellPaintMs"], 321);
        assert!(render_markdown(&snapshot).contains("- Safe shell paint: 321 ms"));
    }

    #[cfg(feature = "performance-harness")]
    #[test]
    fn safe_shell_paint_rejects_missing_or_zero_metric_and_unknown_milestones() {
        let state = Counters::new();
        let missing: FrontendPerformanceMetrics = serde_json::from_value(serde_json::json!({
            "reactCommits": null,
            "longTasks": 0,
            "maxLongTaskMs": 0
        }))
        .unwrap();

        assert_eq!(
            record_frontend_milestone(&state, "safe_shell_paint", missing).unwrap_err(),
            "The safe-shell paint milestone requires a positive safeShellPaintMs."
        );
        assert_eq!(
            record_frontend_milestone(&state, "safe_shell_paint", frontend_metrics(Some(0)),)
                .unwrap_err(),
            "The safe-shell paint milestone requires a positive safeShellPaintMs."
        );
        assert_eq!(
            record_frontend_milestone(&state, "not_a_real_milestone", frontend_metrics(Some(417)),)
                .unwrap_err(),
            "Unknown local performance milestone."
        );
        assert_eq!(optional_milestone(&state.safe_shell_paint_ms), None);
    }

    #[test]
    fn performance_regular_build_ignores_profile_overrides_by_construction() {
        #[cfg(not(feature = "performance-harness"))]
        {
            let default = PathBuf::from("default-profile");
            assert_eq!(isolated_profile_root(default.clone()).unwrap(), default);
        }
    }
}
