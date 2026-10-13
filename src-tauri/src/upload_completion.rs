//! Channel-bound completion evidence and targeted follow-up repair. Uploads are never restarted here.
use super::*;
use std::io::Write;

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS upload_completion_checks (
  channel_id TEXT NOT NULL,
  item_id TEXT NOT NULL,
  processing_status TEXT NOT NULL DEFAULT 'unknown',
  processing_detail TEXT NOT NULL DEFAULT 'YouTube processing has not been checked.',
  checked_at TEXT,
  next_check_at TEXT,
  attempt_count INTEGER NOT NULL DEFAULT 0,
  repair_state TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(channel_id, item_id),
  FOREIGN KEY(item_id) REFERENCES upload_items(id)
);
CREATE INDEX IF NOT EXISTS upload_completion_pending ON upload_completion_checks(channel_id, processing_status, next_check_at);
"#;
const MAX_SELECTED: usize = 200;
const MAX_PROCESSING: usize = 50;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Step {
    key: String,
    outcome: String,
    detail: String,
    repairable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    checked_at: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    repair_job_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    publication_snapshot: Option<PublicationSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicationSnapshot {
    publish_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ReviewedJob {
    key: String,
    #[serde(default)]
    job_ids: Vec<String>,
    publish_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Item {
    item_id: String,
    channel_id: String,
    title: String,
    file_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    video_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_title: Option<String>,
    updated_at: String,
    steps: Vec<Step>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Page {
    channel_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    upload_batch_id: Option<String>,
    items: Vec<Item>,
    total: usize,
    pending_processing: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ExportReceipt {
    count: usize,
    format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    upload_batch_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BatchSummary {
    batch_id: String,
    created_at: String,
    item_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BatchPage {
    channel_id: String,
    total_batches: usize,
    batches: Vec<BatchSummary>,
    offset: usize,
    limit: usize,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn selected_ids(ids: Option<Vec<String>>) -> Result<Option<Vec<String>>, String> {
    let Some(ids) = ids else { return Ok(None) };
    if ids.is_empty() || ids.len() > MAX_SELECTED || ids.iter().any(|id| !valid_id(id)) {
        return Err("Choose between 1 and 200 saved uploads for this completion report.".into());
    }
    let mut seen = HashSet::new();
    Ok(Some(
        ids.into_iter()
            .filter(|id| seen.insert(id.clone()))
            .collect(),
    ))
}

fn ensure_channel(state: &AppState, channel: &str) -> Result<(), String> {
    if !valid_id(channel) {
        return Err("Choose a valid connected channel.".into());
    }
    let connection = database(state)?;
    let settings = connection_settings(&connection)?;
    if !settings.connected || settings.active_channel_id.as_deref() != Some(channel) {
        return Err("The connected channel changed. Reopen its completion checklist.".into());
    }
    Ok(())
}

fn step(
    key: &str,
    outcome: &str,
    detail: &str,
    repairable: bool,
    checked_at: Option<String>,
) -> Step {
    Step {
        key: key.into(),
        outcome: outcome.into(),
        detail: detail.into(),
        repairable,
        checked_at,
        repair_job_ids: Vec::new(),
        publication_snapshot: None,
    }
}

fn safe_retained_reason(detail: Option<&str>) -> Option<&'static str> {
    match detail {
        Some("Original source was retained because the verified source path or digest is unavailable.") => Some("The original was retained because its saved locator or trusted digest is unavailable."),
        Some("Original source was retained because a linked path cannot be cleaned up automatically.") => Some("The original was retained because the location contains a link or reparse point."),
        Some("Original source was retained because it is no longer available at the saved location.") => Some("The original is unavailable at its saved location. Locate the verified original before cleanup."),
        Some("The original is unavailable at its saved location. No local deletion was confirmed; the YouTube video was retained." | "The original became unavailable before deletion. No local deletion was confirmed; the YouTube video was retained.") => Some("The original is unavailable at its saved location. No local deletion was confirmed; locate the verified original before cleanup."),
        Some("Original source was retained because the cleanup target was not a verified external file.") => Some("The original was retained because this target is not a verified external file."),
        Some("Original source was retained because it changed after import.") => Some("The original was retained because its current bytes differ from the trusted uploaded content."),
        _ => None,
    }
}

fn cleanup_step(
    status: Option<&str>,
    requested: bool,
    uploaded: bool,
    processing: &str,
    retained_detail: Option<&str>,
) -> Step {
    let (outcome, detail) = match status {
        Some("deleted") => ("succeeded", "The unchanged original was deleted after verified YouTube processing."),
        Some("retained") => ("failed", "The original was retained because its file identity, availability or delete checks did not pass."),
        Some("retained_youtube_processing_failed") => ("failed", "The original was retained because YouTube processing failed or was abandoned."),
        Some("retained_youtube_upload_failed") => ("failed", "The original was retained because YouTube reported upload failure."),
        Some("retained_youtube_upload_rejected") => ("failed", "The original was retained because YouTube rejected the video."),
        Some("pending" | "waiting_for_youtube_processing" | "processing_verified") => ("waiting", "The original is retained until processing and source-integrity checks permit cleanup."),
        _ if !requested => ("not_requested", "Keeping the original was selected for this upload."),
        _ => ("unknown", "No confirmed original-source cleanup receipt is available."),
    };
    let detail = if status == Some("retained") {
        safe_retained_reason(retained_detail).unwrap_or(detail)
    } else {
        detail
    };
    step(
        "source_cleanup",
        outcome,
        detail,
        requested
            && uploaded
            && processing == "succeeded"
            && matches!(status, Some("retained" | "processing_verified")),
        None,
    )
}

fn load_items(
    connection: &Connection,
    channel: &str,
    selected: Option<&[String]>,
    offset: usize,
    limit: usize,
) -> Result<Page, String> {
    load_items_for_scope(connection, channel, selected, None, offset, limit)
}

fn validate_batch_scope(
    connection: &Connection,
    channel: &str,
    selected: Option<&[String]>,
    upload_batch_id: Option<&str>,
) -> Result<(), String> {
    let Some(batch_id) = upload_batch_id else {
        return Ok(());
    };
    if selected.is_some() {
        return Err("Choose either a saved batch or individual uploads for this report, then review that scope again.".into());
    }
    if !valid_id(batch_id) || !connection.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE channel_id=?1 AND upload_batch_id=?2)", params![channel,batch_id], |row| row.get::<_,bool>(0)).map_err(user_error)? {
        return Err("This saved batch is unavailable for the connected channel. Refresh the batch selection.".into());
    }
    Ok(())
}

fn load_items_for_scope(
    connection: &Connection,
    channel: &str,
    selected: Option<&[String]>,
    upload_batch_id: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Page, String> {
    validate_batch_scope(connection, channel, selected, upload_batch_id)?;
    let mut values = vec![channel.to_string()];
    let mut selection_sql = String::new();
    if let Some(ids) = selected {
        let placeholders = ids
            .iter()
            .enumerate()
            .map(|(index, _)| format!("?{}", index + 2))
            .collect::<Vec<_>>()
            .join(",");
        selection_sql = format!(" AND u.id IN ({placeholders})");
        values.extend(ids.iter().cloned());
    }
    if let Some(batch_id) = upload_batch_id {
        selection_sql = format!(" AND u.upload_batch_id=?{}", values.len() + 1);
        values.push(batch_id.to_string());
    }
    let from = format!("FROM upload_items u LEFT JOIN upload_completion_checks c ON c.item_id=u.id AND c.channel_id=u.channel_id WHERE u.channel_id=?1{selection_sql}");
    let (total, pending_processing): (usize,usize) = connection.query_row(&format!("SELECT COUNT(*),COALESCE(SUM(CASE WHEN u.status='uploaded' AND u.video_id IS NOT NULL AND COALESCE(c.processing_status,'unknown') IN ('waiting','unknown') THEN 1 ELSE 0 END),0) {from}"),rusqlite::params_from_iter(&values),|row|Ok((row.get(0)?,row.get(1)?))).map_err(user_error)?;
    if selected.is_some_and(|ids| ids.len() != total) {
        return Err("One selected upload is unavailable or belongs to another channel. Review the selection again.".into());
    }
    let mut statement = connection.prepare(&format!("SELECT u.id,u.channel_id,u.title,u.file_name,u.video_id,u.playlist_id,u.playlist_title,u.updated_at,u.status,u.playlist_status,u.delete_source_after_upload,u.source_delete_status,COALESCE(c.processing_status,'unknown'),COALESCE(c.processing_detail,'YouTube processing has not been checked.'),c.checked_at,c.repair_state,u.detail,(SELECT a.detail FROM audit_events a WHERE a.item_id=u.id AND a.channel_id=u.channel_id AND a.kind IN ('source_cleanup_retained','source_cleanup_unavailable') ORDER BY a.created_at DESC,a.rowid DESC LIMIT 1) {from} ORDER BY u.created_at DESC,u.id LIMIT {} OFFSET {}",limit.min(10000),offset.min(100000))).map_err(user_error)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(&values), |row| {
            Ok((
                Item {
                    item_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    title: row.get(2)?,
                    file_name: row.get(3)?,
                    video_id: row.get(4)?,
                    playlist_id: row.get(5)?,
                    playlist_title: row.get(6)?,
                    updated_at: row.get(7)?,
                    steps: Vec::new(),
                },
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, bool>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, Option<String>>(14)?,
                row.get::<_, Option<String>>(15)?,
                row.get::<_, Option<String>>(16)?,
                row.get::<_, Option<String>>(17)?,
            ))
        })
        .map_err(user_error)?;
    let mut items = Vec::new();
    for row in rows {
        let (
            mut item,
            status,
            playlist_status,
            cleanup_requested,
            cleanup_status,
            processing,
            processing_detail,
            checked_at,
            repair_state,
            item_detail,
            retained_detail,
        ) = row.map_err(user_error)?;
        let uploaded = status == "uploaded" && item.video_id.as_deref().is_some_and(valid_id);
        let (upload_outcome, upload_detail) = if uploaded {
            (
                "succeeded",
                "YouTube upload receipt is confirmed; follow-up steps are independent.",
            )
        } else {
            match status.as_str() {
                "failed" | "cancelled" => (
                    "failed",
                    "Upload did not complete. Follow-up repair cannot restart it.",
                ),
                "needs_reconciliation" => (
                    "unknown",
                    "Upload identity needs reconciliation; follow-up writes are blocked.",
                ),
                _ => ("waiting", "The existing upload has not yet completed."),
            }
        };
        item.steps
            .push(step("upload", upload_outcome, upload_detail, false, None));
        item.steps.push(step(
            "processing",
            if uploaded { &processing } else { "waiting" },
            if uploaded {
                &processing_detail
            } else {
                "Available after a confirmed video receipt."
            },
            false,
            checked_at,
        ));
        let (playlist_outcome, playlist_detail) = if item.playlist_id.is_none() {
            ("not_requested", "No playlist was selected for this upload.")
        } else if repair_state.as_deref() == Some("playlist")
            || playlist_status == "needs_reconciliation"
        {
            ("unknown","A playlist write may have completed. Repair checks current membership before any insertion.")
        } else {
            match playlist_status.as_str(){"complete"=>("succeeded","YouTube confirmed the requested playlist step."),"failed"=>("failed","The playlist step failed. The confirmed upload and original video ID remain intact."),"pending" if item_detail.as_deref().is_some_and(|detail| detail.contains("waiting for playlist-management permission"))=>("waiting","The uploaded video is retained. This playlist step waits for separately granted management permission and its temporary mode."),"pending"=>("waiting","The selected playlist step is pending."),_=>("unknown","No confirmed playlist receipt is available.")}
        };
        item.steps.push(step(
            "playlist",
            playlist_outcome,
            playlist_detail,
            uploaded && matches!(playlist_outcome, "failed" | "unknown"),
            None,
        ));
        item.steps.push(cleanup_step(
            cleanup_status.as_deref(),
            cleanup_requested,
            uploaded,
            &processing,
            retained_detail.as_deref(),
        ));
        for asset in video_assets::completion_steps(connection, channel, &item.item_id)? {
            let mut asset_step = step(
                &asset.key,
                &asset.outcome,
                &asset.detail,
                uploaded && asset.repairable,
                asset.checked_at,
            );
            if asset_step.repairable {
                asset_step.repair_job_ids = asset.repair_job_ids;
            }
            item.steps.push(asset_step);
        }
        if let Some(publication) =
            publication_schedule::completion_snapshot(connection, &item.item_id, channel)?
        {
            let mut publication_step = step(
                "publication",
                &publication.outcome,
                &publication.detail,
                uploaded && publication.repairable,
                publication.checked_at,
            );
            publication_step.publication_snapshot = Some(PublicationSnapshot {
                publish_at: publication.publish_at,
                updated_at: publication.updated_at,
            });
            item.steps.push(publication_step);
        } else {
            item.steps.push(step(
                "publication",
                "not_requested",
                "No publication schedule was requested for this upload.",
                false,
                None,
            ));
        }
        items.push(item);
    }
    Ok(Page {
        channel_id: channel.into(),
        upload_batch_id: upload_batch_id.map(str::to_string),
        items,
        total,
        pending_processing,
    })
}

fn load_batch_summaries(
    connection: &Connection,
    channel: &str,
    offset: usize,
    limit: usize,
) -> Result<BatchPage, String> {
    let offset = offset.min(100000);
    let limit = limit.clamp(1, 100);
    let total_batches = connection.query_row("SELECT COUNT(DISTINCT upload_batch_id) FROM upload_items WHERE channel_id=?1 AND upload_batch_id IS NOT NULL AND upload_batch_id!=''", [channel], |row| row.get::<_,usize>(0)).map_err(user_error)?;
    let batches = connection.prepare("SELECT upload_batch_id,MIN(created_at) AS batch_created_at,COUNT(*),(SELECT title FROM upload_items first WHERE first.channel_id=?1 AND first.upload_batch_id=u.upload_batch_id ORDER BY first.created_at,first.id LIMIT 1) FROM upload_items u WHERE u.channel_id=?1 AND u.upload_batch_id IS NOT NULL AND u.upload_batch_id!='' GROUP BY u.upload_batch_id ORDER BY batch_created_at DESC,u.upload_batch_id LIMIT ?2 OFFSET ?3").map_err(user_error)?.query_map(params![channel,limit,offset], |row| Ok(BatchSummary { batch_id:row.get(0)?,created_at:row.get(1)?,item_count:row.get(2)?,title:row.get(3)? })).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    Ok(BatchPage {
        channel_id: channel.into(),
        total_batches,
        batches,
        offset,
        limit,
    })
}

fn processing_outcome(video: &serde_json::Value) -> (&'static str, &'static str) {
    match source_cleanup_processing(video) {
        SourceCleanupProcessing::Succeeded => (
            "succeeded",
            "YouTube owner-authorized processing details confirm success.",
        ),
        SourceCleanupProcessing::RetainedProcessingFailed => (
            "failed",
            "YouTube reported failed or abandoned processing. Original files are retained.",
        ),
        SourceCleanupProcessing::RetainedUploadFailed => (
            "failed",
            "YouTube reported upload failure. Original files are retained.",
        ),
        SourceCleanupProcessing::RetainedUploadRejected => (
            "failed",
            "YouTube rejected the video. Original files are retained.",
        ),
        SourceCleanupProcessing::Waiting => {
            if video
                .pointer("/processingDetails/processingStatus")
                .and_then(|value| value.as_str())
                == Some("processing")
            {
                ("waiting", "YouTube is still processing this video.")
            } else {
                (
                    "unknown",
                    "YouTube did not provide a terminal owner-authorized processing result.",
                )
            }
        }
    }
}

/// Inventory integration can reuse authenticated owner payloads without another provider read.
pub(super) fn record_processing_from_inventory(
    connection: &Connection,
    channel: &str,
    video: &serde_json::Value,
) -> Result<(), String> {
    let Some(video_id) = video
        .get("id")
        .and_then(|value| value.as_str())
        .filter(|id| valid_id(id))
    else {
        return Ok(());
    };
    if video
        .pointer("/snippet/channelId")
        .and_then(|value| value.as_str())
        != Some(channel)
    {
        return Ok(());
    }
    let (status, detail) = processing_outcome(video);
    let timestamp = now();
    let next = (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc3339();
    connection.execute("INSERT INTO upload_completion_checks(channel_id,item_id,processing_status,processing_detail,checked_at,next_check_at,attempt_count,updated_at) SELECT channel_id,id,?1,?2,?3,?4,1,?3 FROM upload_items WHERE channel_id=?5 AND video_id=?6 AND status='uploaded' ON CONFLICT(channel_id,item_id) DO UPDATE SET processing_status=excluded.processing_status,processing_detail=excluded.processing_detail,checked_at=excluded.checked_at,next_check_at=excluded.next_check_at,attempt_count=upload_completion_checks.attempt_count+1,updated_at=excluded.updated_at",params![status,detail,timestamp,next,channel,video_id]).map_err(user_error)?;
    if status == "succeeded" {
        // This advances only the original, already-approved automatic cleanup.
        // A retained/failed result requires a new explicit repair review.
        let ready = connection
            .prepare("UPDATE upload_items SET source_delete_status='processing_verified',updated_at=?1 WHERE channel_id=?2 AND channel_id=COALESCE((SELECT active_channel_id FROM connection_settings WHERE singleton=1),'') AND video_id=?3 AND status='uploaded' AND delete_source_after_upload=1 AND source_delete_status IN ('pending','waiting_for_youtube_processing') RETURNING id")
            .map_err(user_error)?
            .query_map(params![timestamp, channel, video_id], |row| row.get::<_, String>(0))
            .map_err(user_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(user_error)?;
        for item_id in ready {
            audit(connection, &item_id, "completion_cleanup_processing_verified", "Fresh owner-authorized YouTube processing confirms readiness for the saved, already-approved original cleanup; source-integrity checks still apply.")?;
        }
    }
    Ok(())
}

trait CompletionProvider {
    fn ensure(&mut self, channel: &str) -> Result<(), String>;
    fn processing(
        &mut self,
        channel: &str,
        video_ids: &[String],
    ) -> Result<serde_json::Value, String>;
    fn repair_playlist(
        &mut self,
        channel: &str,
        playlist: &str,
        video: &str,
    ) -> Result<(), (String, bool)>;
    fn repair_assets(
        &mut self,
        channel: &str,
        item: &str,
        video: &str,
        kind: &str,
        jobs: &[String],
    ) -> Result<(), String>;
    fn repair_publication(
        &mut self,
        channel: &str,
        item: &str,
        video: &str,
        snapshot: &PublicationSnapshot,
    ) -> Result<(), String>;
}

struct NativeProvider<'a> {
    state: &'a AppState,
}

fn playlist_membership(
    value: &serde_json::Value,
    playlist: &str,
    video: &str,
) -> Result<bool, String> {
    let entries = value
        .get("items")
        .and_then(|items| items.as_array())
        .ok_or("YouTube did not confirm current playlist membership.")?;
    if entries.is_empty() {
        if value
            .get("nextPageToken")
            .is_some_and(|token| !token.is_null())
        {
            return Err("YouTube returned incomplete membership; no insertion was sent.".into());
        }
        return Ok(false);
    }
    if entries.iter().all(|entry| {
        entry
            .pointer("/snippet/playlistId")
            .and_then(|id| id.as_str())
            == Some(playlist)
            && entry
                .pointer("/snippet/resourceId/videoId")
                .and_then(|id| id.as_str())
                == Some(video)
    }) {
        return Ok(true);
    }
    Err("YouTube returned inconsistent membership; no insertion was sent.".into())
}

fn persist_playlist_result(
    state: &AppState,
    item_id: &str,
    channel: &str,
    video: &str,
    playlist: &str,
    result: Result<(), (String, bool)>,
) -> Result<(), String> {
    let (status, kind, detail) = match result {
        Ok(()) => ("complete", "completion_playlist_repaired", "Current membership confirms the existing uploaded video is in its requested playlist."),
        Err((_, true)) => ("needs_reconciliation", "completion_playlist_uncertain", "The playlist write outcome is unknown; reread membership before retry."),
        Err((_, false)) => ("failed", "completion_playlist_failed", "The playlist step could not finish. The confirmed upload receipt is retained; review management permission and the saved playlist before repair."),
    };
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    channel_matches(&transaction, channel)?;
    if transaction.execute("UPDATE upload_items SET playlist_status=?1,updated_at=?2 WHERE id=?3 AND channel_id=?4 AND video_id=?5 AND playlist_id=?6 AND status='uploaded'", params![status, now(), item_id, channel, video, playlist]).map_err(user_error)? != 1 {
        return Err("The confirmed video or saved playlist changed. Refresh its retained checklist.".into());
    }
    transaction.execute("UPDATE upload_completion_checks SET repair_state=NULL,updated_at=?1 WHERE channel_id=?2 AND item_id=?3", params![now(),channel,item_id]).map_err(user_error)?;
    audit(&transaction, item_id, kind, detail)?;
    transaction.commit().map_err(user_error)
}

fn channel_matches(connection: &Connection, channel: &str) -> Result<(), String> {
    let settings = connection_settings(connection)?;
    if !settings.connected || settings.active_channel_id.as_deref() != Some(channel) {
        return Err(
            "The connected channel changed. The confirmed upload receipt remains intact.".into(),
        );
    }
    Ok(())
}

fn process_requested_playlist_with(
    state: &AppState,
    item_id: &str,
    permission_ready: bool,
    provider: &mut impl CompletionProvider,
) -> Result<(), String> {
    let mut connection = database(state)?;
    let settings = connection_settings(&connection)?;
    let Some(channel) = settings.active_channel_id.filter(|_| settings.connected) else {
        return Ok(());
    };
    let job: Option<(String, String)> = connection.query_row("SELECT video_id,playlist_id FROM upload_items WHERE id=?1 AND channel_id=?2 AND status='uploaded' AND playlist_status='pending' AND video_id IS NOT NULL AND playlist_id IS NOT NULL", params![item_id,channel], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(user_error)?;
    let Some((video, playlist)) = job else {
        return Ok(());
    };
    if !valid_id(&video) || !valid_id(&playlist) {
        return Err("The saved playlist job has invalid provider identities; the video receipt is retained.".into());
    }
    provider.ensure(&channel)?;
    if !permission_ready {
        let detail = "Uploaded video retained. The requested playlist step is waiting for playlist-management permission and its temporary mode.";
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(user_error)?;
        channel_matches(&transaction, &channel)?;
        let changed = transaction.execute("UPDATE upload_items SET detail=?1,updated_at=?2 WHERE id=?3 AND channel_id=?4 AND video_id=?5 AND playlist_id=?6 AND playlist_status='pending' AND status='uploaded' AND COALESCE(detail,'')!=?1", params![detail,now(),item_id,channel,video,playlist]).map_err(user_error)?;
        if changed != 0 {
            audit(&transaction,item_id,"completion_playlist_permission_pending","The confirmed upload is retained; playlist addition waits for separately authorized management permission.")?;
        }
        return transaction.commit().map_err(user_error);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    channel_matches(&transaction, &channel)?;
    let current: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE id=?1 AND channel_id=?2 AND video_id=?3 AND playlist_id=?4 AND status='uploaded' AND playlist_status='pending')",params![item_id,channel,video,playlist],|row|row.get(0)).map_err(user_error)?;
    if !current {
        return Err("The requested playlist job changed before it could start; review its retained receipt.".into());
    }
    transaction.execute("INSERT INTO upload_completion_checks(channel_id,item_id,repair_state,updated_at) VALUES(?1,?2,'playlist',?3) ON CONFLICT(channel_id,item_id) DO UPDATE SET repair_state='playlist',updated_at=excluded.updated_at", params![channel,item_id,now()]).map_err(user_error)?;
    audit(&transaction,item_id,"completion_playlist_started","Requested playlist follow-up checks the original channel, video and current membership before inserting.")?;
    transaction.commit().map_err(user_error)?;
    drop(connection);
    let result = provider.repair_playlist(&channel, &playlist, &video);
    provider.ensure(&channel)?;
    persist_playlist_result(state, item_id, &channel, &video, &playlist, result)
}

/// Automatic follow-up reuses the same durable, membership-first repair path.
/// Missing separate management permission leaves the requested step pending.
pub(super) fn process_requested_playlist(state: &AppState, item_id: &str) -> Result<(), String> {
    let _execution_guard = deletion_execution_lock(state)
        .lock()
        .map_err(|_| "Playlist management is unavailable.".to_string())?;
    {
        let _credential_guard = oauth_replacement::lock_credentials()?;
        oauth_replacement::ensure_no_pending(state)?;
    }
    let connection = database(state)?;
    clear_expired_deletion_authorization(&connection)?;
    let settings = connection_settings(&connection)?;
    let permitted = settings.deletion_authorized && settings.deletion_sudo_active;
    drop(connection);
    let mut provider = NativeProvider { state };
    process_requested_playlist_with(state, item_id, permitted, &mut provider)
}
impl CompletionProvider for NativeProvider<'_> {
    fn ensure(&mut self, channel: &str) -> Result<(), String> {
        ensure_channel(self.state, channel)
    }
    fn repair_assets(
        &mut self,
        channel: &str,
        item: &str,
        video: &str,
        kind: &str,
        jobs: &[String],
    ) -> Result<(), String> {
        video_assets::repair_job_ids_locked(self.state, channel, item, video, kind, jobs)
            .map(|_| ())
    }
    fn repair_publication(
        &mut self,
        channel: &str,
        item: &str,
        video: &str,
        snapshot: &PublicationSnapshot,
    ) -> Result<(), String> {
        publication_schedule::repair_completion(
            self.state,
            item,
            channel,
            video,
            &snapshot.publish_at,
            &snapshot.updated_at,
        )
    }
    fn processing(
        &mut self,
        channel: &str,
        video_ids: &[String],
    ) -> Result<serde_json::Value, String> {
        self.ensure(channel)?;
        let token = refreshed_access_token(self.state)?;
        if active_channel_id(self.state, &token)? != channel {
            return Err("Google's credential does not confirm the reviewed channel.".into());
        }
        self.ensure(channel)?;
        youtube_json(
            self.state,
            &token,
            "videos",
            &[
                ("part", "snippet,status,processingDetails"),
                ("id", &video_ids.join(",")),
            ],
        )
    }
    fn repair_playlist(
        &mut self,
        channel: &str,
        playlist: &str,
        video: &str,
    ) -> Result<(), (String, bool)> {
        let check = |state: &AppState| -> Result<(), String> {
            ensure_channel(state, channel)?;
            let connection = database(state)?;
            clear_expired_deletion_authorization(&connection)?;
            let settings = connection_settings(&connection)?;
            if !settings.deletion_authorized || !settings.deletion_sudo_active {
                return Err("Grant playlist-management permission and enter its temporary 15-minute mode before repairing a playlist.".into());
            }
            Ok(())
        };
        check(self.state).map_err(|detail| (detail, false))?;
        let token =
            refreshed_deletion_access_token(self.state).map_err(|detail| (detail, false))?;
        if active_channel_id(self.state, &token).map_err(|detail| (detail, false))? != channel {
            return Err((
                "The management credential belongs to another channel.".into(),
                false,
            ));
        }
        let playlist_value = youtube_json(
            self.state,
            &token,
            "playlists",
            &[("part", "snippet"), ("id", playlist)],
        )
        .map_err(|detail| (detail, false))?;
        let owned = playlist_value
            .get("items")
            .and_then(|v| v.as_array())
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("id").and_then(|v| v.as_str()) == Some(playlist)
                        && item.pointer("/snippet/channelId").and_then(|v| v.as_str())
                            == Some(channel)
                })
            });
        if !owned {
            return Err((
                "The saved playlist is unavailable or belongs to another channel.".into(),
                false,
            ));
        }
        let video_value = youtube_json(
            self.state,
            &token,
            "videos",
            &[("part", "snippet"), ("id", video)],
        )
        .map_err(|detail| (detail, false))?;
        if !video_value
            .get("items")
            .and_then(|v| v.as_array())
            .is_some_and(|items| {
                items.iter().any(|item| {
                    item.get("id").and_then(|v| v.as_str()) == Some(video)
                        && item.pointer("/snippet/channelId").and_then(|v| v.as_str())
                            == Some(channel)
                })
            })
        {
            return Err((
                "YouTube did not confirm the saved video on the reviewed channel.".into(),
                false,
            ));
        }
        // The videoId filter provides a bounded, targeted membership read rather than a full playlist crawl.
        check(self.state).map_err(|detail| (detail, false))?;
        let membership = youtube_json(
            self.state,
            &token,
            "playlistItems",
            &[
                ("part", "snippet"),
                ("playlistId", playlist),
                ("videoId", video),
                ("maxResults", "50"),
            ],
        )
        .map_err(|detail| (detail, false))?;
        if playlist_membership(&membership, playlist, video).map_err(|detail| (detail, false))? {
            return Ok(());
        }
        check(self.state).map_err(|detail| (detail, false))?;
        let quota_scope = database(self.state)
            .and_then(|connection| oauth_replacement::configured_quota_scope(&connection))
            .map_err(|detail| (detail, false))?;
        let request=provider_transport(self.state).control_client().map_err(|detail|(detail,false))?.post("https://www.googleapis.com/youtube/v3/playlistItems").query(&[("part","id,snippet")]).bearer_auth(token).json(&serde_json::json!({"snippet":{"playlistId":playlist,"resourceId":{"kind":"youtube#video","videoId":video}}}));
        let response = observed_youtube_request(self.state, "playlistItems.insert", || request.send()).map_err(|_|("Playlist addition was not confirmed. Repair will reread membership before any retry.".into(),true))?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let uncertain = status == 408 || status >= 500;
            let mut body = String::new();
            let _ = response.take(64 * 1024).read_to_string(&mut body);
            if youtube_limits::classify(status, &body)
                == Some(youtube_limits::UploadLimitKind::ApiQuota)
            {
                let _ = oauth_replacement::record_scoped_api_pause(
                    self.state,
                    channel,
                    &quota_scope,
                    None,
                    youtube_limits::includes_channel_limit(status, &body),
                );
            }
            return Err((
                "YouTube did not confirm this playlist addition. The uploaded video was retained."
                    .into(),
                uncertain,
            ));
        }
        let body: serde_json::Value = response.json().map_err(|_| {
            (
                "Playlist addition returned an unreadable receipt; reread membership before retry."
                    .into(),
                true,
            )
        })?;
        if body
            .get("id")
            .and_then(|v| v.as_str())
            .is_none_or(|v| v.is_empty())
            || body.pointer("/snippet/playlistId").and_then(|v| v.as_str()) != Some(playlist)
            || body
                .pointer("/snippet/resourceId/videoId")
                .and_then(|v| v.as_str())
                != Some(video)
        {
            return Err((
                "Playlist addition returned an incomplete receipt; reread membership before retry."
                    .into(),
                true,
            ));
        }
        Ok(())
    }
}

fn refresh_with(
    state: &AppState,
    channel: &str,
    ids: &[String],
    provider: &mut impl CompletionProvider,
) -> Result<(), String> {
    provider.ensure(channel)?;
    let connection = database(state)?;
    let page = load_items(&connection, channel, Some(ids), 0, MAX_SELECTED)?;
    let mut due = Vec::new();
    for item in &page.items {
        let pending = item.steps.iter().any(|step| {
            step.key == "processing" && matches!(step.outcome.as_str(), "waiting" | "unknown")
        });
        if !pending || item.steps[0].outcome != "succeeded" {
            continue;
        }
        let next:Option<String>=connection.query_row("SELECT next_check_at FROM upload_completion_checks WHERE channel_id=?1 AND item_id=?2",params![channel,item.item_id],|row|row.get(0)).optional().map_err(user_error)?.flatten();
        if next
            .as_deref()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .is_some_and(|v| v > chrono::Utc::now())
        {
            continue;
        }
        due.push(item.video_id.clone().unwrap());
    }
    due.sort();
    due.dedup();
    due.truncate(MAX_PROCESSING);
    if due.is_empty() {
        return Ok(());
    }
    let next = (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc3339();
    for video_id in &due {
        connection.execute("INSERT INTO upload_completion_checks(channel_id,item_id,next_check_at,updated_at) SELECT channel_id,id,?1,?2 FROM upload_items WHERE channel_id=?3 AND video_id=?4 AND status='uploaded' ON CONFLICT(channel_id,item_id) DO UPDATE SET next_check_at=excluded.next_check_at,updated_at=excluded.updated_at",params![next,now(),channel,video_id]).map_err(user_error)?;
    }
    drop(connection);
    let response = provider.processing(channel, &due)?;
    provider.ensure(channel)?;
    let returned = response
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or("YouTube returned unreadable processing evidence.")?;
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let next = (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc3339();
    for video_id in &due {
        let video = returned.iter().find(|v| {
            v.get("id").and_then(|id| id.as_str()) == Some(video_id.as_str())
                && v.pointer("/snippet/channelId").and_then(|id| id.as_str()) == Some(channel)
        });
        if let Some(video) = video {
            record_processing_from_inventory(&transaction, channel, video)?;
        } else {
            transaction.execute("INSERT INTO upload_completion_checks(channel_id,item_id,processing_status,processing_detail,checked_at,next_check_at,attempt_count,updated_at) SELECT channel_id,id,'unknown','YouTube did not confirm this video on its saved channel. Follow-up writes remain blocked.',?1,?2,1,?1 FROM upload_items WHERE channel_id=?3 AND video_id=?4 AND status='uploaded' ON CONFLICT(channel_id,item_id) DO UPDATE SET processing_status='unknown',processing_detail=excluded.processing_detail,checked_at=excluded.checked_at,next_check_at=excluded.next_check_at,attempt_count=upload_completion_checks.attempt_count+1,updated_at=excluded.updated_at",params![now(),next,channel,video_id]).map_err(user_error)?;
        }
    }
    transaction.commit().map_err(user_error)
}

#[cfg(test)]
fn repair_with(
    state: &AppState,
    channel: &str,
    item_id: &str,
    video_id: &str,
    steps: &[String],
    confirmation: Option<&str>,
    provider: &mut impl CompletionProvider,
) -> Result<Item, String> {
    repair_reviewed_with(
        state,
        channel,
        item_id,
        video_id,
        steps,
        confirmation,
        &[],
        provider,
    )
}

fn validate_reviewed_jobs(
    connection: &Connection,
    item: &Item,
    video_id: &str,
    steps: &[String],
    reviewed_jobs: &[ReviewedJob],
) -> Result<(), String> {
    let job_steps = steps
        .iter()
        .filter(|key| matches!(key.as_str(), "thumbnail" | "caption" | "publication"))
        .collect::<Vec<_>>();
    if reviewed_jobs.len() != job_steps.len()
        || reviewed_jobs
            .iter()
            .map(|job| &job.key)
            .collect::<HashSet<_>>()
            .len()
            != reviewed_jobs.len()
        || reviewed_jobs
            .iter()
            .any(|job| !job_steps.iter().any(|key| key.as_str() == job.key))
    {
        return Err(
            "Review the exact saved attachment and publication jobs for these selected repairs."
                .into(),
        );
    }
    for reviewed in reviewed_jobs {
        let current = item
            .steps
            .iter()
            .find(|step| step.key == reviewed.key && step.repairable)
            .ok_or("This reviewed job is no longer eligible for repair. Refresh the checklist.")?;
        if reviewed.key == "publication" {
            let snapshot = current
                .publication_snapshot
                .as_ref()
                .ok_or("The reviewed publication snapshot is unavailable.")?;
            if !reviewed.job_ids.is_empty()
                || reviewed.publish_at.as_deref() != Some(snapshot.publish_at.as_str())
                || reviewed.updated_at.as_deref() != Some(snapshot.updated_at.as_str())
            {
                return Err("The saved publication time changed after review. Refresh the checklist before repairing.".into());
            }
            let matches = connection.query_row("SELECT EXISTS(SELECT 1 FROM publication_schedule_jobs WHERE item_id=?1 AND channel_id=?2 AND video_id=?3 AND requested_publish_at=?4 AND updated_at=?5 AND ever_public=0 AND state IN ('needs_attention','uncertain','waiting_permission'))", params![item.item_id,item.channel_id,video_id,snapshot.publish_at,snapshot.updated_at], |row| row.get::<_,bool>(0)).map_err(user_error)?;
            if !matches {
                return Err("The reviewed publication job no longer belongs to this confirmed video. Refresh its checklist.".into());
            }
        } else {
            if reviewed.publish_at.is_some()
                || reviewed.updated_at.is_some()
                || reviewed.job_ids.is_empty()
                || reviewed.job_ids.len() > 400
                || reviewed
                    .job_ids
                    .iter()
                    .any(|id| !valid_id(id) || !current.repair_job_ids.contains(id))
                || reviewed.job_ids.iter().collect::<HashSet<_>>().len() != reviewed.job_ids.len()
            {
                return Err("The reviewed attachment jobs changed. Refresh their checklist before repairing.".into());
            }
            // Validate every requested receipt before any other selected step
            // can write. New jobs never join a reviewed family implicitly.
            for job_id in &reviewed.job_ids {
                let matches = connection.query_row("SELECT EXISTS(SELECT 1 FROM video_asset_jobs WHERE id=?1 AND channel_id=?2 AND item_id=?3 AND kind=?4 AND video_id=?5 AND status IN ('failed','needs_reconciliation','waiting_permission','waiting_processing'))", params![job_id,item.channel_id,item.item_id,reviewed.key,video_id], |row| row.get::<_, bool>(0)).map_err(user_error)?;
                if !matches {
                    return Err("One reviewed attachment no longer belongs to this confirmed video. Refresh its checklist.".into());
                }
            }
        }
    }
    Ok(())
}

fn repair_reviewed_with(
    state: &AppState,
    channel: &str,
    item_id: &str,
    video_id: &str,
    steps: &[String],
    confirmation: Option<&str>,
    reviewed_jobs: &[ReviewedJob],
    provider: &mut impl CompletionProvider,
) -> Result<Item, String> {
    ensure_channel(state, channel)?;
    provider.ensure(channel)?;
    if !valid_id(item_id)
        || !valid_id(video_id)
        || steps.is_empty()
        || steps.len() > 5
        || steps.iter().any(|s| {
            !matches!(
                s.as_str(),
                "playlist" | "source_cleanup" | "thumbnail" | "caption" | "publication"
            )
        })
        || steps.iter().collect::<HashSet<_>>().len() != steps.len()
    {
        return Err("Choose only the failed follow-up steps to repair.".into());
    }
    let connection = database(state)?;
    let item = load_items(&connection, channel, Some(&[item_id.into()]), 0, 1)?
        .items
        .remove(0);
    if item.video_id.as_deref() != Some(video_id) || item.steps[0].outcome != "succeeded" {
        return Err("The saved upload receipt changed. Review the confirmed video again.".into());
    }
    for key in steps {
        if !item
            .steps
            .iter()
            .any(|step| &step.key == key && step.repairable)
        {
            return Err("One selected follow-up step is no longer eligible for repair. Refresh the checklist.".into());
        }
    }
    if steps.iter().any(|key| key == "source_cleanup")
        && confirmation != Some(item.file_name.as_str())
    {
        return Err("Type the exact original filename to approve source cleanup.".into());
    }
    validate_reviewed_jobs(&connection, &item, video_id, steps, reviewed_jobs)?;
    drop(connection);
    for key in steps {
        provider.ensure(channel)?;
        let mut connection = database(state)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(user_error)?;
        channel_matches(&transaction, channel)?;
        let current: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE id=?1 AND channel_id=?2 AND video_id=?3 AND file_name=?4 AND status='uploaded' AND playlist_id IS ?5)",params![item_id,channel,video_id,item.file_name,item.playlist_id],|row|row.get(0)).map_err(user_error)?;
        if !current {
            return Err("The reviewed upload identity changed before repair; refresh its retained checklist.".into());
        }
        transaction.execute("INSERT INTO upload_completion_checks(channel_id,item_id,repair_state,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(channel_id,item_id) DO UPDATE SET repair_state=excluded.repair_state,updated_at=excluded.updated_at",params![channel,item_id,key,now()]).map_err(user_error)?;
        audit(
            &transaction,
            item_id,
            "completion_repair_reviewed",
            "Operator reviewed targeted follow-up repair for the existing confirmed video.",
        )?;
        transaction.commit().map_err(user_error)?;
        drop(connection);
        if key == "playlist" {
            let result = provider.repair_playlist(
                channel,
                item.playlist_id
                    .as_deref()
                    .ok_or("The saved playlist is missing.")?,
                video_id,
            );
            provider.ensure(channel)?;
            persist_playlist_result(
                state,
                item_id,
                channel,
                video_id,
                item.playlist_id
                    .as_deref()
                    .ok_or("The saved playlist is missing.")?,
                result,
            )?;
        } else if key == "source_cleanup" {
            // Reconfirm the exact saved owner video immediately before the irreversible local cleanup.
            let response = provider.processing(channel, &[video_id.into()])?;
            provider.ensure(channel)?;
            let video = response
                .get("items")
                .and_then(|v| v.as_array())
                .and_then(|items| {
                    items.iter().find(|v| {
                        v.get("id").and_then(|id| id.as_str()) == Some(video_id)
                            && v.pointer("/snippet/channelId").and_then(|id| id.as_str())
                                == Some(channel)
                    })
                })
                .ok_or(
                    "YouTube did not confirm the original uploaded video; the source was retained.",
                )?;
            if processing_outcome(video).0 != "succeeded" {
                return Err("YouTube processing is not confirmed complete. The original source was retained.".into());
            }
            let connection = database(state)?;
            record_processing_from_inventory(&connection, channel, video)?;
            connection.execute("UPDATE upload_items SET source_delete_status='processing_verified',updated_at=?1 WHERE id=?2 AND channel_id=?3 AND video_id=?4 AND status='uploaded'",params![now(),item_id,channel,video_id]).map_err(user_error)?;
            drop(connection);
            provider.ensure(channel)?;
            finalize_confirmed_source_cleanup(state, item_id)?;
            let connection = database(state)?;
            connection.execute("UPDATE upload_completion_checks SET repair_state=NULL,updated_at=?1 WHERE channel_id=?2 AND item_id=?3",params![now(),channel,item_id]).map_err(user_error)?;
        } else {
            let reviewed = reviewed_jobs
                .iter()
                .find(|job| &job.key == key)
                .ok_or("The exact reviewed job receipt is unavailable.")?;
            if key == "publication" {
                let snapshot = PublicationSnapshot {
                    publish_at: reviewed
                        .publish_at
                        .clone()
                        .ok_or("The reviewed publication time is unavailable.")?,
                    updated_at: reviewed
                        .updated_at
                        .clone()
                        .ok_or("The reviewed publication version is unavailable.")?,
                };
                provider.repair_publication(channel, item_id, video_id, &snapshot)?;
            } else {
                provider.repair_assets(channel, item_id, video_id, key, &reviewed.job_ids)?;
            }
            provider.ensure(channel)?;
            let connection = database(state)?;
            connection.execute("UPDATE upload_completion_checks SET repair_state=NULL,updated_at=?1 WHERE channel_id=?2 AND item_id=?3", params![now(),channel,item_id]).map_err(user_error)?;
        }
    }
    provider.ensure(channel)?;
    load_items(&database(state)?, channel, Some(&[item_id.into()]), 0, 1)?
        .items
        .pop()
        .ok_or("The completion item is unavailable.".into())
}

#[tauri::command]
pub(super) async fn load_upload_completion_batches(
    expected_channel_id: String,
    offset: Option<usize>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<BatchPage, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        ensure_channel(&state, &expected_channel_id)?;
        let mut connection = database(&state)?;
        let transaction = connection.transaction().map_err(user_error)?;
        let page = load_batch_summaries(
            &transaction,
            &expected_channel_id,
            offset.unwrap_or(0),
            limit.unwrap_or(25),
        )?;
        transaction.commit().map_err(user_error)?;
        ensure_channel(&state, &expected_channel_id)?;
        Ok(page)
    })
    .await
    .map_err(|_| "Saved completion batches could not be loaded.".to_string())?
}

