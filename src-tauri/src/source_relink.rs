//! Reviewed relocation of retained originals. This workflow never creates media,
//! changes a watched root, discards a receipt, or replaces a protected checkpoint.
use super::*;

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS source_relink_reviews (
  id TEXT PRIMARY KEY NOT NULL,
  channel_id TEXT NOT NULL,
  state TEXT NOT NULL,
  snapshot_json TEXT NOT NULL,
  review_json TEXT NOT NULL,
  receipt_json TEXT,
  expires_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS source_relink_reviews_channel
  ON source_relink_reviews(channel_id, state, expires_at);
"#;

const MAX_FILES: usize = 100;
const MAX_REVIEWS: i64 = 8;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RelinkFile {
    item_id: String,
    title: String,
    file_name: String,
    previous_location: String,
    replacement_location: String,
    size_bytes: u64,
    status: String,
    detail: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Review {
    review_id: String,
    channel_id: String,
    channel_name: Option<String>,
    state: String,
    expires_at: String,
    previous_root: Option<String>,
    replacement_root: String,
    files: Vec<RelinkFile>,
    verified_count: usize,
    blocked_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Receipt {
    review_id: String,
    channel_id: String,
    relinked_item_ids: Vec<String>,
    resumed_item_ids: Vec<String>,
    attention_item_ids: Vec<String>,
    detail: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
struct Snapshot {
    id: String,
    title: String,
    file_name: String,
    source_path: Option<String>,
    workspace_path: String,
    size_bytes: u64,
    digest: Option<String>,
    hash_status: String,
    status: String,
    confirmed_bytes: u64,
    video_id: Option<String>,
    updated_at: String,
}

fn channel(connection: &Connection, expected: &str) -> Result<ConnectionSettings, String> {
    let settings = connection_settings(connection)?;
    if expected.is_empty()
        || !settings.connected
        || settings.active_channel_id.as_deref() != Some(expected)
    {
        return Err(
            "The connected channel changed. Reopen Locate originals in the reviewed channel."
                .into(),
        );
    }
    Ok(settings)
}

fn selected_ids(ids: &[String]) -> Result<(), String> {
    let mut unique = HashSet::new();
    if ids.is_empty()
        || ids.len() > MAX_FILES
        || ids
            .iter()
            .any(|id| id.is_empty() || id.len() > 200 || !unique.insert(id))
    {
        return Err("Select between 1 and 100 distinct existing uploads.".into());
    }
    Ok(())
}

fn snapshot(connection: &Connection, id: &str, channel_id: &str) -> Result<Snapshot, String> {
    connection.query_row(
        "SELECT id, title, file_name, source_path, workspace_path, size_bytes, digest, background_hash_status, status, confirmed_bytes, video_id, updated_at FROM upload_items WHERE id = ?1 AND channel_id = ?2",
        params![id, channel_id],
        |row| Ok(Snapshot {
            id: row.get(0)?, title: row.get(1)?, file_name: row.get(2)?, source_path: row.get(3)?, workspace_path: row.get(4)?, size_bytes: row.get::<_, i64>(5)?.max(0) as u64,
            digest: row.get(6)?, hash_status: row.get(7)?, status: row.get(8)?, confirmed_bytes: row.get::<_, i64>(9)?.max(0) as u64, video_id: row.get(10)?, updated_at: row.get(11)?,
        }),
    ).map_err(|_| "A selected upload is unavailable in this channel. Refresh the queue.".to_string())
}

fn original(item: &Snapshot) -> &Path {
    Path::new(item.source_path.as_deref().unwrap_or(&item.workspace_path))
}

fn trusted_digest(item: &Snapshot) -> Result<&str, String> {
    let digest = item
        .digest
        .as_deref()
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if matches!(
        item.hash_status.as_str(),
        "duplicate" | "duplicate_after_upload"
    ) {
        return Err(
            "This item was withheld as an exact duplicate. Resolve that decision separately."
                .into(),
        );
    }
    digest.ok_or_else(|| "No trusted full BLAKE3 digest was saved. File identity cannot be established safely; filename and size are insufficient.".into())
}

fn eligibility(item: &Snapshot) -> Result<(), String> {
    if matches!(
        item.status.as_str(),
        "importing" | "dispatching" | "uploading"
    ) {
        return Err("This item is being processed. Pause or cancel it and refresh before locating its original.".into());
    }
    if item.hash_status == "running" {
        return Err("Content verification is already running for this item. Wait for it to finish before locating its original.".into());
    }
    trusted_digest(item).map(|_| ())
}

fn common_parent(items: &[Snapshot]) -> Result<PathBuf, String> {
    let mut common = original(&items[0])
        .parent()
        .ok_or("An original has no saved parent folder.")?
        .to_path_buf();
    for item in items {
        if !original(item).is_absolute() {
            return Err("A saved original location is not absolute. Choose each replacement file separately.".into());
        }
        while !original(item).starts_with(&common) {
            if !common.pop() {
                return Err(
                    "These originals do not share a folder. Locate them in separate groups.".into(),
                );
            }
        }
    }
    if common.file_name().is_none() {
        return Err("Select originals from a shared folder below the drive root, or locate them separately.".into());
    }
    Ok(common)
}

fn safe_selected_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if value.len() > 32_768
        || !path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err("Choose an absolute local location without relative path segments.".into());
    }
    if has_link_or_reparse_component(&path)? {
        return Err("Choose a regular local location. Links and linked parent folders cannot be used for relocation.".into());
    }
    Ok(path)
}

pub(super) fn prepare(
    state: &AppState,
    ids: &[String],
    expected: &str,
    path: &str,
    mode: &str,
) -> Result<Review, String> {
    selected_ids(ids)?;
    let mut connection = database(state)?;
    let settings = channel(&connection, expected)?;
    let replacement = safe_selected_path(path)?;
    if !matches!(mode, "file" | "folder") || (mode == "file" && ids.len() != 1) {
        return Err(
            "Choose a folder for several originals, or a replacement file for one upload.".into(),
        );
    }
    if mode == "folder" && !replacement.is_dir() {
        return Err("Choose an available local folder.".into());
    }
    if mode == "file" {
        open_reference_source(&replacement)?;
    }
    let items = ids
        .iter()
        .map(|id| snapshot(&connection, id, expected))
        .collect::<Result<Vec<_>, _>>()?;
    let previous_root = if mode == "folder" {
        Some(common_parent(&items)?)
    } else {
        None
    };
    let files = items
        .iter()
        .map(|item| {
            let destination = match &previous_root {
                Some(root) => replacement.join(
                    original(item)
                        .strip_prefix(root)
                        .expect("common root was checked"),
                ),
                None => replacement.clone(),
            };
            let problem = eligibility(item).err();
            RelinkFile {
                item_id: item.id.clone(),
                title: item.title.clone(),
                file_name: item.file_name.clone(),
                previous_location: original(item).to_string_lossy().into_owned(),
                replacement_location: destination.to_string_lossy().into_owned(),
                size_bytes: item.size_bytes,
                status: if problem.is_some() {
                    "blocked"
                } else {
                    "pending"
                }
                .into(),
                detail: problem.unwrap_or_else(|| {
                    "Ready for full content verification; no location has changed.".into()
                }),
            }
        })
        .collect::<Vec<_>>();
    let review = Review {
        review_id: Uuid::new_v4().to_string(),
        channel_id: expected.into(),
        channel_name: settings.active_channel,
        state: "prepared".into(),
        expires_at: (Utc::now() + ChronoDuration::minutes(15)).to_rfc3339(),
        previous_root: previous_root.map(|root| root.to_string_lossy().into_owned()),
        replacement_root: replacement.to_string_lossy().into_owned(),
        verified_count: 0,
        blocked_count: files.iter().filter(|file| file.status == "blocked").count(),
        files,
    };
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    channel(&transaction, expected)?;
    let count: i64 = transaction.query_row("SELECT COUNT(*) FROM source_relink_reviews WHERE channel_id = ?1 AND state IN ('prepared', 'verifying', 'verified') AND expires_at > ?2", params![expected, now()], |row| row.get(0)).map_err(user_error)?;
    if count >= MAX_REVIEWS {
        return Err("Close another relocation review before starting a new one.".into());
    }
    transaction.execute("INSERT INTO source_relink_reviews(id,channel_id,state,snapshot_json,review_json,expires_at,created_at,updated_at) VALUES(?1,?2,'prepared',?3,?4,?5,?6,?6)", params![review.review_id, expected, serde_json::to_string(&items).map_err(user_error)?, serde_json::to_string(&review).map_err(user_error)?, review.expires_at, now()]).map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    Ok(review)
}

fn load(
    connection: &Connection,
    review_id: &str,
    expected: &str,
) -> Result<(Review, Vec<Snapshot>), String> {
    channel(connection, expected)?;
    let (state, review_json, snapshots, expires_at): (String, String, String, String) = connection.query_row("SELECT state,review_json,snapshot_json,expires_at FROM source_relink_reviews WHERE id = ?1 AND channel_id = ?2", params![review_id, expected], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(|_| "This relocation review is unavailable in the active channel.".to_string())?;
    if state != "committed" && expires_at <= now() {
        return Err("This relocation review expired. Choose and verify the location again.".into());
    }
    let mut review: Review = serde_json::from_str(&review_json).map_err(user_error)?;
    review.state = state;
    Ok((
        review,
        serde_json::from_str(&snapshots).map_err(user_error)?,
    ))
}

fn cancelled(connection: &Connection, id: &str, expected: &str, required_state: &str) -> bool {
    channel(connection, expected).is_err() || connection.query_row("SELECT state != ?3 OR expires_at <= ?4 FROM source_relink_reviews WHERE id = ?1 AND channel_id = ?2", params![id,expected,required_state,now()], |row| row.get::<_,bool>(0)).unwrap_or(true)
}

#[cfg(windows)]
fn file_identity(file: &File) -> Result<(u64, u64), String> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    #[derive(Default)]
    struct HandleInfo {
        fields: [u32; 13],
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut HandleInfo) -> i32;
    }
    let mut information = HandleInfo::default();
    // The open no-follow handle owns the file object for this synchronous read.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        return Err("The replacement's file identity could not be established safely.".into());
    }
    Ok((
        information.fields[7] as u64,
        ((information.fields[11] as u64) << 32) | information.fields[12] as u64,
    ))
}

