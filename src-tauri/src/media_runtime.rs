//! Bounded device-local media I/O and FFprobe execution.
//!
//! This module owns the process/read concurrency limits, active-upload
//! priority signal, resumable copy-plus-BLAKE3 primitives, and stable-signature
//! probe cache. Database cancellation adapters and product-specific metadata
//! schemas stay with their owning workflows in `lib.rs`.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, Condvar, Mutex, OnceLock},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};

#[cfg(windows)]
use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};

use crate::{performance, spawn_worker};

pub(crate) const HASH_READ_BUFFER_BYTES: usize = 8 * 1024 * 1024;
// A 1 MiB buffer with ordinary file opens avoids the durable-flush tail
// regression measured on rotational storage. Read-only hashing keeps the
// larger sequential-scan path below.
const COPY_READ_BUFFER_BYTES: usize = 1024 * 1024;
const FFPROBE_METADATA_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const FFPROBE_STDOUT_MAX_BYTES: usize = 2 * 1024 * 1024;
const FFPROBE_DURATION_STDOUT_MAX_BYTES: usize = 64 * 1024;
const MAX_CONCURRENT_FFPROBE_PROCESSES: usize = 2;
const MAX_CONCURRENT_MEDIA_READERS_PER_VOLUME: usize = 1;
const MEDIA_PROBE_CACHE_ENTRIES: usize = 256;

fn user_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub(crate) fn source_volume_id(path: &Path) -> String {
    #[cfg(windows)]
    {
        use std::path::Component;
        if let Some(Component::Prefix(prefix)) = path.components().next() {
            return prefix.as_os_str().to_string_lossy().to_ascii_lowercase();
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as UnixMetadataExt;
        if let Ok(metadata) = fs::metadata(path).or_else(|_| {
            path.parent()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
                .and_then(fs::metadata)
        }) {
            return format!("dev:{}", metadata.dev());
        }
    }
    path.components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown-volume".into())
}

#[derive(Clone, Copy)]
pub(crate) enum MediaIoPriority {
    Foreground,
    Background,
}

#[derive(Default)]
struct MediaResourceState {
    ffprobe_processes: usize,
    readers_by_volume: HashMap<String, usize>,
    uploads_by_volume: HashMap<String, usize>,
}

#[derive(Default)]
struct MediaResourceScheduler {
    state: Mutex<MediaResourceState>,
    wake: Condvar,
}

enum MediaPermitKind {
    Probe,
    Reader(String),
}

struct MediaResourcePermit<'a> {
    scheduler: &'a MediaResourceScheduler,
    kind: Option<MediaPermitKind>,
}

pub(crate) struct MediaReadGuard<'a> {
    permit: MediaResourcePermit<'a>,
}

pub(crate) struct ActiveUploadGuard<'a> {
    scheduler: &'a MediaResourceScheduler,
    volume_id: String,
}

impl MediaResourceScheduler {
    fn acquire_probe(
        &self,
        path: &Path,
        cancelled: &impl Fn() -> bool,
    ) -> Result<MediaResourcePermit<'_>, String> {
        let volume_id = source_volume_id(path);
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The media resource scheduler is unavailable.".to_string())?;
        loop {
            if cancelled() {
                return Err("Media inspection was cancelled.".into());
            }
            let upload_active = state
                .uploads_by_volume
                .get(&volume_id)
                .copied()
                .unwrap_or(0)
                > 0;
            if state.ffprobe_processes < MAX_CONCURRENT_FFPROBE_PROCESSES && !upload_active {
                state.ffprobe_processes += 1;
                return Ok(MediaResourcePermit {
                    scheduler: self,
                    kind: Some(MediaPermitKind::Probe),
                });
            }
            state = self
                .wake
                .wait_timeout(state, Duration::from_millis(25))
                .map_err(|_| "The media resource scheduler is unavailable.".to_string())?
                .0;
        }
    }

    fn acquire_reader(
        &self,
        path: &Path,
        cancelled: &impl Fn() -> bool,
    ) -> Result<MediaResourcePermit<'_>, String> {
        let volume_id = source_volume_id(path);
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The media resource scheduler is unavailable.".to_string())?;
        loop {
            if cancelled() {
                return Err("Local media work was cancelled.".into());
            }
            let readers = state
                .readers_by_volume
                .get(&volume_id)
                .copied()
                .unwrap_or(0);
            if readers < MAX_CONCURRENT_MEDIA_READERS_PER_VOLUME {
                *state
                    .readers_by_volume
                    .entry(volume_id.clone())
                    .or_default() += 1;
                return Ok(MediaResourcePermit {
                    scheduler: self,
                    kind: Some(MediaPermitKind::Reader(volume_id)),
                });
            }
            state = self
                .wake
                .wait_timeout(state, Duration::from_millis(25))
                .map_err(|_| "The media resource scheduler is unavailable.".to_string())?
                .0;
        }
    }

    fn active_upload(&self, path: &Path) -> Result<ActiveUploadGuard<'_>, String> {
        let volume_id = source_volume_id(path);
        let mut state = self
            .state
            .lock()
            .map_err(|_| "The media resource scheduler is unavailable.".to_string())?;
        *state
            .uploads_by_volume
            .entry(volume_id.clone())
            .or_default() += 1;
        self.wake.notify_all();
        Ok(ActiveUploadGuard {
            scheduler: self,
            volume_id,
        })
    }
}