#[tauri::command]
pub(super) async fn load_upload_completion(
    expected_channel_id: String,
    item_ids: Option<Vec<String>>,
    offset: Option<usize>,
    limit: Option<usize>,
    upload_batch_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<Page, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        ensure_channel(&state, &expected_channel_id)?;
        let ids = selected_ids(item_ids)?;
        let mut connection = database(&state)?;
        let transaction = connection.transaction().map_err(user_error)?;
        let page = load_items_for_scope(
            &transaction,
            &expected_channel_id,
            ids.as_deref(),
            upload_batch_id.as_deref(),
            offset.unwrap_or(0).min(100000),
            limit.unwrap_or(25).clamp(1, 100),
        )?;
        transaction.commit().map_err(user_error)?;
        ensure_channel(&state, &expected_channel_id)?;
        Ok(page)
    })
    .await
    .map_err(|_| "Completion checklist could not be loaded.".to_string())?
}

#[tauri::command]
pub(super) async fn check_upload_completion(
    expected_channel_id: String,
    item_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Page, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let ids = selected_ids(Some(item_ids))?.unwrap();
        let _guard = deletion_execution_lock(&state)
            .lock()
            .map_err(|_| "Completion checks are unavailable.".to_string())?;
        {
            let _credential_guard = oauth_replacement::lock_credentials()?;
            oauth_replacement::ensure_no_pending(&state)?;
        }
        let mut provider = NativeProvider { state: &state };
        refresh_with(&state, &expected_channel_id, &ids, &mut provider)?;
        let page = load_items(
            &database(&state)?,
            &expected_channel_id,
            Some(&ids),
            0,
            MAX_SELECTED,
        )?;
        drop(_guard);
        let _ = ensure_upload_postprocess_worker(state.clone());
        Ok(page)
    })
    .await
    .map_err(|_| {
        "Completion checks stopped. The saved upload receipts remain intact.".to_string()
    })?
}

