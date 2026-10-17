//! Short, memory-only compatibility segments for originals the webview cannot
//! decode. The opaque preview grant remains the sole source authority.
use super::*;
use crate::media_runtime;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    cell::Cell,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
};

const SEGMENT_SECONDS: f64 = 6.0;
const MAX_SEGMENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_PROBE_BYTES: usize = 64 * 1024;
const PROCESS_TIMEOUT: Duration = Duration::from_secs(20);
const MIME_TYPE: &str = "video/mp4; codecs=\"avc1.42c01e, mp4a.40.2\"";
const CANCELLED: &str = "The comparison closed, its channel changed, or its original became unavailable. Reopen the comparison.";
const UNPLAYABLE: &str =
    "This original could not produce a compatibility preview. Its file and upload are unchanged.";
const TIMEOUT: &str =
    "Compatibility playback exceeded its 20-second preview budget. Retry this position.";

#[derive(Default)]
pub(super) struct State {
    decoder: Option<VerifiedDecoder>,
    duration: Option<f64>,
}

struct VerifiedDecoder {
    path: PathBuf,
    signature: (u64, String),
    identity: FileIdentity,
}

fn entry(token: &str) -> Result<Arc<PreviewEntry>, String> {
    if !Uuid::parse_str(token).is_ok_and(|id| id.to_string() == token) {
        return Err(CANCELLED.into());
    }
    grants()
        .lock()
        .ok()
        .and_then(|registry| registry.get(token).cloned())
        .ok_or_else(|| CANCELLED.into())
}

fn current(entry: &PreviewEntry, grant: &PreviewGrant) -> bool {
    !entry.revoked.load(Ordering::Acquire) && validate_grant(grant).is_ok()
}

fn require_current(entry: &PreviewEntry, grant: &PreviewGrant) -> Result<(), String> {
    if current(entry, grant) {
        Ok(())
    } else {
        entry.revoked.store(true, Ordering::Release);
        Err(CANCELLED.into())
    }
}

fn decoder_identity(path: &Path) -> Option<((u64, String), FileIdentity)> {
    let file = open_reference_source(path).ok()?;
    Some((
        monitored_file_signature(&file.metadata().ok()?).ok()?,
        file_identity(&file).ok()?,
    ))
}

fn ensure_decoder(grant: &mut PreviewGrant) -> Result<(), String> {
    if let Some(decoder) = &grant.compatibility.decoder {
        if decoder_identity(&decoder.path).is_some_and(|(signature, identity)| {
            signature == decoder.signature && identity == decoder.identity
        }) {
            return Ok(());
        }
        return Err(
            "The bundled compatibility decoder changed. Reopen the app before retrying.".into(),
        );
    }
    // This verifies the pinned bundled binary digest. Cache that expensive
    // verification for this grant; each segment still checks its file identity.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let before = std::env::current_exe().ok().and_then(|executable| {
        media_runtime::bundled_sidecar_path(
            &executable,
            if cfg!(windows) {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            },
        )
        .and_then(|path| decoder_identity(&path).map(|identity| (path, identity)))
    });
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let before: Option<(PathBuf, ((u64, String), FileIdentity))> = None;
    let (candidate, before) = before.ok_or(
        "Compatibility playback is unavailable because the verified bundled decoder is missing on this device.",
    )?;
    let path = crate::perceptual_review::decoder_path().filter(|path| path == &candidate).ok_or(
        "Compatibility playback is unavailable because the verified bundled decoder is missing on this device.",
    )?;
    let (signature, identity) = decoder_identity(&path).ok_or(UNPLAYABLE)?;
    if (signature.clone(), identity) != before {
        return Err(UNPLAYABLE.into());
    }
    let identity = before.1;
    grant.compatibility.decoder = Some(VerifiedDecoder {
        path,
        signature,
        identity,
    });
    Ok(())
}

