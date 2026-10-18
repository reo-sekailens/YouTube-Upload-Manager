//! Opt-in, memory-only remote comparison. yt-dlp resolves metadata; native HTTP
//! serves bounded ranges without exposing signed CDN URLs or headers to JS.
use super::*;
use reqwest::blocking::Client;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    process::{Child, Command, Stdio},
    sync::{atomic::AtomicUsize, mpsc},
    thread,
};
use url::Url;

const MAX_REMOTE_GRANTS: usize = 16;
const MAX_CANCELLATIONS: usize = 128;
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_CONFIGURATION_BYTES: u64 = 32 * 1024;
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(45);
const NETWORK_TIMEOUT: Duration = Duration::from_secs(15);
const REMOTE_LIFETIME: Duration = Duration::from_secs(20 * 60);
const UNAVAILABLE: &str = "A directly playable stream is unavailable. Use the YouTube embed, or update your selected yt-dlp installation and retry.";
const CANCELLED: &str =
    "This comparison closed, expired, or changed channel. Reopen the comparison.";
const SETTINGS_FILE: &str = "duplicate-stream-tools.json";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemotePreviewConfiguration {
    configured: bool,
    runtime_configured: bool,
    supported: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteMediaPreview {
    token: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolConfiguration {
    tool_path: PathBuf,
    runtime_path: Option<PathBuf>,
}

struct RemoteGrant {
    url: Url,
    headers: reqwest::header::HeaderMap,
    length: u64,
}

struct RemoteEntry {
    database_path: PathBuf,
    channel_id: String,
    created_at: Instant,
    revoked: AtomicBool,
    grant: Mutex<Option<RemoteGrant>>,
}

#[derive(Default)]
struct Registry {
    active: HashMap<String, Arc<RemoteEntry>>,
    // Closing can race command dispatch. A short-lived tombstone prevents an
    // already-closed request from starting extraction after release completes.
    cancelled: HashMap<String, Instant>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

fn configuration_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

fn supported() -> bool {
    !cfg!(any(target_os = "android", target_os = "ios"))
}

fn configuration_path(state: &AppState) -> Result<PathBuf, String> {
    state
        .database_path
        .parent()
        .map(|parent| parent.join(SETTINGS_FILE))
        .ok_or_else(|| "The comparison tool preferences are unavailable.".into())
}

fn selected_executable(value: &Path, runtime: bool) -> Result<PathBuf, String> {
    let error = if runtime {
        "Choose an existing Deno or Node.js executable."
    } else {
        "Choose an existing yt-dlp executable."
    };
    if !value.is_absolute() || value.as_os_str().len() > 16 * 1024 {
        return Err(error.into());
    }
    let valid_name = if runtime {
        runtime_name(value).is_some()
    } else {
        let expected = if cfg!(windows) {
            "yt-dlp.exe"
        } else {
            "yt-dlp"
        };
        value
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(expected))
    };
    if !valid_name {
        return Err(error.into());
    }
    let canonical = value.canonicalize().map_err(|_| error.to_string())?;
    if runtime && runtime_name(&canonical).is_none() {
        return Err(error.into());
    }
    let metadata = fs::metadata(&canonical).map_err(|_| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(error.into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(error.into());
        }
    }
    Ok(canonical)
}

fn runtime_name(path: &Path) -> Option<&'static str> {
    match path.file_name()?.to_str()?.to_ascii_lowercase().as_str() {
        "deno" | "deno.exe" => Some("deno"),
        "node" | "node.exe" => Some("node"),
        _ => None,
    }
}

fn load_configuration(state: &AppState) -> Result<Option<ToolConfiguration>, String> {
    let path = configuration_path(state)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("The saved comparison tools could not be read.".into()),
    };
    if file.metadata().map_err(|_| UNAVAILABLE)?.len() > MAX_CONFIGURATION_BYTES {
        return Err(
            "Choose the comparison tools again; their saved preferences are invalid.".into(),
        );
    }
    serde_json::from_reader(file.take(MAX_CONFIGURATION_BYTES + 1))
        .map(Some)
        .map_err(|_| {
            "Choose the comparison tools again; their saved preferences are invalid.".into()
        })
}

pub(super) fn configuration(state: &AppState) -> Result<RemotePreviewConfiguration, String> {
    let _guard = configuration_lock().lock().map_err(|_| UNAVAILABLE)?;
    // Invalid preferences must not prevent the official embed from opening.
    let saved = load_configuration(state).ok().flatten();
    Ok(RemotePreviewConfiguration {
        configured: supported()
            && saved
                .as_ref()
                .is_some_and(|saved| selected_executable(&saved.tool_path, false).is_ok()),
        runtime_configured: supported()
            && saved
                .as_ref()
                .and_then(|saved| saved.runtime_path.as_ref())
                .is_some_and(|path| selected_executable(path, true).is_ok()),
        supported: supported(),
    })
}

