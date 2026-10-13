//! Read-only comparison grants. The webview receives an opaque capability,
//! never a source locator, and each request reads at most one bounded range.
use super::{
    database, monitored_file_signature, open_reference_source, AppState, RemoteTitleIndex,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::http::{header, Method, Request, Response, StatusCode};
use tauri_plugin_fs::FilePath;
use uuid::Uuid;

const MAX_PREVIEW_SOURCES: usize = 16;
const MAX_PREVIEW_VIDEOS: usize = 32;
const MAX_ACTIVE_GRANTS: usize = 64;
const MAX_RANGE_BYTES: u64 = 1024 * 1024;
const GRANT_LIFETIME: Duration = Duration::from_secs(30 * 60);
const UNAVAILABLE: &str =
    "The original video is missing, changed, or cannot be played safely. Restore it and reopen the comparison.";

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(super) enum DuplicatePreviewTarget {
    Upload {
        #[serde(rename = "itemId")]
        item_id: String,
    },
    Preflight {
        #[serde(rename = "jobId")]
        job_id: String,
        ordinal: u64,
    },
    Candidate {
        #[serde(rename = "candidateId")]
        candidate_id: String,
    },
    Perceptual {
        #[serde(rename = "reviewId")]
        review_id: String,
        #[serde(rename = "candidateId")]
        candidate_id: String,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DuplicateMediaPreview {
    local_sources: Vec<LocalPreviewSource>,
    youtube_videos: Vec<YouTubePreviewVideo>,
    unavailable_local_sources: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalPreviewSource {
    token: String,
    title: String,
    file_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct YouTubePreviewVideo {
    video_id: String,
    title: String,
}

struct LocalReference {
    path: Option<PathBuf>,
    title: String,
    file_name: String,
    expected_bytes: Option<u64>,
    expected_modified_key: Option<String>,
}

struct PreviewGrant {
    database_path: PathBuf,
    channel_id: String,
    path: PathBuf,
    file: File,
    signature: (u64, String),
    identity: FileIdentity,
    content_type: &'static str,
    created_at: Instant,
}

struct PreviewEntry {
    grant: Mutex<PreviewGrant>,
    revoked: AtomicBool,
}

type GrantMap = HashMap<String, Arc<PreviewEntry>>;

fn grants() -> &'static Mutex<GrantMap> {
    static GRANTS: OnceLock<Mutex<GrantMap>> = OnceLock::new();
    GRANTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn active_channel(connection: &Connection) -> Result<String, String> {
    connection
        .query_row(
            "SELECT COALESCE(active_channel_id, '') FROM connection_settings WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "The channel session could not be checked.".to_string())
        .map(|value| value.unwrap_or_default())
}

fn display_file_name(value: &str) -> String {
    value
        .rsplit(['/', '\\'])
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Original video")
        .to_string()
}

fn local_upload_reference(
    connection: &Connection,
    item_id: &str,
    channel_id: &str,
) -> Result<LocalReference, String> {
    connection
        .query_row(
            "SELECT workspace_path, title, file_name, size_bytes, source_modified_key FROM upload_items WHERE id = ?1 AND COALESCE(channel_id, '') = ?2",
            params![item_id, channel_id],
            |row| {
                Ok(LocalReference {
                    path: Some(PathBuf::from(row.get::<_, String>(0)?)),
                    title: row.get(1)?,
                    file_name: display_file_name(&row.get::<_, String>(2)?),
                    expected_bytes: Some(row.get::<_, i64>(3)?.max(0) as u64),
                    expected_modified_key: row.get(4)?,
                })
            },
        )
        .map_err(|_| "This local duplicate no longer belongs to the active channel.".to_string())
}

fn matching_youtube_videos(
    connection: &Connection,
    channel_id: &str,
    title: &str,
) -> Result<Vec<YouTubePreviewVideo>, String> {
    if channel_id.is_empty() {
        return Ok(Vec::new());
    }
    Ok(RemoteTitleIndex::load(connection, channel_id)?
        .matches(title)
        .into_iter()
        .take(MAX_PREVIEW_VIDEOS)
        .map(|video| YouTubePreviewVideo {
            video_id: video.video_id.clone(),
            title: video.title.clone(),
        })
        .collect())
}

fn resolve_upload(
    connection: &Connection,
    item_id: &str,
    channel_id: &str,
) -> Result<(Vec<LocalReference>, Vec<YouTubePreviewVideo>), String> {
    let title: String = connection
        .query_row(
            "SELECT title FROM upload_items WHERE id = ?1 AND COALESCE(channel_id, '') = ?2 AND status = 'draft' AND duplicate_decision = 'pending'",
            params![item_id, channel_id],
            |row| row.get(0),
        )
        .map_err(|_| "This upload is no longer awaiting duplicate review.".to_string())?;
    let videos = matching_youtube_videos(connection, channel_id, &title)?;
    if videos.is_empty() {
        return Err("This upload no longer has a matching YouTube video.".into());
    }
    Ok((
        vec![local_upload_reference(connection, item_id, channel_id)?],
        videos,
    ))
}

fn preflight_source(
    locator: &str,
    file_name: String,
    bytes: i64,
    metadata_json: Option<String>,
    deep: bool,
) -> LocalReference {
    let file_path: Option<FilePath> = serde_json::from_str(locator).ok();
    let metadata = metadata_json
        .as_deref()
        .and_then(|value| serde_json::from_str::<super::PreIngestLocalMetadata>(value).ok());
    let modified_key = metadata
        .as_ref()
        .and_then(|metadata| metadata.modified_at.as_deref())
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .filter(|time| time.timestamp() >= 0)
        .map(|time| format!("{}:{}", time.timestamp(), time.timestamp_subsec_nanos()));
    LocalReference {
        path: file_path.and_then(|path| path.as_path().map(Path::to_path_buf)),
        title: display_file_name(&file_name),
        file_name: display_file_name(&file_name),
        expected_bytes: if deep {
            Some(bytes.max(0) as u64)
        } else {
            metadata.and_then(|metadata| metadata.size_bytes)
        },
        expected_modified_key: modified_key,
    }
}

fn resolve_preflight(
    connection: &Connection,
    job_id: &str,
    ordinal: u64,
    channel_id: &str,
) -> Result<(Vec<LocalReference>, Vec<YouTubePreviewVideo>), String> {
    let ordinal = i64::try_from(ordinal)
        .map_err(|_| "Choose an available duplicate scan file.".to_string())?;
    let mode: String = connection
        .query_row(
            "SELECT mode FROM preflight_scan_jobs WHERE id = ?1 AND channel_id = ?2 AND status = 'complete'",
            params![job_id, channel_id],
            |row| row.get(0),
        )
        .map_err(|_| "This duplicate scan is not available in the active channel.".to_string())?;
    let (locator, file_name, bytes, digest, metadata_json): (String, String, i64, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT source_locator, file_name, size_bytes, digest, metadata_json FROM preflight_scan_files WHERE job_id = ?1 AND ordinal = ?2 AND status = 'complete' AND error IS NULL",
            params![job_id, ordinal],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .map_err(|_| "This file did not complete duplicate checking.".to_string())?;
    let deep = mode == "deep";
    let title = Path::new(&file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(&file_name);
    let mut videos = matching_youtube_videos(connection, channel_id, title)?;
    let mut sources = Vec::new();
    sources.push(preflight_source(
        &locator,
        file_name.clone(),
        bytes,
        metadata_json,
        deep,
    ));
    let local_matches = connection
        .prepare(
            "SELECT id, title, video_id, file_name FROM upload_items WHERE status = 'uploaded' AND COALESCE(channel_id, '') = ?1 AND ((?2 = 'deep' AND ?3 IS NOT NULL AND digest = ?3) OR (?2 = 'light' AND LOWER(file_name) = LOWER(?4))) ORDER BY created_at ASC LIMIT ?5",
        )
        .map_err(|_| "The saved duplicate evidence could not be read.".to_string())?
        .query_map(
            params![channel_id, mode, digest, file_name, MAX_PREVIEW_SOURCES as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .map_err(|_| "The saved duplicate evidence could not be read.".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "The saved duplicate evidence could not be read.".to_string())?;
    let mut has_match = !videos.is_empty() || !local_matches.is_empty();
    for (item_id, title, video_id, matched_file_name) in local_matches {
        // Materialization uses ASCII lowercase, not Unicode normalization.
        if !deep && file_name.to_ascii_lowercase() != matched_file_name.to_ascii_lowercase() {
            continue;
        }
        if sources.len() < MAX_PREVIEW_SOURCES {
            sources.push(local_upload_reference(connection, &item_id, channel_id)?);
        }
        if let Some(video_id) = video_id {
            videos.push(YouTubePreviewVideo { video_id, title });
        }
    }
    let dropped_matches = connection
        .prepare(
            "SELECT source_locator, file_name, size_bytes, metadata_json FROM preflight_scan_files WHERE job_id = ?1 AND ordinal != ?2 AND status = 'complete' AND error IS NULL AND ((?3 = 'deep' AND ?4 IS NOT NULL AND digest = ?4) OR (?3 = 'light' AND LOWER(file_name) = LOWER(?5))) ORDER BY ordinal ASC LIMIT ?6",
        )
        .map_err(|_| "The selected-file duplicate evidence could not be read.".to_string())?
        .query_map(
            params![job_id, ordinal, mode, digest, file_name, MAX_PREVIEW_SOURCES as i64],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .map_err(|_| "The selected-file duplicate evidence could not be read.".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "The selected-file duplicate evidence could not be read.".to_string())?;
    has_match |= !dropped_matches.is_empty();
    for (locator, matched_file_name, bytes, metadata_json) in dropped_matches {
        if sources.len() == MAX_PREVIEW_SOURCES {
            break;
        }
        if !deep && file_name.to_ascii_lowercase() != matched_file_name.to_ascii_lowercase() {
            continue;
        }
        sources.push(preflight_source(
            &locator,
            matched_file_name,
            bytes,
            metadata_json,
            deep,
        ));
    }
    if !has_match {
        return Err("This file no longer has duplicate evidence to compare.".into());
    }
    Ok((sources, videos))
}

fn resolve_candidate(
    connection: &Connection,
    candidate_id: &str,
    channel_id: &str,
) -> Result<(Vec<LocalReference>, Vec<YouTubePreviewVideo>), String> {
    let candidate = super::current_duplicate_candidates(connection)?
        .into_iter()
        .find(|candidate| candidate.id == candidate_id)
        .ok_or_else(|| "This duplicate candidate is no longer available.".to_string())?;
    if super::ignored_duplicate_candidate_ids(connection)?.contains(candidate_id) {
        return Err("This duplicate candidate has been marked as a false positive.".into());
    }
    if let (Some(left), Some(right)) = (candidate.left_video_id, candidate.right_video_id) {
        return Ok((
            Vec::new(),
            vec![
                YouTubePreviewVideo {
                    video_id: left,
                    title: candidate.left_title,
                },
                YouTubePreviewVideo {
                    video_id: right,
                    title: candidate.right_title,
                },
            ],
        ));
    }
    let (left, right) = candidate_id
        .strip_prefix("local:")
        .and_then(|value| value.split_once(':'))
        .ok_or_else(|| "This local comparison has invalid saved evidence.".to_string())?;
    let mut sources = Vec::new();
    let mut videos = Vec::new();
    for item_id in [left, right] {
        sources.push(local_upload_reference(connection, item_id, channel_id)?);
        let video: Option<(String, String)> = connection
            .query_row(
                "SELECT video_id, title FROM upload_items WHERE id = ?1 AND COALESCE(channel_id, '') = ?2 AND status = 'uploaded' AND video_id IS NOT NULL",
                params![item_id, channel_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| "The uploaded duplicate receipt could not be read.".to_string())?;
        if let Some((video_id, title)) = video {
            videos.push(YouTubePreviewVideo { video_id, title });
        }
    }
    Ok((sources, videos))
}

fn content_type(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" | "m4v" | "insv" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "mpeg" | "mpg" => "video/mpeg",
        "3gp" => "video/3gpp",
        "3g2" => "video/3gpp2",
        "flv" => "video/x-flv",
        _ => "application/octet-stream",
    }
}

fn open_grant(
    state: &AppState,
    channel_id: &str,
    reference: &LocalReference,
) -> Result<PreviewGrant, String> {
    let path = reference
        .path
        .as_deref()
        .ok_or_else(|| UNAVAILABLE.to_string())?;
    let file = open_reference_source(path).map_err(|_| UNAVAILABLE.to_string())?;
    let metadata = file.metadata().map_err(|_| UNAVAILABLE.to_string())?;
    let signature = monitored_file_signature(&metadata).map_err(|_| UNAVAILABLE.to_string())?;
    if signature.0 == 0
        || reference
            .expected_bytes
            .is_some_and(|size| size != signature.0)
        || reference
            .expected_modified_key
            .as_ref()
            .is_some_and(|key| key != &signature.1)
    {
        return Err(UNAVAILABLE.into());
    }
    if monitored_file_signature(&file.metadata().map_err(|_| UNAVAILABLE.to_string())?)
        .map_err(|_| UNAVAILABLE.to_string())?
        != signature
    {
        return Err(UNAVAILABLE.into());
    }
    let identity = file_identity(&file).map_err(|_| UNAVAILABLE.to_string())?;
    Ok(PreviewGrant {
        database_path: state.database_path.clone(),
        channel_id: channel_id.to_string(),
        path: path.to_path_buf(),
        file,
        signature,
        identity,
        content_type: content_type(&reference.file_name),
        created_at: Instant::now(),
    })
}

pub(super) fn prepare(
    state: &AppState,
    target: DuplicatePreviewTarget,
    expected_channel_id: Option<&str>,
) -> Result<DuplicateMediaPreview, String> {
    let connection = database(state)?;
    let channel_id = active_channel(&connection)?;
    if expected_channel_id.is_some_and(|expected| expected != channel_id) {
        return Err("The active YouTube channel changed. Reopen duplicate review.".into());
    }
    let (sources, mut videos) = match target {
        DuplicatePreviewTarget::Upload { item_id } => {
            resolve_upload(&connection, &item_id, &channel_id)?
        }
        DuplicatePreviewTarget::Preflight { job_id, ordinal } => {
            resolve_preflight(&connection, &job_id, ordinal, &channel_id)?
        }
        DuplicatePreviewTarget::Candidate { candidate_id } => {
            resolve_candidate(&connection, &candidate_id, &channel_id)?
        }
        DuplicatePreviewTarget::Perceptual {
            review_id,
            candidate_id,
        } => {
            let ids = super::perceptual_review::preview_member_ids(
                &connection,
                &channel_id,
                &review_id,
                &candidate_id,
            )?;
            let sources = ids
                .iter()
                .map(|id| local_upload_reference(&connection, id, &channel_id))
                .collect::<Result<Vec<_>, _>>()?;
            (sources, Vec::new())
        }
    };
    let mut seen_videos = HashSet::new();
    videos.retain(|video| {
        !video.video_id.is_empty()
            && video.video_id.len() <= 128
            && video
                .video_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            && seen_videos.insert(video.video_id.clone())
    });
    videos.truncate(MAX_PREVIEW_VIDEOS);
    let mut seen_paths = HashSet::new();
    let mut prepared = Vec::new();
    let mut unavailable_local_sources = 0;
    for source in sources.into_iter().take(MAX_PREVIEW_SOURCES) {
        if source.path.is_none() {
            unavailable_local_sources += 1;
            continue;
        }
        if !seen_paths.insert(source.path.clone()) {
            continue;
        }
        if let Ok(grant) = open_grant(state, &channel_id, &source) {
            prepared.push((source, grant));
        } else {
            unavailable_local_sources += 1;
        }
    }
    if prepared.is_empty() && videos.is_empty() {
        return Err(UNAVAILABLE.into());
    }
    if active_channel(&connection)? != channel_id {
        return Err("The active YouTube channel changed. Reopen duplicate review.".into());
    }
    let mut registry = grants()
        .lock()
        .map_err(|_| "The media preview could not be prepared.".to_string())?;
    registry.retain(|_, grant| {
        grant
            .grant
            .lock()
            .is_ok_and(|grant| grant.created_at.elapsed() < GRANT_LIFETIME)
    });
    if registry.len() + prepared.len() > MAX_ACTIVE_GRANTS {
        return Err("Close another comparison before opening more video previews.".into());
    }
    let mut local_sources = Vec::new();
    for (source, grant) in prepared {
        let token = Uuid::new_v4().to_string();
        registry.insert(
            token.clone(),
            Arc::new(PreviewEntry {
                grant: Mutex::new(grant),
                revoked: AtomicBool::new(false),
            }),
        );
        local_sources.push(LocalPreviewSource {
            token,
            title: source.title,
            file_name: source.file_name,
        });
    }
    Ok(DuplicateMediaPreview {
        local_sources,
        youtube_videos: videos,
        unavailable_local_sources,
    })
}

pub(super) fn release(tokens: &[String]) -> Result<(), String> {
    if tokens.len() > MAX_ACTIVE_GRANTS {
        return Err("Too many media previews were requested for release.".into());
    }
    let mut registry = grants()
        .lock()
        .map_err(|_| "The media preview could not be closed.".to_string())?;
    for token in tokens {
        if let Some(entry) = registry.remove(token) {
            entry.revoked.store(true, Ordering::Release);
        }
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
struct FileIdentity(u64, u64);

#[cfg(unix)]
fn file_identity(file: &File) -> std::io::Result<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(FileIdentity(metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn file_identity(file: &File) -> std::io::Result<FileIdentity> {
    use std::os::windows::io::AsRawHandle;
    // Query the held handle: metadata from a path cannot identify a replacement
    // with the same size and timestamp. No write or privileged access is needed.
    #[repr(C)]
    struct HandleInformation {
        attributes: u32,
        creation_time: [u32; 2],
        access_time: [u32; 2],
        write_time: [u32; 2],
        volume_serial: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            information: *mut HandleInformation,
        ) -> i32;
    }
    let mut information = std::mem::MaybeUninit::<HandleInformation>::uninit();
    // SAFETY: the valid File handle stays open and Windows initializes the
    // complete documented BY_HANDLE_FILE_INFORMATION layout on success.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let information = unsafe { information.assume_init() };
    Ok(FileIdentity(
        information.volume_serial as u64,
        ((information.index_high as u64) << 32) | information.index_low as u64,
    ))
}

fn validate_grant(grant: &PreviewGrant) -> Result<(), StatusCode> {
    if grant.created_at.elapsed() >= GRANT_LIFETIME {
        return Err(StatusCode::GONE);
    }
    let connection = Connection::open_with_flags(
        &grant.database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| StatusCode::FORBIDDEN)?;
    if active_channel(&connection).map_err(|_| StatusCode::FORBIDDEN)? != grant.channel_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let current = open_reference_source(&grant.path).map_err(|_| StatusCode::GONE)?;
    let metadata = current.metadata().map_err(|_| StatusCode::GONE)?;
    if monitored_file_signature(&metadata).map_err(|_| StatusCode::GONE)? != grant.signature
        || file_identity(&current).map_err(|_| StatusCode::GONE)? != grant.identity
        || monitored_file_signature(&grant.file.metadata().map_err(|_| StatusCode::GONE)?)
            .map_err(|_| StatusCode::GONE)?
            != grant.signature
    {
        return Err(StatusCode::GONE);
    }
    Ok(())
}

fn range_bounds(value: &str, length: u64) -> Option<(u64, u64)> {
    let range = value.strip_prefix("bytes=")?;
    if length == 0 || range.contains(',') {
        return None;
    }
    let (start, end) = range.split_once('-')?;
    let parse = |value: &str| {
        (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse::<u64>().ok())
            .flatten()
    };
    let (start, end) = if start.is_empty() {
        let suffix = parse(end)?;
        if suffix == 0 {
            return None;
        }
        (length.saturating_sub(suffix), length - 1)
    } else {
        let start = parse(start)?;
        let end = if end.is_empty() {
            length - 1
        } else {
            parse(end)?
        };
        if start >= length || start > end {
            return None;
        }
        (start, end.min(length - 1))
    };
    Some((start, end.min(start.saturating_add(MAX_RANGE_BYTES - 1))))
}

fn empty_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(Vec::new())
        .expect("fixed preview response headers")
}

pub(super) fn respond(request: Request<Vec<u8>>, main_webview: bool) -> Response<Vec<u8>> {
    if !main_webview {
        return empty_response(StatusCode::FORBIDDEN);
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        let mut response = empty_response(StatusCode::METHOD_NOT_ALLOWED);
        response
            .headers_mut()
            .insert(header::ALLOW, "GET, HEAD".parse().unwrap());
        return response;
    }
    let token = request.uri().path().strip_prefix('/').unwrap_or_default();
    if request.uri().query().is_some()
        || !Uuid::parse_str(token).is_ok_and(|id| id.to_string() == token)
    {
        return empty_response(StatusCode::NOT_FOUND);
    }
    let grant = grants()
        .lock()
        .ok()
        .and_then(|registry| registry.get(token).cloned());
    let Some(entry) = grant else {
        return empty_response(StatusCode::NOT_FOUND);
    };
    if entry.revoked.load(Ordering::Acquire) {
        return empty_response(StatusCode::GONE);
    }
    let mut grant = match entry.grant.lock() {
        Ok(grant) => grant,
        Err(_) => return empty_response(StatusCode::GONE),
    };
    if let Err(status) = validate_grant(&grant) {
        // Drop the per-grant lock before revocation; preparation holds the
        // registry lock while pruning, so this avoids an inverted lock order.
        drop(grant);
        let _ = release(&[token.to_string()]);
        return empty_response(status);
    }
    let length = grant.signature.0;
    let range = request.headers().get(header::RANGE);
    let bounds = if let Some(range) = range {
        range
            .to_str()
            .ok()
            .and_then(|range| range_bounds(range, length))
    } else {
        Some((0, length.min(MAX_RANGE_BYTES) - 1))
    };
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
    let mut body = Vec::new();
    if !head {
        let cancelled = || {
            entry.revoked.load(Ordering::Acquire) || grant.created_at.elapsed() >= GRANT_LIFETIME
        };
        let permit = match super::media_runtime::acquire_reader(&grant.path, &cancelled) {
            Ok(permit) => permit,
            Err(_) => return empty_response(StatusCode::GONE),
        };
        if permit
            .yield_to_upload(super::MediaIoPriority::Foreground, &cancelled)
            .is_err()
        {
            return empty_response(StatusCode::GONE);
        }
        body.resize(bytes as usize, 0);
        if grant.file.seek(SeekFrom::Start(start)).is_err()
            || grant.file.read_exact(&mut body).is_err()
            || validate_grant(&grant).is_err()
            || entry.revoked.load(Ordering::Acquire)
        {
            drop(grant);
            let _ = release(&[token.to_string()]);
            return empty_response(StatusCode::GONE);
        }
    }
    let mut response = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, grant.content_type)
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
    response.body(body).expect("fixed preview response headers")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture {
        root: PathBuf,
        state: AppState,
    }

    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("youtube-preview-test-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("media")).unwrap();
            let state = AppState {
                database_path: root.join("queue.sqlite3"),
                media_directory: root.join("media"),
                folder_monitor_lock: Arc::new(Mutex::new(())),
                oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
            };
            database(&state).unwrap().execute(
                "INSERT INTO connection_settings(singleton, active_channel_id, updated_at) VALUES (1, 'channel-a', ?1)",
                [super::super::now()],
            ).unwrap();
            Self { root, state }
        }

        fn upload(&self, id: &str, status: &str, contents: &[u8]) -> PathBuf {
            let path = self.root.join(format!("{id}.mp4"));
            fs::write(&path, contents).unwrap();
            let signature = monitored_file_signature(&fs::metadata(&path).unwrap()).unwrap();
            database(&self.state).unwrap().execute(
                "INSERT INTO upload_items(id, channel_id, title, file_name, workspace_path, size_bytes, source_modified_key, digest, status, duplicate_decision, total_bytes, created_at, updated_at) VALUES (?1, 'channel-a', 'Clip', ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?4, ?8, ?8)",
                params![id, format!("{id}.mp4"), path.to_string_lossy(), signature.0 as i64, signature.1, blake3::hash(contents).to_hex().to_string(), status, super::super::now()],
            ).unwrap();
            path
        }

        fn remote(&self, id: &str, title: &str, channel: &str, status: &str) {
            database(&self.state).unwrap().execute(
                "INSERT INTO remote_videos(video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES (?1, 'Channel', ?2, ?3, ?4, ?5)",
                params![id, channel, title, status, super::super::now()],
            ).unwrap();
        }

        fn preview_upload(&self, id: &str) -> DuplicateMediaPreview {
            prepare(
                &self.state,
                DuplicatePreviewTarget::Upload { item_id: id.into() },
                Some("channel-a"),
            )
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            grants().lock().unwrap().retain(|_, grant| {
                grant.grant.lock().unwrap().database_path != self.state.database_path
            });
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    fn request(token: &str, method: Method, range: Option<&str>) -> Response<Vec<u8>> {
        let mut request = Request::builder()
            .uri(format!("http://media-preview.localhost/{token}"))
            .method(method);
        if let Some(range) = range {
            request = request.header(header::RANGE, range);
        }
        respond(request.body(Vec::new()).unwrap(), true)
    }

    #[test]
    fn upload_preview_uses_only_active_channel_matches_and_serves_original_ranges() {
        let fixture = Fixture::new();
        let source = fixture.upload("selected", "draft", b"0123456789");
        fixture.remote("matched-video", "Clip (2)", "channel-a", "processed");
        fixture.remote("other-channel", "Clip", "channel-b", "processed");
        fixture.remote("unprocessed", "Clip", "channel-a", "uploaded");
        fixture.remote("unrelated", "Different", "channel-a", "processed");
        let preview = fixture.preview_upload("selected");
        assert_eq!(preview.local_sources.len(), 1);
        assert_eq!(preview.youtube_videos.len(), 1);
        assert_eq!(preview.youtube_videos[0].video_id, "matched-video");
        let serialized = serde_json::to_string(&preview).unwrap();
        assert!(!serialized.contains(source.to_string_lossy().as_ref()));
        assert!(!serialized.contains("workspacePath"));
        assert_eq!(
            fs::read_dir(&fixture.state.media_directory)
                .unwrap()
                .count(),
            0
        );
        let token = &preview.local_sources[0].token;
        let response = request(token, Method::GET, Some("bytes=2-5"));
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.body(), b"2345");
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
        assert_eq!(request(token, Method::GET, Some("bytes=-3")).body(), b"789");
        let head = request(token, Method::HEAD, None);
        assert_eq!(head.status(), StatusCode::OK);
        assert!(head.body().is_empty());
        assert_eq!(head.headers()[header::CONTENT_LENGTH], "10");
        for range in [
            "bytes=10-",
            "bytes=6-2",
            "bytes=0-1,3-4",
            "bytes=-0",
            "bytes=+1-3",
            "bad",
        ] {
            assert_eq!(
                request(token, Method::GET, Some(range)).status(),
                StatusCode::RANGE_NOT_SATISFIABLE
            );
        }
        assert_eq!(
            request(token, Method::POST, None).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            request("../selected.mp4", Method::GET, None).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            respond(
                Request::builder()
                    .uri(format!("/{token}"))
                    .body(Vec::new())
                    .unwrap(),
                false
            )
            .status(),
            StatusCode::FORBIDDEN
        );
        release(&[token.clone()]).unwrap();
        assert_eq!(
            request(token, Method::GET, None).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(fs::read(source).unwrap(), b"0123456789");
    }

    #[test]
    fn preview_ranges_are_bounded_and_head_does_not_read_the_media() {
        let fixture = Fixture::new();
        fixture.upload("large", "draft", &vec![7; MAX_RANGE_BYTES as usize + 37]);
        fixture.remote("matching", "Clip", "channel-a", "processed");
        let preview = fixture.preview_upload("large");
        let token = &preview.local_sources[0].token;
        let first = request(token, Method::GET, None);
        assert_eq!(first.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(first.body().len(), MAX_RANGE_BYTES as usize);
        let ranged = request(token, Method::GET, Some("bytes=0-18446744073709551615"));
        assert_eq!(ranged.body().len(), MAX_RANGE_BYTES as usize);
        let tail = request(
            token,
            Method::GET,
            Some(&format!("bytes={MAX_RANGE_BYTES}-")),
        );
        assert_eq!(tail.body(), &vec![7; 37]);
        let head = request(token, Method::HEAD, Some("bytes=12-14"));
        assert!(head.body().is_empty());
        assert_eq!(head.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(head.headers()[header::CONTENT_LENGTH], "3");
        assert_eq!(
            head.headers()[header::CONTENT_RANGE],
            format!("bytes 12-14/{}", MAX_RANGE_BYTES + 37)
        );
    }

    #[test]
    fn previews_revoke_on_changed_missing_replaced_expired_and_other_channel_sources() {
        let fixture = Fixture::new();
        let source = fixture.upload("changed", "draft", b"0123456789");
        fixture.remote("matching", "Clip", "channel-a", "processed");
        let preview = fixture.preview_upload("changed");
        fs::write(&source, b"changed-length").unwrap();
        assert_eq!(
            request(&preview.local_sources[0].token, Method::GET, None).status(),
            StatusCode::GONE
        );
        assert_eq!(
            request(&preview.local_sources[0].token, Method::GET, None).status(),
            StatusCode::NOT_FOUND
        );
        let source = fixture.upload("missing", "draft", b"0123456789");
        let preview = fixture.preview_upload("missing");
        fs::remove_file(source).unwrap();
        assert_eq!(
            request(&preview.local_sources[0].token, Method::GET, None).status(),
            StatusCode::GONE
        );
        let source = fixture.upload("replaced", "draft", b"0123456789");
        let modified = fs::metadata(&source).unwrap().modified().unwrap();
        let preview = fixture.preview_upload("replaced");
        fs::rename(&source, fixture.root.join("replaced-old.mp4")).unwrap();
        fs::write(&source, b"abcdefghij").unwrap();
        File::options()
            .write(true)
            .open(source)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(
            request(&preview.local_sources[0].token, Method::GET, None).status(),
            StatusCode::GONE
        );
        fixture.upload("expired", "draft", b"0123456789");
        let preview = fixture.preview_upload("expired");
        grants()
            .lock()
            .unwrap()
            .get(&preview.local_sources[0].token)
            .unwrap()
            .grant
            .lock()
            .unwrap()
            .created_at = Instant::now() - GRANT_LIFETIME;
        assert_eq!(
            request(&preview.local_sources[0].token, Method::GET, None).status(),
            StatusCode::GONE
        );
        fixture.upload("channel", "draft", b"0123456789");
        let preview = fixture.preview_upload("channel");
        database(&fixture.state)
            .unwrap()
            .execute(
                "UPDATE connection_settings SET active_channel_id = 'channel-b'",
                [],
            )
            .unwrap();
        assert_eq!(
            request(&preview.local_sources[0].token, Method::GET, None).status(),
            StatusCode::FORBIDDEN
        );
        assert!(prepare(
            &fixture.state,
            DuplicatePreviewTarget::Upload {
                item_id: "channel".into()
            },
            Some("channel-a")
        )
        .is_err());
    }

    #[test]
    fn remote_candidates_and_missing_originals_still_return_reviewable_youtube_matches() {
        let fixture = Fixture::new();
        let source = fixture.upload("missing", "draft", b"0123456789");
        fixture.remote("remote-a", "Clip", "channel-a", "processed");
        fixture.remote("remote-b", "Clip (2)", "channel-a", "processed");
        fixture.remote("foreign", "Clip", "channel-b", "processed");
        fs::remove_file(source).unwrap();
        let preview = fixture.preview_upload("missing");
        assert!(preview.local_sources.is_empty());
        assert_eq!(preview.unavailable_local_sources, 1);
        assert_eq!(preview.youtube_videos.len(), 2);
        let candidate = prepare(
            &fixture.state,
            DuplicatePreviewTarget::Candidate {
                candidate_id: "remote:remote-a:remote-b".into(),
            },
            Some("channel-a"),
        )
        .unwrap();
        assert!(candidate.local_sources.is_empty());
        assert_eq!(candidate.youtube_videos.len(), 2);
        assert!(prepare(
            &fixture.state,
            DuplicatePreviewTarget::Candidate {
                candidate_id: "remote:remote-a:foreign".into()
            },
            Some("channel-a")
        )
        .is_err());
    }

    #[test]
    fn preflight_preview_resolves_exact_selected_and_uploaded_matches_without_ingest() {
        let fixture = Fixture::new();
        fixture.upload("completed", "uploaded", b"exact-content");
        let selected = fixture.root.join("Clip.mp4");
        let other = fixture.root.join("Clip-other.mp4");
        fs::write(&selected, b"exact-content").unwrap();
        fs::write(&other, b"exact-content").unwrap();
        let digest = blake3::hash(b"exact-content").to_hex().to_string();
        let connection = database(&fixture.state).unwrap();
        connection.execute("INSERT INTO preflight_scan_jobs(id, channel_id, mode, status, total_files, created_at, updated_at) VALUES ('job', 'channel-a', 'deep', 'complete', 2, ?1, ?1)", [super::super::now()]).unwrap();
        for (ordinal, source) in [(0, &selected), (1, &other)] {
            connection.execute("INSERT INTO preflight_scan_files(job_id, ordinal, source_locator, file_name, size_bytes, digest, status) VALUES ('job', ?1, ?2, ?3, 13, ?4, 'complete')", params![ordinal, serde_json::to_string(&FilePath::Path(source.clone())).unwrap(), source.file_name().unwrap().to_str().unwrap(), digest]).unwrap();
        }
        fixture.remote("remote-match", "Clip (3)", "channel-a", "processed");
        let preview = prepare(
            &fixture.state,
            DuplicatePreviewTarget::Preflight {
                job_id: "job".into(),
                ordinal: 0,
            },
            Some("channel-a"),
        )
        .unwrap();
        assert_eq!(preview.local_sources.len(), 3);
        assert_eq!(preview.youtube_videos.len(), 1);
        for source in &preview.local_sources {
            assert_eq!(
                request(&source.token, Method::GET, Some("bytes=0-4")).body(),
                b"exact"
            );
        }
        assert_eq!(
            fs::read_dir(&fixture.state.media_directory)
                .unwrap()
                .count(),
            0
        );
        fs::write(&selected, b"wrong-content-longer").unwrap();
        let changed = prepare(
            &fixture.state,
            DuplicatePreviewTarget::Preflight {
                job_id: "job".into(),
                ordinal: 0,
            },
            Some("channel-a"),
        )
        .unwrap();
        assert_eq!(changed.local_sources.len(), 2);
        connection
            .execute(
                "UPDATE preflight_scan_jobs SET channel_id = 'channel-b' WHERE id = 'job'",
                [],
            )
            .unwrap();
        assert!(prepare(
            &fixture.state,
            DuplicatePreviewTarget::Preflight {
                job_id: "job".into(),
                ordinal: 0
            },
            Some("channel-a")
        )
        .is_err());
    }

    #[test]
    fn exact_local_candidates_return_both_uploaded_sources_and_no_unrelated_files() {
        let fixture = Fixture::new();
        fixture.upload("left", "uploaded", b"matching-payload");
        fixture.upload("right", "uploaded", b"matching-payload");
        fixture.upload("unrelated", "uploaded", b"other-payload");
        let preview = prepare(
            &fixture.state,
            DuplicatePreviewTarget::Candidate {
                candidate_id: "local:left:right".into(),
            },
            Some("channel-a"),
        )
        .unwrap();
        assert_eq!(preview.local_sources.len(), 2);
        assert!(preview.youtube_videos.is_empty());
        assert_eq!(
            preview
                .local_sources
                .iter()
                .map(|source| source.file_name.as_str())
                .collect::<HashSet<_>>(),
            HashSet::from(["left.mp4", "right.mp4"])
        );
        assert!(prepare(
            &fixture.state,
            DuplicatePreviewTarget::Candidate {
                candidate_id: "local:left:unrelated".into()
            },
            Some("channel-a")
        )
        .is_err());
    }

    #[test]
    fn light_preflight_preview_preserves_ascii_filename_and_completed_upload_boundaries() {
        let fixture = Fixture::new();
        fixture.upload("camera", "uploaded", b"completed-content");
        fixture.upload("queued", "queued", b"queued-content");
        fixture.upload("foreign", "uploaded", b"foreign-content");
        fixture.upload("unicode", "uploaded", b"unicode-content");
        fs::create_dir_all(fixture.root.join("selected")).unwrap();
        let selected = fixture.root.join("selected").join("CAMERA.mp4");
        fs::write(&selected, b"selected-content").unwrap();
        let connection = database(&fixture.state).unwrap();
        connection
            .execute(
                "UPDATE upload_items SET file_name = 'camera.mp4' WHERE id = 'queued'",
                [],
            )
            .unwrap();
        connection.execute("UPDATE upload_items SET file_name = 'camera.mp4', channel_id = 'channel-b' WHERE id = 'foreign'", []).unwrap();
        connection
            .execute(
                "UPDATE upload_items SET file_name = 'camerä.mp4' WHERE id = 'unicode'",
                [],
            )
            .unwrap();
        connection.execute("INSERT INTO preflight_scan_jobs(id, channel_id, mode, status, total_files, created_at, updated_at) VALUES ('light', 'channel-a', 'light', 'complete', 1, ?1, ?1)", [super::super::now()]).unwrap();
        let metadata = super::super::preflight_local_metadata(
            &FilePath::Path(selected.clone()),
            "CAMERA.mp4",
            false,
        );
        connection.execute("INSERT INTO preflight_scan_files(job_id, ordinal, source_locator, file_name, status, metadata_json) VALUES ('light', 0, ?1, 'CAMERA.mp4', 'complete', ?2)", params![serde_json::to_string(&FilePath::Path(selected.clone())).unwrap(), serde_json::to_string(&metadata).unwrap()]).unwrap();
        let target = || DuplicatePreviewTarget::Preflight {
            job_id: "light".into(),
            ordinal: 0,
        };
        let preview = prepare(&fixture.state, target(), Some("channel-a")).unwrap();
        assert_eq!(preview.local_sources.len(), 2);
        assert!(preview.youtube_videos.is_empty());
        assert!(preview
            .local_sources
            .iter()
            .all(|source| source.file_name.eq_ignore_ascii_case("camera.mp4")));
        assert_eq!(preview.unavailable_local_sources, 0);
        // The saved metadata rejects a changed source before opening a grant.
        fs::write(&selected, b"selected-content-changed").unwrap();
        let changed = prepare(&fixture.state, target(), Some("channel-a")).unwrap();
        assert_eq!(changed.local_sources.len(), 1);
        assert_eq!(changed.unavailable_local_sources, 1);
        // URI-only sources are reported as unavailable without exposing the URI,
        // while the completed matching local upload remains reviewable.
        connection
            .execute(
                "UPDATE preflight_scan_files SET source_locator = ?1 WHERE job_id = 'light'",
                [serde_json::to_string(&FilePath::Url(
                    url::Url::parse("content://provider/selected-video").unwrap(),
                ))
                .unwrap()],
            )
            .unwrap();
        let uri = prepare(&fixture.state, target(), Some("channel-a")).unwrap();
        assert_eq!(uri.local_sources.len(), 1);
        assert_eq!(uri.unavailable_local_sources, 1);
        assert!(!serde_json::to_string(&uri).unwrap().contains("content://"));
    }
}