#[cfg(test)]
pub(super) fn install_test_decoder(grant: &mut PreviewGrant, path: &Path) {
    use sha2::{Digest, Sha256};
    let mut reader = open_reference_source(path).unwrap();
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes = reader.read(&mut buffer).unwrap();
        if bytes == 0 {
            break;
        }
        hash.update(&buffer[..bytes]);
    }
    let expected = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "04e1307997530f9cf2fe35cba2ca7e8875ca91da02f89d6c7243df819c94ad00"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "e7e7fb30477f717e6f55f9180a70386c62677ef8a4d4d1a5d948f4098aa3eb99"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "a90e3db6a3fd35f6074b013f948b1aa45b31c6375489d39e572bea3f18336584"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "ebdddc936f61e14049a2d4b549a412b8a40deeff6540e58a9f2a2da9e6b18894"
    } else {
        panic!("No pinned preview decoder fixture exists for this platform.");
    };
    assert_eq!(format!("{:x}", hash.finalize()), expected);
    let (signature, identity) = decoder_identity(path).unwrap();
    grant.compatibility.decoder = Some(VerifiedDecoder {
        path: path.into(),
        signature,
        identity,
    });
}

fn duration(grant: &PreviewGrant, cancelled: &impl Fn() -> bool) -> Result<f64, String> {
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    let admission_cancelled = || Instant::now() >= deadline || cancelled();
    let admission_error = |error| {
        if Instant::now() >= deadline {
            TIMEOUT.to_string()
        } else {
            error
        }
    };
    let reader = media_runtime::acquire_reader(&grant.path, &admission_cancelled)
        .map_err(admission_error)?;
    reader
        .yield_to_upload(crate::MediaIoPriority::Foreground, &admission_cancelled)
        .map_err(admission_error)?;
    let container = media_runtime::iso_bmff_duration_seconds(&grant.path);
    drop(reader);
    if Instant::now() >= deadline {
        return Err(TIMEOUT.into());
    }
    if cancelled() {
        return Err(CANCELLED.into());
    }
    if let Some(duration) = container.filter(|duration| duration.is_finite() && *duration > 0.0) {
        return Ok(duration);
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let executable = std::env::current_exe().map_err(|_| UNPLAYABLE)?;
        let probe = media_runtime::bundled_sidecar_path(
            &executable,
            if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            },
        )
        .ok_or("The bundled duration inspector is unavailable on this device.")?;
        let probe_identity = decoder_identity(&probe).ok_or(UNPLAYABLE)?;
        // Acquire the shared process slot before the reader, matching segment
        // lock order. Foreground playback may inspect an original during uploads.
        let _process = media_runtime::acquire_preview_decoder(&grant.path, &admission_cancelled)
            .map_err(admission_error)?;
        let reader = media_runtime::acquire_reader(&grant.path, &admission_cancelled)
            .map_err(admission_error)?;
        reader
            .yield_to_upload(crate::MediaIoPriority::Foreground, &admission_cancelled)
            .map_err(admission_error)?;
        let source = held_decode_source(grant)?;
        let mut command = Command::new(&probe);
        let input = configure_input(&mut command, &source, &grant.path);
        command
            .args([
                "-v",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-format_whitelist",
                "mov,matroska,avi,flv,mpeg,asf",
                "-show_entries",
                "format=duration",
                "-of",
                "json",
                "-i",
            ])
            .arg(input)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = KillOnDrop(command.spawn().map_err(|_| UNPLAYABLE)?);
        let bytes = read_child_with_limit(&mut child.0, cancelled, deadline, MAX_PROBE_BYTES)?;
        if decoder_identity(&probe).is_none_or(|identity| identity != probe_identity) {
            return Err(UNPLAYABLE.into());
        }
        let report = serde_json::from_slice(&bytes).map_err(|_| UNPLAYABLE)?;
        media_runtime::probe_duration_from_report(&report)
            .filter(|duration| *duration > 0.0)
            .ok_or_else(|| UNPLAYABLE.into())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Err(UNPLAYABLE.into())
    }
}