pub(super) fn configure(
    state: &AppState,
    tool_path: &str,
    runtime_path: Option<&str>,
) -> Result<RemotePreviewConfiguration, String> {
    if !supported() {
        return Err("Direct remote comparison requires a desktop yt-dlp installation. Use the YouTube embed on this device.".into());
    }
    let saved = ToolConfiguration {
        tool_path: selected_executable(Path::new(tool_path), false)?,
        runtime_path: runtime_path
            .filter(|value| !value.trim().is_empty())
            .map(|value| selected_executable(Path::new(value), true))
            .transpose()?,
    };
    let bytes = serde_json::to_vec(&saved).map_err(|_| UNAVAILABLE)?;
    if bytes.len() as u64 > MAX_CONFIGURATION_BYTES {
        return Err("The selected comparison tool paths are too long.".into());
    }
    {
        let _guard = configuration_lock().lock().map_err(|_| UNAVAILABLE)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(configuration_path(state)?)
            .map_err(|_| "The comparison tool preferences could not be saved.".to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "The comparison tool preferences could not be saved.".to_string())?;
    }
    // Paths are the only persistent data. Resolution URLs, headers, and media
    // remain native memory; changing the tools does not grant a video target.
    configuration(state)
}

fn current(entry: &RemoteEntry) -> bool {
    if entry.revoked.load(Ordering::Acquire) || entry.created_at.elapsed() >= REMOTE_LIFETIME {
        return false;
    }
    let channel = Connection::open_with_flags(
        &entry.database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
    .and_then(|connection| active_channel(&connection).ok());
    channel.as_deref() == Some(entry.channel_id.as_str())
}

fn require_current(entry: &RemoteEntry) -> Result<(), String> {
    if current(entry) {
        Ok(())
    } else {
        entry.revoked.store(true, Ordering::Release);
        Err(CANCELLED.into())
    }
}

fn valid_id(value: &str) -> bool {
    value.len() == 11
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_token(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

fn authorize_target(
    state: &AppState,
    target: DuplicatePreviewTarget,
    expected_channel_id: Option<&str>,
    video_id: &str,
) -> Result<String, String> {
    if !valid_id(video_id) {
        return Err("Choose a YouTube video from this duplicate comparison.".into());
    }
    let connection = database(state)?;
    let channel_id = active_channel(&connection)?;
    if channel_id.is_empty() || expected_channel_id.is_some_and(|expected| expected != channel_id) {
        return Err("The active YouTube channel changed. Reopen duplicate review.".into());
    }
    let (_, videos) = resolve_target(&connection, target, &channel_id)?;
    if !videos.iter().any(|video| video.video_id == video_id) {
        return Err("That video is not saved evidence for this duplicate comparison.".into());
    }
    if active_channel(&connection)? != channel_id {
        return Err("The active YouTube channel changed. Reopen duplicate review.".into());
    }
    Ok(channel_id)
}

fn prune(registry: &mut Registry) {
    registry.active.retain(|_, entry| {
        let live =
            !entry.revoked.load(Ordering::Acquire) && entry.created_at.elapsed() < REMOTE_LIFETIME;
        if !live {
            entry.revoked.store(true, Ordering::Release);
        }
        live
    });
    registry
        .cancelled
        .retain(|_, created| created.elapsed() < REMOTE_LIFETIME);
}

pub(super) fn prepare(
    state: &AppState,
    target: DuplicatePreviewTarget,
    expected_channel_id: Option<&str>,
    video_id: &str,
    request_id: &str,
) -> Result<RemoteMediaPreview, String> {
    if !supported() {
        return Err("Use the YouTube embed on this device; direct remote comparison requires desktop yt-dlp.".into());
    }
    if !valid_token(request_id) {
        return Err("The comparison request is invalid. Reopen it.".into());
    }
    let channel_id = authorize_target(state, target, expected_channel_id, video_id)?;
    let tools = {
        let _guard = configuration_lock().lock().map_err(|_| UNAVAILABLE)?;
        load_configuration(state)?
            .ok_or("Choose your yt-dlp executable to enable direct comparison.")?
    };
    let entry = Arc::new(RemoteEntry {
        database_path: state.database_path.clone(),
        channel_id,
        created_at: Instant::now(),
        revoked: AtomicBool::new(false),
        grant: Mutex::new(None),
    });
    {
        let mut registry = registry().lock().map_err(|_| UNAVAILABLE)?;
        prune(&mut registry);
        if registry.cancelled.contains_key(request_id) {
            return Err(CANCELLED.into());
        }
        if registry.active.contains_key(request_id)
            || grants()
                .lock()
                .map_err(|_| UNAVAILABLE)?
                .contains_key(request_id)
        {
            return Err("This comparison request has already been used. Reopen it.".into());
        }
        if registry.active.len() >= MAX_REMOTE_GRANTS {
            return Err("Close another comparison before opening more remote videos.".into());
        }
        registry.active.insert(request_id.into(), entry.clone());
    }
    let result = (|| {
        let _permit = ExtractPermit::acquire(&entry)?;
        require_current(&entry)?;
        let mut grant = extract(&tools, video_id, &entry)?;
        require_current(&entry)?;
        // A real range probe establishes content length and seek support. Never
        // trust extractor size estimates or accept an unbounded 200 response.
        let (length, _) = fetch_range(&grant, 0, 0, None, &entry)?;
        grant.length = length;
        require_current(&entry)?;
        *entry.grant.lock().map_err(|_| UNAVAILABLE)? = Some(grant);
        Ok(RemoteMediaPreview {
            token: request_id.into(),
        })
    })();
    if result.is_err() {
        let _ = release(&[request_id.into()]);
    }
    result
}

pub(super) fn release(tokens: &[String]) -> Result<(), String> {
    if tokens.len() > MAX_ACTIVE_GRANTS {
        return Err("Too many remote previews were requested for release.".into());
    }
    let mut registry = registry().lock().map_err(|_| UNAVAILABLE)?;
    prune(&mut registry);
    for token in tokens.iter().filter(|token| valid_token(token)) {
        if let Some(entry) = registry.active.remove(token) {
            entry.revoked.store(true, Ordering::Release);
        }
        if registry.cancelled.len() >= MAX_CANCELLATIONS {
            if let Some(oldest) = registry
                .cancelled
                .iter()
                .min_by_key(|(_, created)| **created)
                .map(|(token, _)| token.clone())
            {
                registry.cancelled.remove(&oldest);
            }
        }
        registry.cancelled.insert(token.clone(), Instant::now());
    }
    Ok(())
}

static EXTRACTORS: AtomicUsize = AtomicUsize::new(0);

struct ExtractPermit;

impl ExtractPermit {
    fn acquire(entry: &RemoteEntry) -> Result<Self, String> {
        if !current(entry) {
            return Err(CANCELLED.into());
        }
        EXTRACTORS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < 2).then_some(count + 1)
            })
            .map_err(|_| {
                "Two remote videos are already being prepared. Retry after one finishes."
                    .to_string()
            })?;
        Ok(Self)
    }
}

