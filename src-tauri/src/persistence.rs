use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::{collections::HashSet, time::Duration};

pub(crate) const DATABASE_SCHEMA_VERSION: i64 = 8;

#[cfg(all(test, feature = "performance-harness"))]
static TEST_DATABASE_TRACE: std::sync::Mutex<Option<(std::thread::ThreadId, Vec<String>)>> =
    std::sync::Mutex::new(None);

#[cfg(all(test, feature = "performance-harness"))]
pub(crate) fn begin_test_database_trace() {
    *TEST_DATABASE_TRACE.lock().unwrap() = Some((std::thread::current().id(), Vec::new()));
}

#[cfg(all(test, feature = "performance-harness"))]
pub(crate) fn record_test_database_statement(event: &rusqlite::trace::TraceEvent<'_>) {
    let rusqlite::trace::TraceEvent::Stmt(statement, _) = event else {
        return;
    };
    let mut capture = TEST_DATABASE_TRACE.lock().unwrap();
    if let Some((thread_id, statements)) = capture.as_mut() {
        if *thread_id == std::thread::current().id() {
            statements.push(statement.sql().into_owned());
        }
    }
}

#[cfg(all(test, feature = "performance-harness"))]
pub(crate) fn take_test_database_trace() -> Vec<String> {
    TEST_DATABASE_TRACE
        .lock()
        .unwrap()
        .take()
        .map(|(_, statements)| statements)
        .unwrap_or_default()
}