pub(super) fn info(token: &str) -> Result<PreviewCompatibilityInfo, String> {
    let entry = entry(token)?;
    let mut grant = entry.grant.lock().map_err(|_| CANCELLED)?;
    require_current(&entry, &grant)?;
    ensure_decoder(&mut grant)?;
    if grant.compatibility.duration.is_none() {
        let cancelled = || !current(&entry, &grant);
        grant.compatibility.duration = Some(duration(&grant, &cancelled)?);
    }
    require_current(&entry, &grant)?;
    Ok(PreviewCompatibilityInfo {
        duration_seconds: grant.compatibility.duration.ok_or(UNPLAYABLE)?,
        mime_type: MIME_TYPE,
    })
}

fn segment_duration(start: f64, duration: f64) -> Result<f64, String> {
    if !start.is_finite()
        || start < 0.0
        || !duration.is_finite()
        || duration <= 0.0
        || start >= duration
    {
        return Err("Choose a position within the original video.".into());
    }
    Ok((duration - start).min(SEGMENT_SECONDS))
}

fn held_decode_source(grant: &PreviewGrant) -> Result<File, String> {
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        // FFmpeg opens the path on Windows. Holding FILE_SHARE_READ only for
        // this decode prevents a writer or replacement racing that open.
        let mut options = std::fs::OpenOptions::new();
        options
            .read(true)
            .share_mode(1)
            .custom_flags(0x0020_0000 | 0x0800_0000);
        options.open(&grant.path).map_err(|_| CANCELLED)?
    };
    #[cfg(not(windows))]
    let file = grant.file.try_clone().map_err(|_| CANCELLED)?;
    if monitored_file_signature(&file.metadata().map_err(|_| CANCELLED)?).map_err(|_| CANCELLED)?
        != grant.signature
        || file_identity(&file).map_err(|_| CANCELLED)? != grant.identity
    {
        return Err(CANCELLED.into());
    }
    Ok(file)
}