#[tauri::command]
pub(super) async fn repair_upload_completion(
    expected_channel_id: String,
    item_id: String,
    video_id: String,
    steps: Vec<String>,
    confirmation: Option<String>,
    reviewed_jobs: Option<Vec<ReviewedJob>>,
    state: State<'_, AppState>,
) -> Result<Item, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = deletion_execution_lock(&state)
            .lock()
            .map_err(|_| "Completion repair is unavailable.".to_string())?;
        {
            let _credential_guard = oauth_replacement::lock_credentials()?;
            oauth_replacement::ensure_no_pending(&state)?;
        }
        let mut provider = NativeProvider { state: &state };
        repair_reviewed_with(
            &state,
            &expected_channel_id,
            &item_id,
            &video_id,
            &steps,
            confirmation.as_deref(),
            &reviewed_jobs.unwrap_or_default(),
            &mut provider,
        )
    })
    .await
    .map_err(|_| "Repair stopped. Refresh the checklist; uncertain provider writes retain their original job receipts.".to_string())?
}

fn report_json(page: &Page) -> serde_json::Value {
    let mut report = serde_json::json!({"version":1,"channelId":page.channel_id,"generatedAt":now(),"items":page.items.iter().map(|item|serde_json::json!({"itemId":item.item_id,"channelId":item.channel_id,"title":item.title,"videoId":item.video_id,"playlistId":item.playlist_id,"updatedAt":item.updated_at,"steps":item.steps})).collect::<Vec<_>>()});
    if let Some(batch_id) = &page.upload_batch_id {
        report["uploadBatchId"] = serde_json::Value::String(batch_id.clone());
    }
    report
}