#[cfg(unix)]
fn file_identity(file: &File) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(user_error)?;
    Ok((metadata.dev(), metadata.ino()))
}

/// Reject link components before opening; hash the native no-follow handle;
/// verify handle signatures before/after the stream and after reopening the name.
fn verify_bytes(
    path: &Path,
    item: &Snapshot,
    cancelled: &impl Fn() -> bool,
) -> Result<(u64, String), String> {
    verify_bytes_with(path, item, cancelled, || {})
}

fn verify_bytes_with(
    path: &Path,
    item: &Snapshot,
    cancelled: &impl Fn() -> bool,
    after_hash: impl FnOnce(),
) -> Result<(u64, String), String> {
    safe_selected_path(&path.to_string_lossy())?;
    let digest = trusted_digest(item)?;
    let mut source = open_reference_source(path)?;
    let identity = file_identity(&source)?;
    let signature = monitored_file_signature(&source.metadata().map_err(user_error)?)?;
    if signature.0 != item.size_bytes {
        return Err(
            "The replacement has a different length. The saved upload was retained.".into(),
        );
    }
    let (bytes, actual_digest) =
        digest_reader_for_path(&mut source, path, MediaIoPriority::Background, cancelled)?;
    if cancelled() {
        return Err(
            "Relocation verification was cancelled; saved uploads remain unchanged.".into(),
        );
    }
    if bytes != item.size_bytes || actual_digest != digest {
        return Err(
            "The replacement contains different bytes. Filename and size do not prove identity."
                .into(),
        );
    }
    after_hash();
    let final_signature = monitored_file_signature(&source.metadata().map_err(user_error)?)?;
    if signature != final_signature {
        return Err(
            "The replacement changed during verification. Choose a stable original again.".into(),
        );
    }
    safe_selected_path(&path.to_string_lossy())?;
    let reopened = open_reference_source(path)?;
    let reopened_signature = monitored_file_signature(&reopened.metadata().map_err(user_error)?)?;
    if signature != reopened_signature || identity != file_identity(&reopened)? {
        return Err("The replacement location changed during verification.".into());
    }
    Ok(signature)
}