impl Drop for ExtractPermit {
    fn drop(&mut self) {
        EXTRACTORS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn tool_identity(path: &Path) -> Result<((u64, String), FileIdentity), String> {
    let file = open_reference_source(path).map_err(|_| UNAVAILABLE)?;
    Ok((
        monitored_file_signature(&file.metadata().map_err(|_| UNAVAILABLE)?)
            .map_err(|_| UNAVAILABLE)?,
        file_identity(&file).map_err(|_| UNAVAILABLE)?,
    ))
}

fn extraction_command(tool: &Path, runtime: Option<&Path>, video_id: &str) -> Command {
    let mut command = Command::new(tool);
    command.args([
        "--ignore-config",
        "--no-config-locations",
        "--no-plugin-dirs",
        "--no-cache-dir",
        "--no-remote-components",
        "--no-cookies",
        "--no-cookies-from-browser",
        "--no-js-runtimes",
        "--no-playlist",
        "--simulate",
        "--no-mark-watched",
        "--no-progress",
        "--no-warnings",
        "--socket-timeout",
        "10",
        "--retries",
        "0",
        "--extractor-retries",
        "0",
        "--format",
        "best[ext=mp4][vcodec^=avc1][acodec^=mp4a][protocol=https]",
        "--dump-single-json",
    ]);
    if let Some(runtime) = runtime {
        if let Some(name) = runtime_name(runtime) {
            command
                .arg("--js-runtimes")
                .arg(format!("{name}:{}", runtime.display()));
        }
    }
    command
        .arg("--")
        .arg(format!("https://www.youtube.com/watch?v={video_id}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // yt-dlp diagnostics can contain signed URLs. Only a fixed redacted
        // failure reaches the webview and no extractor output reaches logs.
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    command
}

struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn extract(
    tools: &ToolConfiguration,
    video_id: &str,
    entry: &RemoteEntry,
) -> Result<RemoteGrant, String> {
    let tool = selected_executable(&tools.tool_path, false)?;
    let runtime = tools
        .runtime_path
        .as_deref()
        .map(|path| selected_executable(path, true))
        .transpose()?;
    let tool_before = tool_identity(&tool)?;
    let runtime_before = runtime.as_deref().map(tool_identity).transpose()?;
    let mut child = KillOnDrop(
        extraction_command(&tool, runtime.as_deref(), video_id)
            .spawn()
            .map_err(|_| {
                "The selected yt-dlp executable could not start. Choose it again or use the embed."
            })?,
    );
    let mut stdout = child.0.stdout.take().ok_or(UNAVAILABLE)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    crate::spawn_worker(move || {
        let result = crate::media_runtime::read_bounded_output(&mut stdout, MAX_METADATA_BYTES)
            .ok()
            .flatten();
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + EXTRACT_TIMEOUT;
    let mut output = None;
    let bytes = loop {
        require_current(entry)?;
        if Instant::now() >= deadline {
            return Err(
                "Remote stream preparation exceeded 45 seconds. Use the embed or retry.".into(),
            );
        }
        match receiver.try_recv() {
            Ok(Some(bytes)) => output = Some(bytes),
            Ok(None) => return Err(UNAVAILABLE.into()),
            Err(mpsc::TryRecvError::Disconnected) if output.is_none() => {
                return Err(UNAVAILABLE.into())
            }
            Err(_) => {}
        }
        match child.0.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(UNAVAILABLE.into());
                }
                break output
                    .or_else(|| {
                        receiver
                            .recv_timeout(Duration::from_millis(250))
                            .ok()
                            .flatten()
                    })
                    .ok_or(UNAVAILABLE)?;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => return Err(UNAVAILABLE.into()),
        }
    };
    if tool_identity(&tool)? != tool_before
        || runtime.as_deref().map(tool_identity).transpose()? != runtime_before
    {
        return Err("A selected comparison tool changed. Choose it again before retrying.".into());
    }
    parse_metadata(&bytes, video_id)
}

fn valid_stream_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && url.host_str().is_some_and(|host| {
            host.ends_with(".googlevideo.com")
                && host.len() > ".googlevideo.com".len()
                && host
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
        })
        && url.path() == "/videoplayback"
        && url.as_str().len() <= 32 * 1024
}

fn parse_metadata(bytes: &[u8], video_id: &str) -> Result<RemoteGrant, String> {
    let info: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| UNAVAILABLE)?;
    let text = |name: &str| {
        info.get(name)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
    };
    if text("id") != video_id
        || text("ext") != "mp4"
        || text("protocol") != "https"
        || !(text("vcodec").starts_with("avc1") || text("vcodec") == "h264")
        || !(text("acodec").starts_with("mp4a") || text("acodec") == "aac")
        || info
            .get("requested_formats")
            .is_some_and(|value| !value.is_null())
        || info.get("is_live").and_then(serde_json::Value::as_bool) == Some(true)
        || info.get("has_drm").and_then(serde_json::Value::as_bool) == Some(true)
    {
        return Err(UNAVAILABLE.into());
    }
    let url = Url::parse(text("url")).map_err(|_| UNAVAILABLE)?;
    if !valid_stream_url(&url) {
        return Err(UNAVAILABLE.into());
    }
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(values) = info
        .get("http_headers")
        .and_then(serde_json::Value::as_object)
    {
        for (name, value) in values {
            // Never copy cookies, Authorization, Host or proxy credentials.
            let lower = name.to_ascii_lowercase();
            if !matches!(
                lower.as_str(),
                "user-agent" | "accept" | "accept-language" | "origin" | "referer"
            ) {
                continue;
            }
            let value = value
                .as_str()
                .filter(|value| value.len() <= 4096)
                .ok_or(UNAVAILABLE)?;
            if matches!(lower.as_str(), "origin" | "referer") {
                let parsed = Url::parse(value).map_err(|_| UNAVAILABLE)?;
                if parsed.scheme() != "https"
                    || !matches!(parsed.host_str(), Some("www.youtube.com" | "youtube.com"))
                    || parsed.port_or_known_default() != Some(443)
                    || !parsed.username().is_empty()
                    || parsed.password().is_some()
                {
                    return Err(UNAVAILABLE.into());
                }
            }
            headers.insert(
                reqwest::header::HeaderName::from_bytes(lower.as_bytes())
                    .map_err(|_| UNAVAILABLE)?,
                reqwest::header::HeaderValue::from_str(value).map_err(|_| UNAVAILABLE)?,
            );
        }
    }
    Ok(RemoteGrant {
        url,
        headers,
        length: 0,
    })
}

fn client() -> Result<Client, String> {
    static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .no_proxy()
                .connect_timeout(Duration::from_secs(8))
                .timeout(NETWORK_TIMEOUT)
                .redirect(reqwest::redirect::Policy::custom(|attempt| {
                    if attempt.previous().len() >= 3 || !valid_stream_url(attempt.url()) {
                        attempt.stop()
                    } else {
                        attempt.follow()
                    }
                }))
                .build()
                .map_err(|_| UNAVAILABLE.into())
        })
        .clone()
}

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.strip_prefix("bytes ")?;
    let (range, length) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let parse = |value: &str| {
        (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse::<u64>().ok())
            .flatten()
    };
    let (start, end, length) = (parse(start)?, parse(end)?, parse(length)?);
    (length > 0 && start <= end && end < length).then_some((start, end, length))
}