fn configure_input(command: &mut Command, file: &File, path: &Path) -> PathBuf {
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    #[cfg(unix)]
    let input = {
        use std::os::{fd::AsRawFd, unix::process::CommandExt};
        let descriptor = file.as_raw_fd();
        // SAFETY: fcntl is async-signal-safe and this read-only descriptor is
        // held until the bounded child exits. No allocation occurs in pre_exec.
        unsafe {
            command.pre_exec(move || {
                if libc::fcntl(descriptor, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let _ = path;
        PathBuf::from(format!(
            "{}/{descriptor}",
            if cfg!(target_os = "linux") {
                "/proc/self/fd"
            } else {
                "/dev/fd"
            }
        ))
    };
    #[cfg(not(unix))]
    let input = {
        let _ = file;
        path.to_path_buf()
    };
    input
}

fn configure_command(command: &mut Command, file: &File, path: &Path, start: f64, length: f64) {
    let input = configure_input(command, file, path);
    command
        .args([
            "-hide_banner", "-loglevel", "error", "-nostdin", "-threads", "1",
            "-protocol_whitelist", "file,pipe", "-format_whitelist", "mov,matroska,avi,flv,mpeg,asf", "-ss",
        ])
        .arg(format!("{start:.6}"))
        .arg("-i").arg(input)
        .arg("-t").arg(format!("{length:.6}"))
        .args([
            "-map", "0:v:0", "-map", "0:a:0?", "-sn", "-dn", "-map_metadata", "-1",
            "-vf", "scale=640:360:force_original_aspect_ratio=decrease,pad=640:360:(ow-iw)/2:(oh-ih)/2,setsar=1,format=yuv420p",
            "-r", "30", "-c:v", "libx264", "-profile:v", "baseline", "-level:v", "3.0",
            "-preset", "ultrafast", "-tune", "zerolatency", "-crf", "28", "-maxrate", "1200k", "-bufsize", "1200k",
            "-g", "30", "-keyint_min", "30", "-sc_threshold", "0", "-threads", "1", "-filter_threads", "1",
            "-c:a", "aac", "-b:a", "96k", "-ac", "2", "-ar", "48000",
            "-avoid_negative_ts", "make_zero", "-movflags", "frag_keyframe+empty_moov+default_base_moof",
            "-f", "mp4", "pipe:1",
        ])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
}

struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn read_child(
    child: &mut Child,
    cancelled: &impl Fn() -> bool,
    deadline: Instant,
) -> Result<Vec<u8>, String> {
    read_child_with_limit(child, cancelled, deadline, MAX_SEGMENT_BYTES)
}

fn read_child_with_limit(
    child: &mut Child,
    cancelled: &impl Fn() -> bool,
    deadline: Instant,
    output_limit: usize,
) -> Result<Vec<u8>, String> {
    let mut stdout = child.stdout.take().ok_or(UNPLAYABLE)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    crate::spawn_worker(move || {
        let result = media_runtime::read_bounded_output(&mut stdout, output_limit)
            .ok()
            .flatten();
        let _ = sender.send(result);
    });
    let mut output = None;
    loop {
        if cancelled() {
            return Err(CANCELLED.into());
        }
        if Instant::now() >= deadline {
            return Err(TIMEOUT.into());
        }
        match receiver.try_recv() {
            Ok(Some(bytes)) => output = Some(bytes),
            Ok(None) => {
                return Err(if output_limit == MAX_SEGMENT_BYTES {
                    "Compatibility playback exceeded its 4 MiB segment budget."
                } else {
                    "The duration inspector exceeded its bounded output budget."
                }
                .into())
            }
            Err(mpsc::TryRecvError::Disconnected) if output.is_none() => {
                return Err(UNPLAYABLE.into())
            }
            Err(_) => {}
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let bytes = output
                    .or_else(|| {
                        receiver
                            .recv_timeout(
                                deadline
                                    .saturating_duration_since(Instant::now())
                                    .min(Duration::from_secs(1)),
                            )
                            .ok()
                            .flatten()
                    })
                    .ok_or_else(|| {
                        if Instant::now() >= deadline {
                            TIMEOUT
                        } else {
                            UNPLAYABLE
                        }
                    })?;
                if Instant::now() >= deadline {
                    return Err(TIMEOUT.into());
                }
                return if status.success() && !bytes.is_empty() {
                    Ok(bytes)
                } else {
                    Err(UNPLAYABLE.into())
                };
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => return Err(UNPLAYABLE.into()),
        }
    }
}

pub(super) fn segment(token: &str, start: f64) -> Result<Vec<u8>, String> {
    // Reject invalid IPC numbers even before any decoder/probe work.
    if !start.is_finite() || start < 0.0 {
        return Err("Choose a position within the original video.".into());
    }
    let entry = entry(token)?;
    let mut grant = entry.grant.lock().map_err(|_| CANCELLED)?;
    require_current(&entry, &grant)?;
    ensure_decoder(&mut grant)?;
    if grant.compatibility.duration.is_none() {
        let cancelled = || !current(&entry, &grant);
        grant.compatibility.duration = Some(duration(&grant, &cancelled)?);
    }
    let length = segment_duration(start, grant.compatibility.duration.ok_or(UNPLAYABLE)?)?;
    let last_validation = Cell::new(Instant::now());
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    let cancelled = || {
        if entry.revoked.load(Ordering::Acquire) || grant.created_at.elapsed() >= GRANT_LIFETIME {
            return true;
        }
        if last_validation.get().elapsed() >= Duration::from_millis(200) {
            last_validation.set(Instant::now());
            return !current(&entry, &grant);
        }
        false
    };
    let admission_cancelled = || Instant::now() >= deadline || cancelled();
    let admission_error = |error| {
        if Instant::now() >= deadline {
            TIMEOUT.to_string()
        } else {
            error
        }
    };
    let _process = media_runtime::acquire_preview_decoder(&grant.path, &admission_cancelled)
        .map_err(admission_error)?;
    let reader = media_runtime::acquire_reader(&grant.path, &admission_cancelled)
        .map_err(admission_error)?;
    reader
        .yield_to_upload(crate::MediaIoPriority::Foreground, &admission_cancelled)
        .map_err(admission_error)?;
    require_current(&entry, &grant)?;
    let decoder = grant.compatibility.decoder.as_ref().ok_or(UNPLAYABLE)?;
    let source = held_decode_source(&grant)?;
    let mut command = Command::new(&decoder.path);
    configure_command(&mut command, &source, &grant.path, start, length);
    let mut child = KillOnDrop(command.spawn().map_err(|_| UNPLAYABLE)?);
    let bytes = read_child(&mut child.0, &cancelled, deadline)?;
    require_current(&entry, &grant)?;
    // No path, original bytes or unbounded payload can reach the webview.
    if decoder_identity(&decoder.path).is_none_or(|(signature, identity)| {
        signature != decoder.signature || identity != decoder.identity
    }) {
        return Err(UNPLAYABLE.into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn compatibility_segment_positions_are_finite_in_range_and_six_seconds() {
        assert_eq!(segment_duration(12.25, 90.0).unwrap(), 6.0);
        assert_eq!(segment_duration(89.25, 90.0).unwrap(), 0.75);
        for start in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 90.0, 91.0] {
            assert!(segment_duration(start, 90.0).is_err());
        }
        for duration in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            assert!(segment_duration(0.0, duration).is_err());
        }
    }

    #[test]
    fn compatibility_rejects_unknown_or_path_tokens_before_decoder_work() {
        for token in ["", "C:\\private\\clip.mp4", "../original", "not-a-token"] {
            assert!(info(token).is_err());
            assert!(segment(token, 0.0).is_err());
        }
        assert!(info(&Uuid::new_v4().to_string()).is_err());
    }

    #[test]
    #[ignore = "subprocess fixture invoked by compatibility process tests"]
    fn compatibility_child_fixture() {
        match std::env::var("YUM_COMPAT_CHILD_FIXTURE").as_deref() {
            Ok("oversize") => {
                let payload = [1_u8; 16 * 1024];
                for _ in 0..512 {
                    std::io::stdout().write_all(&payload).unwrap();
                }
            }
            Ok("wait") => thread::sleep(Duration::from_secs(30)),
            _ => {}
        }
    }

    fn fixture_child(mode: &str) -> KillOnDrop {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "media_preview::compatibility::tests::compatibility_child_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("YUM_COMPAT_CHILD_FIXTURE", mode)
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        KillOnDrop(command.spawn().unwrap())
    }

    #[test]
    fn compatibility_child_output_is_bounded_and_cancelled_children_are_reaped() {
        let mut large = fixture_child("oversize");
        assert!(read_child(&mut large.0, &|| false, Instant::now() + PROCESS_TIMEOUT).is_err());
        drop(large);
        let mut waiting = fixture_child("wait");
        let started = Instant::now();
        assert!(read_child(
            &mut waiting.0,
            &|| started.elapsed() > Duration::from_millis(50),
            Instant::now() + PROCESS_TIMEOUT
        )
        .is_err());
        waiting.0.kill().unwrap();
        waiting.0.wait().unwrap();
        assert!(waiting.0.try_wait().unwrap().is_some());
        let mut timeout = fixture_child("wait");
        assert!(read_child(&mut timeout.0, &|| false, Instant::now()).is_err());
    }

    #[test]
    fn compatibility_duration_probe_output_is_bounded_and_deadline_is_explicit() {
        let mut large = fixture_child("oversize");
        let error = read_child_with_limit(
            &mut large.0,
            &|| false,
            Instant::now() + PROCESS_TIMEOUT,
            MAX_PROBE_BYTES,
        )
        .unwrap_err();
        assert!(error.contains("duration inspector"));
        drop(large);
        let mut timeout = fixture_child("wait");
        assert_eq!(
            read_child_with_limit(&mut timeout.0, &|| false, Instant::now(), MAX_PROBE_BYTES,)
                .unwrap_err(),
            TIMEOUT
        );
    }
}