pub(super) fn verify(state: &AppState, id: &str, expected: &str) -> Result<Review, String> {
    let connection = database(state)?;
    let (mut review, snapshots) = load(&connection, id, expected)?;
    if !matches!(review.state.as_str(), "prepared" | "verified") {
        return Err("This relocation review cannot be verified again.".into());
    }
    if connection.execute("UPDATE source_relink_reviews SET state = 'verifying', updated_at = ?3 WHERE id = ?1 AND channel_id = ?2 AND state IN ('prepared','verified')", params![id,expected,now()]).map_err(user_error)? != 1 { return Err("This relocation review is already in use.".into()); }
    let cancellation = ThrottledCancellationCheck::new(Duration::from_millis(100), || {
        cancelled(&connection, id, expected, "verifying")
    });
    for (position, saved) in snapshots.iter().enumerate() {
        let file = &mut review.files[position];
        if cancellation.is_cancelled() {
            return Err(
                "Relocation verification was cancelled; saved uploads remain unchanged.".into(),
            );
        }
        let result = snapshot(&connection, &saved.id, expected).and_then(|current| {
            if &current != saved {
                return Err(
                    "The upload changed while this review was open. Refresh and select it again."
                        .into(),
                );
            }
            eligibility(saved)?;
            verify_bytes(Path::new(&file.replacement_location), saved, &|| {
                cancellation.is_cancelled()
            })
        });
        match result {
            Ok(_) => {
                file.status = "verified".into();
                file.detail = "Full BLAKE3 digest and length match the trusted original.".into();
            }
            Err(detail) => {
                file.status = "blocked".into();
                file.detail = detail;
            }
        }
        review.verified_count = review
            .files
            .iter()
            .filter(|file| file.status == "verified")
            .count();
        review.blocked_count = review
            .files
            .iter()
            .filter(|file| file.status == "blocked")
            .count();
        // The command response and this bounded status snapshot are local-only.
        connection.execute("UPDATE source_relink_reviews SET review_json = ?3, updated_at = ?4 WHERE id = ?1 AND channel_id = ?2 AND state = 'verifying'", params![id,expected,serde_json::to_string(&review).map_err(user_error)?,now()]).map_err(user_error)?;
    }
    channel(&connection, expected)?;
    review.state = "verified".into();
    if connection.execute("UPDATE source_relink_reviews SET state = 'verified',review_json = ?3,updated_at = ?4 WHERE id = ?1 AND channel_id = ?2 AND state = 'verifying'",params![id,expected,serde_json::to_string(&review).map_err(user_error)?,now()]).map_err(user_error)? != 1 { return Err("Relocation verification was cancelled; saved uploads remain unchanged.".into()); }
    Ok(review)
}

