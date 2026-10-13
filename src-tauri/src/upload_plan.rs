//! Read-only batch planning. Plans are bounded native snapshots, never queue jobs.
use super::*;

const PLAN_TTL_SECONDS: i64 = 15 * 60;
const PLAN_LIMIT: usize = 8;
const MAX_FILES: usize = 512;
const PAGE_LIMIT: usize = 50;
const MAX_EVIDENCE_ROWS: usize = 25_600;
const MAX_EXPORT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UploadPlanRow {
    ordinal: usize,
    file_name: String,
    title: String,
    channel_id: String,
    visibility: String,
    made_for_kids: bool,
    playlist_id: Option<String>,
    playlist_title: Option<String>,
    playlist_evidence: String,
    delete_source_after_upload: bool,
    size_bytes: u64,
    source_status: String,
    duplicate_disposition: String,
    duplicate_matches: Vec<String>,
    metadata: serde_json::Value,
    publication_context: Option<publication_schedule::PublicationContext>,
    asset_jobs: Vec<video_assets::PlanAssetSummary>,
    warnings: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UploadPlanPage {
    pub(super) id: String,
    channel_id: String,
    channel_name: String,
    created_at: String,
    expires_at: String,
    evidence_mode: String,
    evidence_as_of: Option<String>,
    total_files: usize,
    ready_files: usize,
    duplicate_files: usize,
    blocked_files: usize,
    offset: usize,
    limit: usize,
    files: Vec<UploadPlanRow>,
    warnings: Vec<String>,
}

#[derive(Clone)]
struct SourceProof {
    path: PathBuf,
    signature: (u64, String),
    digest: String,
    identity: FileIdentity,
}

#[derive(Clone, PartialEq, Eq)]
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
    // SAFETY: File keeps the handle valid and Windows writes the documented
    // BY_HANDLE_FILE_INFORMATION layout only when the call succeeds.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let information = unsafe { information.assume_init() };
    Ok(FileIdentity(
        information.volume_serial as u64,
        ((information.index_high as u64) << 32) | information.index_low as u64,
    ))
}

#[derive(Clone)]
struct Plan {
    database_path: PathBuf,
    page: UploadPlanPage,
    settings: ManualUploadSettings,
    sources: Vec<Option<SourceProof>>,
    evidence_fingerprint: String,
    created: DateTime<Utc>,
    assets: Option<video_assets::PreparedPlanAssets>,
}

#[derive(Clone, Default)]
struct Evidence {
    remote: Vec<(String, String, String)>,
    local: Vec<(String, String, Option<String>, String)>,
    as_of: Option<String>,
}

fn plans() -> &'static Mutex<HashMap<String, Plan>> {
    static PLANS: OnceLock<Mutex<HashMap<String, Plan>>> = OnceLock::new();
    PLANS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn readonly(state: &AppState) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(
        &state.database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|_| "The saved workspace is unavailable for preview.".to_string())?;
    connection
        .busy_timeout(DATABASE_BUSY_TIMEOUT)
        .map_err(user_error)?;
    Ok(connection)
}

fn require_channel(connection: &Connection, expected: &str) -> Result<ConnectionSettings, String> {
    let settings = connection_settings(connection)?;
    if expected.is_empty() || settings.active_channel_id.as_deref() != Some(expected) {
        return Err("The connected channel changed. Create a new batch preview.".into());
    }
    Ok(settings)
}

fn cached_evidence(connection: &Connection, channel: &str) -> Result<Evidence, String> {
    let rows = connection.prepare("SELECT video_id, title, COALESCE(privacy_status, ''), updated_at FROM remote_videos WHERE channel_id = ?1 ORDER BY video_id LIMIT ?2")
        .map_err(user_error)?.query_map(params![channel, MAX_EVIDENCE_ROWS + 1], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)))
        .map_err(user_error)?.collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    let as_of = rows.iter().map(|row| row.3.clone()).min();
    let local = connection.prepare("SELECT id, title, digest, status FROM upload_items WHERE channel_id = ?1 AND status != 'cancelled' ORDER BY id LIMIT ?2")
        .map_err(user_error)?.query_map(params![channel, MAX_EVIDENCE_ROWS + 1], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .map_err(user_error)?.collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    if rows.len() > MAX_EVIDENCE_ROWS || local.len() > MAX_EVIDENCE_ROWS {
        return Err("This library exceeds the bounded batch preview. Review a smaller channel workspace before planning.".into());
    }
    Ok(Evidence {
        remote: rows.into_iter().map(|row| (row.0, row.1, row.2)).collect(),
        local,
        as_of,
    })
}