fn validate_range_response(
    status: reqwest::StatusCode,
    url: &Url,
    headers: &reqwest::header::HeaderMap,
    start: u64,
    end: u64,
    expected_length: Option<u64>,
) -> Result<u64, String> {
    if status != reqwest::StatusCode::PARTIAL_CONTENT
        || !valid_stream_url(url)
        || headers
            .get(reqwest::header::CONTENT_ENCODING)
            .is_some_and(|value| value != "identity")
    {
        return Err(UNAVAILABLE.into());
    }
    let (actual_start, actual_end, length) = headers
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range)
        .ok_or(UNAVAILABLE)?;
    let content_length = headers
        .get(reqwest::header::CONTENT_LENGTH)
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or(UNAVAILABLE)
        })
        .transpose()?;
    if actual_start != start
        || actual_end != end
        || expected_length.is_some_and(|expected| expected != length)
        || content_length.is_some_and(|bytes| bytes != end - start + 1)
    {
        return Err(UNAVAILABLE.into());
    }
    Ok(length)
}

fn read_range_body(
    reader: &mut impl Read,
    expected_bytes: u64,
    check: impl Fn() -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    if expected_bytes == 0 || expected_bytes > MAX_RANGE_BYTES {
        return Err(UNAVAILABLE.into());
    }
    let mut body = Vec::with_capacity(expected_bytes as usize);
    let mut chunk = [0_u8; 32 * 1024];
    loop {
        check()?;
        let remaining = expected_bytes as usize + 1 - body.len();
        let limit = remaining.min(chunk.len());
        let bytes = reader.read(&mut chunk[..limit]).map_err(|_| UNAVAILABLE)?;
        if bytes == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..bytes]);
        if body.len() > expected_bytes as usize {
            return Err(UNAVAILABLE.into());
        }
    }
    check()?;
    if body.len() != expected_bytes as usize {
        return Err(UNAVAILABLE.into());
    }
    Ok(body)
}