impl MediaResourcePermit<'_> {
    fn yield_to_upload(
        &self,
        priority: MediaIoPriority,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), String> {
        if cancelled() {
            return Err("Local media work was cancelled.".into());
        }
        let Some(MediaPermitKind::Reader(volume_id)) = self.kind.as_ref() else {
            return Ok(());
        };
        let state = self
            .scheduler
            .state
            .lock()
            .map_err(|_| "The media resource scheduler is unavailable.".to_string())?;
        if state.uploads_by_volume.get(volume_id).copied().unwrap_or(0) == 0 {
            return Ok(());
        }
        let delay = match priority {
            MediaIoPriority::Foreground => Duration::from_millis(10),
            MediaIoPriority::Background => Duration::from_millis(40),
        };
        let _ = self
            .scheduler
            .wake
            .wait_timeout(state, delay)
            .map_err(|_| "The media resource scheduler is unavailable.".to_string())?;
        if cancelled() {
            return Err("Local media work was cancelled.".into());
        }
        Ok(())
    }
}

impl MediaReadGuard<'_> {
    pub(crate) fn yield_to_upload(
        &self,
        priority: MediaIoPriority,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), String> {
        self.permit.yield_to_upload(priority, cancelled)
    }
}

impl Drop for MediaResourcePermit<'_> {
    fn drop(&mut self) {
        let Some(kind) = self.kind.take() else {
            return;
        };
        if let Ok(mut state) = self.scheduler.state.lock() {
            match kind {
                MediaPermitKind::Probe => {
                    state.ffprobe_processes = state.ffprobe_processes.saturating_sub(1);
                }
                MediaPermitKind::Reader(volume_id) => {
                    if let Some(readers) = state.readers_by_volume.get_mut(&volume_id) {
                        *readers = readers.saturating_sub(1);
                        if *readers == 0 {
                            state.readers_by_volume.remove(&volume_id);
                        }
                    }
                }
            }
            self.scheduler.wake.notify_all();
        }
    }
}

impl Drop for ActiveUploadGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.scheduler.state.lock() {
            if let Some(uploads) = state.uploads_by_volume.get_mut(&self.volume_id) {
                *uploads = uploads.saturating_sub(1);
                if *uploads == 0 {
                    state.uploads_by_volume.remove(&self.volume_id);
                }
            }
            self.scheduler.wake.notify_all();
        }
    }
}

fn media_resource_scheduler() -> &'static MediaResourceScheduler {
    static SCHEDULER: OnceLock<MediaResourceScheduler> = OnceLock::new();
    SCHEDULER.get_or_init(MediaResourceScheduler::default)
}

pub(crate) fn active_upload(path: &Path) -> Result<ActiveUploadGuard<'static>, String> {
    media_resource_scheduler().active_upload(path)
}

pub(crate) fn acquire_reader(
    path: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<MediaReadGuard<'static>, String> {
    media_resource_scheduler()
        .acquire_reader(path, cancelled)
        .map(|permit| MediaReadGuard { permit })
}

#[cfg(test)]
pub(crate) fn copy_and_digest(
    source: &Path,
    destination_partial: &Path,
) -> Result<(u64, String), String> {
    copy_and_digest_with_cancel(source, destination_partial, &|| false)
}

#[cfg(test)]
pub(crate) struct CopyAndDigestPhaseTimings {
    pub(crate) stream_and_hash: Duration,
    pub(crate) durable_flush: Duration,
}