/// Explicit owner-authorized GETs; no inventory replacement or upload-job writes.
/// The shared transport retains request observations and exact provider quota holds.
fn online_evidence(
    state: &AppState,
    channel: &str,
    mut evidence: Evidence,
) -> Result<Evidence, String> {
    // Keep the reviewed token and request observations in one selected profile.
    // This guard covers only explicit online reads, not local source hashing.
    let _execution_guard = deletion_execution_lock(state).lock().map_err(|_| {
        "The reviewed online preview is unavailable. No work was imported.".to_string()
    })?;
    require_channel(&readonly(state)?, channel)?;
    let token = refreshed_access_token(state)?;
    require_channel(&readonly(state)?, channel)?;
    let payload = youtube_json(
        state,
        &token,
        "channels",
        &[("part", "contentDetails"), ("mine", "true")],
    )?;
    let own = payload
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(serde_json::Value::as_str) == Some(channel))
        })
        .ok_or(
            "Google did not confirm the reviewed channel. Reconnect before previewing online.",
        )?;
    let playlist = own
        .pointer("/contentDetails/relatedPlaylists/uploads")
        .and_then(serde_json::Value::as_str)
        .ok_or("YouTube did not return this channel's upload library.")?;
    let mut ids = BTreeSet::new();
    let mut page_tokens = HashSet::new();
    let mut page_token = String::new();
    for page in 0..512 {
        let mut query = vec![
            ("part", "contentDetails"),
            ("playlistId", playlist),
            ("maxResults", "50"),
        ];
        if !page_token.is_empty() {
            query.push(("pageToken", page_token.as_str()));
        }
        require_channel(&readonly(state)?, channel)?;
        let page_payload = youtube_json(state, &token, "playlistItems", &query)?;
        let rows = page_payload
            .get("items")
            .and_then(serde_json::Value::as_array)
            .ok_or("YouTube returned an incomplete library page.")?;
        for row in rows {
            let id = row
                .pointer("/contentDetails/videoId")
                .and_then(serde_json::Value::as_str)
                .ok_or("YouTube returned an incomplete video identity.")?;
            ids.insert(id.to_string());
        }
        page_token = page_payload
            .get("nextPageToken")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string();
        if page_token.is_empty() {
            break;
        }
        if !page_tokens.insert(page_token.clone()) {
            return Err(
                "YouTube repeated a library page. The preview stopped without changing saved work."
                    .into(),
            );
        }
        if page == 511 {
            return Err("This library exceeds the bounded online preview. Use cached evidence and refresh the library separately.".into());
        }
    }
    let mut remote = Vec::new();
    let ids: Vec<_> = ids.into_iter().collect();
    for chunk in ids.chunks(50) {
        let joined = chunk.join(",");
        require_channel(&readonly(state)?, channel)?;
        let payload = youtube_json(
            state,
            &token,
            "videos",
            &[("part", "snippet,status"), ("id", joined.as_str())],
        )?;
        let rows = payload
            .get("items")
            .and_then(serde_json::Value::as_array)
            .ok_or("YouTube returned incomplete video evidence.")?;
        for row in rows {
            if !row
                .get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| chunk.iter().any(|expected| expected == id))
            {
                return Err(
                    "YouTube returned an unrequested video identity. Preview stopped safely."
                        .into(),
                );
            }
            if row
                .pointer("/snippet/channelId")
                .and_then(serde_json::Value::as_str)
                != Some(channel)
            {
                return Err(
                    "YouTube returned another channel's video. Preview stopped safely.".into(),
                );
            }
            let id = row
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or("YouTube returned an incomplete video identity.")?;
            let title = row
                .pointer("/snippet/title")
                .and_then(serde_json::Value::as_str)
                .ok_or("YouTube returned an incomplete video title.")?;
            let privacy = row
                .pointer("/status/privacyStatus")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            remote.push((id.to_string(), title.to_string(), privacy.to_string()));
        }
    }
    remote.sort();
    evidence.remote = remote;
    evidence.as_of = Some(now());
    Ok(evidence)
}

fn fingerprint(evidence: &Evidence) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(&evidence.remote, &evidence.local)).map_err(user_error)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn verify_source(path: &Path, expected: Option<&SourceProof>) -> Result<SourceProof, String> {
    if !path.is_absolute() || !is_supported_monitored_video(path) {
        return Err("Choose a supported video from an absolute local location.".into());
    }
    let mut source = open_reference_source(path)?;
    let identity = file_identity(&source).map_err(user_error)?;
    let canonical = fs::canonicalize(path)
        .map_err(|_| "The original source location is unavailable.".to_string())?;
    let current = open_reference_source(&canonical)?;
    if identity != file_identity(&current).map_err(user_error)? {
        return Err("The original source location changed while creating this preview.".into());
    }
    let before = monitored_file_signature(&source.metadata().map_err(user_error)?)?;
    if before.0 == 0 {
        return Err("An empty video cannot be uploaded.".into());
    }
    if let Some(error) = youtube_upload_size_limit_error(before.0) {
        return Err(error);
    }
    if expected.is_some_and(|proof| {
        proof.signature != before || proof.identity != identity || proof.path != canonical
    }) {
        return Err(
            "The original changed after this plan was reviewed. Create a new preview.".into(),
        );
    }
    let (bytes, digest) =
        digest_reader_for_path(&mut source, path, MediaIoPriority::Foreground, &|| false)?;
    let after = monitored_file_signature(&source.metadata().map_err(user_error)?)?;
    if bytes != before.0
        || before != after
        || file_identity(&open_reference_source(&canonical)?).map_err(user_error)? != identity
        || expected.is_some_and(|proof| proof.digest != digest)
    {
        return Err("The original bytes changed. Create a new preview before importing.".into());
    }
    Ok(SourceProof {
        path: canonical,
        signature: after,
        digest,
        identity,
    })
}

