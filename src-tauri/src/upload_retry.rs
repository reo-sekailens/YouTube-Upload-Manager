//! Reviewed retries retain the canonical upload, original and protected checkpoint.
//! Provider uncertainty is reconciled through an existing session, never a new insert.
use super::*;

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS upload_retry_failures (
  item_id TEXT PRIMARY KEY NOT NULL REFERENCES upload_items(id),
  channel_id TEXT NOT NULL,
  stage TEXT NOT NULL,
  http_status INTEGER NOT NULL,
  reason_key TEXT NOT NULL,
  failure_key TEXT NOT NULL,
  detail TEXT NOT NULL,
  recorded_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS upload_retry_failure_group ON upload_retry_failures(channel_id, failure_key);
CREATE TABLE IF NOT EXISTS upload_retry_reviews (
  id TEXT PRIMARY KEY NOT NULL,
  channel_id TEXT NOT NULL,
  failure_key TEXT NOT NULL,
  snapshot_json TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  receipt_json TEXT,
  created_at TEXT NOT NULL
);
"#;

const MAX_ITEMS: usize = 200;
const MAX_OPEN_REVIEWS: usize = 8;
const LEGACY_SETUP_DETAIL: &str = "YouTube rejected the upload setup request.";
const LEGACY_SETUP_KEY: &str = "setup:legacy_unknown";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RetryItem {
    pub item_id: String,
    pub title: String,
    pub file_name: String,
    pub detail: String,
    pub eligible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RetryReview {
    pub review_id: String,
    pub channel_id: String,
    pub channel_name: Option<String>,
    pub failure_key: String,
    pub failure_detail: String,
    pub expires_at: String,
    pub items: Vec<RetryItem>,
    pub eligible_count: usize,
    pub blocked_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct BlockedItem {
    pub item_id: String,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RetryReceipt {
    pub review_id: String,
    pub channel_id: String,
    pub queued_item_ids: Vec<String>,
    pub blocked_items: Vec<BlockedItem>,
    pub detail: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Snapshot {
    id: String,
    title: String,
    file_name: String,
    channel_id: String,
    status: String,
    detail: Option<String>,
    source_path: Option<String>,
    workspace_path: String,
    total_bytes: u64,
    confirmed_bytes: u64,
    digest: Option<String>,
    source_modified_key: Option<String>,
    hash_status: String,
    video_id: Option<String>,
    duplicate_decision: Option<String>,
    /// Includes every upload choice and the legacy checkpoint reference without
    /// persisting its contents again or exposing it in a command response.
    choices_digest: String,
    updated_at: String,
    failure_key: String,
    failure_detail: String,
    http_status: Option<u16>,
}

#[derive(Serialize, Deserialize)]
struct SavedReview {
    snapshots: Vec<Snapshot>,
    public_items: Vec<RetryItem>,
}

#[derive(Serialize, Deserialize)]
struct SavedReceipt {
    selected_ids: Vec<String>,
    receipt: RetryReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SessionState {
    Absent,
    Present,
    Unavailable,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn selected_ids(ids: &[String]) -> Result<Vec<String>, String> {
    if ids.is_empty() || ids.len() > MAX_ITEMS || ids.iter().any(|id| !valid_id(id)) {
        return Err("Choose between 1 and 200 saved uploads to retry.".into());
    }
    let mut unique = HashSet::new();
    if ids.iter().any(|id| !unique.insert(id.clone())) {
        return Err("Choose each saved upload only once.".into());
    }
    let mut result = ids.to_vec();
    result.sort();
    Ok(result)
}

fn channel(connection: &Connection, expected: &str) -> Result<ConnectionSettings, String> {
    if !valid_id(expected) {
        return Err("Choose a valid connected channel before reviewing retries.".into());
    }
    let settings = connection_settings(connection)?;
    if !settings.connected || settings.active_channel_id.as_deref() != Some(expected) {
        return Err(
            "The connected channel changed. Reopen the retry review for its saved channel.".into(),
        );
    }
    Ok(settings)
}

fn session_state(id: &str) -> SessionState {
    let Ok(entry) = upload_session_entry(id) else {
        return SessionState::Unavailable;
    };
    match entry.get_password() {
        Ok(uri) if !uri.is_empty() => SessionState::Present,
        Ok(_) => SessionState::Unavailable,
        Err(keyring::Error::NoEntry) => SessionState::Absent,
        Err(_) => SessionState::Unavailable,
    }
}

/// Only bounded, sorted error reason identifiers form a group. Unknown provider
/// identifiers are hashed; provider messages, request bodies and credentials are
/// never stored. An unclassified body stays unique to its upload.
fn setup_reason_key(item_id: &str, body: &str) -> String {
    const KNOWN: &[&str] = &[
        "invalidTitle",
        "invalidDescription",
        "invalidTags",
        "invalidCategoryId",
        "invalidDefaultLanguage",
        "invalidDefaultAudioLanguage",
        "invalidPublishAt",
        "invalidLicense",
        "invalidVideoMetadata",
        "invalidMetadata",
        "invalidRequest",
        "invalidValue",
        "required",
        "badRequest",
        "forbidden",
        "unauthorized",
        "authError",
        "insufficientPermissions",
        "youtubeSignupRequired",
        "channelNotFound",
        "uploadLimitExceeded",
        "quotaExceeded",
        "dailyLimitExceeded",
        "rateLimitExceeded",
        "userRateLimitExceeded",
        "backendError",
        "internalError",
        "serviceUnavailable",
    ];
    let Some(payload) = (body.len() <= 1_048_576)
        .then(|| serde_json::from_str::<serde_json::Value>(body).ok())
        .flatten()
    else {
        return format!("unclassified-{item_id}");
    };
    let Some(errors) = payload.pointer("/error/errors").and_then(|v| v.as_array()) else {
        return format!("unclassified-{item_id}");
    };
    if errors.len() > 32 {
        return format!("unclassified-{item_id}");
    }
    let mut reasons = Vec::new();
    for error in errors {
        let Some(reason) = error.get("reason").and_then(|v| v.as_str()) else {
            return format!("unclassified-{item_id}");
        };
        if reason.is_empty() || reason.len() > 128 {
            return format!("unclassified-{item_id}");
        }
        reasons.push(reason.to_owned());
    }
    if reasons.is_empty() {
        return format!("unclassified-{item_id}");
    }
    reasons.sort();
    reasons.dedup();
    if reasons
        .iter()
        .all(|reason| KNOWN.contains(&reason.as_str()))
    {
        reasons.join("+")
    } else {
        let encoded = serde_json::to_vec(&reasons).expect("reason strings serialize");
        format!("other-{}", &blake3::hash(&encoded).to_hex()[..24])
    }
}

/// Call only after a non-success response to the metadata-only resumable setup
/// request, before any upload session or video bytes have been accepted locally.
pub(super) fn record_setup_failure(
    connection: &Connection,
    item_id: &str,
    http_status: u16,
    body: &str,
) -> Result<(), String> {
    if !(400..=599).contains(&http_status) || !valid_id(item_id) {
        return Err("The upload setup failure could not be recorded safely.".into());
    }
    let channel_id: Option<String> = connection
        .query_row(
            "SELECT channel_id FROM upload_items WHERE id=?1",
            [item_id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    let channel_id = channel_id
        .filter(|v| valid_id(v))
        .ok_or("This failed upload has no saved channel identity.")?;
    let reason = setup_reason_key(item_id, body);
    let key = format!("setup:{http_status}:{reason}");
    let detail = if reason.starts_with("other-") || reason.starts_with("unclassified-") {
        format!("YouTube rejected the upload setup request (HTTP {http_status}); its detailed reason is unavailable.")
    } else {
        format!("YouTube rejected the upload setup request (HTTP {http_status}; {reason}).")
    };
    connection.execute("INSERT INTO upload_retry_failures(item_id,channel_id,stage,http_status,reason_key,failure_key,detail,recorded_at) VALUES(?1,?2,'setup',?3,?4,?5,?6,?7) ON CONFLICT(item_id) DO UPDATE SET channel_id=excluded.channel_id,stage=excluded.stage,http_status=excluded.http_status,reason_key=excluded.reason_key,failure_key=excluded.failure_key,detail=excluded.detail,recorded_at=excluded.recorded_at",params![item_id,channel_id,http_status,reason,key,detail,now()]).map_err(user_error)?;
    audit(connection, item_id, "upload_setup_rejected", &detail)?;
    Ok(())
}

pub(super) fn failure_key(
    connection: &Connection,
    item_id: &str,
) -> Result<Option<String>, String> {
    let recorded: Option<String> = connection
        .query_row(
            "SELECT failure_key FROM upload_retry_failures WHERE item_id=?1",
            [item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?;
    if recorded.is_some() {
        return Ok(recorded);
    }
    let detail: Option<String> = connection
        .query_row(
            "SELECT detail FROM upload_items WHERE id=?1",
            [item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?
        .flatten();
    Ok((detail.as_deref() == Some(LEGACY_SETUP_DETAIL)).then(|| LEGACY_SETUP_KEY.into()))
}

/// Clear only the current grouping projection. Append-only rejection audits and
/// review snapshots remain; a later transfer failure cannot inherit this stage.
pub(super) fn clear_failure(connection: &Connection, item_id: &str) -> Result<(), String> {
    connection
        .execute(
            "DELETE FROM upload_retry_failures WHERE item_id=?1",
            [item_id],
        )
        .map_err(user_error)?;
    Ok(())
}

/// A worker may expose these definitive setup refusals as failed. Every other
/// interruption retains the normal provider reconciliation lane.
pub(super) fn definitive_setup_rejection(
    connection: &Connection,
    item_id: &str,
) -> Result<bool, String> {
    connection.query_row("SELECT EXISTS(SELECT 1 FROM upload_retry_failures f JOIN upload_items u ON u.id=f.item_id AND u.channel_id=f.channel_id WHERE f.item_id=?1 AND f.stage='setup' AND f.http_status BETWEEN 400 AND 499 AND f.http_status != 408 AND u.confirmed_bytes=0 AND u.video_id IS NULL AND u.total_bytes>0)",[item_id],|row|row.get(0)).map_err(user_error)
}

fn snapshot(connection: &Connection, id: &str, expected: &str) -> Result<Snapshot, String> {
    let mut item = connection.query_row("SELECT id,title,file_name,channel_id,status,detail,source_path,workspace_path,total_bytes,confirmed_bytes,digest,source_modified_key,background_hash_status,video_id,duplicate_decision,updated_at,upload_metadata_json,visibility,made_for_kids,playlist_id,playlist_title,playlist_status,delete_source_after_upload,source_delete_status,folder_monitor_id,resumable_session_uri,upload_batch_id,upload_plan_id FROM upload_items WHERE id=?1 AND channel_id=?2",params![id,expected],|row| {
        let choices = serde_json::json!([
            row.get::<_,String>(16)?,row.get::<_,String>(17)?,row.get::<_,i64>(18)?,
            row.get::<_,Option<String>>(19)?,row.get::<_,Option<String>>(20)?,row.get::<_,String>(21)?,
            row.get::<_,i64>(22)?,row.get::<_,Option<String>>(23)?,row.get::<_,Option<String>>(24)?,
            row.get::<_,Option<String>>(25)?,row.get::<_,Option<String>>(26)?,row.get::<_,Option<String>>(27)?
        ]);
        Ok(Snapshot {
            id:row.get(0)?,title:row.get(1)?,file_name:row.get(2)?,channel_id:row.get(3)?,status:row.get(4)?,detail:row.get(5)?,source_path:row.get(6)?,workspace_path:row.get(7)?,
            total_bytes:row.get::<_,i64>(8)?.max(0) as u64,confirmed_bytes:row.get::<_,i64>(9)?.max(0) as u64,digest:row.get(10)?,source_modified_key:row.get(11)?,hash_status:row.get(12)?,video_id:row.get(13)?,duplicate_decision:row.get(14)?,updated_at:row.get(15)?,
            choices_digest:blake3::hash(choices.to_string().as_bytes()).to_hex().to_string(),failure_key:String::new(),failure_detail:String::new(),http_status:None
        })
    }).map_err(|_| "This saved upload no longer belongs to the reviewed channel.".to_string())?;
    let recorded:Option<(String,String,u16,String)> = connection.query_row("SELECT failure_key,detail,http_status,recorded_at FROM upload_retry_failures WHERE item_id=?1 AND channel_id=?2",params![id,expected],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional().map_err(user_error)?;
    // A later error must not inherit an old setup refusal from a previous attempt.
    if let Some((key, detail, status, recorded_at)) = recorded.filter(|(_, _, _, at)| {
        at >= &item.updated_at
            || item.detail.as_deref().is_some_and(|v| {
                v == LEGACY_SETUP_DETAIL
                    || v.starts_with("YouTube rejected the upload setup request (HTTP ")
            })
    }) {
        item.failure_key = key;
        item.failure_detail = detail;
        item.http_status = Some(status);
        let _ = recorded_at;
    } else if item.detail.as_deref() == Some(LEGACY_SETUP_DETAIL) {
        item.failure_key = LEGACY_SETUP_KEY.into();
        item.failure_detail="Same reported upload setup error; the older provider detail is unavailable. No video bytes were sent during this setup request.".into();
    } else {
        let digest = blake3::hash(
            item.detail
                .as_deref()
                .unwrap_or("Unknown saved failure")
                .as_bytes(),
        )
        .to_hex();
        item.failure_key = format!("saved-error:{}", &digest[..24]);
        item.failure_detail="Same saved upload error. Only an available protected session may resume this uncertain provider attempt.".into();
    }
    Ok(item)
}

fn original_path(item: &Snapshot) -> &Path {
    Path::new(
        item.source_path
            .as_deref()
            .filter(|_| item.source_path.as_deref() == Some(item.workspace_path.as_str()))
            .unwrap_or(&item.workspace_path),
    )
}

fn eligibility(item: &Snapshot, session: SessionState) -> Result<(), String> {
    if !matches!(item.status.as_str(), "failed" | "needs_reconciliation") {
        return Err(
            "This upload is no longer a failed or interrupted item. Refresh its status.".into(),
        );
    }
    if item.video_id.is_some() {
        return Err("A YouTube video result is already saved. Check or repair completion instead of uploading again.".into());
    }
    if item.total_bytes == 0 || item.confirmed_bytes >= item.total_bytes {
        return Err("The provider may already have received the complete video. Reconcile its result before any retry.".into());
    }
    if item.duplicate_decision.as_deref() == Some("pending")
        || item.duplicate_decision.as_deref() == Some("skip")
    {
        return Err("Resolve this upload's duplicate decision before retrying.".into());
    }
    if matches!(item.hash_status.as_str(), "pending" | "running")
        || !item
            .digest
            .as_deref()
            .is_some_and(|v| v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err("A trusted full content digest is required. Finish or repair original-file verification first.".into());
    }
    match session {
        SessionState::Unavailable=>return Err("Protected upload checkpoint storage is unavailable. Restore access before retrying; the saved checkpoint remains unchanged.".into()),
        SessionState::Present=>return Ok(()),
        SessionState::Absent=>{}
    }
    if item.confirmed_bytes != 0 {
        return Err("Provider progress is saved without an available protected session. Reconcile the channel before any new upload.".into());
    }
    let definite = item
        .http_status
        .is_some_and(|status| (400..500).contains(&status) && status != 408);
    let legacy = item.http_status.is_none()
        && item.failure_key == LEGACY_SETUP_KEY
        && item.detail.as_deref() == Some(LEGACY_SETUP_DETAIL);
    if !definite && !legacy {
        return Err("This attempt's provider outcome is uncertain. No new upload is started without a protected session or confirmed setup rejection.".into());
    }
    Ok(())
}

fn source_signature(item: &Snapshot) -> Result<(u64, String), String> {
    let path = original_path(item);
    if has_link_or_reparse_component(path)? {
        return Err("The saved original uses a linked location. Locate a verified regular original before retrying.".into());
    }
    let source = open_reference_source(path).map_err(|_| {
        "The saved original is unavailable. Locate the original before retrying.".to_string()
    })?;
    let signature = monitored_file_signature(&source.metadata().map_err(user_error)?)?;
    if signature.0 != item.total_bytes
        || item
            .source_modified_key
            .as_deref()
            .is_some_and(|v| v != signature.1)
    {
        return Err(
            "The original changed after import. Locate the verified original before retrying."
                .into(),
        );
    }
    Ok(signature)
}

fn prepare_with_sessions(
    state: &AppState,
    expected: &str,
    anchor: &str,
    all_matching: bool,
    session: impl Fn(&str) -> SessionState,
) -> Result<RetryReview, String> {
    if !valid_id(anchor) {
        return Err("Choose a saved upload to review.".into());
    }
    oauth_replacement::ensure_no_pending(state)?;
    let mut connection = database(state)?;
    let settings = channel(&connection, expected)?;
    let anchor_item = snapshot(&connection, anchor, expected)?;
    if !matches!(
        anchor_item.status.as_str(),
        "failed" | "needs_reconciliation"
    ) {
        return Err("This upload no longer needs a retry. Refresh the queue.".into());
    }
    let mut snapshots = Vec::new();
    if all_matching {
        // Bound the read to the actual group through structured records or the
        // exact legacy detail. Never fetch an unlimited channel into memory.
        let ids=connection.prepare("SELECT u.id FROM upload_items u LEFT JOIN upload_retry_failures f ON f.item_id=u.id AND f.channel_id=u.channel_id WHERE u.channel_id=?1 AND u.status IN ('failed','needs_reconciliation') AND ((?2 != 'setup:legacy_unknown' AND f.failure_key=?2) OR (?2='setup:legacy_unknown' AND f.item_id IS NULL AND u.detail=?3) OR (?2 LIKE 'saved-error:%' AND u.detail IS ?4)) ORDER BY u.created_at,u.id LIMIT 201").map_err(user_error)?.query_map(params![expected,anchor_item.failure_key,LEGACY_SETUP_DETAIL,anchor_item.detail],|row|row.get::<_,String>(0)).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
        if ids.len() > MAX_ITEMS {
            return Err(
                "More than 200 uploads share this issue. Retry this upload remains available for individual items; reopen Retry all when 200 or fewer matching items remain. No uploads were queued.".into(),
            );
        }
        for id in ids {
            let item = snapshot(&connection, &id, expected)?;
            if item.failure_key == anchor_item.failure_key {
                snapshots.push(item);
            }
        }
    } else {
        snapshots.push(anchor_item.clone());
    }
    if snapshots.is_empty() {
        return Err("No matching failed uploads remain. Refresh the queue.".into());
    }
    let items=snapshots.iter().map(|item| {
        let blocked_reason=eligibility(item,session(&item.id)).and_then(|_|source_signature(item).map(|_|())).err();
        RetryItem {item_id:item.id.clone(),title:item.title.clone(),file_name:item.file_name.clone(),detail:if item.failure_key.starts_with("setup:") {item.failure_detail.clone()} else {"A protected session must confirm the provider's byte range before transfer resumes.".into()},eligible:blocked_reason.is_none(),blocked_reason}
    }).collect::<Vec<_>>();
    let review = RetryReview {
        review_id: Uuid::new_v4().to_string(),
        channel_id: expected.into(),
        channel_name: settings.active_channel,
        failure_key: anchor_item.failure_key.clone(),
        failure_detail: anchor_item.failure_detail,
        expires_at: (Utc::now() + ChronoDuration::minutes(15)).to_rfc3339(),
        eligible_count: items.iter().filter(|i| i.eligible).count(),
        blocked_count: items.iter().filter(|i| !i.eligible).count(),
        items,
    };
    let saved = SavedReview {
        snapshots,
        public_items: review.items.clone(),
    };
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    channel(&transaction, expected)?;
    transaction
        .execute(
            "DELETE FROM upload_retry_reviews WHERE receipt_json IS NULL AND expires_at<=?1",
            [now()],
        )
        .map_err(user_error)?;
    let count:i64=transaction.query_row("SELECT COUNT(*) FROM upload_retry_reviews WHERE receipt_json IS NULL AND channel_id=?1",[expected],|row|row.get(0)).map_err(user_error)?;
    if count >= MAX_OPEN_REVIEWS as i64 {
        // Reopening a dialog is reversible. Evict the oldest unsubmitted local
        // review instead of forcing a 15-minute wait after repeated close/open.
        transaction.execute("DELETE FROM upload_retry_reviews WHERE id IN (SELECT id FROM upload_retry_reviews WHERE receipt_json IS NULL AND channel_id=?1 ORDER BY created_at,id LIMIT ?2)",params![expected,count-MAX_OPEN_REVIEWS as i64+1]).map_err(user_error)?;
    }
    transaction.execute("INSERT INTO upload_retry_reviews(id,channel_id,failure_key,snapshot_json,expires_at,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![review.review_id,expected,review.failure_key,serde_json::to_string(&saved).map_err(user_error)?,review.expires_at,now()]).map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    Ok(review)
}

#[cfg(windows)]
fn file_identity(file: &File) -> Result<(u64, u64), String> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    #[derive(Default)]
    struct FileInformation {
        words: [u32; 13],
    }
    #[link(name = "kernel32")]
    extern "system" {
        #[link_name = "GetFileInformationByHandle"]
        fn retry_file_information(handle: *mut std::ffi::c_void, info: *mut FileInformation)
            -> i32;
    }
    let mut info = FileInformation::default();
    if unsafe { retry_file_information(file.as_raw_handle(), &mut info) } == 0 {
        return Err("The original file identity could not be verified.".into());
    }
    Ok((
        info.words[7] as u64,
        ((info.words[11] as u64) << 32) | info.words[12] as u64,
    ))
}

#[cfg(unix)]
fn file_identity(file: &File) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(user_error)?;
    Ok((metadata.dev(), metadata.ino()))
}

struct VerifiedOriginal {
    handle: File,
    signature: (u64, String),
    identity: (u64, u64),
}

fn verify_original(item: &Snapshot, after_hash: impl FnOnce()) -> Result<VerifiedOriginal, String> {
    source_signature(item)?;
    let path = original_path(item);
    let mut handle = open_reference_source(path)?;
    let signature = monitored_file_signature(&handle.metadata().map_err(user_error)?)?;
    let identity = file_identity(&handle)?;
    let (bytes, digest) = digest_open_reference_source(&mut handle)?;
    if bytes != item.total_bytes || item.digest.as_deref() != Some(digest.as_str()) {
        return Err("The original's current bytes differ from its trusted upload digest. It was not queued.".into());
    }
    after_hash();
    let verified = VerifiedOriginal {
        handle,
        signature,
        identity,
    };
    recheck_original(item, &verified)?;
    Ok(verified)
}

fn recheck_original(item: &Snapshot, verified: &VerifiedOriginal) -> Result<(), String> {
    if monitored_file_signature(&verified.handle.metadata().map_err(user_error)?)?
        != verified.signature
    {
        return Err("The original changed during retry verification. It was not queued.".into());
    }
    source_signature(item)?;
    let reopened = open_reference_source(original_path(item))?;
    if file_identity(&reopened)? != verified.identity
        || monitored_file_signature(&reopened.metadata().map_err(user_error)?)?
            != verified.signature
    {
        return Err(
            "The saved original location changed during retry verification. It was not queued."
                .into(),
        );
    }
    Ok(())
}

fn load_saved(
    connection: &Connection,
    expected: &str,
    review_id: &str,
) -> Result<(SavedReview, String, Option<SavedReceipt>), String> {
    if !valid_id(review_id) {
        return Err("Choose a valid retry review.".into());
    }
    let (saved,expires,receipt):(String,String,Option<String>)=connection.query_row("SELECT snapshot_json,expires_at,receipt_json FROM upload_retry_reviews WHERE id=?1 AND channel_id=?2",params![review_id,expected],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).map_err(|_|"This retry review no longer belongs to the connected channel. Open a new review.".to_string())?;
    Ok((
        serde_json::from_str(&saved)
            .map_err(|_| "The saved retry review is invalid.".to_string())?,
        expires,
        receipt
            .map(|v| serde_json::from_str(&v))
            .transpose()
            .map_err(|_| "The saved retry receipt is invalid.".to_string())?,
    ))
}

fn retry_with_sessions(
    state: &AppState,
    expected: &str,
    review_id: &str,
    ids: &[String],
    session: impl Fn(&str) -> SessionState,
) -> Result<RetryReceipt, String> {
    let selected = selected_ids(ids)?;
    oauth_replacement::ensure_no_pending(state)?;
    let connection = database(state)?;
    channel(&connection, expected)?;
    let (saved, expires, previous) = load_saved(&connection, expected, review_id)?;
    if let Some(previous) = previous {
        return if previous.selected_ids == selected {
            Ok(previous.receipt)
        } else {
            Err("This retry review was already submitted with a different selection. Open a new review.".into())
        };
    }
    if expires <= now() {
        return Err(
            "This retry review expired. Refresh the failed uploads and review them again.".into(),
        );
    }
    if selected
        .iter()
        .any(|id| !saved.snapshots.iter().any(|s| &s.id == id))
    {
        return Err(
            "The selection includes an upload outside this reviewed issue. Open a new review."
                .into(),
        );
    }
    let mut verified = Vec::new();
    let mut blocked = Vec::new();
    for id in &selected {
        let original = saved
            .snapshots
            .iter()
            .find(|s| &s.id == id)
            .expect("selection checked");
        let result = (|| {
            if !saved
                .public_items
                .iter()
                .any(|i| &i.item_id == id && i.eligible)
            {
                return Err("This upload was blocked during review. Resolve its original or provider outcome and reopen the review.".into());
            }
            let current = snapshot(&connection, id, expected)?;
            if &current != original {
                return Err(
                    "This upload changed after review. Refresh and review its current issue."
                        .into(),
                );
            }
            eligibility(&current, session(id))?;
            let verified = verify_original(&current, || {})?;
            Ok((current, verified))
        })();
        match result {
            Ok(v) => verified.push(v),
            Err(detail) => blocked.push(BlockedItem {
                item_id: id.clone(),
                detail,
            }),
        }
    }
    drop(connection);
    // The hash stream is outside the dispatch/credential lock. The short final
    // claim rechecks current account, immutable choices and retained open files.
    let scheduler = upload_scheduler(state);
    let _dispatch = scheduler.lock_dispatch()?;
    let _credentials = oauth_replacement::lock_credentials()?;
    oauth_replacement::ensure_no_pending(state)?;
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let settings = channel(&transaction, expected)?;
    let (_, current_expiry, prior) = load_saved(&transaction, expected, review_id)?;
    if let Some(prior) = prior {
        return if prior.selected_ids == selected {
            Ok(prior.receipt)
        } else {
            Err("This retry review already has a different completed selection.".into())
        };
    }
    if current_expiry <= now() {
        return Err("This retry review expired during verification. Open a new review.".into());
    }
    let mut queued = Vec::new();
    for (original, verified) in verified {
        transaction
            .execute_batch("SAVEPOINT upload_retry_item")
            .map_err(user_error)?;
        let result = (|| {
            let current = snapshot(&transaction, &original.id, expected)?;
            if current != original {
                return Err(
                    "This upload changed during verification. Refresh before retrying.".into(),
                );
            }
            eligibility(&current, session(&original.id))?;
            recheck_original(&current, &verified)?;
            let changed=transaction.execute("UPDATE upload_items SET status='queued',detail='Retry approved after original verification. Saved provider sessions must reconcile before transfer resumes.',updated_at=?1 WHERE id=?2 AND channel_id=?3 AND status IN ('failed','needs_reconciliation') AND video_id IS NULL",params![now(),original.id,expected]).map_err(user_error)?;
            if changed != 1 {
                return Err("This upload changed before its retry was saved.".into());
            }
            // Explicitly reviewed retries may recheck the same protected
            // session; they still cannot start a second uncertain upload.
            upload_recovery::clear(&transaction, &original.id)?;
            audit(&transaction,&original.id,"upload_retry_approved","Operator reviewed the same upload failure and approved a retry after full original BLAKE3 and file identity checks. Upload identity, channel, choices and protected checkpoint were retained; provider sessions reconcile before transfer.")?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                transaction
                    .execute_batch("RELEASE upload_retry_item")
                    .map_err(user_error)?;
                queued.push(original.id);
            }
            Err(detail) => {
                transaction
                    .execute_batch("ROLLBACK TO upload_retry_item; RELEASE upload_retry_item")
                    .map_err(user_error)?;
                blocked.push(BlockedItem {
                    item_id: original.id,
                    detail,
                });
            }
        }
    }
    let _ = settings;
    queued.sort();
    blocked.sort_by(|a, b| a.item_id.cmp(&b.item_id));
    let receipt=RetryReceipt {review_id:review_id.into(),channel_id:expected.into(),queued_item_ids:queued,blocked_items:blocked,detail:"Verified uploads were returned to their existing queue records. Normal account pauses, quotas and upload admission still apply.".into()};
    let durable = SavedReceipt {
        selected_ids: selected,
        receipt: receipt.clone(),
    };
    transaction.execute("UPDATE upload_retry_reviews SET receipt_json=?1 WHERE id=?2 AND channel_id=?3 AND receipt_json IS NULL",params![serde_json::to_string(&durable).map_err(user_error)?,review_id,expected]).map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    Ok(receipt)
}

#[tauri::command]
pub(super) async fn load_upload_retry_review(
    state: State<'_, AppState>,
    expected_channel_id: String,
    anchor_item_id: String,
    all_matching: bool,
) -> Result<RetryReview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        prepare_with_sessions(
            &state,
            &expected_channel_id,
            &anchor_item_id,
            all_matching,
            session_state,
        )
    })
    .await
    .map_err(|_| "Preparing the retry review stopped unexpectedly.".to_string())?
}

#[tauri::command]
pub(super) async fn retry_upload_review(
    state: State<'_, AppState>,
    expected_channel_id: String,
    review_id: String,
    item_ids: Vec<String>,
) -> Result<RetryReceipt, String> {
    let state = state.inner().clone();
    let receipt = tauri::async_runtime::spawn_blocking(move || {
        let receipt = retry_with_sessions(
            &state,
            &expected_channel_id,
            &review_id,
            &item_ids,
            session_state,
        )?;
        if !receipt.queued_item_ids.is_empty() {
            // The durable decision and receipt have already committed. A
            // scheduler wake failure cannot turn that result into an unknown
            // approval; startup/ordinary queue wake can resume it later.
            let _ = ensure_quota_resume_worker(state.clone());
            let _ = start_queued_uploads_impl(&state);
        }
        Ok::<_, String>(receipt)
    })
    .await
    .map_err(|_| {
        "Retry verification stopped unexpectedly. Reopen the saved review for its receipt."
            .to_string()
    })??;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-retry-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
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
    fn seed(connection: &Connection, root: &Path, id: &str, status: &str, detail: &str) -> PathBuf {
        let path = root.join(format!("{id}.mp4"));
        let bytes = b"same verified original bytes";
        fs::write(&path, bytes).unwrap();
        let signature = monitored_file_signature(&fs::metadata(&path).unwrap()).unwrap();
        connection.execute("INSERT INTO upload_items(id,title,file_name,channel_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,digest,source_modified_key,background_hash_status,status,confirmed_bytes,detail,visibility,made_for_kids,playlist_id,playlist_title,delete_source_after_upload,upload_metadata_json,created_at,updated_at) VALUES(?1,?1,?2,'Fixture channel','channel-a',?3,?3,?4,?4,?5,?6,'complete',?7,0,?8,'unlisted',1,'playlist-a','Saved playlist',1,'{\"description\":\"Saved description\"}',?9,?9)",params![id,format!("{id}.mp4"),path.to_string_lossy(),bytes.len() as i64,blake3::hash(bytes).to_hex().to_string(),signature.1,status,detail,now()]).unwrap();
        path
    }
    fn prepare(state: &AppState, id: &str, all: bool) -> RetryReview {
        prepare_with_sessions(state, "channel-a", id, all, |_| SessionState::Absent).unwrap()
    }
    #[test]
    fn legacy_setup_retry_preserves_identity_original_choices_and_has_durable_replay() {
        let (root, state, connection) = fixture();
        let path = seed(
            &connection,
            &root,
            "item-a",
            "needs_reconciliation",
            LEGACY_SETUP_DETAIL,
        );
        let before: Snapshot = snapshot(&connection, "item-a", "channel-a").unwrap();
        let review = prepare(&state, "item-a", false);
        assert_eq!(review.eligible_count, 1);
        let receipt = retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["item-a".into()],
            |_| SessionState::Absent,
        )
        .unwrap();
        assert_eq!(receipt.queued_item_ids, vec!["item-a"]);
        let after = snapshot(&connection, "item-a", "channel-a").unwrap();
        assert_eq!(after.id, before.id);
        assert_eq!(after.source_path, before.source_path);
        assert_eq!(after.workspace_path, before.workspace_path);
        assert_eq!(after.choices_digest, before.choices_digest);
        assert_eq!(after.digest, before.digest);
        assert_eq!(after.confirmed_bytes, 0);
        assert_eq!(after.status, "queued");
        assert_eq!(fs::read(&path).unwrap(), b"same verified original bytes");
        assert!(!state.media_directory.exists());
        assert_eq!(
            retry_with_sessions(
                &state,
                "channel-a",
                &review.review_id,
                &["item-a".into()],
                |_| SessionState::Absent
            )
            .unwrap(),
            receipt
        );
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE kind='upload_retry_approved'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn grouping_separates_status_reason_and_channel_and_retries_only_reviewed_ids() {
        let (root, state, connection) = fixture();
        for id in [
            "item-a",
            "item-b",
            "other-reason",
            "other-status",
            "foreign",
        ] {
            seed(&connection, &root, id, "failed", LEGACY_SETUP_DETAIL);
        }
        for id in ["item-a", "item-b", "foreign"] {
            record_setup_failure(
                &connection,
                id,
                400,
                r#"{"error":{"errors":[{"reason":"invalidTitle"}]}}"#,
            )
            .unwrap();
        }
        record_setup_failure(
            &connection,
            "other-reason",
            400,
            r#"{"error":{"errors":[{"reason":"invalidTags"}]}}"#,
        )
        .unwrap();
        record_setup_failure(
            &connection,
            "other-status",
            403,
            r#"{"error":{"errors":[{"reason":"invalidTitle"}]}}"#,
        )
        .unwrap();
        connection
            .execute(
                "UPDATE upload_items SET channel_id='channel-b' WHERE id='foreign'",
                [],
            )
            .unwrap();
        let review = prepare(&state, "item-a", true);
        assert_eq!(
            review
                .items
                .iter()
                .map(|i| i.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["item-a", "item-b"]
        );
        assert!(retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["other-reason".into()],
            |_| SessionState::Absent
        )
        .is_err());
        let receipt = retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["item-b".into()],
            |_| SessionState::Absent,
        )
        .unwrap();
        assert_eq!(receipt.queued_item_ids, vec!["item-b"]);
        assert_eq!(
            snapshot(&connection, "item-a", "channel-a").unwrap().status,
            "failed"
        );
    }
    #[test]
    fn current_unsafe_sources_and_provider_outcomes_are_blocked_independently() {
        let (root, state, connection) = fixture();
        for id in [
            "good",
            "missing",
            "changed",
            "has-video",
            "progress",
            "store-denied",
            "running",
            "all-bytes",
        ] {
            seed(&connection, &root, id, "failed", LEGACY_SETUP_DETAIL);
        }
        fs::remove_file(root.join("missing.mp4")).unwrap();
        fs::write(root.join("changed.mp4"), b"different bytes").unwrap();
        connection
            .execute(
                "UPDATE upload_items SET video_id='saved-video' WHERE id='has-video'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE upload_items SET confirmed_bytes=3 WHERE id='progress'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE upload_items SET confirmed_bytes=total_bytes WHERE id='all-bytes'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE upload_items SET background_hash_status='running' WHERE id='running'",
                [],
            )
            .unwrap();
        let review = prepare_with_sessions(&state, "channel-a", "good", true, |id| {
            if id == "store-denied" {
                SessionState::Unavailable
            } else {
                SessionState::Absent
            }
        })
        .unwrap();
        assert_eq!(review.eligible_count, 1);
        assert_eq!(review.blocked_count, 7);
        let ids = review
            .items
            .iter()
            .map(|i| i.item_id.clone())
            .collect::<Vec<_>>();
        let receipt = retry_with_sessions(&state, "channel-a", &review.review_id, &ids, |id| {
            if id == "store-denied" {
                SessionState::Unavailable
            } else {
                SessionState::Absent
            }
        })
        .unwrap();
        assert_eq!(receipt.queued_item_ids, vec!["good"]);
        assert_eq!(receipt.blocked_items.len(), 7);
    }
    #[test]
    fn protected_session_can_resume_partial_bytes_but_is_never_removed_or_reset() {
        let (root, state, connection) = fixture();
        seed(
            &connection,
            &root,
            "partial",
            "needs_reconciliation",
            "YouTube rejected an upload chunk; the session was preserved for reconciliation.",
        );
        connection.execute("UPDATE upload_items SET confirmed_bytes=3,resumable_session_uri='protected-reference' WHERE id='partial'",[]).unwrap();
        let review = prepare_with_sessions(&state, "channel-a", "partial", false, |_| {
            SessionState::Present
        })
        .unwrap();
        assert_eq!(review.eligible_count, 1);
        let receipt = retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["partial".into()],
            |_| SessionState::Present,
        )
        .unwrap();
        assert_eq!(receipt.queued_item_ids, vec!["partial"]);
        let retained: (i64, String) = connection
            .query_row(
                "SELECT confirmed_bytes,resumable_session_uri FROM upload_items WHERE id='partial'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(retained, (3, "protected-reference".into()));
    }
    #[test]
    fn session_disappearing_or_original_mutation_after_review_leaves_partial_receipt() {
        let (root, state, connection) = fixture();
        for id in ["good", "mutated", "partial"] {
            seed(&connection, &root, id, "failed", LEGACY_SETUP_DETAIL);
        }
        connection
            .execute(
                "UPDATE upload_items SET confirmed_bytes=3 WHERE id='partial'",
                [],
            )
            .unwrap();
        let review = prepare_with_sessions(&state, "channel-a", "good", true, |id| {
            if id == "partial" {
                SessionState::Present
            } else {
                SessionState::Absent
            }
        })
        .unwrap();
        fs::write(root.join("mutated.mp4"), b"new").unwrap();
        let receipt = retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["good".into(), "mutated".into(), "partial".into()],
            |_| SessionState::Absent,
        )
        .unwrap();
        assert_eq!(receipt.queued_item_ids, vec!["good"]);
        assert_eq!(receipt.blocked_items.len(), 2);
    }
    #[test]
    fn stale_channel_stale_metadata_expiry_and_replayed_selection_are_rejected() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "item-a", "failed", LEGACY_SETUP_DETAIL);
        let review = prepare(&state, "item-a", false);
        connection
            .execute(
                "UPDATE upload_items SET made_for_kids=0 WHERE id='item-a'",
                [],
            )
            .unwrap();
        let receipt = retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["item-a".into()],
            |_| SessionState::Absent,
        )
        .unwrap();
        assert!(receipt.queued_item_ids.is_empty());
        assert_eq!(receipt.blocked_items.len(), 1);
        let review = prepare(&state, "item-a", false);
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        assert!(retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["item-a".into()],
            |_| SessionState::Absent
        )
        .is_err());
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-a'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE upload_retry_reviews SET expires_at='2000-01-01T00:00:00Z' WHERE id=?1",
                [&review.review_id],
            )
            .unwrap();
        assert!(retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["item-a".into()],
            |_| SessionState::Absent
        )
        .is_err());
    }
    #[test]
    fn setup_failures_are_redacted_sorted_and_do_not_collapse_unknown_reasons() {
        let (root, state, connection) = fixture();
        for id in ["item-a", "item-b", "item-c"] {
            seed(&connection, &root, id, "failed", LEGACY_SETUP_DETAIL);
        }
        let raw = r#"{"error":{"message":"secret path C:/private?access_token=token","errors":[{"reason":"invalidTags"},{"reason":"invalidTitle"}]}}"#;
        record_setup_failure(&connection, "item-a", 400, raw).unwrap();
        record_setup_failure(
            &connection,
            "item-b",
            400,
            r#"{"error":{"errors":[{"reason":"invalidTitle"},{"reason":"invalidTags"}]}}"#,
        )
        .unwrap();
        assert_eq!(
            failure_key(&connection, "item-a").unwrap(),
            failure_key(&connection, "item-b").unwrap()
        );
        let review = prepare(&state, "item-a", true);
        let json = serde_json::to_string(&review).unwrap();
        assert!(!json.contains("private"));
        assert!(!json.contains("access_token"));
        assert!(!json.contains("secret"));
        assert_eq!(review.items.len(), 2);
        let key = setup_reason_key(
            "item-a",
            r#"{"error":{"errors":[{"reason":"sensitive-token-value"}]}}"#,
        );
        assert!(key.starts_with("other-"));
        assert!(!key.contains("sensitive"));
        assert_ne!(
            setup_reason_key("item-a", "bad JSON"),
            setup_reason_key("item-b", "bad JSON")
        );
        record_setup_failure(
            &connection,
            "item-c",
            503,
            r#"{"error":{"errors":[{"reason":"backendError"}]}}"#,
        )
        .unwrap();
        assert!(!definitive_setup_rejection(&connection, "item-c").unwrap());
        assert_eq!(prepare(&state, "item-c", false).eligible_count, 0);
        assert!(definitive_setup_rejection(&connection, "item-a").unwrap());
    }
    #[test]
    fn changes_during_hash_and_checkpoint_loss_at_final_claim_are_blocked() {
        let (root, state, connection) = fixture();
        let path = seed(&connection, &root, "item-a", "failed", LEGACY_SETUP_DETAIL);
        let item = snapshot(&connection, "item-a", "channel-a").unwrap();
        assert!(
            verify_original(&item, || fs::write(&path, b"changed while hashing")
                .unwrap())
            .is_err()
        );
        seed(&connection, &root, "partial", "failed", LEGACY_SETUP_DETAIL);
        connection
            .execute(
                "UPDATE upload_items SET confirmed_bytes=3 WHERE id='partial'",
                [],
            )
            .unwrap();
        let review = prepare_with_sessions(&state, "channel-a", "partial", false, |_| {
            SessionState::Present
        })
        .unwrap();
        let calls = std::cell::Cell::new(0);
        let receipt = retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["partial".into()],
            |_| {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    SessionState::Present
                } else {
                    SessionState::Unavailable
                }
            },
        )
        .unwrap();
        assert!(receipt.queued_item_ids.is_empty());
        assert_eq!(receipt.blocked_items.len(), 1);
    }
    #[test]
    fn durable_reviews_survive_reopen_and_have_bounded_selection() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "item-a", "failed", LEGACY_SETUP_DETAIL);
        let review = prepare(&state, "item-a", false);
        drop(connection);
        let reopened = database(&state).unwrap();
        let (saved, _, _) = load_saved(&reopened, "channel-a", &review.review_id).unwrap();
        assert_eq!(saved.snapshots[0].id, "item-a");
        drop(reopened);
        assert!(retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["item-a".into(), "item-a".into()],
            |_| SessionState::Absent
        )
        .is_err());
        assert!(retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["foreign".into()],
            |_| SessionState::Absent
        )
        .is_err());
        assert_eq!(
            retry_with_sessions(
                &state,
                "channel-a",
                &review.review_id,
                &["item-a".into()],
                |_| SessionState::Absent
            )
            .unwrap()
            .queued_item_ids,
            vec!["item-a"]
        );
    }

    #[test]
    fn matching_limit_does_not_silently_truncate_or_claim_retry_all() {
        let (root, state, connection) = fixture();
        seed(&connection, &root, "anchor", "failed", LEGACY_SETUP_DETAIL);
        for index in 0..200 {
            connection.execute("INSERT INTO upload_items(id,title,file_name,channel_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,digest,source_modified_key,background_hash_status,status,confirmed_bytes,detail,created_at,updated_at) SELECT ?1,?1,file_name,channel_name,channel_id,source_path,workspace_path,size_bytes,total_bytes,digest,source_modified_key,background_hash_status,status,confirmed_bytes,detail,created_at,updated_at FROM upload_items WHERE id='anchor'", [format!("same-issue-{index}")]).unwrap();
        }
        let error = prepare_with_sessions(&state, "channel-a", "anchor", true, |_| {
            SessionState::Absent
        })
        .err()
        .expect("oversized group rejects");
        assert!(error.contains("More than 200"));
        let reviews: i64 = connection
            .query_row("SELECT COUNT(*) FROM upload_retry_reviews", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(reviews, 0);
        let single = prepare(&state, "anchor", false);
        assert_eq!(single.items.len(), 1);
        assert_eq!(single.items[0].item_id, "anchor");
    }

    #[test]
    fn failed_audit_rolls_back_that_item_while_other_verified_items_commit() {
        let (root, state, connection) = fixture();
        for id in ["good", "audit-fails"] {
            seed(&connection, &root, id, "failed", LEGACY_SETUP_DETAIL);
        }
        let review = prepare(&state, "good", true);
        connection.execute_batch("CREATE TRIGGER fail_retry_audit BEFORE INSERT ON audit_events WHEN NEW.kind='upload_retry_approved' AND NEW.item_id='audit-fails' BEGIN SELECT RAISE(ABORT,'Fixture audit rejection'); END;").unwrap();
        let receipt = retry_with_sessions(
            &state,
            "channel-a",
            &review.review_id,
            &["good".into(), "audit-fails".into()],
            |_| SessionState::Absent,
        )
        .unwrap();
        assert_eq!(receipt.queued_item_ids, vec!["good"]);
        assert_eq!(receipt.blocked_items.len(), 1);
        assert_eq!(
            snapshot(&connection, "audit-fails", "channel-a")
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            snapshot(&connection, "good", "channel-a").unwrap().status,
            "queued"
        );
        clear_failure(&connection, "good").unwrap();
        assert_eq!(failure_key(&connection, "good").unwrap(), None);
    }

    #[test]
    fn setup_failure_key_matches_item_dashboard_and_scoped_live_delta_then_clears_for_transfer() {
        let (root, state, connection) = fixture();
        for id in ["item-a", "foreign-item"] {
            seed(&connection, &root, id, "failed", LEGACY_SETUP_DETAIL);
        }
        connection
            .execute(
                "UPDATE upload_items SET channel_id='channel-b' WHERE id='foreign-item'",
                [],
            )
            .unwrap();
        let cursor = state_events::current_revision(&connection).unwrap();
        let body = r#"{"error":{"message":"neverExposeThisCredential C:/private-video?access_token=token","errors":[{"reason":"invalidTitle"}]}}"#;
        record_setup_failure(&connection, "item-a", 400, body).unwrap();
        record_setup_failure(&connection, "foreign-item", 400, body).unwrap();
        for id in ["item-a", "foreign-item"] {
            mark_upload_state(&state, id, "failed", 0, LEGACY_SETUP_DETAIL, None).unwrap();
        }
        let expected_key = "setup:400:invalidTitle";
        let item = find_item(&connection, "item-a").unwrap();
        assert_eq!(item.retry_failure_key.as_deref(), Some(expected_key));
        let dashboard = dashboard_snapshot_impl(&state).unwrap();
        assert_eq!(dashboard.items.len(), 1);
        assert_eq!(dashboard.items[0].id, "item-a");
        assert_eq!(
            dashboard.items[0].retry_failure_key.as_deref(),
            Some(expected_key)
        );
        let delta = state_events::load_state_changes(&connection, "channel-a", cursor).unwrap();
        let live_item = delta
            .changes
            .iter()
            .find(|change| change.surface == "upload" && change.entity_id == "item-a")
            .expect("worker state mutation emits the upload row");
        assert_eq!(
            live_item
                .payload
                .get("retryFailureKey")
                .and_then(|key| key.as_str()),
            Some(expected_key)
        );
        assert!(delta
            .changes
            .iter()
            .all(|change| change.channel_id == "channel-a"));
        assert!(!delta
            .changes
            .iter()
            .any(|change| change.entity_id == "foreign-item"));
        assert!(state_events::load_state_changes(&connection, "channel-b", cursor).is_err());
        assert!(snapshot(&connection, "foreign-item", "channel-a").is_err());
        let public_json =
            serde_json::to_string(&dashboard).unwrap() + &serde_json::to_string(&delta).unwrap();
        assert!(!public_json.contains("neverExposeThisCredential"));
        assert!(!public_json.contains("C:/private-video"));
        assert!(!public_json.contains("access_token"));

        let transfer_cursor = state_events::current_revision(&connection).unwrap();
        clear_failure(&connection, "item-a").unwrap();
        mark_upload_state(
            &state,
            "item-a",
            "needs_reconciliation",
            3,
            "YouTube rejected an upload chunk; the session was preserved for reconciliation.",
            None,
        )
        .unwrap();
        assert!(find_item(&connection, "item-a")
            .unwrap()
            .retry_failure_key
            .is_none());
        assert!(dashboard_snapshot_impl(&state).unwrap().items[0]
            .retry_failure_key
            .is_none());
        let transfer_delta =
            state_events::load_state_changes(&connection, "channel-a", transfer_cursor).unwrap();
        let live_transfer = transfer_delta
            .changes
            .iter()
            .find(|change| change.surface == "upload" && change.entity_id == "item-a")
            .unwrap();
        assert!(live_transfer
            .payload
            .get("retryFailureKey")
            .is_none_or(|key| key.is_null()));
        assert_eq!(
            live_transfer
                .payload
                .get("confirmedBytes")
                .and_then(|bytes| bytes.as_u64()),
            Some(3)
        );
    }
}