fn fetch_range(
    grant: &RemoteGrant,
    start: u64,
    end: u64,
    expected_length: Option<u64>,
    entry: &RemoteEntry,
) -> Result<(u64, Vec<u8>), String> {
    require_current(entry)?;
    let expected_bytes = end
        .checked_sub(start)
        .and_then(|value| value.checked_add(1))
        .ok_or(UNAVAILABLE)?;
    if expected_bytes > MAX_RANGE_BYTES || !valid_stream_url(&grant.url) {
        return Err(UNAVAILABLE.into());
    }
    let mut response = client()?
        .get(grant.url.clone())
        .headers(grant.headers.clone())
        .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|_| UNAVAILABLE)?;
    require_current(entry)?;
    let length = validate_range_response(
        response.status(),
        response.url(),
        response.headers(),
        start,
        end,
        expected_length,
    )?;
    let body = read_range_body(&mut response, expected_bytes, || require_current(entry))?;
    Ok((length, body))
}

pub(super) fn respond(request: &Request<Vec<u8>>, token: &str) -> Option<Response<Vec<u8>>> {
    let entry = {
        let registry = registry().lock().ok()?;
        if registry.cancelled.contains_key(token) {
            return Some(empty_response(StatusCode::GONE));
        }
        registry.active.get(token).cloned()?
    };
    Some(respond_entry(request, &entry))
}