fn create_with(
    state: &AppState,
    paths: Vec<String>,
    settings: ManualUploadSettings,
    expected: &str,
    online: bool,
    refresh: impl FnOnce(Evidence) -> Result<Evidence, String>,
    playlist_owner: impl FnOnce(&str) -> Result<String, String>,
) -> Result<Plan, String> {
    if paths.is_empty() || paths.len() > MAX_FILES {
        return Err("Select between 1 and 512 videos for a batch preview.".into());
    }
    if settings
        .expected_channel_id
        .as_deref()
        .is_some_and(|channel| channel != expected)
        || settings.item_metadata.as_ref().is_some_and(|items| {
            items.len() > MAX_FILES || items.keys().any(|path| !paths.contains(path))
        })
        || settings
            .publication_contexts
            .as_ref()
            .is_some_and(|contexts| {
                contexts.len() > MAX_FILES || contexts.keys().any(|path| !paths.contains(path))
            })
    {
        return Err(
            "These upload choices do not match the reviewed channel and selected originals.".into(),
        );
    }
    valid_upload_visibility(&settings.visibility)?;
    valid_playlist_selection(
        settings.playlist_id.clone(),
        settings.playlist_title.clone(),
    )?;
    let connection = readonly(state)?;
    let channel = require_channel(&connection, expected)?;
    let cached = cached_evidence(&connection, expected)?;
    let evidence = if online { refresh(cached)? } else { cached };
    let playlist_title = if online {
        settings
            .playlist_id
            .as_deref()
            .map(playlist_owner)
            .transpose()?
    } else {
        settings.playlist_title.clone()
    };
    let mut sources = Vec::with_capacity(paths.len());
    let mut rows = Vec::with_capacity(paths.len());
    let mut batch_digests = HashMap::new();
    let mut batch_paths = HashSet::new();
    for (ordinal, locator) in paths.iter().enumerate() {
        let path = Path::new(locator);
        let file_name = safe_batch_file_name(locator, ordinal);
        let metadata = upload_metadata::resolve_for_source(&settings, path, ordinal + 1).and_then(
            |mut metadata| {
                metadata
                    .title
                    .get_or_insert_with(|| upload_title_from_source(path));
                upload_metadata::validate(&metadata)?;
                Ok(metadata)
            },
        );
        let title = metadata
            .as_ref()
            .ok()
            .and_then(|metadata| metadata.title.clone())
            .unwrap_or_else(|| upload_title_from_source(path));
        let publication_context = metadata
            .as_ref()
            .ok()
            .and_then(|metadata| metadata.publish_at.as_ref())
            .map(|_| {
                settings
                    .publication_contexts
                    .as_ref()
                    .and_then(|contexts| contexts.get(locator))
                    .cloned()
                    .unwrap_or(publication_schedule::PublicationContext {
                        time_zone: "UTC".into(),
                        tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
                    })
            });
        let context_error = publication_context.as_ref().and_then(|context| {
            if context.tzdb_version != chrono_tz::IANA_TZDB_VERSION
                || context.time_zone.parse::<chrono_tz::Tz>().is_err()
            {
                Some("Publication timezone rules or zone changed; resolve the schedule again.")
            } else {
                None
            }
        });
        let result = verify_source(path, None).and_then(|proof| {
            if !batch_paths.insert(proof.path.clone()) {
                return Err(
                    "This original was selected more than once. Select it once before importing."
                        .into(),
                );
            }
            Ok(proof)
        });
        let mut warnings = Vec::new();
        let mut matches = Vec::new();
        let (source_status, size_bytes, duplicate_disposition, proof) = match result {
            Ok(proof) => {
                if let Err(error) = &metadata {
                    warnings.push(error.clone());
                }
                if let Some(error) = context_error {
                    warnings.push(error.into());
                }
                for (_, remote_title, _) in &evidence.remote {
                    if uploaded_titles_match(&title, remote_title) {
                        matches.push(format!("YouTube title: {remote_title}"));
                    }
                }
                for (_, local_title, digest, status) in &evidence.local {
                    if digest.as_deref() == Some(proof.digest.as_str()) {
                        matches.push(format!("Exact saved bytes ({status}): {local_title}"));
                    }
                }
                if let Some(name) = batch_digests.insert(proof.digest.clone(), file_name.clone()) {
                    matches.push(format!("Same bytes in this batch: {name}"));
                }
                let disposition = if metadata.is_err() || context_error.is_some() {
                    "blocked"
                } else if matches.is_empty() {
                    "ready"
                } else {
                    "review_duplicate"
                };
                ("verified", proof.signature.0, disposition, Some(proof))
            }
            Err(error) => {
                warnings.push(error);
                ("unavailable", 0, "blocked", None)
            }
        };
        if !online && evidence.as_of.is_none() {
            warnings.push(
                "No cached YouTube library exists. Remote duplicate evidence is unknown.".into(),
            );
        }
        if settings.playlist_id.is_some() && !online {
            warnings.push(
                "Playlist ownership is unverified offline and must be checked before importing."
                    .into(),
            );
        }
        rows.push(UploadPlanRow {
            ordinal,
            file_name,
            title,
            channel_id: expected.into(),
            visibility: settings.visibility.clone(),
            made_for_kids: settings.made_for_kids,
            playlist_id: settings.playlist_id.clone(),
            playlist_title: playlist_title.clone(),
            playlist_evidence: if settings.playlist_id.is_none() {
                "not_requested"
            } else if online {
                "owner_verified"
            } else {
                "unverified_cached"
            }
            .into(),
            delete_source_after_upload: settings.delete_source_after_upload,
            size_bytes,
            source_status: source_status.into(),
            duplicate_disposition: duplicate_disposition.into(),
            duplicate_matches: matches.into_iter().take(12).collect(),
            metadata: metadata
                .as_ref()
                .ok()
                .map(serde_json::to_value)
                .transpose()
                .map_err(user_error)?
                .unwrap_or_else(|| serde_json::json!({})),
            warnings,
            publication_context,
            asset_jobs: Vec::new(),
        });
        sources.push(proof);
    }
    require_channel(&readonly(state)?, expected)?;
    let created = Utc::now();
    let mut warnings = vec!["Previewing does not import, upload, delete files or enable a watched folder. Every execution needs fresh source and channel checks.".into()];
    if !online {
        warnings.push("Offline preview uses cached library evidence; it cannot confirm current YouTube ownership, processing or quota.".into());
    }
    let page = UploadPlanPage {
        id: Uuid::new_v4().to_string(),
        channel_id: expected.into(),
        channel_name: channel
            .active_channel
            .unwrap_or_else(|| "Connected channel".into()),
        created_at: created.to_rfc3339(),
        expires_at: (created + ChronoDuration::seconds(PLAN_TTL_SECONDS)).to_rfc3339(),
        evidence_mode: if online { "online" } else { "cached" }.into(),
        evidence_as_of: evidence.as_of.clone(),
        total_files: rows.len(),
        ready_files: rows
            .iter()
            .filter(|row| row.duplicate_disposition == "ready")
            .count(),
        duplicate_files: rows
            .iter()
            .filter(|row| row.duplicate_disposition == "review_duplicate")
            .count(),
        blocked_files: rows
            .iter()
            .filter(|row| row.duplicate_disposition == "blocked")
            .count(),
        offset: 0,
        limit: 25,
        files: rows,
        warnings,
    };
    Ok(Plan {
        database_path: state.database_path.clone(),
        page,
        settings,
        sources,
        evidence_fingerprint: fingerprint(&evidence)?,
        created,
        assets: None,
    })
}

fn retain(plan: Plan) -> Result<UploadPlanPage, String> {
    let mut store = plans()
        .lock()
        .map_err(|_| "Batch previews are temporarily unavailable.".to_string())?;
    store.retain(|_, plan| Utc::now() - plan.created < ChronoDuration::seconds(PLAN_TTL_SECONDS));
    if store.len() >= PLAN_LIMIT {
        if let Some(oldest) = store
            .iter()
            .min_by_key(|(_, plan)| plan.created)
            .map(|(id, _)| id.clone())
        {
            store.remove(&oldest);
        }
    }
    let page = paged(&plan.page, 0, 25);
    store.insert(plan.page.id.clone(), plan);
    Ok(page)
}