fn load_report_items(
    connection: &Connection,
    channel: &str,
    selected: Option<&[String]>,
    upload_batch_id: Option<&str>,
) -> Result<Page, String> {
    validate_batch_scope(connection, channel, selected, upload_batch_id)?;
    let total: usize = if let Some(batch_id) = upload_batch_id {
        connection
            .query_row(
                "SELECT COUNT(*) FROM upload_items WHERE channel_id=?1 AND upload_batch_id=?2",
                params![channel, batch_id],
                |row| row.get(0),
            )
            .map_err(user_error)?
    } else if let Some(ids) = selected {
        ids.len()
    } else {
        connection
            .query_row(
                "SELECT COUNT(*) FROM upload_items WHERE channel_id=?1",
                [channel],
                |row| row.get(0),
            )
            .map_err(user_error)?
    };
    if total > 10000 {
        return Err(
            "Narrow the report to at most 10,000 uploads. No partial report was exported.".into(),
        );
    }
    let page = load_items_for_scope(connection, channel, selected, upload_batch_id, 0, 10000)?;
    if page.total > 10000 || page.items.len() != page.total {
        return Err(
            "The saved report scope changed. Refresh it before exporting a complete report.".into(),
        );
    }
    Ok(page)
}
fn csv_cell(value: &str) -> String {
    let safe = if value
        .trim_start()
        .starts_with(['=', '+', '-', '@', '\t', '\r'])
    {
        format!("'{value}")
    } else {
        value.into()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}
fn report_text(page: &Page, format: &str) -> Result<String, String> {
    if format == "json" {
        return serde_json::to_string_pretty(&report_json(page)).map_err(user_error);
    }
    if format != "csv" {
        return Err("Choose CSV or JSON for the completion report.".into());
    }
    let mut out = String::from(
        "channel_id,upload_batch_id,item_id,title,video_id,step,outcome,detail,checked_at\r\n",
    );
    for item in &page.items {
        for step in &item.steps {
            out.push_str(
                &[
                    item.channel_id.as_str(),
                    page.upload_batch_id.as_deref().unwrap_or(""),
                    item.item_id.as_str(),
                    item.title.as_str(),
                    item.video_id.as_deref().unwrap_or(""),
                    &step.key,
                    &step.outcome,
                    &step.detail,
                    step.checked_at.as_deref().unwrap_or(""),
                ]
                .map(csv_cell)
                .join(","),
            );
            out.push_str("\r\n");
        }
    }
    Ok(out)
}
#[tauri::command]
pub(super) async fn export_upload_completion(
    expected_channel_id: String,
    item_ids: Option<Vec<String>>,
    format: String,
    path: String,
    upload_batch_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<ExportReceipt, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        ensure_channel(&state, &expected_channel_id)?;
        let ids = selected_ids(item_ids)?;
        let mut connection = database(&state)?;
        let transaction = connection.transaction().map_err(user_error)?;
        let page = load_report_items(
            &transaction,
            &expected_channel_id,
            ids.as_deref(),
            upload_batch_id.as_deref(),
        )?;
        transaction.commit().map_err(user_error)?;
        let contents = report_text(&page, &format)?;
        if contents.len() > 8 * 1024 * 1024 {
            return Err("Narrow this completion report before exporting.".into());
        }
        let destination = Path::new(&path);
        if !destination.is_absolute() || destination.file_name().is_none() {
            return Err("Choose an absolute report destination.".into());
        }
        ensure_channel(&state, &expected_channel_id)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|_| {
                "Choose a new report filename; existing files will not be overwritten.".to_string()
            })?;
        if file
            .write_all(contents.as_bytes())
            .and_then(|_| file.sync_all())
            .is_err()
        {
            let _ = std::fs::remove_file(destination);
            return Err("The completion report could not be written.".into());
        }
        Ok(ExportReceipt {
            count: page.items.len(),
            format,
            upload_batch_id: page.upload_batch_id,
        })
    })
    .await
    .map_err(|_| "Completion report export stopped.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-completion-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute_batch(SCHEMA_SQL).unwrap();
        connection.execute_batch(video_assets::SCHEMA_SQL).unwrap();
        connection
            .execute_batch(publication_schedule::SCHEMA_SQL)
            .unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET active_channel_id='channel-a'",[now()]).unwrap();
        connection.execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,status,total_bytes,confirmed_bytes,video_id,playlist_id,playlist_title,playlist_status,created_at,updated_at) VALUES('item-a','=Example','original.mp4','channel-a','D:/private/original.mp4','D:/private/original.mp4',3,'uploaded',3,3,'video-a','playlist-a','Saved playlist','failed',?1,?1),('item-b','Other channel','hidden.mp4','channel-b','private','private',3,'uploaded',3,3,'video-b',NULL,NULL,'not_requested',?1,?1)",[now()]).unwrap();
        (root, state, connection)
    }
    fn video(status: &str) -> serde_json::Value {
        serde_json::json!({"id":"video-a","snippet":{"channelId":"channel-a"},"status":{"uploadStatus":if status=="succeeded"{"processed"}else{"uploaded"}},"processingDetails":{"processingStatus":status}})
    }
    struct MockProvider {
        channel: String,
        reads: usize,
        writes: usize,
        response: serde_json::Value,
        failure: Option<bool>,
        read_error: bool,
        asset_repairs: Vec<(String, Vec<String>)>,
        publication_repairs: Vec<PublicationSnapshot>,
    }
    impl MockProvider {
        fn permitted() -> Self {
            Self {
                channel: "channel-a".into(),
                reads: 0,
                writes: 0,
                response: serde_json::json!({"items":[video("succeeded")]}),
                failure: None,
                read_error: false,
                asset_repairs: Vec::new(),
                publication_repairs: Vec::new(),
            }
        }
    }
    impl CompletionProvider for MockProvider {
        fn repair_assets(
            &mut self,
            channel: &str,
            item: &str,
            video: &str,
            kind: &str,
            jobs: &[String],
        ) -> Result<(), String> {
            assert_eq!((channel, item, video), ("channel-a", "item-a", "video-a"));
            self.asset_repairs.push((kind.into(), jobs.to_vec()));
            Ok(())
        }
        fn repair_publication(
            &mut self,
            channel: &str,
            item: &str,
            video: &str,
            snapshot: &PublicationSnapshot,
        ) -> Result<(), String> {
            assert_eq!((channel, item, video), ("channel-a", "item-a", "video-a"));
            self.publication_repairs.push(snapshot.clone());
            Ok(())
        }
        fn ensure(&mut self, channel: &str) -> Result<(), String> {
            if channel == self.channel {
                Ok(())
            } else {
                Err("Channel changed.".into())
            }
        }
        fn processing(&mut self, _: &str, ids: &[String]) -> Result<serde_json::Value, String> {
            assert!(ids.len() <= MAX_PROCESSING);
            self.reads += 1;
            if self.read_error {
                Err("Provider unavailable.".into())
            } else {
                Ok(self.response.clone())
            }
        }
        fn repair_playlist(
            &mut self,
            _: &str,
            playlist: &str,
            video: &str,
        ) -> Result<(), (String, bool)> {
            assert_eq!(playlist, "playlist-a");
            assert_eq!(video, "video-a");
            self.writes += 1;
            if let Some(uncertain) = self.failure {
                Err(("Safe failure.".into(), uncertain))
            } else {
                Ok(())
            }
        }
    }
    fn seed_asset(connection: &Connection, id: &str, kind: &str, status: &str) {
        connection.execute("INSERT INTO video_asset_jobs(id,item_id,channel_id,video_id,kind,source_path,file_name,source_size,source_modified,source_identity,source_digest,declaration_key,status,detail,created_at,updated_at) VALUES(?1,'item-a','channel-a','video-a',?2,'private-attachment-locator','private-attachment.srt',3,'signature','identity','digest',?1,?3,'Retained attachment receipt.',?4,?4)", params![id,kind,status,now()]).unwrap();
    }
    fn seed_publication(connection: &Connection) {
        let publish_at = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        connection.execute("INSERT INTO publication_schedule_jobs(item_id,channel_id,requested_publish_at,reviewed_time_zone,reviewed_local_time,tzdb_version,state,video_id,detail,created_at,updated_at) VALUES('item-a','channel-a',?1,'UTC',?1,'fixture','needs_attention','video-a','Existing private schedule requires review.',?2,?2)", params![publish_at,now()]).unwrap();
    }
    fn asset_review(key: &str, ids: &[&str]) -> ReviewedJob {
        ReviewedJob {
            key: key.into(),
            job_ids: ids.iter().map(|id| (*id).into()).collect(),
            publish_at: None,
            updated_at: None,
        }
    }
    fn seed_batch(connection: &Connection, batch: &str, count: usize) {
        connection.execute("WITH RECURSIVE numbers(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM numbers WHERE n<?2) INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,status,total_bytes,confirmed_bytes,video_id,upload_batch_id,created_at,updated_at) SELECT ?1||'-'||n,'Batch clip '||n,'original-'||n||'.mp4','channel-a','private-batch-locator','private-batch-locator',3,CASE WHEN n%3=0 THEN 'uploaded' WHEN n%3=1 THEN 'cancelled' ELSE 'failed' END,3,CASE WHEN n%3=0 THEN 3 ELSE 0 END,CASE WHEN n%3=0 THEN 'batch-video-'||n ELSE NULL END,?1,'2026-10-13T01:00:00Z','2026-10-13T01:00:00Z' FROM numbers", params![batch,count]).unwrap();
    }
    #[test]
    fn saved_batch_scope_contains_all_512_retained_items_across_pages() {
        let (root, _state, connection) = fixture();
        seed_batch(&connection, "batch-512", 512);
        connection.execute("UPDATE upload_items SET upload_batch_id='foreign-batch',created_at='2099-01-01T00:00:00Z' WHERE id='item-b'",[]).unwrap();
        let batches = load_batch_summaries(&connection, "channel-a", 0, 25).unwrap();
        assert_eq!(batches.total_batches, 1);
        assert_eq!(batches.batches[0].batch_id, "batch-512");
        assert_eq!(batches.batches[0].item_count, 512);
        assert_eq!(batches.batches[0].created_at, "2026-10-13T01:00:00Z");
        let first =
            load_items_for_scope(&connection, "channel-a", None, Some("batch-512"), 0, 25).unwrap();
        let last = load_items_for_scope(&connection, "channel-a", None, Some("batch-512"), 500, 25)
            .unwrap();
        assert_eq!(first.upload_batch_id.as_deref(), Some("batch-512"));
        assert_eq!(
            (first.total, first.items.len(), last.total, last.items.len()),
            (512, 25, 512, 12)
        );
        assert_eq!(first.pending_processing, 170);
        assert!(first
            .items
            .iter()
            .all(|item| item.item_id.starts_with("batch-512-")));
        assert!(last.items.iter().all(|item| !first
            .items
            .iter()
            .any(|prior| prior.item_id == item.item_id)));
        assert!(
            load_items_for_scope(&connection, "channel-a", None, Some("batch-512"), 512, 25)
                .unwrap()
                .items
                .is_empty()
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn batch_summaries_are_bounded_paginated_and_channel_scoped() {
        let (root, _state, connection) = fixture();
        seed_batch(&connection, "batch-first", 3);
        seed_batch(&connection, "batch-second", 2);
        connection
            .execute(
                "UPDATE upload_items SET upload_batch_id='foreign-batch' WHERE id='item-b'",
                [],
            )
            .unwrap();
        let first = load_batch_summaries(&connection, "channel-a", 0, 1).unwrap();
        let next = load_batch_summaries(&connection, "channel-a", 1, 1).unwrap();
        assert_eq!(
            (
                first.total_batches,
                first.offset,
                first.limit,
                first.batches.len()
            ),
            (2, 0, 1, 1)
        );
        assert_eq!(next.batches.len(), 1);
        assert_ne!(first.batches[0].batch_id, next.batches[0].batch_id);
        assert!(first
            .batches
            .iter()
            .chain(&next.batches)
            .all(|batch| batch.batch_id != "foreign-batch"));
        assert_eq!(
            load_batch_summaries(&connection, "channel-a", usize::MAX, usize::MAX)
                .unwrap()
                .limit,
            100
        );
        assert!(load_batch_summaries(&connection, "channel-a", 2, 1)
            .unwrap()
            .batches
            .is_empty());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unknown_foreign_and_mixed_batch_scopes_reject_without_widening() {
        let (root, _state, connection) = fixture();
        seed_batch(&connection, "batch-owned", 3);
        connection
            .execute(
                "UPDATE upload_items SET upload_batch_id='foreign-batch' WHERE id='item-b'",
                [],
            )
            .unwrap();
        let unavailable = |batch| match load_items_for_scope(
            &connection,
            "channel-a",
            None,
            Some(batch),
            0,
            25,
        ) {
            Ok(_) => panic!("unexpected batch admission"),
            Err(error) => error,
        };
        assert_eq!(unavailable("unknown-batch"), unavailable("foreign-batch"));
        assert_eq!(
            unavailable("../../native-scope"),
            unavailable("foreign-batch")
        );
        assert!(load_items_for_scope(
            &connection,
            "channel-a",
            Some(&["item-a".into()]),
            Some("batch-owned"),
            0,
            25
        )
        .is_err());
        assert!(load_report_items(
            &connection,
            "channel-a",
            Some(&["item-a".into()]),
            Some("batch-owned")
        )
        .is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            5
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn batch_exports_include_complete_native_scope_and_reject_oversized_scope() {
        let (root, _state, connection) = fixture();
        seed_batch(&connection, "batch-export", 512);
        let page = load_report_items(&connection, "channel-a", None, Some("batch-export")).unwrap();
        let report = report_json(&page);
        assert_eq!(report["uploadBatchId"], "batch-export");
        assert_eq!(report["items"].as_array().unwrap().len(), 512);
        assert_eq!(page.total, page.items.len());
        let serialized = report_text(&page, "json").unwrap();
        assert!(!serialized.contains("private-batch-locator"));
        assert!(!serialized.contains("item-b"));
        assert!(report_text(&page, "csv")
            .unwrap()
            .starts_with("channel_id,upload_batch_id,"));
        seed_batch(&connection, "batch-oversized", 10001);
        assert!(
            load_report_items(&connection, "channel-a", None, Some("batch-oversized"))
                .err()
                .unwrap()
                .contains("No partial report was exported")
        );
        let same = load_report_items(&connection, "channel-a", None, Some("batch-export")).unwrap();
        assert_eq!(same.items.len(), 512);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM upload_items WHERE upload_batch_id='batch-oversized'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            10001
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fresh_owner_processing_advances_only_saved_approved_pending_cleanup() {
        let (root, state, connection) = fixture();
        for (status, approved, expected) in [
            ("pending", true, "processing_verified"),
            (
                "waiting_for_youtube_processing",
                true,
                "processing_verified",
            ),
            ("pending", false, "pending"),
            ("retained", true, "retained"),
            (
                "retained_youtube_processing_failed",
                true,
                "retained_youtube_processing_failed",
            ),
            (
                "retained_youtube_upload_failed",
                true,
                "retained_youtube_upload_failed",
            ),
            ("deleted", true, "deleted"),
        ] {
            connection.execute("UPDATE upload_items SET source_delete_status=?1,delete_source_after_upload=?2 WHERE id='item-a'", params![status,approved]).unwrap();
            record_processing_from_inventory(&connection, "channel-a", &video("succeeded"))
                .unwrap();
            let saved: String = connection
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(saved, expected, "{status}, approved={approved}");
        }
        connection.execute("UPDATE upload_items SET source_delete_status='waiting_for_youtube_processing',delete_source_after_upload=1 WHERE id='item-a'",[]).unwrap();
        let foreign = serde_json::json!({"id":"video-a","snippet":{"channelId":"channel-b"},"processingDetails":{"processingStatus":"succeeded"},"status":{"uploadStatus":"processed"}});
        record_processing_from_inventory(&connection, "channel-a", &foreign).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "waiting_for_youtube_processing"
        );
        let mut provider = MockProvider::permitted();
        connection.execute("UPDATE upload_completion_checks SET processing_status='waiting',next_check_at=NULL WHERE item_id='item-a'",[]).unwrap();
        refresh_with(&state, "channel-a", &["item-a".into()], &mut provider).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT source_delete_status FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "processing_verified"
        );
        record_processing_from_inventory(&connection, "channel-a", &video("succeeded")).unwrap();
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE kind='completion_cleanup_processing_verified'",[],|row|row.get::<_,i64>(0)).unwrap(),3);
        assert_eq!(
            connection
                .query_row(
                    "SELECT status,video_id,confirmed_bytes FROM upload_items WHERE id='item-a'",
                    [],
                    |row| Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?
                    ))
                )
                .unwrap(),
            ("uploaded".into(), "video-a".into(), 3)
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn reviewed_attachment_repair_targets_only_frozen_ids_and_never_creates_jobs() {
        let (root, state, connection) = fixture();
        seed_asset(&connection, "caption-reviewed", "caption", "failed");
        let reviewed = asset_review("caption", &["caption-reviewed"]);
        seed_asset(
            &connection,
            "caption-later",
            "caption",
            "waiting_permission",
        );
        let mut provider = MockProvider::permitted();
        let item = repair_reviewed_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["caption".into()],
            None,
            &[reviewed],
            &mut provider,
        )
        .unwrap();
        assert_eq!(
            provider.asset_repairs,
            vec![("caption".into(), vec!["caption-reviewed".into()])]
        );
        assert_eq!((provider.reads, provider.writes), (0, 0));
        assert_eq!(item.video_id.as_deref(), Some("video-a"));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM video_asset_jobs WHERE id='caption-later'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "waiting_permission"
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
    fn replaced_attachment_review_blocks_every_selected_write_before_dispatch() {
        let (root, state, connection) = fixture();
        seed_asset(&connection, "thumbnail-reviewed", "thumbnail", "failed");
        let reviewed = asset_review("thumbnail", &["thumbnail-reviewed"]);
        connection
            .execute(
                "UPDATE video_asset_jobs SET status='superseded' WHERE id='thumbnail-reviewed'",
                [],
            )
            .unwrap();
        seed_asset(&connection, "thumbnail-replacement", "thumbnail", "failed");
        let mut provider = MockProvider::permitted();
        assert!(repair_reviewed_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["playlist".into(), "thumbnail".into()],
            None,
            &[reviewed],
            &mut provider
        )
        .is_err());
        assert_eq!(
            (
                provider.reads,
                provider.writes,
                provider.asset_repairs.len()
            ),
            (0, 0, 0)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE kind='completion_repair_reviewed'",
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
    fn publication_repair_uses_retained_time_and_rejects_later_calendar_edits() {
        let (root, state, connection) = fixture();
        seed_publication(&connection);
        let item = load_items(&connection, "channel-a", Some(&["item-a".into()]), 0, 1)
            .unwrap()
            .items
            .remove(0);
        let snapshot = item
            .steps
            .iter()
            .find(|step| step.key == "publication")
            .unwrap()
            .publication_snapshot
            .clone()
            .unwrap();
        let reviewed = ReviewedJob {
            key: "publication".into(),
            job_ids: Vec::new(),
            publish_at: Some(snapshot.publish_at.clone()),
            updated_at: Some(snapshot.updated_at.clone()),
        };
        let mut provider = MockProvider::permitted();
        repair_reviewed_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["publication".into()],
            None,
            &[reviewed.clone()],
            &mut provider,
        )
        .unwrap();
        assert_eq!(provider.publication_repairs, vec![snapshot]);
        connection.execute("UPDATE publication_schedule_jobs SET requested_publish_at=?1,updated_at=?2 WHERE item_id='item-a'",params![(chrono::Utc::now()+chrono::Duration::days(2)).to_rfc3339(),now()]).unwrap();
        assert!(repair_reviewed_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["playlist".into(), "publication".into()],
            None,
            &[reviewed],
            &mut provider
        )
        .is_err());
        assert_eq!(provider.writes, 0);
        assert_eq!(provider.publication_repairs.len(), 1);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM publication_schedule_jobs",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_or_extra_job_authority_cannot_repair_existing_assets() {
        let (root, state, connection) = fixture();
        seed_asset(&connection, "caption-reviewed", "caption", "failed");
        let mut provider = MockProvider::permitted();
        assert!(repair_reviewed_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["caption".into()],
            None,
            &[],
            &mut provider
        )
        .is_err());
        assert!(repair_reviewed_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["playlist".into()],
            None,
            &[asset_review("caption", &["caption-reviewed"])],
            &mut provider
        )
        .is_err());
        assert!(repair_reviewed_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["caption".into()],
            None,
            &[asset_review(
                "caption",
                &["caption-reviewed", "caption-reviewed"]
            )],
            &mut provider
        )
        .is_err());
        assert_eq!((provider.writes, provider.asset_repairs.len()), (0, 0));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn checklist_is_paginated_and_never_adopts_other_channel_selection() {
        let (root, _state, connection) = fixture();
        let page = load_items(&connection, "channel-a", None, 0, 1).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].channel_id, "channel-a");
        assert_eq!(page.items[0].steps[0].outcome, "succeeded");
        assert_eq!(page.items[0].steps[1].outcome, "unknown");
        assert!(load_items(&connection, "channel-a", Some(&["item-b".into()]), 0, 1).is_err());
        assert!(load_items(&connection, "channel-a", None, 1, 1)
            .unwrap()
            .items
            .is_empty());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn owner_processing_is_persisted_and_terminal_checks_stop() {
        let (root, state, connection) = fixture();
        let mut provider = MockProvider::permitted();
        refresh_with(&state, "channel-a", &["item-a".into()], &mut provider).unwrap();
        assert_eq!(provider.reads, 1);
        let page = load_items(&connection, "channel-a", None, 0, 1).unwrap();
        assert_eq!(page.items[0].steps[1].outcome, "succeeded");
        assert!(page.items[0].steps[1].checked_at.is_some());
        refresh_with(&state, "channel-a", &["item-a".into()], &mut provider).unwrap();
        assert_eq!(provider.reads, 1);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn upload_receipt_alone_and_foreign_processing_do_not_prove_success() {
        let (root, state, connection) = fixture();
        let mut provider = MockProvider::permitted();
        provider.response = serde_json::json!({"items":[{"id":"video-a","snippet":{"channelId":"channel-b"},"status":{"uploadStatus":"processed"},"processingDetails":{"processingStatus":"succeeded"}}]});
        refresh_with(&state, "channel-a", &["item-a".into()], &mut provider).unwrap();
        let page = load_items(&connection, "channel-a", None, 0, 1).unwrap();
        assert_eq!(page.items[0].steps[0].outcome, "succeeded");
        assert_eq!(page.items[0].steps[1].outcome, "unknown");
        assert_eq!(
            processing_outcome(&serde_json::json!({"status":{"uploadStatus":"uploaded"}})).0,
            "unknown"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_provider_processing_still_observes_durable_cooldown() {
        let (root, state, connection) = fixture();
        let mut provider = MockProvider::permitted();
        provider.read_error = true;
        assert!(refresh_with(&state, "channel-a", &["item-a".into()], &mut provider).is_err());
        assert_eq!(provider.reads, 1);
        refresh_with(&state, "channel-a", &["item-a".into()], &mut provider).unwrap();
        assert_eq!(provider.reads, 1);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn targeted_playlist_repair_preserves_uploaded_video_and_bytes() {
        let (root, state, connection) = fixture();
        let mut provider = MockProvider::permitted();
        let item = repair_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["playlist".into()],
            None,
            &mut provider,
        )
        .unwrap();
        assert_eq!(item.video_id.as_deref(), Some("video-a"));
        assert_eq!(item.steps[2].outcome, "succeeded");
        assert_eq!(provider.writes, 1);
        let receipt: (String, String, i64) = connection
            .query_row(
                "SELECT status,video_id,confirmed_bytes FROM upload_items WHERE id='item-a'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(receipt, ("uploaded".into(), "video-a".into(), 3));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn uncertain_or_interrupted_playlist_write_is_reconciled_as_same_video() {
        let (root, state, connection) = fixture();
        let mut provider = MockProvider::permitted();
        provider.failure = Some(true);
        let item = repair_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["playlist".into()],
            None,
            &mut provider,
        )
        .unwrap();
        assert_eq!(item.steps[2].outcome, "unknown");
        assert!(item.steps[2].repairable);
        connection.execute("UPDATE upload_completion_checks SET repair_state='playlist' WHERE item_id='item-a'",[]).unwrap();
        provider.failure = None;
        let item = repair_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["playlist".into()],
            None,
            &mut provider,
        )
        .unwrap();
        assert_eq!(item.steps[2].outcome, "succeeded");
        assert_eq!(provider.writes, 2);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_video_and_wrong_cleanup_confirmation_never_call_provider() {
        let (root, state, connection) = fixture();
        let mut provider = MockProvider::permitted();
        assert!(repair_with(
            &state,
            "channel-a",
            "item-a",
            "video-b",
            &["playlist".into()],
            None,
            &mut provider
        )
        .is_err());
        record_processing_from_inventory(&connection, "channel-a", &video("succeeded")).unwrap();
        connection.execute("UPDATE upload_items SET delete_source_after_upload=1,source_delete_status='retained' WHERE id='item-a'",[]).unwrap();
        assert!(repair_with(
            &state,
            "channel-a",
            "item-a",
            "video-a",
            &["source_cleanup".into()],
            Some("wrong.mp4"),
            &mut provider
        )
        .is_err());
        assert_eq!((provider.reads, provider.writes), (0, 0));
        assert!(ensure_channel(&state, "channel-b").is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn redacted_reports_exclude_original_locators_and_escape_spreadsheet_formulas() {
        let (root, _state, connection) = fixture();
        let page = load_items(&connection, "channel-a", None, 0, 1).unwrap();
        let json = report_text(&page, "json").unwrap();
        for private in [
            "D:/private",
            "sourcePath",
            "workspacePath",
            "fileName",
            "original.mp4",
            "resumable",
        ] {
            assert!(!json.contains(private));
        }
        assert!(json.contains("video-a"));
        assert!(!json.contains("video-b"));
        let csv = report_text(&page, "csv").unwrap();
        assert!(csv.contains("\"'=Example\""));
        assert!(!csv.contains("original.mp4"));
        assert!(report_text(&page, "xml").is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn automatic_playlist_waits_for_management_without_retry_noise_then_completes_same_video() {
        let (root, state, connection) = fixture();
        connection
            .execute(
                "UPDATE upload_items SET playlist_status='pending' WHERE id='item-a'",
                [],
            )
            .unwrap();
        let mut provider = MockProvider::permitted();
        process_requested_playlist_with(&state, "item-a", false, &mut provider).unwrap();
        process_requested_playlist_with(&state, "item-a", false, &mut provider).unwrap();
        assert_eq!(provider.writes, 0);
        assert_eq!(
            connection
                .query_row(
                    "SELECT playlist_status FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "pending"
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE item_id='item-a' AND channel_id='channel-a' AND kind='completion_playlist_permission_pending'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
        assert!(
            load_items(&connection, "channel-a", Some(&["item-a".into()]), 0, 1)
                .unwrap()
                .items[0]
                .steps[2]
                .detail
                .contains("management permission")
        );
        process_requested_playlist_with(&state, "item-a", true, &mut provider).unwrap();
        process_requested_playlist_with(&state, "item-a", true, &mut provider).unwrap();
        assert_eq!(provider.writes, 1);
        assert_eq!(connection.query_row("SELECT status,video_id,confirmed_bytes,playlist_status FROM upload_items WHERE id='item-a'",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,row.get::<_,String>(3)?))).unwrap(),("uploaded".into(),"video-a".into(),3,"complete".into()));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn automatic_uncertain_playlist_outcome_retains_receipt_and_blocks_blind_repeat() {
        let (root, state, connection) = fixture();
        connection
            .execute(
                "UPDATE upload_items SET playlist_status='pending' WHERE id='item-a'",
                [],
            )
            .unwrap();
        let mut provider = MockProvider::permitted();
        provider.failure = Some(true);
        process_requested_playlist_with(&state, "item-a", true, &mut provider).unwrap();
        process_requested_playlist_with(&state, "item-a", true, &mut provider).unwrap();
        assert_eq!(provider.writes, 1);
        let item = load_items(&connection, "channel-a", Some(&["item-a".into()]), 0, 1)
            .unwrap()
            .items
            .remove(0);
        assert_eq!(item.steps[0].outcome, "succeeded");
        assert_eq!(item.steps[2].outcome, "unknown");
        assert!(item.steps[2].repairable);
        assert_eq!(
            connection
                .query_row(
                    "SELECT confirmed_bytes FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn automatic_playlist_does_not_inspect_or_write_inactive_channel_jobs() {
        let (root, state, connection) = fixture();
        connection
            .execute(
                "UPDATE upload_items SET playlist_status='pending' WHERE id='item-a'",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        let mut provider = MockProvider::permitted();
        process_requested_playlist_with(&state, "item-a", true, &mut provider).unwrap();
        assert_eq!((provider.reads, provider.writes), (0, 0));
        assert_eq!(
            connection
                .query_row(
                    "SELECT playlist_status FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "pending"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reviewed_cleanup_requires_fresh_owner_processing_and_canonical_content_identity() {
        for correct_bytes in [false, true] {
            let (root, state, connection) = fixture();
            let original = root.join("original.mp4");
            fs::write(&original, if correct_bytes { b"abc" } else { b"bad" }).unwrap();
            let signature = monitored_file_signature(&fs::metadata(&original).unwrap()).unwrap();
            connection.execute("UPDATE upload_items SET source_path=?1,workspace_path=?1,digest=?2,source_modified_key=?3,delete_source_after_upload=1,source_delete_status='retained' WHERE id='item-a'",params![original.to_string_lossy(),blake3::hash(b"abc").to_hex().to_string(),signature.1]).unwrap();
            record_processing_from_inventory(&connection, "channel-a", &video("succeeded"))
                .unwrap();
            let mut provider = MockProvider::permitted();
            let item = repair_with(
                &state,
                "channel-a",
                "item-a",
                "video-a",
                &["source_cleanup".into()],
                Some("original.mp4"),
                &mut provider,
            )
            .unwrap();
            assert_eq!(provider.reads, 1);
            assert_eq!(provider.writes, 0);
            assert_eq!(
                item.steps[3].outcome,
                if correct_bytes { "succeeded" } else { "failed" }
            );
            assert_eq!(original.exists(), !correct_bytes);
            assert_eq!(connection.query_row("SELECT status,video_id,confirmed_bytes FROM upload_items WHERE id='item-a'",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?))).unwrap(),("uploaded".into(),"video-a".into(),3));
            drop(connection);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn retained_reason_projection_never_exports_untrusted_audit_locators() {
        let (root, _state, connection) = fixture();
        connection.execute("UPDATE upload_items SET delete_source_after_upload=1,source_delete_status='retained' WHERE id='item-a'",[]).unwrap();
        audit(
            &connection,
            "item-a",
            "source_cleanup_retained",
            "Original source was retained because it changed after import.",
        )
        .unwrap();
        let page = load_items(&connection, "channel-a", None, 0, 25).unwrap();
        assert!(page.items[0].steps[3].detail.contains("bytes differ"));
        assert!(safe_retained_reason(Some("D:/private/secret.mp4 token=secret")).is_none());
        assert!(!report_text(&page, "json").unwrap().contains("D:/private"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn membership_reread_accepts_only_the_exact_saved_playlist_and_video() {
        let present = serde_json::json!({"items":[{"snippet":{"playlistId":"playlist-a","resourceId":{"kind":"youtube#video","videoId":"video-a"}}}]});
        assert_eq!(
            playlist_membership(&present, "playlist-a", "video-a").unwrap(),
            true
        );
        assert_eq!(
            playlist_membership(&serde_json::json!({"items":[]}), "playlist-a", "video-a").unwrap(),
            false
        );
        assert!(playlist_membership(&present, "playlist-b", "video-a").is_err());
        assert!(playlist_membership(&present, "playlist-a", "video-b").is_err());
        assert!(playlist_membership(
            &serde_json::json!({"items":[],"nextPageToken":"another-page"}),
            "playlist-a",
            "video-a"
        )
        .is_err());
        assert!(playlist_membership(&serde_json::json!({}), "playlist-a", "video-a").is_err());
    }
}