fn commit_with_sessions(
    state: &AppState,
    id: &str,
    expected: &str,
    ids: &[String],
    resume: bool,
    approved: bool,
    session_exists: impl Fn(&str) -> Result<bool, String>,
) -> Result<Receipt, String> {
    if !approved {
        return Err("Review and approve the verified locations before saving.".into());
    }
    selected_ids(ids)?;
    let mut connection = database(state)?;
    let (review, snapshots) = load(&connection, id, expected)?;
    if review.state == "committed" {
        let receipt: String = connection
            .query_row(
                "SELECT receipt_json FROM source_relink_reviews WHERE id = ?1 AND channel_id = ?2",
                params![id, expected],
                |row| row.get(0),
            )
            .map_err(user_error)?;
        return serde_json::from_str(&receipt).map_err(user_error);
    }
    if review.state != "verified" {
        return Err("Verify the selected originals before saving their locations.".into());
    }
    let mut verified = Vec::new();
    let cancellation = ThrottledCancellationCheck::new(Duration::from_millis(100), || {
        cancelled(&connection, id, expected, "verified")
    });
    for selected in ids {
        let position = review
            .files
            .iter()
            .position(|file| &file.item_id == selected && file.status == "verified")
            .ok_or("Only verified files from this review can be relinked.")?;
        let saved = &snapshots[position];
        let current = snapshot(&connection, selected, expected)?;
        if &current != saved {
            return Err(
                "A reviewed upload changed. No locations were saved; reopen this review.".into(),
            );
        }
        let file = &review.files[position];
        let signature = verify_bytes(Path::new(&file.replacement_location), saved, &|| {
            cancellation.is_cancelled()
        })?;
        let checkpoint = if resume && saved.video_id.is_none() {
            session_exists(selected)?
        } else {
            false
        };
        verified.push((saved, file, signature, checkpoint));
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    channel(&transaction, expected)?;
    if cancelled(&transaction, id, expected, "verified") {
        return Err(
            "This relocation review was cancelled or expired. No locations were saved.".into(),
        );
    }
    let mut receipt = Receipt { review_id: id.into(), channel_id: expected.into(), relinked_item_ids: Vec::new(), resumed_item_ids: Vec::new(), attention_item_ids: Vec::new(), detail: "Verified originals were located. Upload identities, choices, receipts and saved progress were preserved.".into() };
    for (saved, file, signature, checkpoint) in verified {
        if snapshot(&transaction, &saved.id, expected)? != *saved {
            return Err(
                "A reviewed upload changed. No locations were saved; reopen this review.".into(),
            );
        }
        let requires_attention = resume
            && saved.video_id.is_none()
            && (saved.confirmed_bytes > 0 || saved.status == "needs_reconciliation")
            && !checkpoint;
        let can_resume = resume
            && !requires_attention
            && saved.video_id.is_none()
            && matches!(
                saved.status.as_str(),
                "failed" | "queued" | "needs_reconciliation"
            )
            && (saved.confirmed_bytes == 0 || checkpoint);
        let status = if requires_attention {
            "needs_reconciliation"
        } else if can_resume {
            "queued"
        } else {
            saved.status.as_str()
        };
        let detail = if requires_attention {
            "Original located with verified byte identity. Saved provider progress has no protected session; reconcile the channel before retrying."
        } else if can_resume {
            "Original located with verified byte identity. Any saved provider session must reconcile its confirmed range before transfer resumes."
        } else {
            "Original located with verified byte identity. Previous upload state and receipt were retained."
        };
        transaction.execute("UPDATE upload_items SET source_path = ?1, workspace_path = ?1, source_modified_key = ?2, background_hash_status = CASE WHEN background_hash_status = 'failed' THEN 'complete' ELSE background_hash_status END, status = ?3, detail = ?4, updated_at = ?5 WHERE id = ?6 AND channel_id = ?7",params![file.replacement_location,signature.1,status,detail,now(),saved.id,expected]).map_err(user_error)?;
        audit(&transaction,&saved.id,"source_relinked","Operator approved a new original locator after full trusted BLAKE3 identity verification; upload identity, channel, choices, receipts and protected resumable checkpoint were retained. Watched-folder roots were not changed.")?;
        receipt.relinked_item_ids.push(saved.id.clone());
        if can_resume {
            receipt.resumed_item_ids.push(saved.id.clone());
        }
        if requires_attention {
            receipt.attention_item_ids.push(saved.id.clone());
        }
    }
    transaction.execute("UPDATE source_relink_reviews SET state = 'committed', receipt_json = ?3, updated_at = ?4 WHERE id = ?1 AND channel_id = ?2 AND state = 'verified'",params![id,expected,serde_json::to_string(&receipt).map_err(user_error)?,now()]).map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    Ok(receipt)
}

pub(super) fn commit(
    state: &AppState,
    id: &str,
    expected: &str,
    ids: &[String],
    resume: bool,
    approved: bool,
) -> Result<Receipt, String> {
    commit_with_sessions(state, id, expected, ids, resume, approved, |item_id| {
        stored_upload_session(item_id).map(|value| value.is_some())
    })
}

pub(super) fn cancel(state: &AppState, id: &str, expected: &str) -> Result<(), String> {
    let connection = database(state)?;
    // A late dialog can release its own opaque review after an account switch;
    // cancellation neither reads an upload nor discloses another channel's data.
    connection.execute("UPDATE source_relink_reviews SET state = 'cancelled',updated_at = ?3 WHERE id = ?1 AND channel_id = ?2 AND state IN ('prepared','verifying','verified')",params![id,expected,now()]).map_err(user_error)?;
    Ok(())
}

#[tauri::command]
pub(super) async fn prepare_source_relink(
    state: State<'_, AppState>,
    item_ids: Vec<String>,
    expected_channel_id: String,
    path: String,
    mode: String,
) -> Result<Review, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        prepare(&state, &item_ids, &expected_channel_id, &path, &mode)
    })
    .await
    .map_err(user_error)?
}