fn selected(state: &AppState, id: &str, expected: &str) -> Result<Plan, String> {
    require_channel(&readonly(state)?, expected)?;
    let plan = plans()
        .lock()
        .map_err(|_| "Batch previews are temporarily unavailable.".to_string())?
        .get(id)
        .cloned()
        .ok_or("This batch preview has expired. Resolve a new preview.")?;
    if plan.database_path != state.database_path
        || plan.page.channel_id != expected
        || Utc::now() - plan.created >= ChronoDuration::seconds(PLAN_TTL_SECONDS)
        || plan
            .assets
            .as_ref()
            .is_some_and(|assets| Utc::now() >= assets.expires_at())
    {
        return Err(
            "This preview is stale or belongs to another channel. Resolve it again.".into(),
        );
    }
    Ok(plan)
}

pub(super) fn prospective_targets(
    state: &AppState,
    plan_id: &str,
    expected: &str,
) -> Result<(Vec<video_assets::ProspectiveAssetTarget>, DateTime<Utc>), String> {
    let plan = selected(state, plan_id, expected)?;
    if plan.page.blocked_files != 0 {
        return Err("Resolve unavailable originals and invalid upload choices before matching prospective attachments.".into());
    }
    let mut targets = Vec::with_capacity(plan.sources.len());
    for (ordinal, source) in plan.sources.iter().enumerate() {
        let proof = source
            .as_ref()
            .ok_or("This prospective original has no verified source proof.")?;
        let file = open_reference_source(&proof.path)?;
        if monitored_file_signature(&file.metadata().map_err(user_error)?)? != proof.signature
            || file_identity(&file).map_err(user_error)? != proof.identity
        {
            return Err(
                "A prospective original changed after preview. Resolve the plan again.".into(),
            );
        }
        targets.push(video_assets::ProspectiveAssetTarget {
            ordinal,
            target_id: format!("prospective-{}-{ordinal}", plan.page.id),
            title: plan.page.files[ordinal].title.clone(),
            file_name: plan.page.files[ordinal].file_name.clone(),
            source_path: proof.path.clone(),
            source_size: proof.signature.0,
            source_modified: proof.signature.1.clone(),
            source_digest: proof.digest.clone(),
            source_identity: video_assets::asset_identity(&file)?,
        });
    }
    Ok((
        targets,
        DateTime::parse_from_rfc3339(&plan.page.expires_at)
            .map_err(user_error)?
            .with_timezone(&Utc),
    ))
}

pub(super) fn seal_assets(
    state: &AppState,
    plan_id: &str,
    expected: &str,
    assets: video_assets::PreparedPlanAssets,
) -> Result<UploadPlanPage, String> {
    let mut plan = selected(state, plan_id, expected)?;
    let (targets, _) = prospective_targets(state, plan_id, expected)?;
    if !assets.matches_targets(&targets) {
        return Err("The attachment proof belongs to another prospective plan.".into());
    }
    video_assets::verify_plan_assets(state, expected, &assets)?;
    plan.page.id = Uuid::new_v4().to_string();
    plan.page.expires_at = DateTime::parse_from_rfc3339(&plan.page.expires_at)
        .map_err(user_error)?
        .with_timezone(&Utc)
        .min(assets.expires_at())
        .to_rfc3339();
    for row in &mut plan.page.files {
        row.asset_jobs = assets.summaries(row.ordinal);
    }
    plan.assets = Some(assets);
    retain(plan)
}

/// Only execution-claim code may copy this native proof into durable local
/// evidence. Display/export commands serialize page summaries exclusively.
pub(super) fn execution_assets(
    connection: &Connection,
    plan_id: &str,
    channel: &str,
) -> Result<Option<video_assets::PreparedPlanAssets>, String> {
    let plan = plans().lock().map_err(user_error)?.get(plan_id).cloned();
    let plan=plan.ok_or("The reviewed plan is no longer retained. Existing upload identities cannot be imported again.")?;
    if plan.page.channel_id != channel
        || connection.path().map(Path::new) != Some(plan.database_path.as_path())
        || Utc::now() - plan.created >= ChronoDuration::seconds(PLAN_TTL_SECONDS)
        || plan
            .assets
            .as_ref()
            .is_some_and(|assets| Utc::now() >= assets.expires_at())
    {
        return Err(
            "This approved asset plan no longer belongs to the current intake claim.".into(),
        );
    }
    require_channel(connection, channel)?;
    Ok(plan.assets)
}
pub(super) fn has_assets(plan_id: &str, channel: &str) -> Result<bool, String> {
    let store = plans().lock().map_err(user_error)?;
    if let Some(plan) = store.get(plan_id) {
        if plan.page.channel_id != channel {
            return Err("The prospective plan belongs to another channel.".into());
        }
        return Ok(plan.assets.is_some());
    }
    Ok(false)
}

#[cfg(test)]
pub(super) fn test_retained_plan(
    state: &AppState,
    paths: Vec<String>,
    settings: ManualUploadSettings,
) -> UploadPlanPage {
    retain(
        create_with(
            state,
            paths,
            settings,
            "channel-a",
            false,
            |_| panic!("Offline fixture cannot read provider state"),
            |_| panic!("Offline fixture cannot read playlists"),
        )
        .unwrap(),
    )
    .unwrap()
}
#[cfg(test)]
pub(super) fn test_verified_intake(
    state: &AppState,
    plan_id: &str,
) -> Result<(Vec<String>, ManualUploadSettings), String> {
    verified_intake_with(state, plan_id, "channel-a", Ok, |_| {
        Err("Fixture has no playlist".into())
    })
}
#[cfg(test)]
pub(super) fn test_discard_plan(state: &AppState, plan_id: &str) -> Result<(), String> {
    discard(state, plan_id, "channel-a")
}
#[cfg(test)]
pub(super) fn test_discard_workspace(state: &AppState) {
    plans()
        .lock()
        .unwrap()
        .retain(|_, plan| plan.database_path != state.database_path);
}

fn paged(page: &UploadPlanPage, offset: usize, limit: usize) -> UploadPlanPage {
    let limit = limit.clamp(1, PAGE_LIMIT);
    let mut result = page.clone();
    result.files = page
        .files
        .iter()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();
    result.offset = offset;
    result.limit = limit;
    result
}