fn respond_entry(request: &Request<Vec<u8>>, entry: &RemoteEntry) -> Response<Vec<u8>> {
    if !current(entry) {
        return empty_response(StatusCode::GONE);
    }
    let grant = match entry.grant.lock() {
        Ok(grant) => grant,
        Err(_) => return empty_response(StatusCode::GONE),
    };
    let Some(grant) = grant.as_ref() else {
        return empty_response(StatusCode::SERVICE_UNAVAILABLE);
    };
    let length = grant.length;
    let range = request.headers().get(header::RANGE);
    let bounds = range.map_or_else(
        || Some((0, length.min(MAX_RANGE_BYTES).saturating_sub(1))),
        |value| {
            value
                .to_str()
                .ok()
                .and_then(|value| range_bounds(value, length))
        },
    );
    let Some((start, end)) = bounds else {
        let mut response = empty_response(StatusCode::RANGE_NOT_SATISFIABLE);
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            format!("bytes */{length}").parse().unwrap(),
        );
        return response;
    };
    let head = request.method() == Method::HEAD;
    let partial = range.is_some() || (!head && length > MAX_RANGE_BYTES);
    let bytes = if head && range.is_none() {
        length
    } else {
        end - start + 1
    };
    let body = if head {
        Vec::new()
    } else {
        match fetch_range(grant, start, end, Some(length), entry) {
            Ok((_, body)) => body,
            Err(_) => {
                return empty_response(if current(entry) {
                    StatusCode::BAD_GATEWAY
                } else {
                    StatusCode::GONE
                })
            }
        }
    };
    if !current(entry) {
        return empty_response(StatusCode::GONE);
    }
    let mut response = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::CONTENT_LENGTH, bytes)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff");
    if partial {
        response = response.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{length}"),
        );
    }
    response
        .body(body)
        .unwrap_or_else(|_| empty_response(StatusCode::BAD_GATEWAY))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(url: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "id": "abcdefghijk", "ext": "mp4", "protocol": "https",
            "vcodec": "avc1.42001E", "acodec": "mp4a.40.2", "url": url,
            "http_headers": {"User-Agent":"fixture", "Cookie":"secret", "Authorization":"secret"}
        }))
        .unwrap()
    }

    #[test]
    fn only_https_googlevideo_progressive_urls_are_accepted() {
        assert!(parse_metadata(
            &metadata("https://rr1---sn.example.googlevideo.com/videoplayback?sig=secret"),
            "abcdefghijk"
        )
        .is_ok());
        for url in [
            "http://rr1.googlevideo.com/videoplayback",
            "https://googlevideo.com/videoplayback",
            "https://googlevideo.com.evil.test/videoplayback",
            "https://127.0.0.1/videoplayback",
            "https://rr1.googlevideo.com:444/videoplayback",
            "https://rr1.googlevideo.com/other",
            "https://user:password@rr1.googlevideo.com/videoplayback",
            "https://rr1.googlevideo.com/videoplayback#fragment",
        ] {
            assert!(
                parse_metadata(&metadata(url), "abcdefghijk").is_err(),
                "{url}"
            );
        }
    }

    #[test]
    fn extraction_metadata_never_authorizes_another_video_or_separate_streams() {
        let bytes = metadata("https://rr1.googlevideo.com/videoplayback?sig=secret");
        assert!(parse_metadata(&bytes, "different01").is_err());
        for (key, value) in [
            ("vcodec", serde_json::json!("vp9")),
            ("acodec", serde_json::json!("none")),
            ("ext", serde_json::json!("webm")),
            ("protocol", serde_json::json!("m3u8_native")),
            ("requested_formats", serde_json::json!([{}, {}])),
            ("is_live", serde_json::json!(true)),
            ("has_drm", serde_json::json!(true)),
        ] {
            let mut value_info: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value_info[key] = value;
            assert!(
                parse_metadata(&serde_json::to_vec(&value_info).unwrap(), "abcdefghijk").is_err(),
                "{key}"
            );
        }
    }

    #[test]
    fn cookies_and_credentials_are_not_forwarded_or_serialized() {
        let grant = parse_metadata(
            &metadata("https://rr1.googlevideo.com/videoplayback?sig=secret"),
            "abcdefghijk",
        )
        .unwrap();
        assert_eq!(grant.headers.get("user-agent").unwrap(), "fixture");
        assert!(!grant.headers.contains_key("cookie"));
        assert!(!grant.headers.contains_key("authorization"));
        let response = RemoteMediaPreview {
            token: Uuid::new_v4().to_string(),
        };
        let serialized = serde_json::to_string(&response).unwrap();
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("googlevideo"));
    }

    #[test]
    fn malformed_or_overreaching_content_ranges_are_rejected() {
        assert_eq!(parse_content_range("bytes 0-0/1234"), Some((0, 0, 1234)));
        assert_eq!(parse_content_range("bytes 3-7/8"), Some((3, 7, 8)));
        for value in [
            "bytes 0-1/*",
            "bytes 0-1/1",
            "bytes 2-1/3",
            "bytes -1-2/3",
            "bytes 0-0/0",
            "bytes 0-0/1,2",
            "bytes 0-0/+1",
        ] {
            assert_eq!(parse_content_range(value), None, "{value}");
        }
    }

    #[test]
    fn ignored_ranges_wrong_offsets_and_changed_lengths_are_rejected() {
        let url = Url::parse("https://rr1.googlevideo.com/videoplayback").unwrap();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_RANGE,
            "bytes 3-7/100".parse().unwrap(),
        );
        headers.insert(reqwest::header::CONTENT_LENGTH, "5".parse().unwrap());
        assert_eq!(
            validate_range_response(
                reqwest::StatusCode::PARTIAL_CONTENT,
                &url,
                &headers,
                3,
                7,
                Some(100)
            )
            .unwrap(),
            100
        );
        assert!(
            validate_range_response(reqwest::StatusCode::OK, &url, &headers, 3, 7, Some(100))
                .is_err()
        );
        assert!(validate_range_response(
            reqwest::StatusCode::PARTIAL_CONTENT,
            &url,
            &headers,
            0,
            4,
            Some(100)
        )
        .is_err());
        assert!(validate_range_response(
            reqwest::StatusCode::PARTIAL_CONTENT,
            &url,
            &headers,
            3,
            7,
            Some(101)
        )
        .is_err());
        headers.insert(reqwest::header::CONTENT_LENGTH, "6".parse().unwrap());
        assert!(validate_range_response(
            reqwest::StatusCode::PARTIAL_CONTENT,
            &url,
            &headers,
            3,
            7,
            None
        )
        .is_err());
        headers.insert(reqwest::header::CONTENT_LENGTH, "5".parse().unwrap());
        headers.insert(reqwest::header::CONTENT_ENCODING, "gzip".parse().unwrap());
        assert!(validate_range_response(
            reqwest::StatusCode::PARTIAL_CONTENT,
            &url,
            &headers,
            3,
            7,
            None
        )
        .is_err());
        headers.remove(reqwest::header::CONTENT_ENCODING);
        headers.remove(reqwest::header::CONTENT_RANGE);
        assert!(validate_range_response(
            reqwest::StatusCode::PARTIAL_CONTENT,
            &url,
            &headers,
            3,
            7,
            None
        )
        .is_err());
    }

    #[test]
    fn remote_body_reads_are_exact_bounded_and_cancelled() {
        use std::io::Cursor;
        assert_eq!(
            read_range_body(&mut Cursor::new(b"abc"), 3, || Ok(())).unwrap(),
            b"abc"
        );
        assert!(read_range_body(&mut Cursor::new(b"ab"), 3, || Ok(())).is_err());
        let mut oversized = Cursor::new(vec![0_u8; MAX_RANGE_BYTES as usize + 100]);
        assert!(read_range_body(&mut oversized, MAX_RANGE_BYTES, || Ok(())).is_err());
        assert_eq!(oversized.position(), MAX_RANGE_BYTES + 1);
        assert!(read_range_body(&mut Cursor::new(b"abc"), 3, || Err(CANCELLED.into())).is_err());
        assert!(read_range_body(&mut Cursor::new(b""), MAX_RANGE_BYTES + 1, || Ok(())).is_err());
    }

    #[test]
    fn missing_and_corrupt_preferences_keep_the_embed_available() {
        let fixture = super::super::tests::Fixture::new();
        assert!(!configuration(&fixture.state).unwrap().configured);
        fs::write(configuration_path(&fixture.state).unwrap(), b"broken json").unwrap();
        let info = configuration(&fixture.state).unwrap();
        assert!(!info.configured);
        assert!(!info.runtime_configured);
        fs::write(
            configuration_path(&fixture.state).unwrap(),
            vec![b'x'; MAX_CONFIGURATION_BYTES as usize + 1],
        )
        .unwrap();
        assert!(!configuration(&fixture.state).unwrap().configured);
    }

    #[test]
    fn saved_configuration_contains_only_operator_selected_paths() {
        let fixture = super::super::tests::Fixture::new();
        let root = fixture.state.database_path.parent().unwrap();
        let tool = root.join(if cfg!(windows) {
            "yt-dlp.exe"
        } else {
            "yt-dlp"
        });
        fs::write(&tool, b"selected executable fixture").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
        }
        if supported() {
            let info = configure(&fixture.state, tool.to_str().unwrap(), None).unwrap();
            assert!(info.configured);
            assert!(!info.runtime_configured);
            let saved: serde_json::Value = serde_json::from_slice(
                &fs::read(configuration_path(&fixture.state).unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(saved.as_object().unwrap().len(), 2);
            assert!(saved.get("tool_path").is_some());
            assert!(saved.get("runtime_path").unwrap().is_null());
            let visible = serde_json::to_string(&info).unwrap();
            assert!(!visible.contains("tool_path"));
            assert!(!visible.contains(root.to_str().unwrap()));
        }
    }

    #[test]
    fn requested_video_must_belong_to_current_saved_duplicate_evidence() {
        let fixture = super::super::tests::Fixture::new();
        fixture.upload("pending", "draft", b"source");
        fixture.remote("abcdefghijk", "Clip", "channel-a", "processed");
        fixture.remote("different01", "Unrelated", "channel-a", "processed");
        let target = || DuplicatePreviewTarget::Upload {
            item_id: "pending".into(),
        };
        assert_eq!(
            authorize_target(&fixture.state, target(), Some("channel-a"), "abcdefghijk").unwrap(),
            "channel-a"
        );
        assert!(
            authorize_target(&fixture.state, target(), Some("channel-a"), "different01").is_err()
        );
        assert!(
            authorize_target(&fixture.state, target(), Some("channel-b"), "abcdefghijk").is_err()
        );
        database(&fixture.state)
            .unwrap()
            .execute(
                "UPDATE upload_items SET duplicate_decision='keep_both' WHERE id='pending'",
                [],
            )
            .unwrap();
        assert!(
            authorize_target(&fixture.state, target(), Some("channel-a"), "abcdefghijk").is_err()
        );
    }

    #[test]
    fn extractor_arguments_disable_ambient_auth_code_and_writes() {
        let command =
            extraction_command(Path::new("yt-dlp"), Some(Path::new("deno")), "abcdefghijk");
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for required in [
            "--ignore-config",
            "--no-config-locations",
            "--no-plugin-dirs",
            "--no-cache-dir",
            "--no-remote-components",
            "--no-cookies",
            "--no-cookies-from-browser",
            "--no-js-runtimes",
            "--simulate",
            "--no-mark-watched",
            "--no-playlist",
        ] {
            assert!(arguments.contains(&required.to_string()), "{required}");
        }
        assert_eq!(
            arguments.last().unwrap(),
            "https://www.youtube.com/watch?v=abcdefghijk"
        );
        assert!(!arguments.iter().any(|value| value == "--cookies"
            || value == "--cookies-from-browser"
            || value == "--output"));
    }

    #[test]
    fn selected_runtime_name_controls_the_explicit_ytdlp_prefix() {
        for (file_name, prefix) in [
            ("deno", "deno"),
            ("deno.exe", "deno"),
            ("node", "node"),
            ("Node.EXE", "node"),
        ] {
            let path = Path::new("Selected Tools").join(file_name);
            let command = extraction_command(Path::new("yt-dlp"), Some(&path), "abcdefghijk");
            let arguments = command
                .get_args()
                .map(|value| value.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let position = arguments
                .iter()
                .position(|value| value == "--js-runtimes")
                .unwrap();
            assert_eq!(
                arguments[position + 1],
                format!("{prefix}:{}", path.display())
            );
            assert!(arguments.iter().any(|value| value == "--no-js-runtimes"));
        }
        for file_name in [
            "bun",
            "quickjs.exe",
            "node.cmd",
            "deno.bat",
            "node-wrapper.exe",
        ] {
            assert!(runtime_name(Path::new(file_name)).is_none(), "{file_name}");
        }
    }

    #[test]
    fn configuration_accepts_selected_node_and_rejects_other_runtime_executables() {
        let fixture = super::super::tests::Fixture::new();
        let root = fixture.state.database_path.parent().unwrap();
        let node = root.join(if cfg!(windows) { "node.exe" } else { "node" });
        let other = root.join(if cfg!(windows) { "bun.exe" } else { "bun" });
        for path in [&node, &other] {
            fs::write(path, b"operator-selected test runtime").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let validated = selected_executable(&node, true).unwrap();
        assert_eq!(runtime_name(&validated), Some("node"));
        assert!(selected_executable(&other, true).is_err());
    }

    #[test]
    fn release_before_prepare_is_remembered_and_capacity_is_bounded() {
        let token = Uuid::new_v4().to_string();
        release(std::slice::from_ref(&token)).unwrap();
        let state = registry().lock().unwrap();
        assert!(state.cancelled.contains_key(&token));
        assert!(state.cancelled.len() <= MAX_CANCELLATIONS);
        drop(state);
        assert!(release(&vec![String::new(); MAX_ACTIVE_GRANTS + 1]).is_err());
    }

    #[test]
    fn remote_channel_authority_and_revocation_are_checked_for_head() {
        let fixture = super::super::tests::Fixture::new();
        let entry = RemoteEntry {
            database_path: fixture.state.database_path.clone(),
            channel_id: "channel-a".into(),
            created_at: Instant::now(),
            revoked: AtomicBool::new(false),
            grant: Mutex::new(Some(RemoteGrant {
                url: Url::parse("https://rr1.googlevideo.com/videoplayback").unwrap(),
                headers: Default::default(),
                length: 100,
            })),
        };
        let head = Request::builder()
            .method(Method::HEAD)
            .uri("/test")
            .body(Vec::new())
            .unwrap();
        let response = respond_entry(&head, &entry);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "100");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
        assert!(response.body().is_empty());
        let range_head = Request::builder()
            .method(Method::HEAD)
            .header(header::RANGE, "bytes=10-19")
            .uri("/test")
            .body(Vec::new())
            .unwrap();
        let response = respond_entry(&range_head, &entry);
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 10-19/100");
        database(&fixture.state)
            .unwrap()
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        assert_eq!(respond_entry(&head, &entry).status(), StatusCode::GONE);
        database(&fixture.state)
            .unwrap()
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-a'",
                [],
            )
            .unwrap();
        entry.revoked.store(true, Ordering::Release);
        assert_eq!(respond_entry(&head, &entry).status(), StatusCode::GONE);
    }

    #[test]
    fn expired_remote_grants_cannot_serve_even_head_metadata() {
        let fixture = super::super::tests::Fixture::new();
        let entry = RemoteEntry {
            database_path: fixture.state.database_path.clone(),
            channel_id: "channel-a".into(),
            created_at: Instant::now() - REMOTE_LIFETIME,
            revoked: AtomicBool::new(false),
            grant: Mutex::new(None),
        };
        assert!(!current(&entry));
        let head = Request::builder()
            .method(Method::HEAD)
            .uri("/test")
            .body(Vec::new())
            .unwrap();
        assert_eq!(respond_entry(&head, &entry).status(), StatusCode::GONE);
    }

    #[test]
    #[ignore = "Explicit live-provider smoke: set YUM_TEST_YTDLP_PATH to an operator-selected executable"]
    fn real_operator_selected_extractor_smoke() {
        let path = std::env::var_os("YUM_TEST_YTDLP_PATH")
            .map(PathBuf::from)
            .expect("Set YUM_TEST_YTDLP_PATH for this explicit provider test.");
        let tools = ToolConfiguration {
            tool_path: path,
            runtime_path: std::env::var_os("YUM_TEST_JS_RUNTIME_PATH")
                .or_else(|| std::env::var_os("YUM_TEST_DENO_PATH"))
                .map(PathBuf::from),
        };
        let fixture = super::super::tests::Fixture::new();
        let entry = RemoteEntry {
            database_path: fixture.state.database_path.clone(),
            channel_id: "channel-a".into(),
            created_at: Instant::now(),
            revoked: AtomicBool::new(false),
            grant: Mutex::new(None),
        };
        // YouTube's documented IFrame API example. The historical yt-dlp
        // BaW_jenozKc fixture is unavailable; callers may select another public
        // test video explicitly without admitting an arbitrary URL.
        let video_id = std::env::var("YUM_TEST_VIDEO_ID").unwrap_or_else(|_| "M7lc1UVf-VE".into());
        assert!(
            valid_id(&video_id),
            "Select an eleven-character test video ID."
        );
        // No URL/header, audiovisual bytes or raw diagnostics reach test output.
        match extract(&tools, &video_id, &entry) {
            Ok(grant) => {
                assert!(valid_stream_url(&grant.url));
                let (length, bytes) = fetch_range(&grant, 0, 0, None, &entry)
                    .expect("The live CDN did not supply a bounded range.");
                assert!(length > 0 && bytes.len() == 1);
                println!("Live extractor and bounded CDN probe succeeded; decoded playback remains a separate check.");
            }
            Err(error) => {
                panic!("Live extractor did not provide a direct comparison stream: {error}")
            }
        }
    }
}