const CURRENT_TABLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS upload_items (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  upload_metadata_json TEXT NOT NULL DEFAULT '{}',
  upload_batch_id TEXT,
  upload_plan_id TEXT,
  normalized_title TEXT NOT NULL DEFAULT '',
  canonical_title TEXT NOT NULL DEFAULT '',
  has_copy_marker INTEGER NOT NULL DEFAULT 0,
  numeric_title_key TEXT NOT NULL DEFAULT '',
  title_keys_version INTEGER NOT NULL DEFAULT 0,
  file_name TEXT NOT NULL,
  channel_name TEXT,
  channel_id TEXT,
  folder_monitor_id TEXT,
  source_path TEXT,
  workspace_path TEXT NOT NULL,
  partial_path TEXT,
  size_bytes INTEGER NOT NULL,
  digest TEXT,
  background_hash_status TEXT NOT NULL DEFAULT 'not_required',
  source_modified_key TEXT,
  status TEXT NOT NULL,
  confirmed_bytes INTEGER NOT NULL DEFAULT 0,
  imported_bytes INTEGER NOT NULL DEFAULT 0,
  total_bytes INTEGER NOT NULL,
  resumable_session_uri TEXT,
  video_id TEXT,
  detail TEXT,
  visibility TEXT NOT NULL DEFAULT 'private',
  made_for_kids INTEGER NOT NULL DEFAULT 0,
  playlist_id TEXT,
  playlist_title TEXT,
  playlist_status TEXT NOT NULL DEFAULT 'not_requested',
  duplicate_decision TEXT,
  delete_source_after_upload INTEGER NOT NULL DEFAULT 0,
  source_delete_status TEXT,
  upload_started_at TEXT,
  transfer_bytes_per_second REAL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS connection_settings (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  oauth_client_id TEXT,
  oauth_credential_id TEXT,
  oauth_project_id TEXT,
  active_channel TEXT,
  active_channel_id TEXT,
  connection_detail TEXT,
  deletion_authorized INTEGER NOT NULL DEFAULT 0,
  deletion_sudo_until TEXT,
  manual_made_for_kids_default INTEGER NOT NULL DEFAULT 0,
  manual_visibility_default TEXT NOT NULL DEFAULT 'private' CHECK(manual_visibility_default IN ('private', 'unlisted', 'public')),
  upload_quota_pause_until TEXT,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS remote_videos (
  video_id TEXT PRIMARY KEY NOT NULL,
  channel_name TEXT NOT NULL,
  channel_id TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL,
  normalized_title TEXT NOT NULL DEFAULT '',
  canonical_title TEXT NOT NULL DEFAULT '',
  has_copy_marker INTEGER NOT NULL DEFAULT 0,
  numeric_title_key TEXT NOT NULL DEFAULT '',
  title_keys_version INTEGER NOT NULL DEFAULT 0,
  duration TEXT,
  privacy_status TEXT,
  upload_status TEXT,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS manual_playlist_defaults (
  channel_id TEXT PRIMARY KEY NOT NULL,
  playlist_id TEXT NOT NULL,
  playlist_title TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS remote_video_sync_staging (
  sync_id TEXT NOT NULL,
  video_id TEXT NOT NULL,
  channel_name TEXT NOT NULL,
  channel_id TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL,
  normalized_title TEXT NOT NULL DEFAULT '',
  canonical_title TEXT NOT NULL DEFAULT '',
  has_copy_marker INTEGER NOT NULL DEFAULT 0,
  numeric_title_key TEXT NOT NULL DEFAULT '',
  title_keys_version INTEGER NOT NULL DEFAULT 0,
  duration TEXT,
  privacy_status TEXT,
  upload_status TEXT,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(sync_id, video_id)
);
CREATE TABLE IF NOT EXISTS preflight_scan_jobs (
  id TEXT PRIMARY KEY NOT NULL,
  channel_id TEXT NOT NULL DEFAULT '',
  mode TEXT NOT NULL CHECK(mode IN ('light', 'deep')),
  status TEXT NOT NULL,
  total_files INTEGER NOT NULL,
  completed_files INTEGER NOT NULL DEFAULT 0,
  inventory_status TEXT NOT NULL DEFAULT 'not_requested',
  detail TEXT,
  matched_files INTEGER NOT NULL DEFAULT 0,
  evidence_materialized_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS preflight_scan_files (
  job_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL,
  source_locator TEXT NOT NULL,
  file_name TEXT NOT NULL,
  size_bytes INTEGER NOT NULL DEFAULT 0,
  digest TEXT,
  status TEXT NOT NULL,
  error TEXT,
  metadata_json TEXT,
  metadata_status TEXT NOT NULL DEFAULT 'pending',
  local_matches_json TEXT NOT NULL DEFAULT '[]',
  dropped_duplicate_names_json TEXT NOT NULL DEFAULT '[]',
  uploaded_title_matches_json TEXT NOT NULL DEFAULT '[]',
  local_match_count INTEGER NOT NULL DEFAULT 0,
  dropped_duplicate_count INTEGER NOT NULL DEFAULT 0,
  uploaded_title_match_count INTEGER NOT NULL DEFAULT 0,
  can_delete_local_duplicate INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(job_id, ordinal),
  FOREIGN KEY(job_id) REFERENCES preflight_scan_jobs(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS preflight_scan_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id TEXT NOT NULL,
  file_name TEXT,
  message TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY(job_id) REFERENCES preflight_scan_jobs(id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS deletion_requests (
  id TEXT PRIMARY KEY NOT NULL,
  video_id TEXT NOT NULL UNIQUE,
  channel_id TEXT NOT NULL DEFAULT '',
  title TEXT NOT NULL,
  status TEXT NOT NULL,
  detail TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT,
  channel_name TEXT,
  channel_id TEXT NOT NULL DEFAULT '',
  folder_monitor_id TEXT,
  kind TEXT NOT NULL,
  detail TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY(item_id) REFERENCES upload_items(id)
);
CREATE TABLE IF NOT EXISTS ignored_duplicate_candidates (
  channel_id TEXT NOT NULL DEFAULT '',
  candidate_id TEXT NOT NULL,
  ignored_at TEXT NOT NULL,
  PRIMARY KEY(channel_id, candidate_id)
);
CREATE TABLE IF NOT EXISTS folder_monitor_settings (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  enabled INTEGER NOT NULL DEFAULT 0,
  folder_path TEXT,
  channel_name TEXT,
  channel_id TEXT,
  visibility TEXT NOT NULL DEFAULT 'private',
  made_for_kids INTEGER NOT NULL DEFAULT 0,
  playlist_id TEXT,
  playlist_title TEXT,
  delete_source_after_upload INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'disabled',
  detail TEXT NOT NULL DEFAULT 'Folder monitoring is disabled.',
  last_scan_at TEXT,
  last_file_name TEXT,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS folder_monitor_observations (
  monitor_id TEXT NOT NULL DEFAULT 'legacy-singleton',
  channel_id TEXT NOT NULL DEFAULT '',
  channel_name TEXT NOT NULL,
  file_path TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  modified_key TEXT NOT NULL,
  state TEXT NOT NULL,
  digest TEXT,
  upload_item_id TEXT,
  first_seen_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(monitor_id, file_path)
);
CREATE TABLE IF NOT EXISTS folder_monitors (
  id TEXT PRIMARY KEY NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0,
  removed INTEGER NOT NULL DEFAULT 0,
  folder_path TEXT,
  folder_key TEXT NOT NULL DEFAULT '',
  channel_name TEXT,
  channel_id TEXT,
  visibility TEXT NOT NULL DEFAULT 'private',
  made_for_kids INTEGER NOT NULL DEFAULT 0,
  playlist_id TEXT,
  playlist_title TEXT,
  delete_source_after_upload INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'disabled',
  detail TEXT NOT NULL DEFAULT 'Folder monitoring is disabled.',
  last_scan_at TEXT,
  last_file_name TEXT,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS upload_disk_capabilities (
  volume_id TEXT PRIMARY KEY NOT NULL,
  checked_at TEXT NOT NULL,
  read_bytes_per_second REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS upload_connection_capability (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  checked_at TEXT NOT NULL,
  upload_bytes_per_second REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS upload_quota_pauses (
  channel_id TEXT PRIMARY KEY NOT NULL,
  pause_until TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS api_quota_pauses (
  quota_scope TEXT PRIMARY KEY NOT NULL,
  channel_id TEXT NOT NULL,
  pause_until TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS state_changes (
  revision INTEGER PRIMARY KEY AUTOINCREMENT,
  channel_id TEXT NOT NULL DEFAULT '',
  surface TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  event_kind TEXT NOT NULL,
  payload_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS channel_generations (
  channel_id TEXT PRIMARY KEY NOT NULL,
  inventory_generation INTEGER NOT NULL DEFAULT 0,
  upload_generation INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS duplicate_projection_state (
  channel_id TEXT PRIMARY KEY NOT NULL,
  inventory_generation INTEGER NOT NULL,
  upload_generation INTEGER NOT NULL,
  rebuilt_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS duplicate_candidate_projection (
  channel_id TEXT NOT NULL,
  candidate_id TEXT NOT NULL,
  confidence TEXT NOT NULL,
  left_title TEXT NOT NULL,
  right_title TEXT NOT NULL,
  left_video_id TEXT,
  right_video_id TEXT,
  evidence TEXT NOT NULL,
  PRIMARY KEY(channel_id, candidate_id)
);
"#;

const STATE_CHANGE_TRIGGERS_SQL: &str = r#"
CREATE TRIGGER IF NOT EXISTS state_upload_item_insert
AFTER INSERT ON upload_items BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(NEW.channel_id, ''), 'upload', NEW.id, 'upsert',
    json_object('id', NEW.id, 'title', NEW.title, 'fileName', NEW.file_name,
      'sizeBytes', NEW.size_bytes, 'digest', NEW.digest, 'status', NEW.status,
      'confirmedBytes', NEW.confirmed_bytes, 'totalBytes', NEW.total_bytes,
      'videoId', NEW.video_id, 'detail', NEW.detail, 'visibility', NEW.visibility,
      'retryFailureKey', (SELECT failure_key FROM upload_retry_failures WHERE item_id=NEW.id AND channel_id=NEW.channel_id),
      'madeForKids', json(CASE WHEN NEW.made_for_kids != 0 THEN 'true' ELSE 'false' END),
      'playlistId', NEW.playlist_id, 'playlistTitle', NEW.playlist_title,
      'uploadStartedAt', NEW.upload_started_at,
      'transferBytesPerSecond', NEW.transfer_bytes_per_second,
      'deleteSourceAfterUpload', json(CASE WHEN NEW.delete_source_after_upload != 0 THEN 'true' ELSE 'false' END),
      'sourceDeleteStatus', NEW.source_delete_status, 'updatedAt', NEW.updated_at),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS generation_upload_item_insert
AFTER INSERT ON upload_items BEGIN
  INSERT INTO channel_generations(channel_id, upload_generation, updated_at)
  VALUES(COALESCE(NEW.channel_id, ''), 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
  ON CONFLICT(channel_id) DO UPDATE SET
    upload_generation = upload_generation + 1,
    updated_at = excluded.updated_at;
END;
CREATE TRIGGER IF NOT EXISTS state_upload_item_update
AFTER UPDATE ON upload_items BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(NEW.channel_id, ''), 'upload', NEW.id,
    CASE WHEN NEW.status = 'cancelled' THEN 'delete' ELSE 'upsert' END,
    CASE WHEN NEW.status = 'cancelled' THEN '{}'
      ELSE json_object('id', NEW.id, 'title', NEW.title, 'fileName', NEW.file_name,
        'sizeBytes', NEW.size_bytes, 'digest', NEW.digest, 'status', NEW.status,
        'confirmedBytes', NEW.confirmed_bytes, 'totalBytes', NEW.total_bytes,
        'videoId', NEW.video_id, 'detail', NEW.detail, 'visibility', NEW.visibility,
        'retryFailureKey', (SELECT failure_key FROM upload_retry_failures WHERE item_id=NEW.id AND channel_id=NEW.channel_id),
        'madeForKids', json(CASE WHEN NEW.made_for_kids != 0 THEN 'true' ELSE 'false' END),
        'playlistId', NEW.playlist_id, 'playlistTitle', NEW.playlist_title,
        'uploadStartedAt', NEW.upload_started_at,
        'transferBytesPerSecond', NEW.transfer_bytes_per_second,
        'deleteSourceAfterUpload', json(CASE WHEN NEW.delete_source_after_upload != 0 THEN 'true' ELSE 'false' END),
        'sourceDeleteStatus', NEW.source_delete_status, 'updatedAt', NEW.updated_at) END,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS generation_upload_item_update
AFTER UPDATE OF title, digest, status, channel_id ON upload_items BEGIN
  INSERT INTO channel_generations(channel_id, upload_generation, updated_at)
  VALUES(COALESCE(NEW.channel_id, ''), 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
  ON CONFLICT(channel_id) DO UPDATE SET
    upload_generation = upload_generation + 1,
    updated_at = excluded.updated_at;
  INSERT INTO channel_generations(channel_id, upload_generation, updated_at)
  SELECT COALESCE(OLD.channel_id, ''), 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
  WHERE COALESCE(OLD.channel_id, '') != COALESCE(NEW.channel_id, '')
  ON CONFLICT(channel_id) DO UPDATE SET
    upload_generation = upload_generation + 1,
    updated_at = excluded.updated_at;
END;
CREATE TRIGGER IF NOT EXISTS state_upload_item_delete
AFTER DELETE ON upload_items BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(OLD.channel_id, ''), 'upload', OLD.id, 'delete', '{}',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS generation_upload_item_delete
AFTER DELETE ON upload_items BEGIN
  INSERT INTO channel_generations(channel_id, upload_generation, updated_at)
  VALUES(COALESCE(OLD.channel_id, ''), 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
  ON CONFLICT(channel_id) DO UPDATE SET
    upload_generation = upload_generation + 1,
    updated_at = excluded.updated_at;
END;

CREATE TRIGGER IF NOT EXISTS state_connection_insert
AFTER INSERT ON connection_settings BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(NEW.active_channel_id, ''), 'connection', 'connection', 'invalidate', '{}',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_connection_update
AFTER UPDATE ON connection_settings BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(NEW.active_channel_id, ''), 'connection', 'connection', 'invalidate', '{}',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;

CREATE TRIGGER IF NOT EXISTS state_preflight_job_insert
AFTER INSERT ON preflight_scan_jobs BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'preflight', NEW.id, 'progress',
    json_object('jobId', NEW.id, 'mode', NEW.mode, 'status', NEW.status,
      'totalFiles', NEW.total_files, 'completedFiles', NEW.completed_files,
      'pendingMetadataFiles', 0, 'youtubeTitleChecked', json(CASE WHEN NEW.inventory_status = 'complete' THEN 'true' ELSE 'false' END),
      'youtubeCheckDetail', NEW.detail), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_preflight_job_update
AFTER UPDATE ON preflight_scan_jobs BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'preflight', NEW.id, 'progress',
    json_object('jobId', NEW.id, 'mode', NEW.mode, 'status', NEW.status,
      'totalFiles', NEW.total_files, 'completedFiles', NEW.completed_files,
      'pendingMetadataFiles', (SELECT COUNT(*) FROM preflight_scan_files WHERE job_id = NEW.id AND status = 'complete' AND metadata_status != 'complete'),
      'youtubeTitleChecked', json(CASE WHEN NEW.inventory_status = 'complete' THEN 'true' ELSE 'false' END),
      'youtubeCheckDetail', NEW.detail), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_preflight_file_update
AFTER UPDATE OF status, metadata_status ON preflight_scan_files BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  SELECT jobs.channel_id, 'preflight', jobs.id, 'progress',
    json_object('jobId', jobs.id, 'mode', jobs.mode, 'status', jobs.status,
      'totalFiles', jobs.total_files, 'completedFiles', jobs.completed_files,
      'pendingMetadataFiles', (SELECT COUNT(*) FROM preflight_scan_files WHERE job_id = jobs.id AND status = 'complete' AND metadata_status != 'complete'),
      'youtubeTitleChecked', json(CASE WHEN jobs.inventory_status = 'complete' THEN 'true' ELSE 'false' END),
      'youtubeCheckDetail', jobs.detail), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
  FROM preflight_scan_jobs AS jobs WHERE jobs.id = NEW.job_id;
END;

CREATE TRIGGER IF NOT EXISTS state_deletion_insert
AFTER INSERT ON deletion_requests BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'deletion', NEW.id, 'upsert',
    json_object('id', NEW.id, 'videoId', NEW.video_id, 'title', NEW.title,
      'status', NEW.status, 'detail', NEW.detail, 'updatedAt', NEW.updated_at),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_deletion_update
AFTER UPDATE ON deletion_requests BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'deletion', NEW.id, 'upsert',
    json_object('id', NEW.id, 'videoId', NEW.video_id, 'title', NEW.title,
      'status', NEW.status, 'detail', NEW.detail, 'updatedAt', NEW.updated_at),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;

CREATE TRIGGER IF NOT EXISTS state_folder_settings_insert
AFTER INSERT ON folder_monitor_settings BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(NEW.channel_id, ''), 'folder_monitor', 'settings', 'invalidate',
    json_object('enabled', json(CASE WHEN NEW.enabled != 0 THEN 'true' ELSE 'false' END),
      'status', NEW.status, 'detail', NEW.detail, 'lastScanAt', NEW.last_scan_at,
      'lastFileName', NEW.last_file_name), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_folder_settings_update
AFTER UPDATE ON folder_monitor_settings BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(NEW.channel_id, ''), 'folder_monitor', 'settings', 'invalidate',
    json_object('enabled', json(CASE WHEN NEW.enabled != 0 THEN 'true' ELSE 'false' END),
      'status', NEW.status, 'detail', NEW.detail, 'lastScanAt', NEW.last_scan_at,
      'lastFileName', NEW.last_file_name), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_folder_observation_insert
AFTER INSERT ON folder_monitor_observations BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'folder_monitor', printf('observation-%lld', NEW.rowid), 'invalidate',
    json_object('observationState', NEW.state, 'sizeBytes', NEW.size_bytes,
      'uploadItemId', NEW.upload_item_id, 'updatedAt', NEW.updated_at),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_monitors_insert
AFTER INSERT ON folder_monitors BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(NEW.channel_id, ''), 'folder_monitor', NEW.id, 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_monitors_update
AFTER UPDATE ON folder_monitors BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(COALESCE(NEW.channel_id, ''), 'folder_monitor', NEW.id, 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_folder_observation_update
AFTER UPDATE ON folder_monitor_observations BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'folder_monitor', printf('observation-%lld', NEW.rowid), 'invalidate',
    json_object('observationState', NEW.state, 'sizeBytes', NEW.size_bytes,
      'uploadItemId', NEW.upload_item_id, 'updatedAt', NEW.updated_at),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;

CREATE TRIGGER IF NOT EXISTS state_ignored_insert
AFTER INSERT ON ignored_duplicate_candidates BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'dedupe', NEW.candidate_id, 'invalidate', '{}',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_ignored_delete
AFTER DELETE ON ignored_duplicate_candidates BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(OLD.channel_id, 'dedupe', OLD.candidate_id, 'invalidate', '{}',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;

CREATE TRIGGER IF NOT EXISTS state_quota_insert
AFTER INSERT ON upload_quota_pauses BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'quota', NEW.channel_id, 'upsert',
    json_object('pauseUntil', NEW.pause_until), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_quota_update
AFTER UPDATE ON upload_quota_pauses BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'quota', NEW.channel_id, 'upsert',
    json_object('pauseUntil', NEW.pause_until), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_quota_delete
AFTER DELETE ON upload_quota_pauses BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(OLD.channel_id, 'quota', OLD.channel_id, 'delete', '{}',
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(OLD.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;

CREATE TRIGGER IF NOT EXISTS state_changes_prune
AFTER INSERT ON state_changes WHEN (NEW.revision % 256) = 0 BEGIN
  DELETE FROM state_changes WHERE revision < NEW.revision - 8192;
END;

CREATE TRIGGER IF NOT EXISTS state_api_quota_insert
AFTER INSERT ON api_quota_pauses BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'quota', NEW.channel_id, 'upsert',
    json_object('pauseUntil', NEW.pause_until, 'kind', 'api_quota'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_api_quota_update
AFTER UPDATE ON api_quota_pauses BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'quota', NEW.channel_id, 'upsert',
    json_object('pauseUntil', NEW.pause_until, 'kind', 'api_quota'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_api_quota_delete
AFTER DELETE ON api_quota_pauses BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(OLD.channel_id, 'quota', OLD.channel_id, 'delete', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(OLD.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;

CREATE TRIGGER IF NOT EXISTS state_manual_playlist_default_insert
AFTER INSERT ON manual_playlist_defaults BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_manual_playlist_default_update
AFTER UPDATE ON manual_playlist_defaults BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(NEW.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
CREATE TRIGGER IF NOT EXISTS state_manual_playlist_default_delete
AFTER DELETE ON manual_playlist_defaults BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
  VALUES(OLD.channel_id, 'connection', 'account', 'invalidate', '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
END;
"#;

const CURRENT_INDEXES_SQL: &str = r#"
CREATE INDEX IF NOT EXISTS upload_items_status_idx
  ON upload_items(status);
CREATE INDEX IF NOT EXISTS upload_items_status_created_idx
  ON upload_items(status, created_at, id);
CREATE INDEX IF NOT EXISTS upload_items_channel_status_updated_idx
  ON upload_items(channel_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS upload_items_channel_updated_active_idx
  ON upload_items(channel_id, updated_at DESC)
  WHERE status != 'cancelled';
CREATE INDEX IF NOT EXISTS upload_items_unbound_updated_active_idx
  ON upload_items(updated_at DESC)
  WHERE status != 'cancelled' AND (channel_id IS NULL OR channel_id = '');
CREATE INDEX IF NOT EXISTS upload_items_channel_digest_uploaded_idx
  ON upload_items(channel_id, digest, created_at)
  WHERE digest IS NOT NULL AND status = 'uploaded';
CREATE INDEX IF NOT EXISTS upload_items_channel_normalized_title_idx
  ON upload_items(channel_id, normalized_title, id);
CREATE INDEX IF NOT EXISTS upload_items_channel_canonical_title_idx
  ON upload_items(channel_id, canonical_title, has_copy_marker, id);
CREATE INDEX IF NOT EXISTS upload_items_channel_numeric_title_idx
  ON upload_items(channel_id, numeric_title_key, id)
  WHERE numeric_title_key != '';
CREATE INDEX IF NOT EXISTS upload_items_background_hash_pending_idx
  ON upload_items(background_hash_status, created_at, id)
  WHERE source_path IS NOT NULL AND digest IS NULL;
CREATE INDEX IF NOT EXISTS upload_items_source_cleanup_pending_idx
  ON upload_items(channel_id, id)
  WHERE status = 'uploaded' AND delete_source_after_upload = 1 AND source_delete_status = 'pending';
CREATE INDEX IF NOT EXISTS upload_items_playlist_pending_idx
  ON upload_items(channel_id, id)
  WHERE status = 'uploaded' AND playlist_id IS NOT NULL AND playlist_status = 'pending';
CREATE INDEX IF NOT EXISTS remote_videos_channel_updated_idx
  ON remote_videos(channel_id, updated_at DESC, title);
CREATE INDEX IF NOT EXISTS remote_videos_channel_status_video_idx
  ON remote_videos(channel_id, upload_status, video_id);
CREATE INDEX IF NOT EXISTS remote_videos_channel_normalized_title_idx
  ON remote_videos(channel_id, upload_status, normalized_title, video_id);
CREATE INDEX IF NOT EXISTS remote_videos_channel_canonical_title_idx
  ON remote_videos(channel_id, upload_status, canonical_title, has_copy_marker, video_id);
CREATE INDEX IF NOT EXISTS remote_videos_channel_numeric_title_idx
  ON remote_videos(channel_id, upload_status, numeric_title_key, video_id)
  WHERE numeric_title_key != '';
CREATE INDEX IF NOT EXISTS remote_video_staging_channel_sync_idx
  ON remote_video_sync_staging(channel_id, sync_id, video_id);
CREATE INDEX IF NOT EXISTS preflight_scan_jobs_status_created_idx
  ON preflight_scan_jobs(channel_id, status, created_at, id);
CREATE INDEX IF NOT EXISTS preflight_scan_files_job_status_idx
  ON preflight_scan_files(job_id, status, ordinal);
CREATE INDEX IF NOT EXISTS preflight_scan_files_metadata_pending_idx
  ON preflight_scan_files(status, metadata_status, job_id, ordinal);
CREATE INDEX IF NOT EXISTS preflight_scan_events_job_id_idx
  ON preflight_scan_events(job_id, id);
CREATE INDEX IF NOT EXISTS deletion_requests_channel_updated_idx
  ON deletion_requests(channel_id, updated_at DESC, id);
CREATE INDEX IF NOT EXISTS deletion_requests_channel_status_updated_idx
  ON deletion_requests(channel_id, status, updated_at, id);
CREATE INDEX IF NOT EXISTS audit_events_created_idx
  ON audit_events(created_at DESC);
CREATE INDEX IF NOT EXISTS audit_events_channel_created_idx
  ON audit_events(channel_id, created_at DESC);
CREATE INDEX IF NOT EXISTS folder_monitor_observation_state_idx
  ON folder_monitor_observations(channel_id, state);
CREATE INDEX IF NOT EXISTS folder_monitor_observation_updated_idx
  ON folder_monitor_observations(channel_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS folder_monitor_observation_upload_idx
  ON folder_monitor_observations(upload_item_id, state);
CREATE UNIQUE INDEX IF NOT EXISTS folder_monitors_live_root_idx
  ON folder_monitors(folder_key) WHERE removed = 0 AND folder_key != '';
CREATE INDEX IF NOT EXISTS folder_monitors_channel_idx
  ON folder_monitors(channel_id, removed, enabled, id);
CREATE INDEX IF NOT EXISTS folder_monitors_archived_root_idx
  ON folder_monitors(channel_id, folder_key, removed, updated_at DESC);
CREATE INDEX IF NOT EXISTS folder_monitor_id_activity_idx
  ON folder_monitor_observations(monitor_id, channel_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS folder_monitor_audit_idx
  ON audit_events(folder_monitor_id, channel_id, created_at DESC);
CREATE INDEX IF NOT EXISTS state_changes_channel_revision_idx
  ON state_changes(channel_id, revision);
CREATE INDEX IF NOT EXISTS state_changes_surface_revision_idx
  ON state_changes(surface, revision);
CREATE INDEX IF NOT EXISTS upload_quota_pauses_deadline_idx
  ON upload_quota_pauses(pause_until, channel_id);
CREATE INDEX IF NOT EXISTS duplicate_candidate_projection_channel_idx
  ON duplicate_candidate_projection(channel_id, candidate_id);
"#;

const COLUMN_MIGRATIONS: &[(&str, &str, &str)] = &[
    ("upload_items", "upload_batch_id", "TEXT"),
    ("upload_items", "upload_plan_id", "TEXT"),
    ("upload_items", "upload_metadata_json", "TEXT NOT NULL DEFAULT '{}'"),
    ("upload_items", "folder_monitor_id", "TEXT"),
    ("audit_events", "folder_monitor_id", "TEXT"),
    ("folder_monitor_observations", "monitor_id", "TEXT NOT NULL DEFAULT 'legacy-singleton'"),
    ("upload_items", "partial_path", "TEXT"),
    (
        "upload_items",
        "imported_bytes",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("upload_items", "channel_name", "TEXT"),
    ("upload_items", "channel_id", "TEXT"),
    (
        "upload_items",
        "visibility",
        "TEXT NOT NULL DEFAULT 'private'",
    ),
    ("upload_items", "upload_started_at", "TEXT"),
    ("upload_items", "transfer_bytes_per_second", "REAL"),
    (
        "upload_items",
        "made_for_kids",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("upload_items", "playlist_id", "TEXT"),
    ("upload_items", "playlist_title", "TEXT"),
    (
        "upload_items",
        "playlist_status",
        "TEXT NOT NULL DEFAULT 'not_requested'",
    ),
    ("upload_items", "duplicate_decision", "TEXT"),
    (
        "upload_items",
        "delete_source_after_upload",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("upload_items", "source_delete_status", "TEXT"),
    (
        "upload_items",
        "background_hash_status",
        "TEXT NOT NULL DEFAULT 'not_required'",
    ),
    ("upload_items", "source_modified_key", "TEXT"),
    (
        "upload_items",
        "normalized_title",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "upload_items",
        "canonical_title",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "upload_items",
        "has_copy_marker",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "upload_items",
        "numeric_title_key",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "upload_items",
        "title_keys_version",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("connection_settings", "connection_detail", "TEXT"),
    ("connection_settings", "oauth_credential_id", "TEXT"),
    ("connection_settings", "oauth_project_id", "TEXT"),
    (
        "connection_settings",
        "deletion_authorized",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("connection_settings", "deletion_sudo_until", "TEXT"),
    (
        "connection_settings",
        "manual_made_for_kids_default",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "connection_settings",
        "manual_visibility_default",
        "TEXT NOT NULL DEFAULT 'private' CHECK(manual_visibility_default IN ('private', 'unlisted', 'public'))",
    ),
    ("connection_settings", "upload_quota_pause_until", "TEXT"),
    ("connection_settings", "active_channel_id", "TEXT"),
    ("audit_events", "channel_name", "TEXT"),
    ("audit_events", "channel_id", "TEXT NOT NULL DEFAULT ''"),
    (
        "folder_monitor_settings",
        "visibility",
        "TEXT NOT NULL DEFAULT 'private'",
    ),
    (
        "folder_monitor_settings",
        "made_for_kids",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("folder_monitor_settings", "playlist_id", "TEXT"),
    ("folder_monitor_settings", "playlist_title", "TEXT"),
    ("folder_monitor_settings", "channel_id", "TEXT"),
    (
        "folder_monitor_settings",
        "delete_source_after_upload",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("preflight_scan_files", "metadata_json", "TEXT"),
    (
        "preflight_scan_files",
        "metadata_status",
        "TEXT NOT NULL DEFAULT 'pending'",
    ),
    ("remote_videos", "upload_status", "TEXT"),
    ("remote_videos", "channel_id", "TEXT NOT NULL DEFAULT ''"),
    (
        "remote_videos",
        "normalized_title",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "remote_videos",
        "canonical_title",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "remote_videos",
        "has_copy_marker",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "remote_videos",
        "numeric_title_key",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "remote_videos",
        "title_keys_version",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("remote_video_sync_staging", "upload_status", "TEXT"),
    (
        "remote_video_sync_staging",
        "channel_id",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "remote_video_sync_staging",
        "normalized_title",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "remote_video_sync_staging",
        "canonical_title",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "remote_video_sync_staging",
        "has_copy_marker",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "remote_video_sync_staging",
        "numeric_title_key",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "remote_video_sync_staging",
        "title_keys_version",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "preflight_scan_jobs",
        "matched_files",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("preflight_scan_jobs", "evidence_materialized_at", "TEXT"),
    (
        "preflight_scan_files",
        "local_matches_json",
        "TEXT NOT NULL DEFAULT '[]'",
    ),
    (
        "preflight_scan_files",
        "dropped_duplicate_names_json",
        "TEXT NOT NULL DEFAULT '[]'",
    ),
    (
        "preflight_scan_files",
        "uploaded_title_matches_json",
        "TEXT NOT NULL DEFAULT '[]'",
    ),
    (
        "preflight_scan_files",
        "local_match_count",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "preflight_scan_files",
        "dropped_duplicate_count",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "preflight_scan_files",
        "uploaded_title_match_count",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "preflight_scan_files",
        "can_delete_local_duplicate",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "deletion_requests",
        "channel_id",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "preflight_scan_jobs",
        "channel_id",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "folder_monitor_observations",
        "channel_id",
        "TEXT NOT NULL DEFAULT ''",
    ),
    (
        "ignored_duplicate_candidates",
        "channel_id",
        "TEXT NOT NULL DEFAULT ''",
    ),
];

pub(crate) fn configure_connection(
    connection: &Connection,
    busy_timeout: Duration,
) -> Result<(), String> {
    connection
        .busy_timeout(busy_timeout)
        .map_err(|error| error.to_string())?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|error| error.to_string())
}

fn schema_version(connection: &Connection) -> Result<i64, String> {
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn table_columns(transaction: &Transaction<'_>, table: &str) -> Result<HashSet<String>, String> {
    transaction
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| error.to_string())
}

fn apply_column_migrations(transaction: &Transaction<'_>) -> Result<(), String> {
    let mut columns_by_table = std::collections::HashMap::<&str, HashSet<String>>::new();
    for (table, column, definition) in COLUMN_MIGRATIONS {
        if !columns_by_table.contains_key(table) {
            columns_by_table.insert(table, table_columns(transaction, table)?);
        }
        let columns = columns_by_table
            .get_mut(table)
            .expect("migration table columns are initialized");
        if columns.insert((*column).to_string()) {
            transaction
                .execute(
                    &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                    [],
                )
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn rebuild_channel_scoped_ignored_candidates(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            "DROP TABLE IF EXISTS ignored_duplicate_candidates_v2;
             CREATE TABLE ignored_duplicate_candidates_v2 (
               channel_id TEXT NOT NULL DEFAULT '',
               candidate_id TEXT NOT NULL,
               ignored_at TEXT NOT NULL,
               PRIMARY KEY(channel_id, candidate_id)
             );
             INSERT OR IGNORE INTO ignored_duplicate_candidates_v2(channel_id, candidate_id, ignored_at)
             SELECT CASE WHEN channel_id = '' THEN COALESCE((SELECT active_channel_id FROM connection_settings WHERE singleton = 1), '') ELSE channel_id END,
                    candidate_id, ignored_at
             FROM ignored_duplicate_candidates;
             DROP TABLE ignored_duplicate_candidates;
             ALTER TABLE ignored_duplicate_candidates_v2 RENAME TO ignored_duplicate_candidates;",
        )
        .map_err(|error| error.to_string())
}

fn rebuild_channel_scoped_folder_observations(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(
            "DROP TABLE IF EXISTS folder_monitor_observations_v2;
             CREATE TABLE folder_monitor_observations_v2 (
               monitor_id TEXT NOT NULL DEFAULT 'legacy-singleton',
               channel_id TEXT NOT NULL DEFAULT '',
               channel_name TEXT NOT NULL,
               file_path TEXT NOT NULL,
               size_bytes INTEGER NOT NULL,
               modified_key TEXT NOT NULL,
               state TEXT NOT NULL,
               digest TEXT,
               upload_item_id TEXT,
               first_seen_at TEXT NOT NULL,
               updated_at TEXT NOT NULL,
               PRIMARY KEY(monitor_id, file_path)
             );
             INSERT OR REPLACE INTO folder_monitor_observations_v2(
               monitor_id, channel_id, channel_name, file_path, size_bytes, modified_key,
               state, digest, upload_item_id, first_seen_at, updated_at
             )
             SELECT monitor_id, channel_id, channel_name, file_path, size_bytes, modified_key,
                    state, digest, upload_item_id, first_seen_at, updated_at
             FROM folder_monitor_observations
             ORDER BY updated_at ASC;
             DROP TABLE folder_monitor_observations;
             ALTER TABLE folder_monitor_observations_v2 RENAME TO folder_monitor_observations;",
        )
        .map_err(|error| error.to_string())
}

fn migrate_monitor_configuration(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction.execute_batch(
        "INSERT OR IGNORE INTO folder_monitors(id, enabled, removed, folder_path, channel_name, channel_id, visibility, made_for_kids, playlist_id, playlist_title, delete_source_after_upload, status, detail, last_scan_at, last_file_name, updated_at)
         SELECT 'legacy-singleton', CASE WHEN channel_id IS NULL OR channel_id = '' THEN 0 ELSE enabled END,
                CASE WHEN channel_id IS NULL OR channel_id = '' THEN 1 ELSE 0 END,
                folder_path, channel_name, channel_id, visibility, made_for_kids, playlist_id, playlist_title,
                delete_source_after_upload, status, detail, last_scan_at, last_file_name, updated_at
         FROM folder_monitor_settings WHERE singleton = 1 AND folder_path IS NOT NULL;"
    ).map_err(|error| error.to_string())?;
    let configured: Vec<(String, Option<String>)> = transaction
        .prepare("SELECT id, folder_path FROM folder_monitors WHERE folder_key = ''")
        .map_err(|error| error.to_string())?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| error.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|error| error.to_string())?;
    for (id, path) in configured {
        if let Some(path) = path {
            transaction
                .execute(
                    "UPDATE folder_monitors SET folder_key = ?1 WHERE id = ?2",
                    rusqlite::params![super::folder_monitors::root_key(&path), id],
                )
                .map_err(|error| error.to_string())?;
        }
    }
    let observations: Vec<(String, String, String)> = transaction.prepare("SELECT channel_id, channel_name, file_path FROM folder_monitor_observations WHERE monitor_id = 'legacy-singleton' ORDER BY channel_id, file_path").map_err(|error| error.to_string())?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).map_err(|error| error.to_string())?
        .collect::<Result<_, _>>().map_err(|error| error.to_string())?;
    for (channel, name, path) in observations {
        let root = std::path::Path::new(&path)
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        let key = super::folder_monitors::root_key(&root);
        let monitor_id = transaction.query_row("SELECT id FROM folder_monitors WHERE COALESCE(channel_id, '') = ?1 AND folder_key = ?2 ORDER BY removed ASC, id ASC LIMIT 1", rusqlite::params![channel, key], |row| row.get::<_, String>(0)).optional().map_err(|error| error.to_string())?;
        let monitor_id = match monitor_id {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                transaction.execute("INSERT INTO folder_monitors(id, enabled, removed, folder_path, folder_key, channel_name, channel_id, status, detail, updated_at) VALUES(?1, 0, 1, ?2, ?3, ?4, ?5, 'removed', 'Historical watched-folder evidence retained from the earlier single-folder configuration.', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))", rusqlite::params![id, root, key, name, channel]).map_err(|error| error.to_string())?;
                id
            }
        };
        transaction.execute("UPDATE folder_monitor_observations SET monitor_id = ?1 WHERE monitor_id = 'legacy-singleton' AND channel_id = ?2 AND file_path = ?3", rusqlite::params![monitor_id, channel, path]).map_err(|error| error.to_string())?;
    }
    transaction.execute_batch(
        "UPDATE upload_items SET folder_monitor_id = COALESCE((SELECT monitor_id FROM folder_monitor_observations WHERE upload_item_id = upload_items.id AND channel_id = COALESCE(upload_items.channel_id, '') AND file_path = COALESCE(upload_items.source_path, upload_items.workspace_path) LIMIT 1), (SELECT monitor_id FROM folder_monitor_observations WHERE upload_item_id = upload_items.id AND channel_id = COALESCE(upload_items.channel_id, '') LIMIT 1))
         WHERE folder_monitor_id IS NULL AND EXISTS(SELECT 1 FROM folder_monitor_observations WHERE upload_item_id = upload_items.id AND channel_id = COALESCE(upload_items.channel_id, ''));
         UPDATE audit_events SET folder_monitor_id = (SELECT folder_monitor_id FROM upload_items WHERE id = audit_events.item_id) WHERE folder_monitor_id IS NULL AND item_id IS NOT NULL;
         UPDATE audit_events SET folder_monitor_id = 'legacy-singleton' WHERE folder_monitor_id IS NULL AND item_id IS NULL AND kind LIKE 'folder_monitor_%' AND EXISTS(SELECT 1 FROM folder_monitors WHERE id = 'legacy-singleton' AND COALESCE(channel_id, '') = audit_events.channel_id);"
    ).map_err(|error| error.to_string())
}

fn install_revision_schema(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction.execute_batch("DROP TRIGGER IF EXISTS state_quota_insert; DROP TRIGGER IF EXISTS state_quota_update; DROP TRIGGER IF EXISTS state_quota_delete;").map_err(|error| error.to_string())?;
    transaction
        .execute_batch(CURRENT_TABLES_SQL)
        .map_err(|error| error.to_string())?;
    apply_column_migrations(transaction)?;
    install_upload_workflow_schema(transaction)?;

    transaction
        .execute(
            "UPDATE preflight_scan_jobs SET channel_id = COALESCE(NULLIF(channel_id, ''), (SELECT active_channel_id FROM connection_settings WHERE singleton = 1), '')",
            [],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE audit_events SET channel_id = COALESCE(NULLIF(channel_id, ''), (SELECT channel_id FROM upload_items WHERE upload_items.id = audit_events.item_id), (SELECT channel_id FROM folder_monitor_settings WHERE folder_monitor_settings.channel_name = audit_events.channel_name), '')",
            [],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE folder_monitor_observations SET channel_id = COALESCE(NULLIF(channel_id, ''), (SELECT channel_id FROM upload_items WHERE upload_items.id = folder_monitor_observations.upload_item_id), (SELECT channel_id FROM folder_monitor_settings WHERE folder_monitor_settings.channel_name = folder_monitor_observations.channel_name), '')",
            [],
        )
        .map_err(|error| error.to_string())?;

    rebuild_channel_scoped_ignored_candidates(transaction)?;
    migrate_monitor_configuration(transaction)?;
    rebuild_channel_scoped_folder_observations(transaction)?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO upload_quota_pauses(channel_id, pause_until, updated_at)
             SELECT active_channel_id, upload_quota_pause_until, updated_at
             FROM connection_settings
             WHERE singleton = 1 AND active_channel_id IS NOT NULL AND active_channel_id != '' AND upload_quota_pause_until IS NOT NULL",
            [],
        )
        .map_err(|error| error.to_string())?;

    transaction
        .execute_batch(STATE_CHANGE_TRIGGERS_SQL)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn migrate_zero_to_current(transaction: &Transaction<'_>) -> Result<(), String> {
    install_revision_schema(transaction)?;

    // These names existed in historical schemas with display-name/global
    // definitions. Recreate them transactionally with immutable channel keys.
    transaction
        .execute_batch(
            "DROP INDEX IF EXISTS preflight_scan_files_job_status_idx;
             DROP INDEX IF EXISTS preflight_scan_jobs_status_created_idx;
             DROP INDEX IF EXISTS audit_events_channel_created_idx;
             DROP INDEX IF EXISTS folder_monitor_observation_state_idx;
             DROP INDEX IF EXISTS folder_monitor_observation_updated_idx;",
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(CURRENT_INDEXES_SQL)
        .map_err(|error| error.to_string())
}

fn has_user_tables(transaction: &Transaction<'_>) -> Result<bool, String> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .map_err(|error| error.to_string())
}

/// A genuinely empty database needs no compatibility probes, data backfills,
/// or legacy table/index rebuilds. Keep this distinct from an unversioned
/// version-0 database, which may contain records from an older release.
fn install_current_schema_into_empty_database(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(CURRENT_TABLES_SQL)
        .map_err(|error| error.to_string())?;
    install_upload_workflow_schema(transaction)?;
    transaction
        .execute_batch(STATE_CHANGE_TRIGGERS_SQL)
        .map_err(|error| error.to_string())?;
    // Keep the unique root invariant in the cold transaction. Secondary
    // indexes are installed after the safe shell and before recovery/dispatch.
    transaction.execute_batch("CREATE UNIQUE INDEX IF NOT EXISTS folder_monitors_live_root_idx ON folder_monitors(folder_key) WHERE removed=0 AND folder_key!='';").map_err(|error| error.to_string())
}

pub(crate) fn ensure_runtime_indexes(connection: &mut Connection) -> Result<bool, String> {
    let ready = |connection: &Connection| -> Result<bool, String> {
        connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='index' AND name='duplicate_candidate_projection_channel_idx')", [], |row| row.get(0)).map_err(|error| error.to_string())
    };
    if ready(connection)? {
        return Ok(false);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    if ready(&transaction)? {
        return Ok(false);
    }
    transaction
        .execute_batch(CURRENT_INDEXES_SQL)
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(true)
}

fn migrate_one_to_two(transaction: &Transaction<'_>) -> Result<(), String> {
    install_revision_schema(transaction)?;
    transaction
        .execute_batch(
            "DROP INDEX IF EXISTS preflight_scan_jobs_status_created_idx;
             DROP INDEX IF EXISTS audit_events_channel_created_idx;
             DROP INDEX IF EXISTS folder_monitor_observation_state_idx;
             DROP INDEX IF EXISTS folder_monitor_observation_updated_idx;",
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(CURRENT_INDEXES_SQL)
        .map_err(|error| error.to_string())
}

fn migrate_two_to_three(transaction: &Transaction<'_>) -> Result<(), String> {
    install_revision_schema(transaction)?;
    transaction
        .execute_batch(CURRENT_INDEXES_SQL)
        .map_err(|error| error.to_string())
}

fn migrate_three_to_four(transaction: &Transaction<'_>) -> Result<(), String> {
    install_revision_schema(transaction)?;
    transaction
        .execute_batch("DROP TRIGGER IF EXISTS state_preflight_file_update;")
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(STATE_CHANGE_TRIGGERS_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(CURRENT_INDEXES_SQL)
        .map_err(|error| error.to_string())
}

fn migrate_four_to_five(transaction: &Transaction<'_>) -> Result<(), String> {
    // Add only metadata/pointers. Protected credentials and existing queued
    // source/checkpoint records remain at their original locations.
    install_revision_schema(transaction)?;
    transaction
        .execute_batch(CURRENT_INDEXES_SQL)
        .map_err(|error| error.to_string())
}

fn migrate_five_to_six(transaction: &Transaction<'_>) -> Result<(), String> {
    install_revision_schema(transaction)?;
    transaction
        .execute_batch(CURRENT_INDEXES_SQL)
        .map_err(|error| error.to_string())
}

fn migrate_six_to_seven(transaction: &Transaction<'_>) -> Result<(), String> {
    install_revision_schema(transaction)?;
    transaction
        .execute_batch(CURRENT_INDEXES_SQL)
        .map_err(|error| error.to_string())
}

fn install_upload_workflow_schema(transaction: &Transaction<'_>) -> Result<(), String> {
    transaction
        .execute_batch(super::perceptual_review::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::channel_profiles::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::channel_profiles::STATE_TRIGGERS_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::upload_retry::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::publication_schedule::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::desktop_presence::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::upload_controls::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::video_assets::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::folder_rules::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::upload_plan_execution::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::quota_planner::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    super::quota_planner::install_cost_table(transaction)?;
    transaction
        .execute_batch(super::source_relink::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::upload_completion::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(super::upload_metadata::SCHEMA_SQL)
        .map_err(|error| error.to_string())?;
    transaction
        .execute_batch(UPLOAD_WORKFLOW_STATE_TRIGGERS_SQL)
        .map_err(|error| error.to_string())
}

const UPLOAD_WORKFLOW_STATE_TRIGGERS_SQL: &str = "CREATE TRIGGER IF NOT EXISTS state_completion_insert AFTER INSERT ON upload_completion_checks BEGIN INSERT INTO state_changes(channel_id,surface,entity_id,event_kind,created_at) VALUES(NEW.channel_id,'completion',NEW.item_id,'invalidate',strftime('%Y-%m-%dT%H:%M:%fZ','now')); END; CREATE TRIGGER IF NOT EXISTS state_completion_update AFTER UPDATE ON upload_completion_checks BEGIN INSERT INTO state_changes(channel_id,surface,entity_id,event_kind,created_at) VALUES(NEW.channel_id,'completion',NEW.item_id,'invalidate',strftime('%Y-%m-%dT%H:%M:%fZ','now')); END;";

fn migrate_seven_to_eight(transaction: &Transaction<'_>) -> Result<(), String> {
    install_revision_schema(transaction)
}

/// Applies persistent database settings and all pending schema work exactly
/// once. Hot connections never call this function.
pub(crate) fn migrate_database(connection: &mut Connection) -> Result<bool, String> {
    let version = schema_version(connection)?;
    if version > DATABASE_SCHEMA_VERSION {
        return Err(format!(
            "This local database uses schema version {version}, but this app supports up to {DATABASE_SCHEMA_VERSION}."
        ));
    }
    if version == DATABASE_SCHEMA_VERSION {
        return Ok(false);
    }

    let journal_mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err("The local database could not enable WAL mode.".into());
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let locked_version = schema_version(&transaction)?;
    if locked_version > DATABASE_SCHEMA_VERSION {
        return Err(format!(
            "This local database uses schema version {locked_version}, but this app supports up to {DATABASE_SCHEMA_VERSION}."
        ));
    }
    match locked_version {
        0 if !has_user_tables(&transaction)? => {
            install_current_schema_into_empty_database(&transaction)?
        }
        0 => migrate_zero_to_current(&transaction)?,
        1 => migrate_one_to_two(&transaction)?,
        2 => migrate_two_to_three(&transaction)?,
        3 => migrate_three_to_four(&transaction)?,
        4 => migrate_four_to_five(&transaction)?,
        5 => migrate_five_to_six(&transaction)?,
        6 => migrate_six_to_seven(&transaction)?,
        7 => migrate_seven_to_eight(&transaction)?,
        DATABASE_SCHEMA_VERSION => {}
        other => {
            return Err(format!(
                "No safe migration path exists from local schema version {other}."
            ));
        }
    }
    transaction
        .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION)
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::OptionalExtension;

    #[test]
    fn deferred_secondary_indexes_commit_before_worker_readiness_and_do_not_repeat() {
        let (root, mut connection) = test_database("deferred-indexes");
        migrate_database(&mut connection).unwrap();
        let unique: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE type='index' AND name='folder_monitors_live_root_idx'", [], |row| row.get(0)).unwrap();
        assert_eq!(unique, 1);
        assert!(ensure_runtime_indexes(&mut connection).unwrap());
        let installed: std::collections::HashSet<String> = connection
            .prepare("SELECT name FROM sqlite_schema WHERE type='index'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for declaration in CURRENT_INDEXES_SQL
            .split(';')
            .filter(|sql| sql.contains("CREATE INDEX") || sql.contains("CREATE UNIQUE INDEX"))
        {
            let words = declaration.split_whitespace().collect::<Vec<_>>();
            let position = words.iter().position(|word| *word == "EXISTS").unwrap() + 1;
            assert!(
                installed.contains(words[position]),
                "Required runtime index missing: {}",
                words[position]
            );
        }
        assert!(!ensure_runtime_indexes(&mut connection).unwrap());
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deferred_index_failure_rolls_back_and_can_resume_without_changing_queue_evidence() {
        let (root, mut connection) = test_database("deferred-index-failure");
        migrate_database(&mut connection).unwrap();
        connection.execute("INSERT INTO upload_items(id,title,file_name,workspace_path,size_bytes,total_bytes,status,channel_id,confirmed_bytes,resumable_session_uri,created_at,updated_at) VALUES('saved','Clip','clip.mp4','original',10,10,'queued','channel-a',3,'protected-checkpoint','fixture','fixture')", []).unwrap();
        connection
            .execute_batch("CREATE TABLE upload_items_status_created_idx (fault INTEGER);")
            .unwrap();
        assert!(ensure_runtime_indexes(&mut connection).is_err());
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE type='index' AND name='upload_items_status_idx'", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        connection
            .execute_batch("DROP TABLE upload_items_status_created_idx;")
            .unwrap();
        assert!(ensure_runtime_indexes(&mut connection).unwrap());
        let saved: (String,String,i64,String) = connection.query_row("SELECT status,workspace_path,confirmed_bytes,resumable_session_uri FROM upload_items WHERE id='saved'", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
        assert_eq!(
            saved,
            (
                "queued".into(),
                "original".into(),
                3,
                "protected-checkpoint".into()
            )
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_six_migration_keeps_same_path_channel_history_and_past_roots_without_filesystem_reads(
    ) {
        let (root, mut connection) = test_database("multi-folders-v7");
        migrate_database(&mut connection).unwrap();
        ensure_runtime_indexes(&mut connection).unwrap();
        let first_root = root.join("nonexistent-original-root");
        let same_path = first_root
            .join("same-path.mp4")
            .to_string_lossy()
            .to_string();
        let old_path = root
            .join("nonexistent-past-root")
            .join("past.mp4")
            .to_string_lossy()
            .to_string();
        connection.execute_batch(
            "DROP TABLE folder_monitors;
             DROP TABLE folder_monitor_observations;
             DROP INDEX folder_monitor_audit_idx;
             ALTER TABLE upload_items DROP COLUMN folder_monitor_id;
             ALTER TABLE audit_events DROP COLUMN folder_monitor_id;
             CREATE TABLE folder_monitor_observations(channel_id TEXT NOT NULL DEFAULT '', channel_name TEXT NOT NULL, file_path TEXT NOT NULL, size_bytes INTEGER NOT NULL, modified_key TEXT NOT NULL, state TEXT NOT NULL, digest TEXT, upload_item_id TEXT, first_seen_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(channel_id, file_path));
             INSERT INTO connection_settings(singleton, active_channel, active_channel_id, manual_made_for_kids_default, manual_visibility_default, updated_at) VALUES(1, 'Existing channel', 'channel-a', 1, 'unlisted', 'fixture');
             PRAGMA user_version = 6;"
        ).unwrap();
        connection.execute("INSERT INTO folder_monitor_settings(singleton, enabled, folder_path, channel_name, channel_id, visibility, made_for_kids, playlist_id, playlist_title, status, detail, last_scan_at, last_file_name, updated_at) VALUES(1, 1, ?1, 'Existing channel', 'channel-a', 'unlisted', 1, 'saved-playlist', 'Saved playlist', 'watching', 'Original status', 'saved-scan', 'same-path.mp4', 'fixture')", [first_root.to_string_lossy().to_string()]).unwrap();
        connection.execute("INSERT INTO upload_items(id, title, file_name, channel_name, channel_id, workspace_path, source_path, size_bytes, status, confirmed_bytes, total_bytes, resumable_session_uri, created_at, updated_at) VALUES('saved-item', 'Saved video', 'same-path.mp4', 'Existing channel', 'channel-a', ?1, ?1, 1000, 'queued', 400, 1000, 'legacy-protected-reference', 'fixture', 'fixture')", [&same_path]).unwrap();
        for (channel, path, item) in [
            ("channel-a", same_path.as_str(), Some("saved-item")),
            ("channel-b", same_path.as_str(), None),
            ("channel-a", old_path.as_str(), None),
        ] {
            connection.execute("INSERT INTO folder_monitor_observations(channel_id, channel_name, file_path, size_bytes, modified_key, state, upload_item_id, first_seen_at, updated_at) VALUES(?1, 'Existing channel', ?2, 1000, 'original-signature', 'queued', ?3, 'first-seen', 'fixture')", rusqlite::params![channel, path, item]).unwrap();
        }
        connection.execute("INSERT INTO audit_events(id, channel_id, kind, detail, created_at) VALUES('old-folder-log', 'channel-a', 'folder_monitor_enabled', 'Original audit detail', 'fixture')", []).unwrap();
        assert!(!first_root.exists());
        assert!(migrate_database(&mut connection).unwrap());
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM folder_monitor_observations",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
        let ids: Vec<String> = connection.prepare("SELECT monitor_id FROM folder_monitor_observations WHERE file_path = ?1 ORDER BY channel_id").unwrap().query_map([&same_path], |row| row.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert_eq!(ids[0], "legacy-singleton");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM folder_monitors WHERE removed = 1 AND enabled = 0",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
        let original: (bool, String, bool, String, String, String) = connection.query_row("SELECT enabled, visibility, made_for_kids, playlist_id, last_scan_at, last_file_name FROM folder_monitors WHERE id = 'legacy-singleton'", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))).unwrap();
        assert_eq!(
            original,
            (
                true,
                "unlisted".into(),
                true,
                "saved-playlist".into(),
                "saved-scan".into(),
                "same-path.mp4".into()
            )
        );
        let queue: (String, i64, String, String) = connection.query_row("SELECT workspace_path, confirmed_bytes, resumable_session_uri, folder_monitor_id FROM upload_items WHERE id = 'saved-item'", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap();
        assert_eq!(
            queue,
            (
                same_path,
                400,
                "legacy-protected-reference".into(),
                "legacy-singleton".into()
            )
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT folder_monitor_id FROM audit_events WHERE id = 'old-folder-log'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "legacy-singleton"
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_five_migration_preserves_existing_defaults_credentials_and_upload_checkpoints() {
        let (root, mut connection) = test_database("upload-defaults-v6");
        migrate_database(&mut connection).unwrap();
        connection.execute_batch(
            "ALTER TABLE connection_settings DROP COLUMN manual_visibility_default;
             DROP TABLE manual_playlist_defaults;
             INSERT INTO connection_settings(singleton, oauth_client_id, oauth_credential_id, oauth_project_id, active_channel, active_channel_id, manual_made_for_kids_default, updated_at)
             VALUES(1, 'existing.apps.googleusercontent.com', 'protected-pointer', 'existing-project', 'Existing channel', 'channel-a', 1, 'fixture');
             INSERT INTO upload_items(id, title, file_name, workspace_path, channel_id, size_bytes, status, confirmed_bytes, total_bytes, resumable_session_uri, created_at, updated_at)
             VALUES('saved', 'Saved video', 'original.mp4', 'original-source', 'channel-a', 1000, 'queued', 400, 1000, 'legacy-protected-reference', 'fixture', 'fixture');
             INSERT INTO upload_quota_pauses(channel_id, pause_until, updated_at)
             VALUES('channel-a', '2099-01-01T00:00:00+00:00', 'fixture');
             INSERT INTO api_quota_pauses(quota_scope, channel_id, pause_until, updated_at)
             VALUES('project:existing-project', 'channel-a', '2099-01-01T00:00:00+00:00', 'fixture');
             PRAGMA user_version = 5;"
        ).unwrap();
        assert!(migrate_database(&mut connection).unwrap());
        let defaults: (i64, String, String, String) = connection.query_row(
            "SELECT manual_made_for_kids_default, manual_visibility_default, oauth_credential_id, oauth_project_id FROM connection_settings WHERE singleton = 1", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        ).unwrap();
        assert_eq!(
            defaults,
            (
                1,
                "private".into(),
                "protected-pointer".into(),
                "existing-project".into()
            )
        );
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        let queue: (String, String, i64, String) = connection.query_row("SELECT status, workspace_path, confirmed_bytes, resumable_session_uri FROM upload_items WHERE id = 'saved'", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap();
        assert_eq!(
            queue,
            (
                "queued".into(),
                "original-source".into(),
                400,
                "legacy-protected-reference".into()
            )
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM manual_playlist_defaults", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM api_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_four_migration_preserves_queue_and_channel_pause_while_adding_safe_credential_pointer(
    ) {
        let (root, mut connection) = test_database("oauth-pointer-v5");
        migrate_database(&mut connection).unwrap();
        connection.execute_batch(
            "ALTER TABLE connection_settings DROP COLUMN oauth_credential_id;
             ALTER TABLE connection_settings DROP COLUMN oauth_project_id;
             DROP TABLE api_quota_pauses;
             INSERT INTO connection_settings(singleton, oauth_client_id, active_channel, active_channel_id, updated_at)
             VALUES(1, 'legacy.apps.googleusercontent.com', 'Existing channel', 'channel-a', 'fixture');
             INSERT INTO upload_items(id, title, file_name, workspace_path, channel_id, size_bytes, status, confirmed_bytes, total_bytes, created_at, updated_at)
             VALUES('saved', 'Saved video', 'original.mp4', 'original-source', 'channel-a', 1000, 'queued', 400, 1000, 'fixture', 'fixture');
             INSERT INTO upload_quota_pauses(channel_id, pause_until, updated_at)
             VALUES('channel-a', '2099-01-01T00:00:00+00:00', 'fixture');
             PRAGMA user_version = 4;"
        ).unwrap();
        assert!(migrate_database(&mut connection).unwrap());
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        let settings: (String, String, Option<String>, Option<String>) = connection.query_row(
            "SELECT oauth_client_id, active_channel_id, oauth_credential_id, oauth_project_id FROM connection_settings WHERE singleton = 1", [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        ).unwrap();
        assert_eq!(
            settings,
            (
                "legacy.apps.googleusercontent.com".into(),
                "channel-a".into(),
                None,
                None
            )
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT confirmed_bytes FROM upload_items WHERE id = 'saved'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            400
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM upload_quota_pauses WHERE channel_id = 'channel-a'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM api_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let cursor: i64 = connection
            .query_row(
                "SELECT COALESCE(MAX(revision), 0) FROM state_changes",
                [],
                |row| row.get(0),
            )
            .unwrap();
        connection.execute("UPDATE upload_quota_pauses SET pause_until = '2099-01-02T00:00:00+00:00' WHERE channel_id = 'channel-a'", []).unwrap();
        assert!(connection.query_row("SELECT EXISTS(SELECT 1 FROM state_changes WHERE revision > ?1 AND channel_id = 'channel-a' AND surface = 'connection')", [cursor], |row| row.get::<_, bool>(0)).unwrap());
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn test_database(label: &str) -> (std::path::PathBuf, Connection) {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-persistence-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let connection = Connection::open(root.join("queue.sqlite3")).unwrap();
        (root, connection)
    }

    fn query_plan(connection: &Connection, sql: &str, parameter: &str) -> String {
        connection
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap()
            .query_map([parameter], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
    }

    #[test]
    fn empty_database_migrates_to_the_current_schema_once() {
        let (root, mut connection) = test_database("empty");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        assert!(migrate_database(&mut connection).unwrap());
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'upload_items'", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(feature = "performance-harness")]
    #[test]
    fn empty_database_uses_the_direct_current_schema_path() {
        let (root, mut connection) = test_database("empty-direct");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        connection.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(|event| record_test_database_statement(&event)),
        );
        begin_test_database_trace();

        assert!(migrate_database(&mut connection).unwrap());
        let statements = take_test_database_trace();

        assert!(
            statements.len() <= 110,
            "{} schema statements; expected at most 110.\n{statements:#?}",
            statements.len()
        );
        assert!(!statements
            .iter()
            .any(|sql| sql.contains("PRAGMA table_info")));
        assert!(!statements
            .iter()
            .any(|sql| sql.starts_with("UPDATE preflight_scan_jobs")));
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        let expected_state_triggers = STATE_CHANGE_TRIGGERS_SQL
            .matches("CREATE TRIGGER IF NOT EXISTS state_")
            .count() as i64
            + UPLOAD_WORKFLOW_STATE_TRIGGERS_SQL
                .matches("CREATE TRIGGER IF NOT EXISTS state_")
                .count() as i64;
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'trigger' AND name LIKE 'state_%'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            expected_state_triggers
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(feature = "performance-harness")]
    #[test]
    fn unversioned_database_with_a_user_table_keeps_the_legacy_path() {
        let (root, mut connection) = test_database("unversioned-legacy");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE upload_items (
                   id TEXT PRIMARY KEY NOT NULL,
                   title TEXT NOT NULL,
                   file_name TEXT NOT NULL,
                   source_path TEXT,
                   workspace_path TEXT NOT NULL,
                   partial_path TEXT,
                   size_bytes INTEGER NOT NULL,
                   digest TEXT,
                   status TEXT NOT NULL,
                   confirmed_bytes INTEGER NOT NULL DEFAULT 0,
                   imported_bytes INTEGER NOT NULL DEFAULT 0,
                   total_bytes INTEGER NOT NULL,
                   resumable_session_uri TEXT,
                   video_id TEXT,
                   detail TEXT,
                   created_at TEXT NOT NULL,
                   updated_at TEXT NOT NULL
                 );
                 INSERT INTO upload_items (
                   id, title, file_name, workspace_path, size_bytes, status,
                   confirmed_bytes, total_bytes, detail, created_at, updated_at
                 ) VALUES (
                   'preserved', 'Preserved', 'preserved.mp4', 'managed', 10,
                   'uploading', 5, 10, 'checkpoint retained', 'fixture', 'fixture'
                 );",
            )
            .unwrap();

        assert!(migrate_database(&mut connection).unwrap());

        assert_eq!(
            connection
                .query_row(
                    "SELECT status || ':' || confirmed_bytes || ':' || total_bytes || ':' || detail || ':' || visibility FROM upload_items WHERE id = 'preserved'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "uploading:5:10:checkpoint retained:private"
        );
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn current_schema_is_an_explicit_no_migration_path() {
        let (root, mut connection) = test_database("current");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        assert!(migrate_database(&mut connection).unwrap());
        let schema_cookie_before = connection
            .query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0))
            .unwrap();
        assert!(!migrate_database(&mut connection).unwrap());
        let schema_cookie_after = connection
            .query_row("PRAGMA schema_version", [], |row| row.get::<_, i64>(0))
            .unwrap();
        assert_eq!(schema_cookie_after, schema_cookie_before);
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_seven_workflow_migration_retains_queue_recovery_and_profile_evidence() {
        let (root, mut connection) = test_database("workflow-v8");
        migrate_database(&mut connection).unwrap();
        connection.execute_batch(
            "DROP TRIGGER state_completion_insert;
             DROP TRIGGER state_completion_update;
             DROP TABLE upload_completion_checks;
             DROP TABLE source_relink_reviews;
             DROP TABLE upload_metadata_presets;
             ALTER TABLE upload_items DROP COLUMN upload_metadata_json;
             INSERT INTO connection_settings(singleton, active_channel, active_channel_id, oauth_credential_id, oauth_project_id, manual_made_for_kids_default, manual_visibility_default, updated_at)
             VALUES(1, 'Saved channel', 'channel-a', 'protected-pointer', 'saved-project', 1, 'unlisted', 'fixture');
             INSERT INTO upload_items(id, title, file_name, source_path, workspace_path, channel_id, size_bytes, status, confirmed_bytes, total_bytes, resumable_session_uri, created_at, updated_at)
             VALUES('saved', 'Approved title', 'original.mp4', 'unavailable-original', 'unavailable-original', 'channel-a', 1000, 'needs_reconciliation', 400, 1000, 'protected-checkpoint-reference', 'fixture', 'fixture');
             INSERT INTO upload_quota_pauses(channel_id, pause_until, updated_at) VALUES('channel-a', '2099-01-01T00:00:00+00:00', 'fixture');
             INSERT INTO folder_monitors(id, folder_key, folder_path, channel_id, enabled, updated_at) VALUES('saved-monitor', 'saved-root', 'unavailable-root', 'channel-a', 1, 'fixture');
             INSERT INTO folder_monitor_observations(monitor_id, channel_id, channel_name, file_path, size_bytes, modified_key, state, upload_item_id, first_seen_at, updated_at)
             VALUES('saved-monitor', 'channel-a', 'Saved channel', 'unavailable-original', 1000, 'saved-signature', 'queued', 'saved', 'first-seen', 'fixture');
             PRAGMA user_version = 7;"
        ).unwrap();
        assert!(migrate_database(&mut connection).unwrap());
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        let retained: (String, String, String, i64, String, String) = connection.query_row("SELECT title, channel_id, source_path, confirmed_bytes, resumable_session_uri, upload_metadata_json FROM upload_items WHERE id='saved'", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).unwrap();
        assert_eq!(
            retained,
            (
                "Approved title".into(),
                "channel-a".into(),
                "unavailable-original".into(),
                400,
                "protected-checkpoint-reference".into(),
                "{}".into()
            )
        );
        let profile: (String, String) = connection.query_row("SELECT oauth_credential_id, oauth_project_id FROM connection_settings WHERE singleton=1", [], |row| Ok((row.get(0)?,row.get(1)?))).unwrap();
        assert_eq!(
            profile,
            ("protected-pointer".into(), "saved-project".into())
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT manual_visibility_default FROM connection_settings WHERE singleton=1",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "unlisted"
        );
        assert_eq!(connection.query_row("SELECT state FROM folder_monitor_observations WHERE monitor_id='saved-monitor' AND upload_item_id='saved'", [], |row| row.get::<_, String>(0)).unwrap(), "queued");
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM upload_quota_pauses WHERE channel_id='channel-a'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert!(!root.join("unavailable-original").exists());
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn future_schema_version_is_rejected_without_modification() {
        let (root, mut connection) = test_database("future");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        connection
            .pragma_update(None, "user_version", DATABASE_SCHEMA_VERSION + 1)
            .unwrap();
        let error = migrate_database(&mut connection).unwrap_err();
        assert!(error.contains("supports up to"), "{error}");
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION + 1
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_is_transactional_and_preserves_initial_release_records() {
        let (root, mut connection) = test_database("migration");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        connection.execute_batch(
            "CREATE TABLE upload_items (id TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL, file_name TEXT NOT NULL, source_path TEXT, workspace_path TEXT NOT NULL, partial_path TEXT, size_bytes INTEGER NOT NULL, digest TEXT, status TEXT NOT NULL, confirmed_bytes INTEGER NOT NULL DEFAULT 0, imported_bytes INTEGER NOT NULL DEFAULT 0, total_bytes INTEGER NOT NULL, resumable_session_uri TEXT, video_id TEXT, detail TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE connection_settings (singleton INTEGER PRIMARY KEY, oauth_client_id TEXT, active_channel TEXT, connection_detail TEXT, deletion_authorized INTEGER NOT NULL DEFAULT 0, updated_at TEXT NOT NULL);
             CREATE TABLE remote_videos (video_id TEXT PRIMARY KEY NOT NULL, channel_name TEXT NOT NULL, title TEXT NOT NULL, duration TEXT, privacy_status TEXT, updated_at TEXT NOT NULL);
             CREATE TABLE deletion_requests (id TEXT PRIMARY KEY NOT NULL, video_id TEXT NOT NULL UNIQUE, title TEXT NOT NULL, status TEXT NOT NULL, detail TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE audit_events (id TEXT PRIMARY KEY NOT NULL, item_id TEXT, kind TEXT NOT NULL, detail TEXT, created_at TEXT NOT NULL, FOREIGN KEY(item_id) REFERENCES upload_items(id));
             INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, status, confirmed_bytes, total_bytes, resumable_session_uri, detail, created_at, updated_at) VALUES ('queued', 'Queued', 'queued.mp4', 'managed', 10, 'uploading', 5, 10, NULL, 'checkpoint retained', '2026-01-01', '2026-01-01');
             INSERT INTO audit_events (id, item_id, kind, detail, created_at) VALUES ('audit-1', 'queued', 'upload_started', 'safe detail', '2026-01-01');"
        ).unwrap();

        assert!(migrate_database(&mut connection).unwrap());
        assert!(!migrate_database(&mut connection).unwrap());
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        assert_eq!(
            connection.query_row("SELECT status || ':' || confirmed_bytes || ':' || total_bytes || ':' || detail FROM upload_items WHERE id = 'queued'", [], |row| row.get::<_, String>(0)).unwrap(),
            "uploading:5:10:checkpoint retained"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT item_id || ':' || kind FROM audit_events WHERE id = 'audit-1'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "queued:upload_started"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT visibility FROM upload_items WHERE id = 'queued'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "private"
        );
        assert_eq!(
            connection
                .query_row("PRAGMA foreign_key_check", [], |_| Ok(0_i64))
                .optional()
                .unwrap(),
            None
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn version_one_ignored_candidates_are_preserved_and_bound_to_the_active_channel() {
        let (root, mut connection) = test_database("v1-state-revisions");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE connection_settings (
                   singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                   active_channel TEXT,
                   active_channel_id TEXT,
                   updated_at TEXT NOT NULL
                 );
                 CREATE TABLE ignored_duplicate_candidates (
                   candidate_id TEXT PRIMARY KEY NOT NULL,
                   ignored_at TEXT NOT NULL
                 );
                 INSERT INTO connection_settings(singleton, active_channel, active_channel_id, updated_at)
                 VALUES(1, 'Channel A', 'channel-a', 'fixture');
                 INSERT INTO ignored_duplicate_candidates(candidate_id, ignored_at)
                 VALUES('candidate-a', 'fixture');
                 PRAGMA user_version = 1;",
            )
            .unwrap();

        assert!(migrate_database(&mut connection).unwrap());
        assert_eq!(
            connection
                .query_row(
                    "SELECT channel_id || ':' || candidate_id FROM ignored_duplicate_candidates",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
            "channel-a:candidate-a"
        );
        assert_eq!(
            schema_version(&connection).unwrap(),
            DATABASE_SCHEMA_VERSION
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hot_query_plans_use_channel_and_status_indexes_at_10000_records() {
        let (root, mut connection) = test_database("plans");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        migrate_database(&mut connection).unwrap();
        ensure_runtime_indexes(&mut connection).unwrap();
        connection.execute_batch(
            "WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM n WHERE value < 10000)
             INSERT INTO upload_items (id, title, file_name, channel_id, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at)
             SELECT printf('item-%05d', value), 'fixture', 'fixture.mp4', 'channel-a', 'managed', 1, printf('%064d', value), CASE WHEN value % 3 = 0 THEN 'queued' ELSE 'uploaded' END, 1, printf('%05d', value), printf('%05d', value) FROM n;
             WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM n WHERE value < 10000)
             INSERT INTO remote_videos (video_id, channel_name, channel_id, title, upload_status, updated_at)
             SELECT printf('video-%05d', value), 'Channel A', 'channel-a', 'fixture', 'processed', printf('%05d', value) FROM n;
             WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM n WHERE value < 10000)
             INSERT INTO deletion_requests (id, video_id, channel_id, title, status, detail, created_at, updated_at)
             SELECT printf('delete-%05d', value), printf('delete-video-%05d', value), 'channel-a', 'fixture', 'pending', 'fixture', printf('%05d', value), printf('%05d', value) FROM n;
             WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM n WHERE value < 10000)
             INSERT INTO audit_events (id, channel_name, channel_id, kind, detail, created_at)
             SELECT printf('audit-%05d', value), 'Channel A', 'channel-a', 'folder_monitor_fixture', 'fixture', printf('%05d', value) FROM n;
             WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM n WHERE value < 10000)
             INSERT INTO folder_monitor_observations (channel_id, channel_name, file_path, size_bytes, modified_key, state, first_seen_at, updated_at)
             SELECT 'channel-a', 'Channel A', printf('fixture-%05d.mp4', value), 1, printf('%05d', value), 'observed', printf('%05d', value), printf('%05d', value) FROM n;
             WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM n WHERE value < 10000)
             INSERT INTO preflight_scan_jobs (id, channel_id, mode, status, total_files, created_at, updated_at)
             SELECT printf('job-%05d', value), 'channel-a', 'light', 'queued', 1, printf('%05d', value), printf('%05d', value) FROM n;
             WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value + 1 FROM n WHERE value < 10000)
             INSERT INTO preflight_scan_files (job_id, ordinal, source_locator, file_name, status, metadata_status)
             SELECT 'job-00001', value, 'fixture', printf('fixture-%05d.mp4', value), 'complete', 'pending' FROM n;
             ANALYZE;"
        ).unwrap();

        let dashboard = query_plan(
            &connection,
            "SELECT id FROM upload_items WHERE status != 'cancelled' AND channel_id = ?1 ORDER BY updated_at DESC",
            "channel-a",
        );
        assert!(
            dashboard.contains("upload_items_channel_updated_active_idx"),
            "{dashboard}"
        );

        let queue = query_plan(
            &connection,
            "SELECT id FROM upload_items WHERE status = ?1 ORDER BY created_at ASC LIMIT 32",
            "queued",
        );
        assert!(queue.contains("upload_items_status_created_idx"), "{queue}");

        let inventory = query_plan(
            &connection,
            "SELECT video_id FROM remote_videos WHERE channel_id = ?1 ORDER BY updated_at DESC, title ASC",
            "channel-a",
        );
        assert!(
            inventory.contains("remote_videos_channel_updated_idx"),
            "{inventory}"
        );

        let duplicate_lookup = connection
            .prepare("EXPLAIN QUERY PLAN SELECT video_id FROM remote_videos WHERE channel_id = ?1 AND upload_status = 'processed' ORDER BY video_id ASC")
            .unwrap()
            .query_map(["channel-a"], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        assert!(
            duplicate_lookup.contains("remote_videos_channel_status_video_idx"),
            "{duplicate_lookup}"
        );

        let deletion = query_plan(
            &connection,
            "SELECT id FROM deletion_requests WHERE channel_id = ?1 ORDER BY updated_at DESC",
            "channel-a",
        );
        assert!(
            deletion.contains("deletion_requests_channel_updated_idx"),
            "{deletion}"
        );

        let folder = query_plan(
            &connection,
            "SELECT file_path FROM folder_monitor_observations WHERE channel_id = ?1 ORDER BY updated_at DESC LIMIT 200",
            "channel-a",
        );
        assert!(
            folder.contains("folder_monitor_observation_updated_idx"),
            "{folder}"
        );

        let audit = query_plan(
            &connection,
            "SELECT kind FROM audit_events WHERE channel_id = ?1 AND kind LIKE 'folder_monitor_%' ORDER BY created_at DESC LIMIT 200",
            "channel-a",
        );
        assert!(
            audit.contains("audit_events_channel_created_idx"),
            "{audit}"
        );

        let jobs = query_plan(
            &connection,
            "SELECT id FROM preflight_scan_jobs WHERE channel_id = 'channel-a' AND status = ?1 ORDER BY created_at ASC",
            "queued",
        );
        assert!(
            jobs.contains("preflight_scan_jobs_status_created_idx"),
            "{jobs}"
        );

        let files = connection
            .prepare("EXPLAIN QUERY PLAN SELECT ordinal FROM preflight_scan_files WHERE job_id = ?1 AND status = 'complete' AND metadata_status = 'pending' ORDER BY ordinal ASC LIMIT 1")
            .unwrap()
            .query_map(["job-00001"], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n");
        assert!(
            files.contains("preflight_scan_files_job_status_idx")
                || files.contains("preflight_scan_files_metadata_pending_idx")
                || files.contains("sqlite_autoindex_preflight_scan_files_1"),
            "{files}"
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wal_foreign_keys_and_busy_timeout_remain_explicit() {
        let (root, mut connection) = test_database("settings");
        configure_connection(&connection, Duration::from_secs(30)).unwrap();
        migrate_database(&mut connection).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "wal"
        );
        assert_eq!(
            connection
                .query_row("PRAGMA foreign_keys", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("PRAGMA busy_timeout", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            30_000
        );
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_busy_wait_allows_a_short_competing_writer() {
        let (root, mut writer) = test_database("busy");
        configure_connection(&writer, Duration::from_secs(30)).unwrap();
        migrate_database(&mut writer).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();

        let path = root.join("queue.sqlite3");
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let competing = std::thread::spawn(move || {
            let connection = Connection::open(path).unwrap();
            configure_connection(&connection, Duration::from_secs(30)).unwrap();
            ready_tx.send(()).unwrap();
            connection.execute(
                "INSERT INTO connection_settings (singleton, updated_at) VALUES (1, 'fixture')",
                [],
            )
        });
        ready_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        writer.execute_batch("COMMIT").unwrap();
        assert_eq!(competing.join().unwrap().unwrap(), 1);

        drop(writer);
        std::fs::remove_dir_all(root).unwrap();
    }
}