#[tauri::command]
pub(super) async fn create_upload_plan(
    state: State<'_, AppState>,
    paths: Vec<String>,
    settings: ManualUploadSettings,
    expected_channel_id: String,
    online_refresh: Option<bool>,
) -> Result<UploadPlanPage, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let plan = create_with(
            &state,
            paths,
            settings,
            &expected_channel_id,
            online_refresh.unwrap_or(false),
            |evidence| online_evidence(&state, &expected_channel_id, evidence),
            |id| upload_defaults::resolve_owned_playlist(&state, id, &expected_channel_id),
        )?;
        retain(plan)
    })
    .await
    .map_err(|_| "Batch preview stopped unexpectedly. No work was imported.".to_string())?
}

#[tauri::command]
pub(super) fn load_upload_plan(
    state: State<'_, AppState>,
    plan_id: String,
    expected_channel_id: String,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<UploadPlanPage, String> {
    let plan = selected(&state, &plan_id, &expected_channel_id)?;
    Ok(paged(&plan.page, offset.unwrap_or(0), limit.unwrap_or(25)))
}

#[tauri::command]
pub(super) fn export_upload_plan(
    state: State<'_, AppState>,
    plan_id: String,
    expected_channel_id: String,
) -> Result<String, String> {
    let plan = selected(&state, &plan_id, &expected_channel_id)?;
    export_text(&plan)
}

fn export_text(plan: &Plan) -> Result<String, String> {
    // Only reviewed display fields: no locators, source signatures/digests, OAuth or sessions.
    let contents = serde_json::to_string_pretty(&serde_json::json!({ "format": "youtube-upload-plan", "version": 1, "approval": "preview_only", "plan": plan.page })).map_err(user_error)?;
    if contents.len() > MAX_EXPORT_BYTES {
        return Err("This preview is too large to export. Reduce the selected batch.".into());
    }
    Ok(contents)
}

fn save_export(
    state: &AppState,
    plan_id: &str,
    expected: &str,
    path: &Path,
) -> Result<String, String> {
    let plan = selected(state, plan_id, expected)?;
    let contents = export_text(&plan)?;
    if !path.is_absolute()
        || path.file_name().is_none()
        || !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return Err("Choose an absolute destination ending in .json for the batch preview.".into());
    }
    require_channel(&readonly(state)?, expected)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    #[cfg(windows)]
    options.custom_flags(0x0020_0000); // OPEN_REPARSE_POINT
    let mut file = options.open(path).map_err(|_| {
        "Choose a new preview filename; existing files will not be overwritten.".to_string()
    })?;
    if file
        .write_all(contents.as_bytes())
        .and_then(|_| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err("The batch preview could not be written. Choose another destination.".into());
    }
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub(super) async fn save_upload_plan_export(
    state: State<'_, AppState>,
    plan_id: String,
    expected_channel_id: String,
    path: String,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_export(&state, &plan_id, &expected_channel_id, Path::new(&path))
    })
    .await
    .map_err(|_| "The batch preview export stopped unexpectedly.".to_string())?
}

#[tauri::command]
pub(super) fn discard_upload_plan(
    state: State<'_, AppState>,
    plan_id: String,
    expected_channel_id: String,
) -> Result<(), String> {
    discard(&state, &plan_id, &expected_channel_id)
}

fn discard(state: &AppState, plan_id: &str, expected: &str) -> Result<(), String> {
    let mut store = plans()
        .lock()
        .map_err(|_| "Batch previews are temporarily unavailable.".to_string())?;
    if store.get(plan_id).is_some_and(|plan| {
        plan.database_path != state.database_path || plan.page.channel_id != expected
    }) {
        return Err("This preview does not belong to the reviewed channel workspace.".into());
    }
    store.remove(plan_id);
    Ok(())
}

/// Called by the normal native intake adapter after explicit review. This is not an upload action.
pub(super) fn verified_intake(
    state: &AppState,
    plan_id: &str,
    expected_channel_id: &str,
) -> Result<(Vec<String>, ManualUploadSettings), String> {
    verified_intake_with(
        state,
        plan_id,
        expected_channel_id,
        |evidence| online_evidence(state, expected_channel_id, evidence),
        |id| upload_defaults::resolve_owned_playlist(state, id, expected_channel_id),
    )
}