#[cfg(test)]
pub(crate) fn copy_and_digest_profiled(
    source: &Path,
    destination_partial: &Path,
) -> Result<((u64, String), CopyAndDigestPhaseTimings), String> {
    let mut timings = None;
    let result = copy_and_digest_inner::<true>(
        source,
        destination_partial,
        &|| false,
        |stream_and_hash, durable_flush| {
            timings = Some(CopyAndDigestPhaseTimings {
                stream_and_hash,
                durable_flush,
            });
        },
    )?;
    Ok((
        result,
        timings.expect("profiled copy records phase timings"),
    ))
}

pub(crate) fn copy_and_digest_with_cancel(
    source: &Path,
    destination_partial: &Path,
    cancelled: &impl Fn() -> bool,
) -> Result<(u64, String), String> {
    copy_and_digest_inner::<false>(source, destination_partial, cancelled, |_, _| {})
}

fn copy_and_digest_inner<const MEASURE_PHASES: bool>(
    source: &Path,
    destination_partial: &Path,
    cancelled: &impl Fn() -> bool,
    record_phases: impl FnOnce(Duration, Duration),
) -> Result<(u64, String), String> {
    let permit = media_resource_scheduler().acquire_reader(source, cancelled)?;
    let mut input = File::open(source).map_err(user_error)?;
    let existing_bytes = destination_partial
        .metadata()
        .map(|value| value.len())
        .unwrap_or(0);
    let source_bytes = input.metadata().map_err(user_error)?.len();
    if existing_bytes > source_bytes {
        return Err("The interrupted local copy is larger than its original source.".into());
    }
    input
        .seek(SeekFrom::Start(existing_bytes))
        .map_err(user_error)?;
    let mut output = open_partial_for_append(destination_partial)?;
    let mut hasher = blake3::Hasher::new();
    // Keep this buffer on the heap: startup recovery can run on a small native
    // stack, and release inlining must not move the buffer into that frame.
    let mut buffer = vec![0_u8; COPY_READ_BUFFER_BYTES];
    let stream_started = if MEASURE_PHASES {
        Some(Instant::now())
    } else {
        None
    };

    if existing_bytes > 0 {
        let mut prior = File::open(destination_partial).map_err(user_error)?;
        loop {
            permit.yield_to_upload(MediaIoPriority::Foreground, cancelled)?;
            let bytes = prior.read(&mut buffer).map_err(user_error)?;
            if bytes == 0 {
                break;
            }
            hasher.update(&buffer[..bytes]);
        }
    }
    let mut copied = existing_bytes;
    loop {
        permit.yield_to_upload(MediaIoPriority::Foreground, cancelled)?;
        let bytes = input.read(&mut buffer).map_err(user_error)?;
        if bytes == 0 {
            break;
        }
        output.write_all(&buffer[..bytes]).map_err(user_error)?;
        hasher.update(&buffer[..bytes]);
        copied += bytes as u64;
    }
    let stream_and_hash = stream_started.map(|started| started.elapsed());
    let flush_started = if MEASURE_PHASES {
        Some(Instant::now())
    } else {
        None
    };
    output.sync_all().map_err(user_error)?;
    if let (Some(stream_and_hash), Some(flush_started)) = (stream_and_hash, flush_started) {
        record_phases(stream_and_hash, flush_started.elapsed());
    }
    Ok((copied, hasher.finalize().to_hex().to_string()))
}

pub(crate) fn digest_file(path: &Path) -> Result<(u64, String), String> {
    digest_file_scheduled(path, MediaIoPriority::Foreground, &|| false)
}

pub(crate) fn digest_file_scheduled(
    path: &Path,
    priority: MediaIoPriority,
    cancelled: &impl Fn() -> bool,
) -> Result<(u64, String), String> {
    let permit = media_resource_scheduler().acquire_reader(path, cancelled)?;
    let mut input = open_sequential_file_for_read(path)?;
    digest_reader_scheduled(&mut input, &permit, priority, cancelled)
}

pub(crate) struct ThrottledCancellationCheck<F> {
    check: F,
    interval: Duration,
    cached: Mutex<Option<(Instant, bool)>>,
}

