//! Opt-in, channel-scoped local footage hints. Fingerprints never authorize an
//! upload, deletion, duplicate suppression, or a claim about downloaded footage.
use super::*;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    process::{Command, Stdio},
    sync::mpsc,
};

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS perceptual_review_jobs (
 id TEXT PRIMARY KEY NOT NULL, channel_id TEXT NOT NULL, state TEXT NOT NULL,
 snapshots_json TEXT NOT NULL, review_json TEXT NOT NULL, created_at TEXT NOT NULL,
 updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS perceptual_jobs_channel ON perceptual_review_jobs(channel_id,created_at);
CREATE TABLE IF NOT EXISTS perceptual_fingerprints (
 channel_id TEXT NOT NULL, item_id TEXT NOT NULL, digest TEXT NOT NULL,
 algorithm_version TEXT NOT NULL, fingerprint_json TEXT NOT NULL, analyzed_at TEXT NOT NULL,
 PRIMARY KEY(channel_id,item_id,algorithm_version),
 FOREIGN KEY(item_id) REFERENCES upload_items(id)
);
CREATE INDEX IF NOT EXISTS perceptual_fingerprints_channel ON perceptual_fingerprints(channel_id,algorithm_version,analyzed_at);
"#;
pub(super) const ALGORITHM_VERSION: &str = "aligned-dhash-9x8-v1";
const MAX_SELECTED: usize = 16;
const MAX_RETAINED: usize = 128;
const MAX_CANDIDATES: usize = 100;
const POSITIONS: [f64; 7] = [0.08, 0.20, 0.35, 0.50, 0.65, 0.80, 0.92];
const FRAME_BYTES: usize = 9 * 8;
const FRAME_TIMEOUT: Duration = Duration::from_secs(15);
const REVIEW_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Source {
    item_id: String,
    title: String,
    file_name: String,
    state: String,
    detail: String,
    duration_seconds: Option<f64>,
    sample_count: usize,
    informative_samples: usize,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct SampleEvidence {
    position: f64,
    left_time_seconds: f64,
    right_time_seconds: f64,
    distance: u32,
    informative: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Candidate {
    candidate_id: String,
    left_item_id: String,
    right_item_id: String,
    left_title: String,
    right_title: String,
    left_duration_seconds: f64,
    right_duration_seconds: f64,
    duration_difference_percent: f64,
    aligned_samples: usize,
    total_samples: usize,
    median_distance: u32,
    max_distance: u32,
    evidence: String,
    warnings: Vec<String>,
    samples: Vec<SampleEvidence>,
    source_evidence_current: bool,
    retained_evidence_as_of: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Review {
    review_id: String,
    channel_id: String,
    state: String,
    algorithm_version: String,
    decoder_available: bool,
    processed: usize,
    total: usize,
    sources: Vec<Source>,
    candidates: Vec<Candidate>,
    warnings: Vec<String>,
    started_at: String,
    completed_at: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct Snapshot {
    id: String,
    title: String,
    file_name: String,
    path: String,
    size: u64,
    modified_key: String,
    digest: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
struct FrameHash {
    hash: u64,
    contrast: f64,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
struct Fingerprint {
    item_id: String,
    title: String,
    digest: String,
    duration: f64,
    frames: Vec<FrameHash>,
    analyzed_at: String,
    decoder_artifact_id: String,
}

fn decoder_artifact_id() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "04e1307997530f9cf2fe35cba2ca7e8875ca91da02f89d6c7243df819c94ad00"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "e7e7fb30477f717e6f55f9180a70386c62677ef8a4d4d1a5d948f4098aa3eb99"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "a90e3db6a3fd35f6074b013f948b1aa45b31c6375489d39e572bea3f18336584"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "ebdddc936f61e14049a2d4b549a412b8a40deeff6540e58a9f2a2da9e6b18894"
    } else {
        "unsupported"
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
fn require_channel(connection: &Connection, expected: &str) -> Result<(), String> {
    let settings = connection_settings(connection)?;
    if !valid_id(expected)
        || !settings.connected
        || settings.active_channel_id.as_deref() != Some(expected)
    {
        return Err(
            "The connected channel changed. Reopen local footage review in its channel.".into(),
        );
    }
    Ok(())
}
fn selected(ids: &[String]) -> Result<(), String> {
    let mut seen = HashSet::new();
    if ids.is_empty()
        || ids.len() > MAX_SELECTED
        || ids.iter().any(|id| !valid_id(id) || !seen.insert(id))
    {
        return Err("Choose between 1 and 16 distinct saved originals.".into());
    }
    Ok(())
}
fn snapshot(connection: &Connection, channel: &str, id: &str) -> Result<Snapshot, String> {
    let (snapshot, status, hash_status): (Snapshot, String, String) = connection.query_row(
        "SELECT id,title,file_name,COALESCE(source_path,workspace_path),size_bytes,source_modified_key,digest,status,background_hash_status FROM upload_items WHERE channel_id=?1 AND id=?2",
        params![channel,id], |row| Ok((Snapshot { id: row.get(0)?, title: row.get(1)?, file_name: row.get(2)?, path: row.get(3)?, size: row.get::<_,i64>(4)?.max(0) as u64, modified_key: row.get::<_,Option<String>>(5)?.unwrap_or_default(), digest: row.get::<_,Option<String>>(6)?.unwrap_or_default() }, row.get(7)?,row.get(8)?)),
    ).map_err(|_| "A selected original is unavailable in this channel. Refresh the queue.".to_string())?;
    if matches!(status.as_str(), "importing" | "dispatching" | "uploading")
        || matches!(hash_status.as_str(), "pending" | "running")
    {
        return Err("A selected original is being uploaded or verified. Let that work finish before local analysis.".into());
    }
    if snapshot.digest.len() != 64
        || !snapshot.digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        || snapshot.modified_key.is_empty()
        || snapshot.size == 0
        || !Path::new(&snapshot.path).is_absolute()
    {
        return Err("A selected original has no trusted saved content digest and signature. Complete its content verification first.".into());
    }
    Ok(snapshot)
}
fn decoder_path() -> Option<PathBuf> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        None
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let path = std::env::current_exe().ok().and_then(|exe| {
            media_runtime::bundled_sidecar_path(
                &exe,
                if cfg!(windows) {
                    "ffmpeg.exe"
                } else {
                    "ffmpeg"
                },
            )
        })?;
        // Provenance is a native check as well as a build receipt. No PATH
        // fallback or user-provided executable may create this recipe's hashes.
        let mut file = open_original(&path).ok()?;
        let signature = monitored_file_signature(&file.metadata().ok()?).ok()?;
        if signature.0 == 0 || signature.0 > 128 * 1024 * 1024 {
            return None;
        }
        let mut digest = Sha256::new();
        let mut bytes = 0_u64;
        let mut buffer = [0_u8; 256 * 1024];
        loop {
            let count = file.read(&mut buffer).ok()?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            if bytes > signature.0 {
                return None;
            }
            digest.update(&buffer[..count]);
        }
        let observed = format!("{:x}", digest.finalize());
        (bytes == signature.0
            && observed == decoder_artifact_id()
            && monitored_file_signature(&file.metadata().ok()?).ok()? == signature)
            .then_some(path)
    }
}
fn cancellations() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static FLAGS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    FLAGS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn analyzer_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
fn load(
    connection: &Connection,
    channel: &str,
    id: &str,
    check_active: bool,
) -> Result<Review, String> {
    if check_active {
        require_channel(connection, channel)?;
    }
    if !valid_id(id) {
        return Err("Choose a saved local analysis review.".into());
    }
    let (state, json): (String, String) = connection
        .query_row(
            "SELECT state,review_json FROM perceptual_review_jobs WHERE id=?1 AND channel_id=?2",
            params![id, channel],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| {
            "This local footage review is unavailable in the selected channel.".to_string()
        })?;
    let review: Review = serde_json::from_str(&json)
        .map_err(|_| "The saved analysis receipt could not be read safely.".to_string())?;
    let selected = review
        .sources
        .iter()
        .map(|source| source.item_id.as_str())
        .collect::<HashSet<_>>();
    if review.review_id != id
        || review.channel_id != channel
        || review.algorithm_version != ALGORITHM_VERSION
        || review.state != state
        || review.total == 0
        || review.total > MAX_SELECTED
        || review.sources.len() != review.total
        || selected.len() != review.total
        || review.processed > review.total
        || review.candidates.len() > MAX_CANDIDATES
        || review.candidates.iter().any(|candidate| {
            !valid_id(&candidate.candidate_id)
                || !valid_id(&candidate.left_item_id)
                || !valid_id(&candidate.right_item_id)
                || candidate.left_item_id == candidate.right_item_id
                || !(selected.contains(candidate.left_item_id.as_str())
                    || selected.contains(candidate.right_item_id.as_str()))
        })
    {
        return Err("The saved analysis receipt does not match its scoped native identity.".into());
    }
    Ok(review)
}
fn persist_running(connection: &Connection, review: &Review) -> Result<bool, String> {
    require_channel(connection, &review.channel_id)?;
    connection.execute("UPDATE perceptual_review_jobs SET review_json=?3,updated_at=?4 WHERE id=?1 AND channel_id=?2 AND state='running'",params![review.review_id,review.channel_id,serde_json::to_string(review).map_err(user_error)?,now()]).map(|rows| rows == 1).map_err(user_error)
}
fn terminal(connection: &Connection, review: &mut Review, state: &str) -> Result<(), String> {
    review.state = state.into();
    review.completed_at = Some(now());
    let updated = connection.execute("UPDATE perceptual_review_jobs SET state=?3,review_json=?4,updated_at=?5 WHERE id=?1 AND channel_id=?2 AND state='running'",params![review.review_id,review.channel_id,state,serde_json::to_string(review).map_err(user_error)?,now()]).map_err(user_error)?;
    if updated == 1 {
        connection.execute("INSERT INTO audit_events(id,channel_id,kind,detail,created_at) VALUES(?1,?2,'perceptual_review_finished',?3,?4)",params![Uuid::new_v4().to_string(),review.channel_id,format!("Local footage review {}: {} of {} originals analyzed; {} possible pairs. No uploads or files changed.",state,review.processed,review.total,review.candidates.len()),now()]).map_err(user_error)?;
    }
    Ok(())
}
pub(super) fn recover_interrupted(connection: &Connection) -> Result<usize, String> {
    let mut statement = connection
        .prepare("SELECT id,channel_id FROM perceptual_review_jobs WHERE state='running'")
        .map_err(user_error)?;
    let ids = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    drop(statement);
    for (id, channel) in &ids {
        let mut review = match load(connection, channel, id, false) {
            Ok(review) => review,
            Err(_) => {
                connection.execute("UPDATE perceptual_review_jobs SET state='failed',updated_at=?3 WHERE id=?1 AND channel_id=?2 AND state='running'", params![id,channel,now()]).map_err(user_error)?;
                continue;
            }
        };
        review.warnings.push("The app closed before local analysis finished. Existing fingerprints and uploads are retained; select originals to start a new review.".into());
        terminal(connection, &mut review, "interrupted")?;
    }
    Ok(ids.len())
}
fn start(
    state: &AppState,
    channel: &str,
    ids: &[String],
    approved: bool,
    decoder: Option<PathBuf>,
    launch: bool,
) -> Result<Review, String> {
    if !approved {
        return Err("Approve local frame analysis before it reads original media.".into());
    }
    selected(ids)?;
    let decoder = decoder.ok_or_else(|| "The bundled local frame decoder is unavailable on this installation. Install a desktop build that includes it; analysis does not download or invoke another app.".to_string())?;
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_channel(&transaction, channel)?;
    let running: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM perceptual_review_jobs WHERE state='running'",
            [],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if running > 0 {
        return Err(
            "Local frame analysis is already running. Finish or cancel that review first.".into(),
        );
    }
    let snapshots = ids
        .iter()
        .map(|id| snapshot(&transaction, channel, id))
        .collect::<Result<Vec<_>, _>>()?;
    let review = Review { review_id: Uuid::new_v4().to_string(), channel_id: channel.into(), state: "running".into(), algorithm_version: ALGORITHM_VERSION.into(), decoder_available: true, processed: 0, total: snapshots.len(), sources: snapshots.iter().map(|s|Source { item_id:s.id.clone(),title:s.title.clone(),file_name:s.file_name.clone(),state:"pending".into(),detail:"Waiting for bounded local analysis; uploads take priority.".into(),duration_seconds:None,sample_count:0,informative_samples:0 }).collect(), candidates:vec![], warnings:vec!["Possible footage matches are visual hints, not cryptographic byte matches. No uploads are withheld and no files are deleted.".into(),"Seven normalized positions sample both files. Crops, edits, similar intros, still scenes and short clips can produce uncertain or missed matches. Grayscale sampling ignores color differences; audio is not analyzed.".into(),"Analysis compares selected originals and at most 128 retained fingerprints in this channel. It never downloads YouTube media or derives a content hash from embedded playback.".into()], started_at:now(),completed_at:None };
    transaction.execute("INSERT INTO perceptual_review_jobs(id,channel_id,state,snapshots_json,review_json,created_at,updated_at) VALUES(?1,?2,'running',?3,?4,?5,?5)",params![review.review_id,channel,serde_json::to_string(&snapshots).map_err(user_error)?,serde_json::to_string(&review).map_err(user_error)?,now()]).map_err(user_error)?;
    transaction.execute("INSERT INTO audit_events(id,channel_id,kind,detail,created_at) VALUES(?1,?2,'perceptual_review_started',?3,?4)",params![Uuid::new_v4().to_string(),channel,format!("Approved local footage review of {} retained originals; no media copies or remote downloads.",ids.len()),now()]).map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    if launch {
        let flag = Arc::new(AtomicBool::new(false));
        cancellations()
            .lock()
            .map_err(|_| "Analysis cancellation is unavailable.".to_string())?
            .insert(review.review_id.clone(), flag.clone());
        let state = state.clone();
        let review_id = review.review_id.clone();
        let channel = channel.to_string();
        spawn_worker(move || {
            run(&state, &channel, &review_id, &snapshots, &decoder, &flag);
            if let Ok(mut flags) = cancellations().lock() {
                flags.remove(&review_id);
            }
        });
    }
    Ok(review)
}
fn cancelled(state: &AppState, channel: &str, review_id: &str, flag: &AtomicBool) -> bool {
    if flag.load(Ordering::Relaxed) {
        return true;
    }
    let Ok(connection) = database(state) else {
        return true;
    };
    if require_channel(&connection, channel).is_err() {
        return true;
    }
    connection
        .query_row(
            "SELECT state FROM perceptual_review_jobs WHERE id=?1 AND channel_id=?2",
            params![review_id, channel],
            |row| row.get::<_, String>(0),
        )
        .map(|status| status != "running")
        .unwrap_or(true)
}
fn wait_for_uploads(state: &AppState, cancelled: &impl Fn() -> bool) -> Result<(), String> {
    loop {
        if cancelled() {
            return Err("Local frame analysis was cancelled or its channel changed.".into());
        }
        let connection = database(state)?;
        let active: bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE status IN ('dispatching','uploading'))",[],|row|row.get(0)).map_err(user_error)?;
        if !active {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
}
fn run(
    state: &AppState,
    channel: &str,
    id: &str,
    snapshots: &[Snapshot],
    decoder: &Path,
    flag: &AtomicBool,
) {
    let deadline = Instant::now() + REVIEW_TIMEOUT;
    let check = ThrottledCancellationCheck::new(Duration::from_millis(100), || {
        Instant::now() >= deadline || cancelled(state, channel, id, flag)
    });
    let stop = || check.is_cancelled();
    let Ok(_guard) = analyzer_lock().lock() else {
        if let Ok(connection) = database(state) {
            if let Ok(mut review) = load(&connection, channel, id, false) {
                review.warnings.push("The bounded analysis worker could not acquire its local guard. Existing uploads and evidence are retained.".into());
                let _ = terminal(&connection, &mut review, "failed");
            }
        }
        return;
    };
    let Ok(mut review) = database(state).and_then(|conn| load(&conn, channel, id, false)) else {
        // Free admission even if a malformed receipt cannot be decoded. This
        // scoped transition neither reads nor rewrites any upload evidence.
        if let Ok(connection) = database(state) {
            let _ = connection.execute("UPDATE perceptual_review_jobs SET state='failed',updated_at=?3 WHERE id=?1 AND channel_id=?2 AND state='running'", params![id, channel, now()]);
        }
        return;
    };
    let mut current = HashMap::<String, Fingerprint>::new();
    for (ordinal, original) in snapshots.iter().enumerate() {
        if stop() {
            break;
        }
        let result = (|| {
            wait_for_uploads(state, &stop)?;
            let conn = database(state)?;
            require_channel(&conn, channel)?;
            if snapshot(&conn, channel, &original.id)? != *original {
                return Err(
                    "The saved original changed after approval. Start a new review.".into(),
                );
            }
            let fingerprint = fingerprint_original(state, original, decoder, &stop)?;
            let mut conn = database(state)?;
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(user_error)?;
            require_channel(&tx, channel)?;
            if stop() || snapshot(&tx, channel, &original.id)? != *original {
                return Err(
                    "The channel or original changed before analysis could be saved.".into(),
                );
            }
            tx.execute("INSERT INTO perceptual_fingerprints(channel_id,item_id,digest,algorithm_version,fingerprint_json,analyzed_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(channel_id,item_id,algorithm_version) DO UPDATE SET digest=excluded.digest,fingerprint_json=excluded.fingerprint_json,analyzed_at=excluded.analyzed_at",params![channel,original.id,original.digest,ALGORITHM_VERSION,serde_json::to_string(&fingerprint).map_err(user_error)?,fingerprint.analyzed_at]).map_err(user_error)?;
            tx.commit().map_err(user_error)?;
            Ok::<_, String>(fingerprint)
        })();
        let source = &mut review.sources[ordinal];
        match result {
            Ok(fingerprint) => {
                source.state = "analyzed".into();
                source.detail="Saved digest and original signature verified before and after seven bounded grayscale samples.".into();
                source.duration_seconds = Some(fingerprint.duration);
                source.sample_count = fingerprint.frames.len();
                source.informative_samples = fingerprint
                    .frames
                    .iter()
                    .filter(|f| f.contrast >= 12.0)
                    .count();
                current.insert(original.id.clone(), fingerprint);
            }
            Err(detail) => {
                source.state = if detail.contains("unavailable") || detail.contains("missing") {
                    "unavailable"
                } else {
                    "failed"
                }
                .into();
                source.detail = detail;
            }
        }
        review.processed = ordinal + 1;
        match database(state).and_then(|conn| persist_running(&conn, &review)) {
            Ok(true) => {}
            _ => break,
        }
    }
    if let Ok(connection) = database(state) {
        if !stop() {
            match make_candidates(&connection, channel, &current) {
                Ok((candidates, truncated)) => {
                    review.candidates = candidates;
                    if truncated {
                        review.warnings.push("Comparison was bounded to the 128 most recent retained fingerprints and 100 candidate pairs; other retained evidence was not checked.".into());
                    }
                }
                Err(detail) => review.warnings.push(detail),
            }
        }
        let status = if stop() {
            "cancelled"
        } else if current.is_empty() {
            "failed"
        } else {
            "complete"
        };
        if Instant::now() >= deadline {
            review.warnings.push("The ten-minute analysis budget expired. Completed evidence remains saved; continue with a smaller selection.".into());
        }
        let _ = terminal(&connection, &mut review, status);
    }
}
fn open_original(path: &Path) -> Result<File, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        options.custom_flags(0x0020_0000 | 0x0800_0000);
        options.share_mode(1);
    }
    #[cfg(unix)]
    {
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path).map_err(|_| {
        "The original is unavailable or cannot be held safely while it is analyzed.".to_string()
    })?;
    let metadata = file.metadata().map_err(user_error)?;
    #[cfg(windows)]
    if metadata.file_attributes() & 0x400 != 0 {
        return Err("A linked original cannot be analyzed safely.".into());
    }
    if !metadata.is_file() {
        return Err("The original is not a regular file.".into());
    }
    Ok(file)
}
fn fingerprint_original(
    state: &AppState,
    snapshot: &Snapshot,
    decoder: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<Fingerprint, String> {
    let path = Path::new(&snapshot.path);
    let mut file = open_original(path)?;
    let signature = monitored_file_signature(&file.metadata().map_err(user_error)?)?;
    if signature != (snapshot.size, snapshot.modified_key.clone()) {
        return Err("The original signature changed. Existing queue evidence is retained; this file was not analyzed.".into());
    }
    let (bytes, digest) =
        digest_reader_for_path(&mut file, path, MediaIoPriority::Background, cancelled)?;
    if bytes != snapshot.size || digest != snapshot.digest {
        return Err("The original bytes do not match the saved BLAKE3 digest. Restore or locate the trusted original.".into());
    }
    file.seek(SeekFrom::Start(0)).map_err(user_error)?;
    wait_for_uploads(state, cancelled)?;
    let duration=cached_ffprobe_report(path,ProbeDepth::Duration,cancelled).and_then(|probe|probe.duration_seconds).or_else(||iso_bmff_duration_seconds(path)).filter(|value|value.is_finite() && *value>0.0 && *value<=YOUTUBE_MAX_UPLOAD_DURATION_SECONDS).ok_or_else(||"The original duration could not be established safely; no perceptual fingerprint was saved.".to_string())?;
    let mut frames = Vec::with_capacity(POSITIONS.len());
    for position in POSITIONS {
        wait_for_uploads(state, cancelled)?;
        let pixels = decode_frame(decoder, path, &file, duration * position, cancelled)?;
        frames.push(frame_hash(&pixels)?);
    }
    file.seek(SeekFrom::Start(0)).map_err(user_error)?;
    let (bytes, after) =
        digest_reader_for_path(&mut file, path, MediaIoPriority::Background, cancelled)?;
    let after_signature = monitored_file_signature(&file.metadata().map_err(user_error)?)?;
    let reopened = open_original(path)?;
    #[cfg(unix)]
    let same_identity = {
        use std::os::unix::fs::MetadataExt;
        let held = file.metadata().map_err(user_error)?;
        let current = reopened.metadata().map_err(user_error)?;
        (held.dev(), held.ino()) == (current.dev(), current.ino())
    };
    // Windows holds FILE_SHARE_READ only for the complete verification/decode
    // lifetime, so replacing or writing this original cannot race the decoder.
    #[cfg(not(unix))]
    let same_identity = true;
    if cancelled()
        || !same_identity
        || bytes != snapshot.size
        || after != digest
        || signature != after_signature
        || monitored_file_signature(&reopened.metadata().map_err(user_error)?)? != signature
    {
        return Err(
            "The original changed or analysis was cancelled. No fingerprint was saved.".into(),
        );
    }
    Ok(Fingerprint {
        item_id: snapshot.id.clone(),
        title: snapshot.title.clone(),
        digest,
        duration,
        frames,
        analyzed_at: now(),
        decoder_artifact_id: decoder_artifact_id().into(),
    })
}
fn decode_frame(
    decoder: &Path,
    path: &Path,
    file: &File,
    time: f64,
    cancelled: &impl Fn() -> bool,
) -> Result<Vec<u8>, String> {
    let _permit = media_runtime::acquire_background_decoder(path, cancelled)?;
    // FFmpeg reads the original too: decoding must participate in the same
    // per-volume reader bound as hashing and comparison playback.
    let _reader = media_runtime::acquire_reader(path, cancelled)?;
    let mut command = Command::new(decoder);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    #[cfg(unix)]
    let input = {
        use std::os::{fd::AsRawFd, unix::process::CommandExt};
        let descriptor = file.as_raw_fd();
        // SAFETY: fcntl is async-signal-safe. The held read-only descriptor stays
        // valid until this child has exited; no Rust allocation occurs in pre_exec.
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(descriptor, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        if cfg!(target_os = "linux") {
            PathBuf::from(format!("/proc/self/fd/{descriptor}"))
        } else {
            PathBuf::from(format!("/dev/fd/{descriptor}"))
        }
    };
    #[cfg(not(unix))]
    let input = {
        let _ = file;
        path.to_path_buf()
    };
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-threads",
            "1",
            "-protocol_whitelist",
            "file,pipe",
            "-format_whitelist",
            "mov,matroska,avi,flv,mpeg,asf",
            "-ss",
        ])
        .arg(format!("{time:.6}"))
        .arg("-i")
        .arg(input)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-an",
            "-sn",
            "-dn",
            "-vf",
            "scale=9:8:flags=area,format=gray",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| {
        "The bundled decoder could not start. Original files and uploads are retained.".to_string()
    })?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("The decoder output could not be read.")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    spawn_worker(move || {
        let result = media_runtime::read_bounded_output(&mut stdout, FRAME_BYTES)
            .ok()
            .flatten();
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + FRAME_TIMEOUT;
    let status = loop {
        if cancelled() || media_runtime::has_active_upload(path) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancelled() { "Local frame analysis was cancelled or its channel changed." } else { "An upload started while a frame was being decoded. Analysis yielded; retry this review after upload work finishes." }.into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("A frame exceeded its 15-second decoder budget. No partial fingerprint was saved.".into());
            }
        }
    };
    let pixels = receiver
        .recv_timeout(Duration::from_secs(1))
        .ok()
        .flatten()
        .ok_or("The decoder exceeded its bounded output or could not read this video.")?;
    if !status.success() || pixels.len() != FRAME_BYTES {
        return Err("The selected video could not produce a complete grayscale frame. No partial fingerprint was saved.".into());
    }
    Ok(pixels)
}
fn frame_hash(pixels: &[u8]) -> Result<FrameHash, String> {
    if pixels.len() != FRAME_BYTES {
        return Err("A sampled frame was incomplete.".into());
    }
    let mut hash = 0u64;
    let mut index = 0;
    for row in pixels.chunks_exact(9) {
        for x in 0..8 {
            if row[x] > row[x + 1] {
                hash |= 1 << index;
            }
            index += 1;
        }
    }
    let mean = pixels.iter().map(|p| *p as f64).sum::<f64>() / FRAME_BYTES as f64;
    let contrast = (pixels
        .iter()
        .map(|p| (*p as f64 - mean).powi(2))
        .sum::<f64>()
        / FRAME_BYTES as f64)
        .sqrt();
    Ok(FrameHash { hash, contrast })
}
fn valid_fingerprint(fingerprint: &Fingerprint) -> bool {
    valid_id(&fingerprint.item_id)
        && fingerprint.digest.len() == 64
        && fingerprint
            .digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        && fingerprint.decoder_artifact_id.len() == 64
        && fingerprint
            .decoder_artifact_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        && fingerprint.duration.is_finite()
        && fingerprint.duration > 0.0
        && fingerprint.duration <= YOUTUBE_MAX_UPLOAD_DURATION_SECONDS
        && fingerprint.frames.len() == POSITIONS.len()
        && fingerprint.frames.iter().all(|frame| {
            frame.contrast.is_finite() && frame.contrast >= 0.0 && frame.contrast <= 255.0
        })
}
fn compare(left: &Fingerprint, right: &Fingerprint, current: bool) -> Option<Candidate> {
    if !valid_fingerprint(left)
        || !valid_fingerprint(right)
        || left.decoder_artifact_id != right.decoder_artifact_id
        || left.item_id == right.item_id
        || left.digest == right.digest
    {
        return None;
    }
    let difference =
        (left.duration - right.duration).abs() / left.duration.max(right.duration) * 100.0;
    if difference > 10.0 {
        return None;
    }
    let samples = left
        .frames
        .iter()
        .zip(&right.frames)
        .zip(POSITIONS)
        .map(|((l, r), position)| SampleEvidence {
            position,
            left_time_seconds: left.duration * position,
            right_time_seconds: right.duration * position,
            distance: (l.hash ^ r.hash).count_ones(),
            informative: l.contrast >= 12.0 && r.contrast >= 12.0,
        })
        .collect::<Vec<_>>();
    let aligned = samples
        .iter()
        .filter(|sample| sample.distance <= 10)
        .count();
    let mut distances = samples
        .iter()
        .map(|sample| sample.distance)
        .collect::<Vec<_>>();
    distances.sort_unstable();
    let median = distances[distances.len() / 2];
    let maximum = *distances.last()?;
    if aligned < 5 || median > 10 || maximum > 24 {
        return None;
    }
    let informative = samples.iter().filter(|sample| sample.informative).count();
    let distinct_left = left
        .frames
        .iter()
        .map(|frame| frame.hash)
        .collect::<HashSet<_>>()
        .len();
    let distinct_right = right
        .frames
        .iter()
        .map(|frame| frame.hash)
        .collect::<HashSet<_>>()
        .len();
    let mut warnings=vec!["Visual sample similarity is a possible content match. Review both originals; it cannot prove identical footage.".into(), "Grayscale samples ignore color and audio differences, so recolored or differently scored videos may match.".into()];
    let limited = informative < 4
        || distinct_left < 3
        || distinct_right < 3
        || left.duration.min(right.duration) < 5.0;
    if informative < 4 {
        warnings.push("Most aligned samples have low visual contrast; blank or still scenes can match unrelated footage.".into());
    }
    if distinct_left < 3 || distinct_right < 3 {
        warnings.push("Few distinct frame patterns were observed. Similar still scenes or repeated intros can create false positives.".into());
    }
    if left.duration.min(right.duration) < 5.0 {
        warnings.push("A clip is shorter than five seconds, so seven samples provide limited independent evidence.".into());
    }
    if difference > 2.0 {
        warnings.push("Durations differ. Normalized positions are aligned fractions, not proof of matching edit timing.".into());
    }
    if !current {
        warnings.push("Retained fingerprint evidence only; original availability and current bytes have not been verified in this review.".into());
    }
    Some(Candidate {
        candidate_id: Uuid::new_v4().to_string(),
        left_item_id: left.item_id.clone(),
        right_item_id: right.item_id.clone(),
        left_title: left.title.clone(),
        right_title: right.title.clone(),
        left_duration_seconds: left.duration,
        right_duration_seconds: right.duration,
        duration_difference_percent: difference,
        aligned_samples: aligned,
        total_samples: samples.len(),
        median_distance: median,
        max_distance: maximum,
        evidence: if limited { "limited" } else { "possible" }.into(),
        warnings,
        samples,
        source_evidence_current: current,
        retained_evidence_as_of: if current {
            None
        } else {
            Some(right.analyzed_at.clone())
        },
    })
}
fn make_candidates(
    connection: &Connection,
    channel: &str,
    current: &HashMap<String, Fingerprint>,
) -> Result<(Vec<Candidate>, bool), String> {
    require_channel(connection, channel)?;
    let mut checked_current = HashMap::new();
    for (id, fingerprint) in current {
        let saved_digest: Option<String> = connection
            .query_row(
                "SELECT digest FROM upload_items WHERE id=?1 AND channel_id=?2",
                params![id, channel],
                |row| row.get(0),
            )
            .optional()
            .map_err(user_error)?
            .flatten();
        if saved_digest.as_deref() == Some(fingerprint.digest.as_str()) {
            checked_current.insert(id.clone(), fingerprint.clone());
        }
    }
    let current = &checked_current;
    let mut statement=connection.prepare("SELECT p.item_id,p.digest,p.analyzed_at,p.fingerprint_json,u.title FROM perceptual_fingerprints p JOIN upload_items u ON u.id=p.item_id AND u.channel_id=p.channel_id WHERE p.channel_id=?1 AND p.algorithm_version=?2 AND p.digest=u.digest ORDER BY p.analyzed_at DESC,p.item_id LIMIT ?3").map_err(user_error)?;
    let json = statement
        .query_map(
            params![channel, ALGORITHM_VERSION, MAX_RETAINED as i64 + 1],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut truncated = json.len() > MAX_RETAINED;
    let mut pool = json
        .into_iter()
        .take(MAX_RETAINED)
        .filter_map(|(id, digest, at, json, title)| {
            let mut fingerprint: Fingerprint = serde_json::from_str(&json).ok()?;
            if fingerprint.item_id != id
                || fingerprint.digest != digest
                || fingerprint.analyzed_at != at
            {
                return None;
            }
            fingerprint.title = title;
            Some(fingerprint)
        })
        .filter(|fingerprint| {
            valid_fingerprint(fingerprint)
                && fingerprint.decoder_artifact_id == decoder_artifact_id()
        })
        .map(|fp| (fp.item_id.clone(), fp))
        .collect::<HashMap<_, _>>();
    for (id, fp) in current {
        pool.insert(id.clone(), fp.clone());
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut current_ids = current.keys().collect::<Vec<_>>();
    current_ids.sort();
    let mut pool_ids = pool.keys().collect::<Vec<_>>();
    pool_ids.sort();
    for id in current_ids {
        for other in &pool_ids {
            let pair = if id < *other {
                (id.clone(), (*other).clone())
            } else {
                ((*other).clone(), id.clone())
            };
            if id == *other || !seen.insert(pair) {
                continue;
            }
            if let Some(candidate) =
                compare(&current[id], &pool[*other], current.contains_key(*other))
            {
                if result.len() < MAX_CANDIDATES {
                    result.push(candidate);
                } else {
                    truncated = true;
                }
            }
        }
    }
    Ok((result, truncated))
}
fn cancel(state: &AppState, channel: &str, id: &str) -> Result<Review, String> {
    let connection = database(state)?;
    // A late dialog may cancel its own opaque review after switching channels;
    // it cannot enumerate or change another channel's uploads/fingerprints.
    let mut review = load(&connection, channel, id, false)?;
    if let Some(flag) = cancellations()
        .lock()
        .map_err(|_| "Analysis cancellation is unavailable.".to_string())?
        .get(id)
    {
        flag.store(true, Ordering::Relaxed);
    }
    if review.state == "running" {
        terminal(&connection, &mut review, "cancelled")?;
    }
    Ok(review)
}
pub(super) fn preview_member_ids(
    connection: &Connection,
    channel: &str,
    review_id: &str,
    candidate_id: &str,
) -> Result<Vec<String>, String> {
    let review = load(connection, channel, review_id, true)?;
    let candidate = review
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == candidate_id)
        .ok_or("This reviewed local comparison is unavailable. Reopen its analysis.")?;
    for id in [&candidate.left_item_id, &candidate.right_item_id] {
        let owned: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM upload_items WHERE id=?1 AND channel_id=?2)",
                params![id, channel],
                |row| row.get(0),
            )
            .map_err(user_error)?;
        if !owned {
            return Err("A compared upload is unavailable in this channel.".into());
        }
    }
    Ok(vec![
        candidate.left_item_id.clone(),
        candidate.right_item_id.clone(),
    ])
}

#[tauri::command]
pub(super) async fn start_perceptual_review(
    state: State<'_, AppState>,
    expected_channel_id: String,
    item_ids: Vec<String>,
    approved: bool,
) -> Result<Review, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if !approved {
            return Err("Approve local frame analysis before it reads original media.".into());
        }
        selected(&item_ids)?;
        require_channel(&database(&state)?, &expected_channel_id)?;
        start(
            &state,
            &expected_channel_id,
            &item_ids,
            approved,
            decoder_path(),
            true,
        )
    })
    .await
    .map_err(|_| "Local analysis could not start. Original uploads are retained.".to_string())?
}
#[tauri::command]
pub(super) async fn load_perceptual_review(
    state: State<'_, AppState>,
    expected_channel_id: String,
    review_id: String,
) -> Result<Review, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        load(&database(&state)?, &expected_channel_id, &review_id, true)
    })
    .await
    .map_err(user_error)?
}
#[tauri::command]
pub(super) async fn cancel_perceptual_review(
    state: State<'_, AppState>,
    expected_channel_id: String,
    review_id: String,
) -> Result<Review, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cancel(&state, &expected_channel_id, &review_id))
        .await
        .map_err(user_error)?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fingerprint(id: &str, hashes: Vec<u64>, duration: f64) -> Fingerprint {
        Fingerprint {
            item_id: id.into(),
            title: id.into(),
            digest: blake3::hash(id.as_bytes()).to_hex().to_string(),
            duration,
            frames: hashes
                .into_iter()
                .map(|hash| FrameHash {
                    hash,
                    contrast: 40.0,
                })
                .collect(),
            analyzed_at: now(),
            decoder_artifact_id: decoder_artifact_id().into(),
        }
    }
    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-perceptual-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute_batch(SCHEMA_SQL).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture channel','channel-a',?1)", [now()]).unwrap();
        (root, state, connection)
    }
    fn seed(connection: &Connection, root: &Path, id: &str, channel: &str) {
        let path = root.join(format!("{id}.mp4"));
        fs::write(&path, id.as_bytes()).unwrap();
        let signature = monitored_file_signature(&fs::metadata(&path).unwrap()).unwrap();
        connection.execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,digest,background_hash_status,source_modified_key,status,created_at,updated_at) VALUES(?1,?1,?2,?3,?4,?4,?5,?5,?6,'complete',?7,'failed',?8,?8)",params![id,format!("{id}.mp4"),channel,path.to_string_lossy(),signature.0 as i64,blake3::hash(id.as_bytes()).to_hex().to_string(),signature.1,now()]).unwrap();
    }
    #[test]
    fn aligned_reencode_hints_explain_samples_without_treating_byte_matches_as_perceptual() {
        let left = fingerprint(
            "one",
            vec![
                0x12345678, 0x12abcdef, 0xab1234cd, 0x454cabc, 0x9876aaba, 0x672a881, 0x51512e,
            ],
            120.0,
        );
        let right = fingerprint(
            "two",
            left.frames.iter().map(|f| f.hash ^ 1).collect(),
            121.0,
        );
        let candidate = compare(&left, &right, true).unwrap();
        assert_eq!(candidate.aligned_samples, 7);
        assert_eq!(candidate.median_distance, 1);
        assert_eq!(candidate.evidence, "possible");
        assert!(candidate.source_evidence_current);
        assert_eq!(candidate.samples[3].left_time_seconds, 60.0);
        let mut exact = right.clone();
        exact.digest = left.digest.clone();
        assert!(compare(&left, &exact, true).is_none());
    }
    #[test]
    fn matching_intros_and_different_durations_do_not_create_candidates() {
        let left = fingerprint("one", vec![0, 1, 2, 3, 4, 5, 6], 120.0);
        let right = fingerprint(
            "two",
            vec![0, 1, u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX],
            120.0,
        );
        assert!(compare(&left, &right, true).is_none());
        let right = fingerprint("two", vec![0, 1, 2, 3, 4, 5, 6], 90.0);
        assert!(compare(&left, &right, true).is_none());
    }
    #[test]
    fn short_still_and_retained_evidence_is_explicitly_limited() {
        let mut left = fingerprint("one", vec![0; 7], 3.0);
        let mut right = fingerprint("two", vec![0; 7], 3.0);
        for frame in left.frames.iter_mut().chain(&mut right.frames) {
            frame.contrast = 0.0;
        }
        let candidate = compare(&left, &right, false).unwrap();
        assert_eq!(candidate.evidence, "limited");
        assert!(candidate.retained_evidence_as_of.is_some());
        assert!(!candidate.source_evidence_current);
        assert!(candidate.warnings.iter().any(|w| w.contains("still")));
        assert!(candidate.warnings.iter().any(|w| w.contains("shorter")));
    }
    #[test]
    fn bounded_gray_hash_rejects_partial_and_keeps_contrast_evidence() {
        assert!(frame_hash(&[0; 71]).is_err());
        let blank = frame_hash(&[0; 72]).unwrap();
        assert_eq!(blank.hash, 0);
        assert_eq!(blank.contrast, 0.0);
        let gradient = (0..72).map(|i| (i % 9 * 25) as u8).collect::<Vec<_>>();
        let hash = frame_hash(&gradient).unwrap();
        assert_eq!(hash.hash, 0);
        assert!(hash.contrast > 12.0);
    }
    #[test]
    fn no_approval_decoder_foreign_channel_or_active_source_cannot_create_job() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "item-a", "channel-a");
        seed(&connection, &root, "item-b", "channel-b");
        let decoder = Some(root.join("fixture-decoder"));
        assert!(start(
            &state,
            "channel-a",
            &["item-a".into()],
            false,
            decoder.clone(),
            false
        )
        .is_err());
        assert!(start(&state, "channel-a", &["item-a".into()], true, None, false).is_err());
        assert!(start(
            &state,
            "channel-a",
            &["item-b".into()],
            true,
            decoder.clone(),
            false
        )
        .is_err());
        assert!(start(
            &state,
            "channel-b",
            &["item-a".into()],
            true,
            decoder.clone(),
            false
        )
        .is_err());
        connection
            .execute(
                "UPDATE upload_items SET status='uploading' WHERE id='item-a'",
                [],
            )
            .unwrap();
        assert!(start(
            &state,
            "channel-a",
            &["item-a".into()],
            true,
            decoder,
            false
        )
        .is_err());
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM perceptual_review_jobs", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cancellation_and_restart_keep_original_queue_and_checkpoint_identity() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "item-a", "channel-a");
        connection.execute("UPDATE upload_items SET confirmed_bytes=1,resumable_session_uri='protected-checkpoint' WHERE id='item-a'",[]).unwrap();
        let review = start(
            &state,
            "channel-a",
            &["item-a".into()],
            true,
            Some(root.join("fixture-decoder")),
            false,
        )
        .unwrap();
        assert_eq!(review.total, 1);
        assert!(start(
            &state,
            "channel-a",
            &["item-a".into()],
            true,
            Some(root.join("fixture-decoder")),
            false
        )
        .is_err());
        assert_eq!(
            cancel(&state, "channel-a", &review.review_id)
                .unwrap()
                .state,
            "cancelled"
        );
        assert!(load(&connection, "channel-b", &review.review_id, false).is_err());
        let review = start(
            &state,
            "channel-a",
            &["item-a".into()],
            true,
            Some(root.join("fixture-decoder")),
            false,
        )
        .unwrap();
        drop(connection);
        let connection = database(&state).unwrap();
        assert_eq!(recover_interrupted(&connection).unwrap(), 1);
        assert_eq!(
            load(&connection, "channel-a", &review.review_id, true)
                .unwrap()
                .state,
            "interrupted"
        );
        let saved:(String,i64,String)=connection.query_row("SELECT status,confirmed_bytes,resumable_session_uri FROM upload_items WHERE id='item-a'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(saved, ("failed".into(), 1, "protected-checkpoint".into()));
        assert!(root.join("item-a.mp4").is_file());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn retained_fingerprints_are_scoped_and_changed_digests_are_excluded() {
        let (root, state, connection) = fixture();
        for (id, channel) in [
            ("item-a", "channel-a"),
            ("item-b", "channel-a"),
            ("item-c", "channel-b"),
        ] {
            seed(&connection, &root, id, channel);
        }
        let hashes = vec![1, 2, 3, 4, 5, 6, 7];
        let mut current = HashMap::new();
        let left = fingerprint("item-a", hashes.clone(), 120.0);
        current.insert("item-a".into(), left);
        for (id, channel) in [("item-b", "channel-a"), ("item-c", "channel-b")] {
            let fp = fingerprint(id, hashes.clone(), 120.0);
            connection
                .execute(
                    "INSERT INTO perceptual_fingerprints VALUES(?1,?2,?3,?4,?5,?6)",
                    params![
                        channel,
                        id,
                        fp.digest,
                        ALGORITHM_VERSION,
                        serde_json::to_string(&fp).unwrap(),
                        fp.analyzed_at
                    ],
                )
                .unwrap();
        }
        let (candidates, _) = make_candidates(&connection, "channel-a", &current).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].right_item_id, "item-b");
        assert!(!candidates[0].source_evidence_current);
        connection
            .execute(
                "UPDATE upload_items SET digest='changed' WHERE id='item-b'",
                [],
            )
            .unwrap();
        assert!(make_candidates(&connection, "channel-a", &current)
            .unwrap()
            .0
            .is_empty());
        drop(connection);
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_fingerprint_never_compares_and_opaque_preview_checks_pair_membership() {
        let mut left = fingerprint("a", vec![1; 7], 120.0);
        left.duration = f64::NAN;
        assert!(compare(&left, &fingerprint("b", vec![1; 7], 120.0), true).is_none());
        let (root, state, connection) = fixture();
        seed(&connection, &root, "item-a", "channel-a");
        seed(&connection, &root, "item-b", "channel-a");
        let mut review = start(
            &state,
            "channel-a",
            &["item-a".into()],
            true,
            Some(root.join("fixture-decoder")),
            false,
        )
        .unwrap();
        review.candidates.push(
            compare(
                &fingerprint("item-a", vec![1, 2, 3, 4, 5, 6, 7], 120.0),
                &fingerprint("item-b", vec![1, 2, 3, 4, 5, 6, 7], 120.0),
                false,
            )
            .unwrap(),
        );
        persist_running(&connection, &review).unwrap();
        let candidate = &review.candidates[0];
        assert_eq!(
            preview_member_ids(
                &connection,
                "channel-a",
                &review.review_id,
                &candidate.candidate_id
            )
            .unwrap(),
            ["item-a", "item-b"]
        );
        assert!(preview_member_ids(&connection, "channel-a", &review.review_id, "wrong").is_err());
        assert!(preview_member_ids(
            &connection,
            "channel-b",
            &review.review_id,
            &candidate.candidate_id
        )
        .is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_length_changed_bytes_and_cancelled_reads_never_reach_decoder() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "item-a", "channel-a");
        let original = snapshot(&connection, "channel-a", "item-a").unwrap();
        assert!(
            fingerprint_original(&state, &original, &root.join("missing-decoder"), &|| true)
                .is_err()
        );
        fs::write(root.join("item-a.mp4"), b"item-b").unwrap();
        let signature =
            monitored_file_signature(&fs::metadata(root.join("item-a.mp4")).unwrap()).unwrap();
        connection
            .execute(
                "UPDATE upload_items SET source_modified_key=?1 WHERE id='item-a'",
                params![signature.1],
            )
            .unwrap();
        let original = snapshot(&connection, "channel-a", "item-a").unwrap();
        let error =
            fingerprint_original(&state, &original, &root.join("missing-decoder"), &|| false)
                .unwrap_err();
        assert!(error.contains("BLAKE3"));
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM perceptual_fingerprints", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        assert_eq!(fs::read(root.join("item-a.mp4")).unwrap(), b"item-b");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn switching_channel_before_worker_admission_cancels_the_original_review() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "item-a", "channel-a");
        seed(&connection, &root, "item-b", "channel-b");
        let review = start(
            &state,
            "channel-a",
            &["item-a".into()],
            true,
            Some(root.join("fixture-decoder")),
            false,
        )
        .unwrap();
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b' WHERE singleton=1",
                [],
            )
            .unwrap();
        let snapshots: String = connection
            .query_row(
                "SELECT snapshots_json FROM perceptual_review_jobs WHERE id=?1",
                params![review.review_id],
                |row| row.get(0),
            )
            .unwrap();
        let snapshots: Vec<Snapshot> = serde_json::from_str(&snapshots).unwrap();
        run(
            &state,
            "channel-a",
            &review.review_id,
            &snapshots,
            &root.join("missing-decoder"),
            &AtomicBool::new(false),
        );
        assert_eq!(
            load(&connection, "channel-a", &review.review_id, false)
                .unwrap()
                .state,
            "cancelled"
        );
        assert!(start(
            &state,
            "channel-b",
            &["item-b".into()],
            true,
            Some(root.join("fixture-decoder")),
            false
        )
        .is_ok());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_cache_and_review_identities_are_rejected_before_candidate_display() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "item-a", "channel-a");
        seed(&connection, &root, "item-b", "channel-a");
        let mut current = HashMap::new();
        current.insert(
            "item-a".into(),
            fingerprint("item-a", vec![1, 2, 3, 4, 5, 6, 7], 120.0),
        );
        let cache = fingerprint("item-b", vec![1, 2, 3, 4, 5, 6, 7], 120.0);
        let mut foreign = cache.clone();
        foreign.item_id = "foreign-item".into();
        connection
            .execute(
                "INSERT INTO perceptual_fingerprints VALUES('channel-a','item-b',?1,?2,?3,?4)",
                params![
                    cache.digest,
                    ALGORITHM_VERSION,
                    serde_json::to_string(&foreign).unwrap(),
                    cache.analyzed_at
                ],
            )
            .unwrap();
        assert!(make_candidates(&connection, "channel-a", &current)
            .unwrap()
            .0
            .is_empty());
        let mut another_recipe = cache.clone();
        another_recipe.decoder_artifact_id = "f".repeat(64);
        assert!(compare(&current["item-a"], &another_recipe, false).is_none());
        let mut review = start(
            &state,
            "channel-a",
            &["item-a".into()],
            true,
            Some(root.join("fixture-decoder")),
            false,
        )
        .unwrap();
        let id = review.review_id.clone();
        review.channel_id = "channel-b".into();
        connection
            .execute(
                "UPDATE perceptual_review_jobs SET review_json=?2 WHERE id=?1",
                params![id, serde_json::to_string(&review).unwrap()],
            )
            .unwrap();
        assert!(load(&connection, "channel-a", &id, true).is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Requires opt-in generated external fixture directory and pinned decoder"]
    fn generated_fixture_native_decode_and_comparison_preserves_all_originals() {
        let selected_root = PathBuf::from(
            std::env::var("YUM_PERCEPTUAL_FIXTURE_ROOT")
                .expect("Set a controlled generated fixture root"),
        );
        assert!(selected_root.is_absolute());
        let selected_root = fs::canonicalize(selected_root).unwrap();
        let evidence: serde_json::Value = serde_json::from_slice(
            &fs::read(selected_root.join("generated-fixtures.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            evidence["declaredSource"],
            "FFmpeg generated lavfi fixtures; no operator media"
        );
        let decoder = selected_root.join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
        let mut binary = open_original(&decoder).unwrap();
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 256 * 1024];
        loop {
            let count = binary.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        assert_eq!(format!("{:x}", hasher.finalize()), decoder_artifact_id());
        let (root, state, connection) = fixture();
        let mut originals = HashMap::new();
        for name in [
            "moving",
            "reencoded",
            "distinct",
            "sharedIntro",
            "still",
            "short",
            "nearby",
        ] {
            let source =
                fs::canonicalize(evidence["fixtures"][name]["path"].as_str().unwrap()).unwrap();
            assert!(source.starts_with(&selected_root));
            let bytes = fs::read(&source).unwrap();
            let signature = monitored_file_signature(&fs::metadata(&source).unwrap()).unwrap();
            let digest = blake3::hash(&bytes).to_hex().to_string();
            connection.execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,digest,background_hash_status,source_modified_key,status,created_at,updated_at) VALUES(?1,?1,?2,'channel-a',?3,?3,?4,?4,?5,'complete',?6,'failed',?7,?7)", params![name,source.file_name().unwrap().to_string_lossy(),source.to_string_lossy(),signature.0 as i64,digest,signature.1,now()]).unwrap();
            originals.insert(name.to_string(), (source, digest));
        }
        connection.execute("UPDATE upload_items SET confirmed_bytes=3,resumable_session_uri='fixture-protected-checkpoint' WHERE id='moving'", []).unwrap();
        let mut ids = originals.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        let mut snapshots = ids
            .iter()
            .map(|id| snapshot(&connection, "channel-a", id).unwrap())
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| left.id.cmp(&right.id));
        let review = start(
            &state,
            "channel-a",
            &ids,
            true,
            Some(decoder.clone()),
            false,
        )
        .unwrap();
        run(
            &state,
            "channel-a",
            &review.review_id,
            &snapshots,
            &decoder,
            &AtomicBool::new(false),
        );
        let review = load(&connection, "channel-a", &review.review_id, true).unwrap();
        assert_eq!(review.state, "complete");
        assert_eq!(review.processed, 7);
        assert!(review
            .sources
            .iter()
            .all(|source| source.state == "analyzed"));
        let mut statement=connection.prepare("SELECT item_id,fingerprint_json FROM perceptual_fingerprints WHERE channel_id='channel-a'").unwrap();
        let fingerprints = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .map(|row| {
                let (id, json) = row.unwrap();
                (id, serde_json::from_str::<Fingerprint>(&json).unwrap())
            })
            .collect::<HashMap<_, _>>();
        drop(statement);
        assert_eq!(fingerprints.len(), 7);
        for (id, (path, digest)) in &originals {
            assert_eq!(fingerprints[id].frames.len(), 7);
            assert_eq!(&fingerprints[id].digest, digest);
            assert_eq!(
                blake3::hash(&fs::read(path).unwrap()).to_hex().to_string(),
                *digest
            );
        }
        assert_eq!(
            compare(&fingerprints["moving"], &fingerprints["reencoded"], true)
                .unwrap()
                .median_distance,
            0
        );
        assert!(compare(&fingerprints["moving"], &fingerprints["distinct"], true).is_none());
        assert!(compare(&fingerprints["moving"], &fingerprints["sharedIntro"], true).is_none());
        assert!(compare(&fingerprints["moving"], &fingerprints["short"], true).is_none());
        assert!(
            compare(&fingerprints["moving"], &fingerprints["nearby"], true)
                .unwrap()
                .warnings
                .iter()
                .any(|warning| warning.contains("color"))
        );
        assert!(fingerprints["still"]
            .frames
            .iter()
            .all(|frame| frame.contrast < 12.0));
        assert!(fingerprints["short"].duration < 1.0);
        let (candidates, _) = make_candidates(&connection, "channel-a", &fingerprints).unwrap();
        assert!(candidates.iter().any(|pair| matches!(
            (pair.left_item_id.as_str(), pair.right_item_id.as_str()),
            ("moving", "reencoded") | ("reencoded", "moving")
        )));
        let changed: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM upload_items WHERE status!='failed' OR confirmed_bytes!=CASE WHEN id='moving' THEN 3 ELSE 0 END",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(changed, 0);
        let checkpoint: String = connection
            .query_row(
                "SELECT resumable_session_uri FROM upload_items WHERE id='moving'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(checkpoint, "fixture-protected-checkpoint");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
}