fn verified_intake_with(
    state: &AppState,
    plan_id: &str,
    expected_channel_id: &str,
    refresh: impl FnOnce(Evidence) -> Result<Evidence, String>,
    playlist_owner: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(Vec<String>, ManualUploadSettings), String> {
    let plan = selected(state, plan_id, expected_channel_id)?;
    if plan.page.blocked_files != 0 {
        return Err("Resolve unavailable files before importing this preview.".into());
    }
    for source in &plan.sources {
        let proof = source
            .as_ref()
            .ok_or("A preview source has no verified byte identity.")?;
        verify_source(&proof.path, Some(proof))?;
    }
    let evidence = cached_evidence(&readonly(state)?, expected_channel_id)?;
    let mut evidence = refresh(evidence)?;
    // A queue mutation while owner reads run also invalidates the reviewed plan.
    evidence.local = cached_evidence(&readonly(state)?, expected_channel_id)?.local;
    if fingerprint(&evidence)? != plan.evidence_fingerprint {
        return Err("The channel library or queue changed after preview. Resolve a fresh plan before importing.".into());
    }
    if let Some(id) = plan.settings.playlist_id.as_deref() {
        let title = playlist_owner(id)?;
        if plan
            .page
            .files
            .iter()
            .any(|row| row.playlist_title.as_deref() != Some(title.as_str()))
        {
            return Err("The selected playlist changed after preview. Resolve it again.".into());
        }
    }
    selected(state, plan_id, expected_channel_id)?;
    require_channel(&readonly(state)?, expected_channel_id)?;
    if let Some(assets) = &plan.assets {
        video_assets::verify_plan_assets(state, expected_channel_id, assets)?;
    }
    let mut settings = plan.settings.clone();
    settings.metadata = None;
    let mut metadata = HashMap::new();
    let mut contexts = HashMap::new();
    let mut paths = Vec::with_capacity(plan.sources.len());
    for (index, proof) in plan.sources.iter().enumerate() {
        let proof = proof
            .as_ref()
            .ok_or("A preview source has no verified byte identity.")?;
        let path = proof.path.to_string_lossy().to_string();
        let resolved = serde_json::from_value::<upload_metadata::UploadMetadata>(
            plan.page.files[index].metadata.clone(),
        )
        .map_err(user_error)?;
        upload_metadata::validate(&resolved)?;
        metadata.insert(path.clone(), resolved);
        if let Some(context) = plan.page.files[index].publication_context.as_ref() {
            contexts.insert(path.clone(), context.clone());
        }
        paths.push(path);
    }
    settings.item_metadata = Some(metadata);
    settings.publication_contexts = if contexts.is_empty() {
        None
    } else {
        Some(contexts)
    };
    Ok((paths, settings))
}

fn source_plan(plan_id: &str, expected: &str, path: &Path) -> Result<(Plan, usize), String> {
    let plan = plans()
        .lock()
        .map_err(|_| "Batch previews are temporarily unavailable.".to_string())?
        .get(plan_id)
        .cloned()
        .ok_or("This batch preview expired. Resolve it again before importing.")?;
    if expected.is_empty()
        || plan.page.channel_id != expected
        || Utc::now() - plan.created >= ChronoDuration::seconds(PLAN_TTL_SECONDS)
    {
        return Err("The reviewed batch is stale or belongs to another channel.".into());
    }
    let index = plan
        .sources
        .iter()
        .position(|source| source.as_ref().is_some_and(|proof| proof.path == path))
        .ok_or("This source was not included in the reviewed batch.")?;
    Ok((plan, index))
}

/// The intake adapter uses these resolved values without evaluating template
/// tokens a second time (filenames themselves may contain literal braces).
pub(super) fn reviewed_metadata(
    plan_id: &str,
    expected_channel_id: &str,
    path: &Path,
) -> Result<upload_metadata::UploadMetadata, String> {
    let (plan, index) = source_plan(plan_id, expected_channel_id, path)?;
    serde_json::from_value(plan.page.files[index].metadata.clone()).map_err(user_error)
}

/// Check the exact handle held by intake immediately before INSERT and return
/// the trusted reviewed digest to persist as the canonical upload byte identity.
pub(super) fn verify_intake_source(
    plan_id: &str,
    expected_channel_id: &str,
    path: &Path,
    source: &mut File,
) -> Result<String, String> {
    let (plan, index) = source_plan(plan_id, expected_channel_id, path)?;
    let proof = plan.sources[index]
        .as_ref()
        .ok_or("The reviewed original is unavailable.")?;
    let before = monitored_file_signature(&source.metadata().map_err(user_error)?)?;
    if before != proof.signature || file_identity(source).map_err(user_error)? != proof.identity {
        return Err("The original changed after review. Resolve a new batch preview.".into());
    }
    source.seek(SeekFrom::Start(0)).map_err(user_error)?;
    let hashed = digest_reader_for_path(source, path, MediaIoPriority::Foreground, &|| false);
    // Intake and later upload reads must begin at the original's first byte.
    source.seek(SeekFrom::Start(0)).map_err(user_error)?;
    let (bytes, digest) = hashed?;
    let current = open_reference_source(path)?;
    if bytes != proof.signature.0
        || digest != proof.digest
        || monitored_file_signature(&source.metadata().map_err(user_error)?)? != proof.signature
        || monitored_file_signature(&current.metadata().map_err(user_error)?)? != proof.signature
        || file_identity(&current).map_err(user_error)? != proof.identity
    {
        return Err(
            "The original bytes or location changed before import. Resolve a new batch preview."
                .into(),
        );
    }
    source_plan(plan_id, expected_channel_id, path)?;
    Ok(proof.digest.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-upload-plan-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture channel','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET active_channel='Fixture channel',active_channel_id='channel-a'", [now()]).unwrap();
        (root, state, connection)
    }

    fn settings() -> ManualUploadSettings {
        serde_json::from_value(serde_json::json!({ "madeForKids": false, "visibility": "private", "deleteSourceAfterUpload": false })).unwrap()
    }

    fn source(root: &Path, name: &str, bytes: &[u8]) -> String {
        let path = root.join(name);
        fs::write(&path, bytes).unwrap();
        path.to_string_lossy().to_string()
    }

    fn cached(state: &AppState, paths: Vec<String>, choices: ManualUploadSettings) -> Plan {
        create_with(
            state,
            paths,
            choices,
            "channel-a",
            false,
            |_| panic!("Offline preview cannot perform provider reads"),
            |_| panic!("Offline preview cannot resolve a provider playlist"),
        )
        .unwrap()
    }

    fn finish(root: PathBuf, state: AppState, connection: Connection) {
        plans()
            .lock()
            .unwrap()
            .retain(|_, plan| plan.database_path != state.database_path);
        drop(connection);
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn upload_plan_offline_is_read_only_and_paginates_all_reviewed_choices() {
        let (root, state, connection) = fixture();
        connection.execute("INSERT INTO remote_videos(video_id,channel_name,channel_id,title,privacy_status,updated_at) VALUES('hidden','Other','channel-b','HIDDEN OTHER CHANNEL','public',?1)", [now()]).unwrap();
        let before = connection.total_changes();
        let paths: Vec<_> = (0..61)
            .map(|index| {
                source(
                    &root,
                    &format!("camera-{index}.mp4"),
                    format!("unique fixture {index}").as_bytes(),
                )
            })
            .collect();
        let mut choices = settings();
        choices.made_for_kids = true;
        choices.visibility = "unlisted".into();
        choices.delete_source_after_upload = true;
        choices.playlist_id = Some("playlist-a".into());
        choices.playlist_title = Some("Saved archive".into());
        choices.metadata = Some(serde_json::from_value(serde_json::json!({"title":"{sequence} · {filename}","description":"{folder}","notifySubscribers":false})).unwrap());
        let plan = cached(&state, paths.clone(), choices);
        assert_eq!(plan.page.total_files, 61);
        assert_eq!(plan.page.ready_files, 61);
        assert_eq!(plan.page.files[0].title, "1 · camera-0");
        assert!(plan
            .page
            .files
            .iter()
            .all(|row| row.channel_id == "channel-a"
                && row.made_for_kids
                && row.visibility == "unlisted"
                && row.delete_source_after_upload
                && row.playlist_evidence == "unverified_cached"));
        let first = paged(&plan.page, 0, 25);
        let last = paged(&plan.page, 50, 1000);
        assert_eq!(first.files.len(), 25);
        assert_eq!(last.limit, PAGE_LIMIT);
        assert_eq!(last.files.len(), 11);
        assert_eq!(last.files[0].ordinal, 50);
        assert_eq!(connection.total_changes(), before);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM folder_monitors", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let exported = export_text(&plan).unwrap();
        assert!(!exported.contains("HIDDEN OTHER CHANNEL"));
        assert!(!exported.contains(&root.to_string_lossy().to_string()));
        assert!(!exported.contains("sourceLocator"));
        assert!(!exported.contains("digest"));
        assert!(!exported.contains("resumable"));
        assert!(exported.contains("preview_only"));
        assert!(exported.contains("Saved archive"));
        for path in paths {
            assert!(Path::new(&path).is_file());
        }
        finish(root, state, connection);
    }

    #[test]
    fn upload_plan_reports_cached_age_exact_duplicates_missing_sources_and_invalid_metadata() {
        let (root, state, connection) = fixture();
        let path = source(&root, "camera.mp4", b"same reviewed bytes");
        let duplicate = source(&root, "second.mp4", b"same reviewed bytes");
        connection.execute("INSERT INTO remote_videos(video_id,channel_name,channel_id,title,privacy_status,updated_at) VALUES('existing','Fixture','channel-a','camera','private','2026-10-12T01:02:03Z')", []).unwrap();
        let plan = cached(
            &state,
            vec![
                path,
                duplicate,
                root.join("missing.mp4").to_string_lossy().to_string(),
            ],
            settings(),
        );
        assert_eq!(
            plan.page.evidence_as_of.as_deref(),
            Some("2026-10-12T01:02:03Z")
        );
        assert_eq!(plan.page.duplicate_files, 2);
        assert_eq!(plan.page.blocked_files, 1);
        assert!(plan.page.files[0]
            .duplicate_matches
            .iter()
            .any(|row| row.contains("YouTube title")));
        assert!(plan.page.files[1]
            .duplicate_matches
            .iter()
            .any(|row| row.contains("Same bytes in this batch")));
        let mut choices = settings();
        choices.metadata =
            Some(serde_json::from_value(serde_json::json!({"title":"<invalid>"})).unwrap());
        let invalid = cached(
            &state,
            vec![source(&root, "invalid.mp4", b"unchanged")],
            choices,
        );
        assert_eq!(invalid.page.blocked_files, 1);
        assert!(invalid.page.files[0]
            .warnings
            .iter()
            .any(|warning| warning.contains("title")));
        finish(root, state, connection);
    }

    #[test]
    fn upload_plan_online_refresh_is_explicit_and_never_updates_inventory_or_jobs() {
        let (root, state, connection) = fixture();
        let before = connection.total_changes();
        let refreshes = Cell::new(0);
        let owner_reads = Cell::new(0);
        let mut choices = settings();
        choices.playlist_id = Some("playlist-a".into());
        choices.playlist_title = Some("Stale saved title".into());
        let plan = create_with(
            &state,
            vec![source(&root, "fresh.mp4", b"provider fixture")],
            choices,
            "channel-a",
            true,
            |mut evidence| {
                refreshes.set(refreshes.get() + 1);
                evidence.remote = vec![("existing".into(), "fresh".into(), "private".into())];
                evidence.as_of = Some("2026-10-13T01:02:03Z".into());
                Ok(evidence)
            },
            |id| {
                assert_eq!(id, "playlist-a");
                owner_reads.set(owner_reads.get() + 1);
                Ok("Current owned title".into())
            },
        )
        .unwrap();
        assert_eq!(refreshes.get(), 1);
        assert_eq!(owner_reads.get(), 1);
        assert_eq!(plan.page.evidence_mode, "online");
        assert_eq!(plan.page.files[0].playlist_evidence, "owner_verified");
        assert_eq!(
            plan.page.files[0].playlist_title.as_deref(),
            Some("Current owned title")
        );
        assert_eq!(plan.page.duplicate_files, 1);
        assert_eq!(connection.total_changes(), before);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM remote_videos", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        finish(root, state, connection);
    }

    #[test]
    fn upload_plan_export_writes_redacted_json_without_overwriting_originals_or_existing_reports() {
        let (root, state, connection) = fixture();
        let original = source(&root, "original.mp4", b"preserved original");
        let page = retain(cached(&state, vec![original.clone()], settings())).unwrap();
        let destination = root.join("review.json");
        assert_eq!(
            save_export(&state, &page.id, "channel-a", &destination).unwrap(),
            destination.to_string_lossy()
        );
        let exported = fs::read_to_string(&destination).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&exported).unwrap()["approval"],
            "preview_only"
        );
        assert!(!exported.contains(&original));
        assert!(!exported.contains("signature"));
        assert!(save_export(&state, &page.id, "channel-a", &destination)
            .unwrap_err()
            .contains("existing files"));
        assert_eq!(fs::read_to_string(&destination).unwrap(), exported);
        assert!(save_export(&state, &page.id, "channel-a", Path::new("relative.json")).is_err());
        assert!(save_export(&state, &page.id, "channel-a", Path::new(&original)).is_err());
        assert_eq!(fs::read(&original).unwrap(), b"preserved original");
        finish(root, state, connection);
    }

    #[test]
    fn upload_plan_verified_intake_keeps_immutable_expanded_metadata_and_checks_fresh_evidence() {
        let (root, state, connection) = fixture();
        let path = source(&root, "literal-{sequence}.mp4", b"plan proof fixture");
        let mut choices = settings();
        choices.playlist_id = Some("playlist-a".into());
        choices.playlist_title = Some("Owned archive".into());
        choices.metadata = Some(
            serde_json::from_value(
                serde_json::json!({"title":"{filename}","description":"Batch {sequence}"}),
            )
            .unwrap(),
        );
        let page = retain(cached(&state, vec![path.clone()], choices)).unwrap();
        let (paths, returned) = verified_intake_with(&state, &page.id, "channel-a", Ok, |_| {
            Ok("Owned archive".into())
        })
        .unwrap();
        let canonical = fs::canonicalize(&path).unwrap();
        assert_eq!(paths, vec![canonical.to_string_lossy().to_string()]);
        assert_eq!(returned.playlist_id.as_deref(), Some("playlist-a"));
        assert_eq!(
            reviewed_metadata(&page.id, "channel-a", &canonical)
                .unwrap()
                .title
                .as_deref(),
            Some("literal-{sequence}")
        );
        assert_eq!(
            returned.item_metadata.unwrap()[&paths[0]]
                .description
                .as_deref(),
            Some("Batch 1")
        );
        let mut held = open_reference_source(&canonical).unwrap();
        held.seek(SeekFrom::End(0)).unwrap();
        assert_eq!(
            verify_intake_source(&page.id, "channel-a", &canonical, &mut held).unwrap(),
            blake3::hash(b"plan proof fixture").to_hex().to_string()
        );
        assert_eq!(held.stream_position().unwrap(), 0);
        drop(held);
        assert!(verified_intake_with(
            &state,
            &page.id,
            "channel-a",
            |mut evidence| {
                evidence
                    .remote
                    .push(("new".into(), "Changed library".into(), "private".into()));
                Ok(evidence)
            },
            |_| Ok("Owned archive".into())
        )
        .err()
        .unwrap()
        .contains("library or queue changed"));
        assert!(
            verified_intake_with(&state, &page.id, "channel-a", Ok, |_| Ok(
                "Renamed playlist".into()
            ))
            .err()
            .unwrap()
            .contains("playlist changed")
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        finish(root, state, connection);
    }

    #[test]
    fn upload_plan_intake_rejects_same_timestamp_changed_bytes_and_replaced_handles() {
        let (root, state, connection) = fixture();
        let path = source(&root, "original.mp4", b"old bytes");
        let page = retain(cached(&state, vec![path.clone()], settings())).unwrap();
        let canonical = fs::canonicalize(&path).unwrap();
        let modified = fs::metadata(&canonical).unwrap().modified().unwrap();
        fs::write(&canonical, b"new bytes").unwrap();
        File::options()
            .write(true)
            .open(&canonical)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let mut held = open_reference_source(&canonical).unwrap();
        assert!(
            verify_intake_source(&page.id, "channel-a", &canonical, &mut held)
                .unwrap_err()
                .contains("bytes or location changed")
        );
        drop(held);
        assert!(
            verified_intake_with(&state, &page.id, "channel-a", Ok, |_| unreachable!()).is_err()
        );
        fs::write(&canonical, b"old bytes").unwrap();
        File::options()
            .write(true)
            .open(&canonical)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let mut original_handle = open_reference_source(&canonical).unwrap();
        fs::rename(&canonical, root.join("moved.mp4")).unwrap();
        fs::write(&canonical, b"old bytes").unwrap();
        File::options()
            .write(true)
            .open(&canonical)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert!(
            verify_intake_source(&page.id, "channel-a", &canonical, &mut original_handle)
                .unwrap_err()
                .contains("bytes or location changed")
        );
        drop(original_handle);
        finish(root, state, connection);
    }

    #[test]
    fn upload_plan_rejects_stale_channel_profile_and_scope_without_destroying_retained_work() {
        let (root, state, connection) = fixture();
        let (other_root, other_state, other_connection) = fixture();
        let page = retain(cached(
            &state,
            vec![source(&root, "source.mp4", b"preserved")],
            settings(),
        ))
        .unwrap();
        assert!(selected(&other_state, &page.id, "channel-a").is_err());
        assert!(discard(&other_state, &page.id, "channel-a").is_err());
        assert!(discard(&state, &page.id, "channel-b").is_err());
        assert!(selected(&state, &page.id, "channel-a").is_ok());
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        assert!(selected(&state, &page.id, "channel-a")
            .err()
            .unwrap()
            .contains("connected channel changed"));
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-a'",
                [],
            )
            .unwrap();
        plans().lock().unwrap().get_mut(&page.id).unwrap().created -=
            ChronoDuration::seconds(PLAN_TTL_SECONDS);
        assert!(selected(&state, &page.id, "channel-a").is_err());
        assert!(save_export(&state, &page.id, "channel-a", &root.join("stale.json")).is_err());
        assert!(!root.join("stale.json").exists());
        discard(&state, &page.id, "channel-a").unwrap();
        finish(root, state, connection);
        finish(other_root, other_state, other_connection);
    }

    #[test]
    fn upload_plan_rejects_unbounded_selection_and_repeated_original_locations() {
        let (root, state, connection) = fixture();
        assert!(create_with(
            &state,
            vec![],
            settings(),
            "channel-a",
            false,
            Ok,
            |_| unreachable!()
        )
        .is_err());
        assert!(create_with(
            &state,
            vec!["missing".into(); MAX_FILES + 1],
            settings(),
            "channel-a",
            false,
            Ok,
            |_| unreachable!()
        )
        .is_err());
        let path = source(&root, "same.mp4", b"only one original");
        let plan = cached(&state, vec![path.clone(), path], settings());
        assert_eq!(plan.page.ready_files, 1);
        assert_eq!(plan.page.blocked_files, 1);
        assert!(plan.page.files[1]
            .warnings
            .iter()
            .any(|warning| warning.contains("more than once")));
        finish(root, state, connection);
    }
    #[test]
    fn upload_plan_keeps_validated_publication_context_and_pins_utc_for_manifests() {
        let (root, state, connection) = fixture();
        let path = source(&root, "scheduled.mp4", b"Scheduled original");
        let mut choices = settings();
        choices.metadata = Some(
            serde_json::from_value(
                serde_json::json!({"publishAt":(Utc::now()+ChronoDuration::days(30)).to_rfc3339()}),
            )
            .unwrap(),
        );
        let utc = cached(&state, vec![path.clone()], choices.clone());
        assert_eq!(
            utc.page.files[0].publication_context.as_ref().unwrap(),
            &publication_schedule::PublicationContext {
                time_zone: "UTC".into(),
                tzdb_version: chrono_tz::IANA_TZDB_VERSION.into()
            }
        );
        let context = publication_schedule::PublicationContext {
            time_zone: "Asia/Singapore".into(),
            tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        };
        choices.publication_contexts = Some(HashMap::from([(path.clone(), context.clone())]));
        let plan = retain(cached(&state, vec![path.clone()], choices.clone())).unwrap();
        let (paths, canonical) = test_verified_intake(&state, &plan.id).unwrap();
        assert_eq!(
            canonical.publication_contexts.unwrap().get(&paths[0]),
            Some(&context)
        );
        choices
            .publication_contexts
            .as_mut()
            .unwrap()
            .get_mut(&path)
            .unwrap()
            .tzdb_version = "stale-rules".into();
        assert_eq!(cached(&state, vec![path], choices).page.blocked_files, 1);
        finish(root, state, connection);
    }
}