impl<F: Fn() -> bool> ThrottledCancellationCheck<F> {
    pub(crate) fn new(interval: Duration, check: F) -> Self {
        Self {
            check,
            interval,
            cached: Mutex::new(None),
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        let Ok(mut cached) = self.cached.lock() else {
            return true;
        };
        if let Some((checked_at, value)) = *cached {
            if value || checked_at.elapsed() < self.interval {
                return value;
            }
        }
        let value = (self.check)();
        *cached = Some((Instant::now(), value));
        value
    }
}

fn open_sequential_file_for_read(path: &Path) -> Result<File, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    options.custom_flags(0x0800_0000); // FILE_FLAG_SEQUENTIAL_SCAN
    options.open(path).map_err(user_error)
}

fn open_partial_for_append(path: &Path) -> Result<File, String> {
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    options.open(path).map_err(user_error)
}

fn digest_reader_scheduled(
    reader: &mut impl Read,
    permit: &MediaResourcePermit<'_>,
    priority: MediaIoPriority,
    cancelled: &impl Fn() -> bool,
) -> Result<(u64, String), String> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; HASH_READ_BUFFER_BYTES];
    let mut bytes_read = 0_u64;
    loop {
        permit.yield_to_upload(priority, cancelled)?;
        let bytes = reader.read(&mut buffer).map_err(user_error)?;
        if bytes == 0 {
            break;
        }
        hasher.update(&buffer[..bytes]);
        bytes_read += bytes as u64;
    }
    Ok((bytes_read, hasher.finalize().to_hex().to_string()))
}

pub(crate) fn digest_reader_for_path(
    reader: &mut impl Read,
    path: &Path,
    priority: MediaIoPriority,
    cancelled: &impl Fn() -> bool,
) -> Result<(u64, String), String> {
    let permit = media_resource_scheduler().acquire_reader(path, cancelled)?;
    digest_reader_scheduled(reader, &permit, priority, cancelled)
}

pub(crate) fn iso_bmff_duration_seconds(path: &Path) -> Option<f64> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let mut cursor = 0_u64;
    while cursor.saturating_add(8) <= length {
        file.seek(SeekFrom::Start(cursor)).ok()?;
        let mut header = [0_u8; 8];
        file.read_exact(&mut header).ok()?;
        let size = u32::from_be_bytes(header[..4].try_into().ok()?) as u64;
        let mut header_length = 8_u64;
        let box_size = if size == 1 {
            let mut extended_size = [0_u8; 8];
            file.read_exact(&mut extended_size).ok()?;
            header_length = 16;
            u64::from_be_bytes(extended_size)
        } else if size == 0 {
            length.saturating_sub(cursor)
        } else {
            size
        };
        if box_size < header_length || cursor.saturating_add(box_size) > length {
            return None;
        }
        if &header[4..8] == b"moov" {
            let moov_end = cursor + box_size;
            let mut child = cursor + header_length;
            while child.saturating_add(8) <= moov_end {
                file.seek(SeekFrom::Start(child)).ok()?;
                let mut child_header = [0_u8; 8];
                file.read_exact(&mut child_header).ok()?;
                let child_size = u32::from_be_bytes(child_header[..4].try_into().ok()?) as u64;
                if child_size < 8 || child.saturating_add(child_size) > moov_end {
                    return None;
                }
                if &child_header[4..8] == b"mvhd" && child_size >= 28 {
                    let mut content = vec![0_u8; (child_size - 8).min(32) as usize];
                    file.read_exact(&mut content).ok()?;
                    let (timescale, duration) = match content.first().copied()? {
                        0 if content.len() >= 20 => (
                            u32::from_be_bytes(content[12..16].try_into().ok()?) as u64,
                            u32::from_be_bytes(content[16..20].try_into().ok()?) as u64,
                        ),
                        1 if content.len() >= 32 => (
                            u32::from_be_bytes(content[20..24].try_into().ok()?) as u64,
                            u64::from_be_bytes(content[24..32].try_into().ok()?),
                        ),
                        _ => return None,
                    };
                    return (timescale > 0 && duration != u64::MAX)
                        .then(|| duration as f64 / timescale as f64);
                }
                child += child_size;
            }
            return None;
        }
        cursor += box_size;
    }
    None
}