#[tauri::command]
pub(super) async fn verify_source_relink(
    state: State<'_, AppState>,
    review_id: String,
    expected_channel_id: String,
) -> Result<Review, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || verify(&state, &review_id, &expected_channel_id))
        .await
        .map_err(user_error)?
}

#[tauri::command]
pub(super) fn load_source_relink_review(
    state: State<'_, AppState>,
    review_id: String,
    expected_channel_id: String,
) -> Result<Review, String> {
    load(&database(&state)?, &review_id, &expected_channel_id).map(|(review, _)| review)
}

#[tauri::command]
pub(super) async fn commit_source_relink(
    state: State<'_, AppState>,
    review_id: String,
    expected_channel_id: String,
    item_ids: Vec<String>,
    resume_uploads: bool,
    approved: bool,
) -> Result<Receipt, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        commit(
            &state,
            &review_id,
            &expected_channel_id,
            &item_ids,
            resume_uploads,
            approved,
        )
    })
    .await
    .map_err(user_error)?
}

#[tauri::command]
pub(super) fn cancel_source_relink(
    state: State<'_, AppState>,
    review_id: String,
    expected_channel_id: String,
) -> Result<(), String> {
    cancel(&state, &review_id, &expected_channel_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-source-relink-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        fs::create_dir_all(root.join("old")).unwrap();
        fs::create_dir_all(root.join("new")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute_batch(SCHEMA_SQL).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture channel','channel-a',?1)",[now()]).unwrap();
        (root, state, connection)
    }

    fn seed(
        connection: &Connection,
        root: &Path,
        id: &str,
        relative: &str,
        bytes: &[u8],
    ) -> PathBuf {
        let old = root.join("old").join(relative);
        let replacement = root.join("new").join(relative);
        fs::create_dir_all(replacement.parent().unwrap()).unwrap();
        fs::write(&replacement, bytes).unwrap();
        connection.execute("INSERT INTO upload_items(id,title,file_name,channel_name,channel_id,source_path,workspace_path,size_bytes,digest,background_hash_status,source_modified_key,status,confirmed_bytes,total_bytes,resumable_session_uri,visibility,made_for_kids,playlist_id,playlist_title,delete_source_after_upload,created_at,updated_at) VALUES(?1,?1,?2,'Fixture channel','channel-a',?3,?3,?4,?5,'complete','old-signature','failed',3,?4,'protected-checkpoint-reference','unlisted',1,'playlist-a','Original playlist',1,?6,?6)",params![id,relative,old.to_string_lossy(),bytes.len() as i64,blake3::hash(bytes).to_hex().to_string(),now()]).unwrap();
        replacement
    }

    fn verify_file(state: &AppState, id: &str, replacement: &Path) -> Review {
        let review = prepare(
            state,
            &[id.into()],
            "channel-a",
            &replacement.to_string_lossy(),
            "file",
        )
        .unwrap();
        verify(state, &review.review_id, "channel-a").unwrap()
    }

    #[test]
    fn relocation_preserves_identity_receipt_choices_and_checkpoint_without_media_copies() {
        let (root, state, connection) = fixture();
        let replacement = seed(
            &connection,
            &root,
            "existing",
            "clip.mp4",
            b"trusted-original-bytes",
        );
        connection.execute("UPDATE upload_items SET video_id='confirmed-video',status='uploaded',confirmed_bytes=total_bytes WHERE id='existing'",[]).unwrap();
        let review = verify_file(&state, "existing", &replacement);
        assert_eq!(review.verified_count, 1);
        let receipt = commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["existing".into()],
            true,
            true,
            |_| panic!("A completed receipt must not read or replace a protected checkpoint"),
        )
        .unwrap();
        assert_eq!(receipt.relinked_item_ids, vec!["existing"]);
        assert!(receipt.resumed_item_ids.is_empty());
        let retained: (String,String,String,String,String,i64,i64,String,i64) = connection.query_row("SELECT source_path,workspace_path,status,video_id,resumable_session_uri,confirmed_bytes,made_for_kids,playlist_id,delete_source_after_upload FROM upload_items WHERE id='existing'",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?))).unwrap();
        assert_eq!(retained.0, replacement.to_string_lossy());
        assert_eq!(retained.1, retained.0);
        assert_eq!(retained.2, "uploaded");
        assert_eq!(retained.3, "confirmed-video");
        assert_eq!(retained.4, "protected-checkpoint-reference");
        assert_eq!(retained.5, b"trusted-original-bytes".len() as i64);
        assert_eq!(retained.6, 1);
        assert_eq!(retained.7, "playlist-a");
        assert_eq!(retained.8, 1);
        assert_eq!(fs::read_dir(root.join("media")).unwrap().count(), 0);
        assert_eq!(fs::read(&replacement).unwrap(), b"trusted-original-bytes");
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE item_id='existing' AND channel_id='channel-a' AND kind='source_relinked'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(
            commit_with_sessions(
                &state,
                &review.review_id,
                "channel-a",
                &["existing".into()],
                true,
                true,
                |_| panic!("An idempotent receipt must not reopen a checkpoint")
            )
            .unwrap()
            .relinked_item_ids,
            receipt.relinked_item_ids
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_mapping_repairs_multiple_relative_locations_and_keeps_watch_roots() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "first", "first.mp4", b"first");
        seed(&connection, &root, "second", "nested/second.mp4", b"second");
        connection.execute("INSERT INTO folder_monitors(id,channel_id,channel_name,folder_path,folder_key,enabled,visibility,made_for_kids,delete_source_after_upload,status,detail,updated_at) VALUES('monitor-a','channel-a','Fixture channel',?1,?1,1,'unlisted',0,0,'watching','unchanged',?2)",params![root.join("old").to_string_lossy(),now()]).unwrap();
        let review = prepare(
            &state,
            &["first".into(), "second".into()],
            "channel-a",
            &root.join("new").to_string_lossy(),
            "folder",
        )
        .unwrap();
        assert_eq!(
            PathBuf::from(&review.files[1].replacement_location),
            root.join("new").join("nested").join("second.mp4")
        );
        let verified = verify(&state, &review.review_id, "channel-a").unwrap();
        assert_eq!(verified.verified_count, 2);
        let receipt = commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["first".into(), "second".into()],
            true,
            true,
            |_| Ok(true),
        )
        .unwrap();
        assert_eq!(receipt.resumed_item_ids.len(), 2);
        assert_eq!(
            connection
                .query_row(
                    "SELECT folder_path FROM folder_monitors WHERE id='monitor-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            root.join("old").to_string_lossy()
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_length_wrong_bytes_and_missing_digest_remain_blocked() {
        let (root, state, connection) = fixture();
        let replacement = seed(&connection, &root, "wrong", "wrong.mp4", b"trusted");
        fs::write(&replacement, b"changed").unwrap();
        let review = verify_file(&state, "wrong", &replacement);
        assert_eq!(review.blocked_count, 1);
        assert!(review.files[0].detail.contains("different bytes"));
        assert!(commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["wrong".into()],
            false,
            true,
            |_| Ok(false)
        )
        .is_err());
        let missing = seed(&connection, &root, "missing", "missing.mp4", b"bytes");
        connection
            .execute("UPDATE upload_items SET digest=NULL WHERE id='missing'", [])
            .unwrap();
        let review = verify_file(&state, "missing", &missing);
        assert_eq!(review.verified_count, 0);
        assert!(review.files[0].detail.contains("No trusted full BLAKE3"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unavailable_and_active_or_hash_running_sources_cannot_be_relinked() {
        let (root, state, connection) = fixture();
        let replacement = seed(&connection, &root, "existing", "clip.mp4", b"original");
        let review = prepare(
            &state,
            &["existing".into()],
            "channel-a",
            &root.join("new").to_string_lossy(),
            "folder",
        )
        .unwrap();
        fs::remove_file(&replacement).unwrap();
        assert_eq!(
            verify(&state, &review.review_id, "channel-a")
                .unwrap()
                .blocked_count,
            1
        );
        fs::write(&replacement, b"original").unwrap();
        connection
            .execute(
                "UPDATE upload_items SET status='uploading' WHERE id='existing'",
                [],
            )
            .unwrap();
        assert_eq!(
            verify_file(&state, "existing", &replacement).blocked_count,
            1
        );
        connection.execute("UPDATE upload_items SET status='failed',background_hash_status='running' WHERE id='existing'",[]).unwrap();
        assert_eq!(
            verify_file(&state, "existing", &replacement).blocked_count,
            1
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_during_hash_and_changed_after_review_are_rejected() {
        let (root, state, connection) = fixture();
        let replacement = seed(&connection, &root, "existing", "clip.mp4", b"original");
        let saved = snapshot(&connection, "existing", "channel-a").unwrap();
        let error = verify_bytes_with(&replacement, &saved, &|| false, || {
            fs::write(&replacement, b"different length").unwrap();
        })
        .unwrap_err();
        assert!(error.contains("changed during verification"));
        fs::write(&replacement, b"original").unwrap();
        let review = verify_file(&state, "existing", &replacement);
        fs::write(&replacement, b"replaced").unwrap();
        assert!(commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["existing".into()],
            true,
            true,
            |_| Ok(true)
        )
        .unwrap_err()
        .contains("different bytes"));
        assert_eq!(
            snapshot(&connection, "existing", "channel-a").unwrap(),
            saved
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn commit_rolls_back_entire_group_if_any_upload_snapshot_changes() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "first", "first.mp4", b"first");
        seed(&connection, &root, "second", "second.mp4", b"second");
        let before = snapshot(&connection, "first", "channel-a").unwrap();
        let review = prepare(
            &state,
            &["first".into(), "second".into()],
            "channel-a",
            &root.join("new").to_string_lossy(),
            "folder",
        )
        .unwrap();
        verify(&state, &review.review_id, "channel-a").unwrap();
        // Inject a concurrent queue edit during commit's protected-session read.
        let receipt = commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["first".into(), "second".into()],
            true,
            true,
            |item| {
                if item == "second" {
                    connection.execute("UPDATE upload_items SET title='Edited during review' WHERE id='second'",[]).unwrap();
                }
                Ok(true)
            },
        );
        assert!(receipt.is_err());
        assert_eq!(snapshot(&connection, "first", "channel-a").unwrap(), before);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE kind='source_relinked'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelled_expired_and_stale_channel_reviews_retain_all_evidence() {
        let (root, state, connection) = fixture();
        let replacement = seed(&connection, &root, "existing", "clip.mp4", b"original");
        let before = snapshot(&connection, "existing", "channel-a").unwrap();
        let review = verify_file(&state, "existing", &replacement);
        cancel(&state, &review.review_id, "channel-a").unwrap();
        assert!(commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["existing".into()],
            true,
            true,
            |_| Ok(true)
        )
        .is_err());
        let review = verify_file(&state, "existing", &replacement);
        connection
            .execute(
                "UPDATE source_relink_reviews SET expires_at='2000-01-01T00:00:00Z' WHERE id=?1",
                [&review.review_id],
            )
            .unwrap();
        assert!(load(&connection, &review.review_id, "channel-a").is_err());
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        assert!(prepare(
            &state,
            &["existing".into()],
            "channel-a",
            &replacement.to_string_lossy(),
            "file"
        )
        .is_err());
        assert!(load(&connection, &review.review_id, "channel-b").is_err());
        cancel(&state, &review.review_id, "channel-a").unwrap();
        assert_eq!(
            snapshot(&connection, "existing", "channel-a").unwrap(),
            before
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_session_leaves_progress_for_attention_and_explicit_no_resume_retains_state() {
        let (root, state, connection) = fixture();
        let replacement = seed(&connection, &root, "existing", "clip.mp4", b"original");
        let review = verify_file(&state, "existing", &replacement);
        let receipt = commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["existing".into()],
            true,
            true,
            |_| Ok(false),
        )
        .unwrap();
        assert_eq!(receipt.attention_item_ids, vec!["existing"]);
        assert!(receipt.resumed_item_ids.is_empty());
        assert_eq!(
            snapshot(&connection, "existing", "channel-a")
                .unwrap()
                .status,
            "needs_reconciliation"
        );
        let second = seed(&connection, &root, "second", "second.mp4", b"second");
        let review = verify_file(&state, "second", &second);
        let receipt = commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["second".into()],
            false,
            true,
            |_| panic!("Saving without resume must not request provider or credential access"),
        )
        .unwrap();
        assert!(receipt.resumed_item_ids.is_empty());
        assert_eq!(
            snapshot(&connection, "second", "channel-a").unwrap().status,
            "failed"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restart_keeps_staged_review_and_requires_fresh_byte_verification_at_commit() {
        let (root, state, connection) = fixture();
        let replacement = seed(&connection, &root, "existing", "clip.mp4", b"original");
        let review = verify_file(&state, "existing", &replacement);
        let restarted = state.clone();
        drop(connection);
        assert_eq!(
            load(
                &database(&restarted).unwrap(),
                &review.review_id,
                "channel-a"
            )
            .unwrap()
            .0
            .verified_count,
            1
        );
        let receipt = commit_with_sessions(
            &restarted,
            &review.review_id,
            "channel-a",
            &["existing".into()],
            false,
            true,
            |_| Ok(false),
        )
        .unwrap();
        assert_eq!(receipt.relinked_item_ids, vec!["existing"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn review_bounds_and_native_approval_are_enforced() {
        let (root, state, connection) = fixture();
        let replacement = seed(&connection, &root, "existing", "clip.mp4", b"original");
        assert!(prepare(
            &state,
            &[],
            "channel-a",
            &replacement.to_string_lossy(),
            "file"
        )
        .is_err());
        assert!(prepare(
            &state,
            &["existing".into(), "existing".into()],
            "channel-a",
            &replacement.to_string_lossy(),
            "folder"
        )
        .is_err());
        let review = verify_file(&state, "existing", &replacement);
        assert!(commit_with_sessions(
            &state,
            &review.review_id,
            "channel-a",
            &["existing".into()],
            false,
            false,
            |_| Ok(false)
        )
        .is_err());
        for _ in 1..MAX_REVIEWS {
            prepare(
                &state,
                &["existing".into()],
                "channel-a",
                &replacement.to_string_lossy(),
                "file",
            )
            .unwrap();
        }
        assert!(prepare(
            &state,
            &["existing".into()],
            "channel-a",
            &replacement.to_string_lossy(),
            "file"
        )
        .is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn final_links_and_linked_parents_are_rejected() {
        use std::os::unix::fs::symlink;
        let (root, state, connection) = fixture();
        let replacement = seed(&connection, &root, "existing", "clip.mp4", b"original");
        symlink(&replacement, root.join("alias.mp4")).unwrap();
        symlink(root.join("new"), root.join("alias-folder")).unwrap();
        assert!(prepare(
            &state,
            &["existing".into()],
            "channel-a",
            &root.join("alias.mp4").to_string_lossy(),
            "file"
        )
        .is_err());
        assert!(prepare(
            &state,
            &["existing".into()],
            "channel-a",
            &root.join("alias-folder/clip.mp4").to_string_lossy(),
            "file"
        )
        .is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
}
