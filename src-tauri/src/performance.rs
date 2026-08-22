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
use std::{fs, path::Path};

const HARNESS_PROFILE_ENV: &str = "YUM_PERFORMANCE_PROFILE_DIR";
const HARNESS_PROFILE_MARKER: &str = ".youtube-upload-manager-performance-profile";
const FIXTURE_SIZES: [u64; 4] = [0, 100, 1_000, 10_000];

struct Counters {
    process_started_at: Instant,
    initialization_started_ms: AtomicU64,
    recovery_classified_ms: AtomicU64,
    native_ready_ms: AtomicU64,
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
            native_ready_ms: AtomicU64::new(0),
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
pub(crate) fn record_database_statement(_: &str) {
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
    native_ready_ms: Option<u64>,
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

pub(crate) fn snapshot() -> NativePerformanceSnapshot {
    let state = counters();
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
        native_ready_ms: optional_milestone(&state.native_ready_ms),
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
        "# Native performance snapshot\n\n- Local-only: yes\n- Sensitive data included: no\n- App version: {}\n- Build profile: {}\n- Platform: {} ({})\n- Process uptime: {} ms\n- Initialization started: {}\n- Recovery classified: {}\n- Native ready: {}\n- Native invokes: {}\n- Database opens: {}\n- Database schema batches: {}\n- Database statements: {}\n- Event messages: {}\n- Worker threads: {}\n- FFprobe processes: {}\n",
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
            .native_ready_ms
            .map(|elapsed| format!("{elapsed} ms"))
            .unwrap_or_else(|| "not recorded".into()),
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

#[cfg(not(feature = "performance-harness"))]
pub(crate) fn isolated_profile_root(default_root: PathBuf) -> Result<PathBuf, String> {
    Ok(default_root)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn performance_regular_build_ignores_profile_overrides_by_construction() {
        #[cfg(not(feature = "performance-harness"))]
        {
            let default = PathBuf::from("default-profile");
            assert_eq!(isolated_profile_root(default.clone()).unwrap(), default);
        }
    }
}