pub(crate) fn read_bounded_output(
    reader: &mut impl Read,
    limit: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    let mut output = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let bytes = reader.read(&mut buffer)?;
        if bytes == 0 {
            return Ok(Some(output));
        }
        if output.len().saturating_add(bytes) > limit {
            return Ok(None);
        }
        output.extend_from_slice(&buffer[..bytes]);
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct StableFileSignature {
    path: PathBuf,
    size_bytes: u64,
    modified_key: String,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum ProbeDepth {
    Duration,
    Rich,
}

#[derive(Clone, Default)]
pub(crate) struct CachedProbe {
    pub(crate) duration_seconds: Option<f64>,
    pub(crate) rich_report: Option<serde_json::Value>,
    duration_complete: bool,
    rich_complete: bool,
}

#[derive(Default)]
struct MediaProbeCache {
    entries: HashMap<StableFileSignature, CachedProbe>,
    order: VecDeque<StableFileSignature>,
}

#[derive(Default)]
struct MediaProbeCacheState {
    cache: MediaProbeCache,
    in_flight: HashSet<(StableFileSignature, ProbeDepth)>,
}

#[derive(Default)]
struct MediaProbeCacheCoordinator {
    state: Mutex<MediaProbeCacheState>,
    wake: Condvar,
}

impl MediaProbeCache {
    fn get(&self, signature: &StableFileSignature, depth: ProbeDepth) -> Option<CachedProbe> {
        let cached = self.entries.get(signature)?;
        match depth {
            ProbeDepth::Duration if cached.duration_complete || cached.rich_complete => {
                Some(cached.clone())
            }
            ProbeDepth::Rich if cached.rich_complete => Some(cached.clone()),
            _ => None,
        }
    }

    fn record(
        &mut self,
        signature: StableFileSignature,
        depth: ProbeDepth,
        report: serde_json::Value,
    ) {
        if !self.entries.contains_key(&signature) {
            self.order.push_back(signature.clone());
        }
        let cached = self.entries.entry(signature.clone()).or_default();
        let duration_seconds = probe_duration_from_report(&report);
        cached.duration_complete = true;
        cached.duration_seconds = duration_seconds.or(cached.duration_seconds);
        if matches!(depth, ProbeDepth::Rich) {
            cached.rich_complete = true;
            cached.rich_report = Some(report);
        }
        while self.entries.len() > MEDIA_PROBE_CACHE_ENTRIES {
            if let Some(oldest) = self.order.pop_front() {
                if oldest != signature {
                    self.entries.remove(&oldest);
                }
            } else {
                break;
            }
        }
    }
}

fn media_probe_cache() -> &'static MediaProbeCacheCoordinator {
    static CACHE: OnceLock<MediaProbeCacheCoordinator> = OnceLock::new();
    CACHE.get_or_init(MediaProbeCacheCoordinator::default)
}

fn stable_file_signature(path: &Path) -> Option<StableFileSignature> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified = metadata.modified().ok()?;
    let modified = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(StableFileSignature {
        path: path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
        size_bytes: metadata.len(),
        modified_key: format!("{}:{}", modified.as_secs(), modified.subsec_nanos()),
    })
}

pub(crate) fn probe_duration_from_report(report: &serde_json::Value) -> Option<f64> {
    report
        .pointer("/format/duration")
        .and_then(|value| {
            value
                .as_str()
                .and_then(|value| value.parse::<f64>().ok())
                .or_else(|| value.as_f64())
        })
        .filter(|duration| duration.is_finite() && *duration >= 0.0)
}

pub(crate) fn cached_ffprobe_report(
    path: &Path,
    depth: ProbeDepth,
    cancelled: &impl Fn() -> bool,
) -> Option<CachedProbe> {
    let signature = stable_file_signature(path)?;
    let coordinator = media_probe_cache();
    let key = (signature.clone(), depth);
    let mut state = coordinator.state.lock().ok()?;
    loop {
        if let Some(cached) = state.cache.get(&signature, depth) {
            return Some(cached);
        }
        let richer_in_flight = matches!(depth, ProbeDepth::Duration)
            && state
                .in_flight
                .contains(&(signature.clone(), ProbeDepth::Rich));
        if !state.in_flight.contains(&key) && !richer_in_flight {
            state.in_flight.insert(key.clone());
            break;
        }
        if cancelled() {
            return None;
        }
        state = coordinator
            .wake
            .wait_timeout(state, Duration::from_millis(25))
            .ok()?
            .0;
    }
    drop(state);
    let report = ffprobe_report(path, depth, cancelled);
    let mut state = coordinator.state.lock().ok()?;
    state.in_flight.remove(&key);
    if let Some(report) = report {
        state.cache.record(signature.clone(), depth, report);
    }
    let cached = state.cache.get(&signature, depth);
    coordinator.wake.notify_all();
    cached
}

fn ffprobe_report(
    path: &Path,
    depth: ProbeDepth,
    cancelled: &impl Fn() -> bool,
) -> Option<serde_json::Value> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (path, depth, cancelled);
        return None;
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _permit = media_resource_scheduler()
            .acquire_probe(path, cancelled)
            .ok()?;
        let mut command = ffprobe_command();
        command.arg("-v").arg("error");
        let output_limit = match depth {
            ProbeDepth::Duration => {
                command.args(["-show_entries", "format=duration", "-of", "json"]);
                FFPROBE_DURATION_STDOUT_MAX_BYTES
            }
            ProbeDepth::Rich => {
                command.args(["-show_format", "-show_streams", "-of", "json"]);
                FFPROBE_STDOUT_MAX_BYTES
            }
        };
        let mut child = command
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        performance::record_ffprobe_process();
        let mut stdout = child.stdout.take()?;
        let (output_sender, output_receiver) = mpsc::sync_channel(1);
        spawn_worker(move || {
            let output = read_bounded_output(&mut stdout, output_limit)
                .ok()
                .flatten();
            let _ = output_sender.send(output);
        });
        let deadline = Instant::now() + FFPROBE_METADATA_TIMEOUT;
        let mut captured_output = None;
        let exit_status = loop {
            if cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            match output_receiver.try_recv() {
                Ok(Some(output)) => captured_output = Some(output),
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
                Ok(None) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        };
        let output = captured_output.or_else(|| {
            output_receiver
                .recv_timeout(Duration::from_secs(1))
                .ok()
                .flatten()
        })?;
        if !exit_status.success() || output.len() > output_limit {
            return None;
        }
        serde_json::from_slice(&output).ok()
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn ffprobe_command() -> Command {
    let executable_name = if cfg!(target_os = "windows") {
        "ffprobe.exe"
    } else {
        "ffprobe"
    };
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|current_exe| bundled_sidecar_path(&current_exe, executable_name));
    let mut command = Command::new(bundled.unwrap_or_else(|| PathBuf::from("ffprobe")));
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    command
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(crate) fn bundled_sidecar_path(current_exe: &Path, executable_name: &str) -> Option<PathBuf> {
    let executable_dir = current_exe.parent()?;
    [
        executable_dir.join(executable_name),
        executable_dir.parent()?.join(executable_name),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    use uuid::Uuid;

    #[test]
    fn interrupted_copy_resumes_and_keeps_the_original_digest() {
        let root = std::env::temp_dir().join(format!("media-copy-resume-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.mp4");
        let partial = root.join("managed.partial");
        let data = (0..2_600_000_u32)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(&source, &data).unwrap();
        fs::write(&partial, &data[..1_100_000]).unwrap();

        let (copied, digest) = copy_and_digest(&source, &partial).unwrap();

        assert_eq!(copied, data.len() as u64);
        assert_eq!(digest, blake3::hash(&data).to_hex().to_string());
        assert_eq!(fs::read(&partial).unwrap(), data);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ffprobe_output_limit_is_enforced_while_reading() {
        let mut normal = std::io::Cursor::new(b"{\"format\":{}}".to_vec());
        assert_eq!(
            read_bounded_output(&mut normal, FFPROBE_STDOUT_MAX_BYTES).unwrap(),
            Some(b"{\"format\":{}}".to_vec())
        );
        let mut oversized = std::io::Cursor::new(vec![b'x'; FFPROBE_STDOUT_MAX_BYTES + 1]);
        assert_eq!(
            read_bounded_output(&mut oversized, FFPROBE_STDOUT_MAX_BYTES).unwrap(),
            None
        );
    }

    #[test]
    fn ffprobe_duration_parser_accepts_finite_values_and_rejects_malformed_values() {
        assert_eq!(
            probe_duration_from_report(&serde_json::json!({"format": {"duration": "8.25"}})),
            Some(8.25)
        );
        assert_eq!(
            probe_duration_from_report(&serde_json::json!({"format": {"duration": 12.5}})),
            Some(12.5)
        );
        assert_eq!(
            probe_duration_from_report(&serde_json::json!({"format": {"duration": "NaN"}})),
            None
        );
        assert_eq!(probe_duration_from_report(&serde_json::json!({})), None);
    }

    #[test]
    fn rich_probe_cache_reuses_duration_only_for_the_same_stable_signature() {
        let signature = StableFileSignature {
            path: PathBuf::from("fixture.mp4"),
            size_bytes: 42,
            modified_key: "100:1".into(),
        };
        let changed = StableFileSignature {
            size_bytes: 43,
            ..signature.clone()
        };
        let mut cache = MediaProbeCache::default();
        cache.record(
            signature.clone(),
            ProbeDepth::Rich,
            serde_json::json!({"format": {"duration": "19.75"}}),
        );
        assert_eq!(
            cache
                .get(&signature, ProbeDepth::Duration)
                .unwrap()
                .duration_seconds,
            Some(19.75)
        );
        assert!(cache.get(&changed, ProbeDepth::Duration).is_none());
    }

    #[test]
    fn stable_probe_signature_invalidates_when_file_size_changes() {
        let source = std::env::temp_dir().join(format!(
            "youtube-upload-manager-probe-signature-{}.mp4",
            Uuid::new_v4()
        ));
        fs::write(&source, b"one").unwrap();
        let first = stable_file_signature(&source).unwrap();
        fs::write(&source, b"replacement").unwrap();
        let second = stable_file_signature(&source).unwrap();
        assert_ne!(first, second);
        fs::remove_file(source).unwrap();
    }

    #[test]
    fn media_reader_wait_is_bounded_and_cancellable() {
        let scheduler = Arc::new(MediaResourceScheduler::default());
        let source = std::env::temp_dir().join(format!("media-reader-{}", Uuid::new_v4()));
        let first = scheduler.acquire_reader(&source, &|| false).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_scheduler = scheduler.clone();
        let worker_source = source.clone();
        let worker_cancelled = cancelled.clone();
        let worker = thread::spawn(move || {
            worker_scheduler
                .acquire_reader(&worker_source, &|| worker_cancelled.load(Ordering::Acquire))
                .is_err()
        });
        thread::sleep(Duration::from_millis(60));
        cancelled.store(true, Ordering::Release);
        assert!(worker.join().unwrap());
        drop(first);
    }

    #[test]
    fn ffprobe_slots_are_bounded_and_wait_behind_a_same_volume_upload() {
        let scheduler = Arc::new(MediaResourceScheduler::default());
        let source = std::env::temp_dir().join(format!("probe-bound-{}", Uuid::new_v4()));
        let first = scheduler.acquire_probe(&source, &|| false).unwrap();
        let second = scheduler.acquire_probe(&source, &|| false).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_scheduler = scheduler.clone();
        let worker_source = source.clone();
        let worker_cancelled = cancelled.clone();
        let worker = thread::spawn(move || {
            worker_scheduler
                .acquire_probe(&worker_source, &|| worker_cancelled.load(Ordering::Acquire))
                .is_err()
        });
        thread::sleep(Duration::from_millis(60));
        cancelled.store(true, Ordering::Release);
        assert!(worker.join().unwrap());
        drop(first);
        drop(second);

        let upload = scheduler.active_upload(&source).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_scheduler = scheduler.clone();
        let worker_source = source.clone();
        let worker_cancelled = cancelled.clone();
        let worker = thread::spawn(move || {
            worker_scheduler
                .acquire_probe(&worker_source, &|| worker_cancelled.load(Ordering::Acquire))
                .is_err()
        });
        thread::sleep(Duration::from_millis(60));
        cancelled.store(true, Ordering::Release);
        assert!(worker.join().unwrap());
        drop(upload);
    }

    #[test]
    fn background_media_yields_when_an_upload_uses_the_same_volume() {
        let scheduler = MediaResourceScheduler::default();
        let source = std::env::temp_dir().join(format!("media-priority-{}", Uuid::new_v4()));
        let reader = scheduler.acquire_reader(&source, &|| false).unwrap();
        let upload = scheduler.active_upload(&source).unwrap();
        let started = Instant::now();
        reader
            .yield_to_upload(MediaIoPriority::Background, &|| false)
            .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(30));
        drop(upload);
        drop(reader);
    }

    #[test]
    fn cancellation_check_is_time_throttled_across_many_chunks() {
        let checks = AtomicUsize::new(0);
        let cancellation = ThrottledCancellationCheck::new(Duration::from_secs(1), || {
            checks.fetch_add(1, Ordering::Relaxed);
            false
        });
        for _ in 0..10_000 {
            assert!(!cancellation.is_cancelled());
        }
        assert_eq!(checks.load(Ordering::Relaxed), 1);
    }
}
