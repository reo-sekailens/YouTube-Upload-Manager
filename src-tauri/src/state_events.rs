use rusqlite::{params, Connection};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

use crate::{performance, persistence, spawn_worker, DATABASE_BUSY_TIMEOUT};

pub(crate) const STATE_CHANGE_EVENT: &str = "local-state-change";
const STATE_CHANGE_BATCH_LIMIT: usize = 512;
const EVENT_COALESCE_WINDOW: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StateChange {
    pub(crate) revision: u64,
    pub(crate) channel_id: String,
    pub(crate) surface: String,
    pub(crate) entity_id: String,
    pub(crate) event_kind: String,
    pub(crate) payload: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StateChangeBatch {
    pub(crate) from_revision: u64,
    pub(crate) to_revision: u64,
    pub(crate) reset_required: bool,
    pub(crate) changes: Vec<StateChange>,
}

pub(crate) struct StateEventRuntime {
    wake_sender: mpsc::SyncSender<()>,
    wake_receiver: Mutex<Option<mpsc::Receiver<()>>>,
}

impl StateEventRuntime {
    pub(crate) fn new() -> Self {
        let (wake_sender, wake_receiver) = mpsc::sync_channel(1);
        Self {
            wake_sender,
            wake_receiver: Mutex::new(Some(wake_receiver)),
        }
    }

    /// SQLite calls the commit hook immediately before a successful commit.
    /// The receiver retries the durable revision read briefly, so it never
    /// publishes an uncommitted row and a rollback cannot create a fake delta.
    pub(crate) fn attach_commit_hook(&self, connection: &Connection) {
        let wake_sender = self.wake_sender.clone();
        connection.commit_hook(Some(move || {
            let _ = wake_sender.try_send(());
            false
        }));
    }

    pub(crate) fn start(&self, app: AppHandle, database_path: PathBuf) -> Result<(), String> {
        let receiver = self
            .wake_receiver
            .lock()
            .map_err(|_| "The local state-event receiver is unavailable.".to_string())?
            .take()
            .ok_or_else(|| "The local state-event dispatcher is already running.".to_string())?;
        spawn_worker(move || dispatch_loop(app, database_path, receiver));
        Ok(())
    }
}

fn open_event_connection(database_path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(database_path).map_err(|error| error.to_string())?;
    persistence::configure_connection(&connection, DATABASE_BUSY_TIMEOUT)?;
    Ok(connection)
}

pub(crate) fn current_revision(connection: &Connection) -> Result<u64, String> {
    connection
        .query_row(
            "SELECT COALESCE(MAX(revision), 0) FROM state_changes",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value.max(0) as u64)
        .map_err(|error| error.to_string())
}

fn active_channel_id(connection: &Connection) -> Result<String, String> {
    connection
        .query_row(
            "SELECT COALESCE((SELECT active_channel_id FROM connection_settings WHERE singleton = 1), '')",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

pub(crate) fn record_invalidation(
    connection: &Connection,
    channel_id: &str,
    surface: &str,
    entity_id: &str,
    event_kind: &str,
) -> Result<u64, String> {
    connection
        .execute(
            "INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, payload_json, created_at)
             VALUES(?1, ?2, ?3, ?4, '{}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![channel_id, surface, entity_id, event_kind],
        )
        .map_err(|error| error.to_string())?;
    Ok(connection.last_insert_rowid().max(0) as u64)
}

pub(crate) fn load_state_changes(
    connection: &Connection,
    requested_channel_id: &str,
    after_revision: u64,
) -> Result<StateChangeBatch, String> {
    let active_channel_id = active_channel_id(connection)?;
    if requested_channel_id != active_channel_id {
        return Err(
            "The state cursor no longer belongs to the active YouTube channel; reload its local snapshot."
                .into(),
        );
    }

    let current = current_revision(connection)?;
    let minimum = connection
        .query_row(
            "SELECT COALESCE(MIN(revision), 0) FROM state_changes",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        .max(0) as u64;
    let reset_required = after_revision > current
        || (after_revision > 0 && minimum > 0 && after_revision.saturating_add(1) < minimum);
    if reset_required {
        return Ok(StateChangeBatch {
            from_revision: after_revision,
            to_revision: current,
            reset_required: true,
            changes: Vec::new(),
        });
    }

    let mut statement = connection
        .prepare(
            "SELECT revision, channel_id, surface, entity_id, event_kind, payload_json
             FROM state_changes
             WHERE revision > ?1
               AND (channel_id = ?2 OR (channel_id = '' AND surface = 'connection'))
             ORDER BY revision ASC
             LIMIT ?3",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(
            params![
                after_revision.min(i64::MAX as u64) as i64,
                requested_channel_id,
                STATE_CHANGE_BATCH_LIMIT as i64
            ],
            |row| {
                let payload_json = row.get::<_, String>(5)?;
                Ok(StateChange {
                    revision: row.get::<_, i64>(0)?.max(0) as u64,
                    channel_id: row.get(1)?,
                    surface: row.get(2)?,
                    entity_id: row.get(3)?,
                    event_kind: row.get(4)?,
                    payload: serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null),
                })
            },
        )
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let to_revision = if rows.len() == STATE_CHANGE_BATCH_LIMIT {
        rows.last()
            .map(|change| change.revision)
            .unwrap_or(after_revision)
    } else {
        current
    };

    // Keep only the latest state for a changed entity inside this covered
    // revision range. The range cursor still advances over every durable row,
    // including every provider-acknowledged upload checkpoint.
    let mut latest_by_entity = HashMap::<(String, String, String), StateChange>::new();
    for change in rows {
        latest_by_entity.insert(
            (
                change.channel_id.clone(),
                change.surface.clone(),
                change.entity_id.clone(),
            ),
            change,
        );
    }
    let mut changes = latest_by_entity.into_values().collect::<Vec<_>>();
    changes.sort_by_key(|change| change.revision);

    Ok(StateChangeBatch {
        from_revision: after_revision,
        to_revision,
        reset_required: false,
        changes,
    })
}

fn wait_for_committed_revision(database_path: &Path, after_revision: u64) -> Option<u64> {
    for delay in [2_u64, 4, 8, 16, 32, 64, 128, 256, 512] {
        if let Ok(connection) = open_event_connection(database_path) {
            if let Ok(revision) = current_revision(&connection) {
                if revision > after_revision {
                    return Some(revision);
                }
            }
        }
        thread::sleep(Duration::from_millis(delay));
    }
    None
}

fn dispatch_loop(app: AppHandle, database_path: PathBuf, receiver: mpsc::Receiver<()>) {
    let mut emitted_revision = open_event_connection(&database_path)
        .and_then(|connection| current_revision(&connection))
        .unwrap_or(0);
    while receiver.recv().is_ok() {
        // A fixed latest-value window bounds progress traffic while preserving
        // every acknowledged range in SQLite and its durable revision cursor.
        // It is measured from the first wake, so continuous uploads still
        // publish progress at least once per window instead of waiting for an
        // arbitrarily long quiet period.
        let deadline = Instant::now() + EVENT_COALESCE_WINDOW;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(()) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        let Some(_) = wait_for_committed_revision(&database_path, emitted_revision) else {
            continue;
        };
        loop {
            let Ok(connection) = open_event_connection(&database_path) else {
                break;
            };
            let Ok(channel_id) = active_channel_id(&connection) else {
                break;
            };
            let Ok(batch) = load_state_changes(&connection, &channel_id, emitted_revision) else {
                break;
            };
            if batch.to_revision <= emitted_revision {
                break;
            }
            emitted_revision = batch.to_revision;
            if !batch.changes.is_empty() || batch.reset_required {
                performance::record_event_message();
                let _ = app.emit(STATE_CHANGE_EVENT, &batch);
            }
            if batch.to_revision >= current_revision(&connection).unwrap_or(batch.to_revision) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, Connection) {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-state-events-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut connection = Connection::open(root.join("queue.sqlite3")).unwrap();
        persistence::configure_connection(&connection, DATABASE_BUSY_TIMEOUT).unwrap();
        persistence::migrate_database(&mut connection).unwrap();
        connection
            .execute(
                "INSERT INTO connection_settings(singleton, active_channel, active_channel_id, updated_at) VALUES(1, 'Channel A', 'channel-a', 'fixture')",
                [],
            )
            .unwrap();
        (root, connection)
    }

    #[test]
    fn change_batches_coalesce_entities_and_reject_cross_channel_cursors() {
        let (root, connection) = fixture();
        let baseline = current_revision(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO upload_items(id, title, file_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at)
                 VALUES('item-a', 'Clip', 'clip.mp4', 'channel-a', 'managed', 10, 'uploading', 10, 'fixture', 'one')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO upload_items(id, title, file_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at)
                 VALUES('item-b', 'Other', 'other.mp4', 'channel-b', 'managed', 10, 'uploading', 10, 'fixture', 'one')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE upload_items SET confirmed_bytes = 5, updated_at = 'two' WHERE id = 'item-a'",
                [],
            )
            .unwrap();

        let batch = load_state_changes(&connection, "channel-a", baseline).unwrap();
        assert_eq!(batch.from_revision, baseline);
        assert_eq!(batch.to_revision, current_revision(&connection).unwrap());
        assert_eq!(batch.changes.len(), 1);
        assert_eq!(batch.changes[0].entity_id, "item-a");
        assert_eq!(batch.changes[0].payload["confirmedBytes"], 5);
        assert!(serde_json::to_vec(&batch).unwrap().len() < 2_048);
        assert!(load_state_changes(&connection, "channel-b", baseline).is_err());
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn commit_hook_is_quiet_when_idle_and_wakes_after_a_durable_change() {
        let (root, connection) = fixture();
        let runtime = StateEventRuntime::new();
        runtime.attach_commit_hook(&connection);
        let receiver = runtime.wake_receiver.lock().unwrap().take().unwrap();

        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(25)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        record_invalidation(
            &connection,
            "channel-a",
            "inventory",
            "channel-a",
            "invalidate",
        )
        .unwrap();
        assert!(receiver.recv_timeout(Duration::from_secs(1)).is_ok());
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(25)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));

        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rapid_progress_rows_remain_durable_but_collapse_to_one_compact_delta() {
        let (root, mut connection) = fixture();
        connection
            .execute(
                "INSERT INTO upload_items(id, title, file_name, channel_id, workspace_path, size_bytes, status, total_bytes, created_at, updated_at)
                 VALUES('item-a', 'Clip', 'clip.mp4', 'channel-a', 'managed', 200, 'uploading', 200, 'fixture', 'zero')",
                [],
            )
            .unwrap();
        let baseline = current_revision(&connection).unwrap();
        let transaction = connection.transaction().unwrap();
        for confirmed in 1..=200 {
            transaction
                .execute(
                    "UPDATE upload_items SET confirmed_bytes = ?1, updated_at = ?2 WHERE id = 'item-a'",
                    params![confirmed, format!("progress-{confirmed}")],
                )
                .unwrap();
        }
        transaction.commit().unwrap();

        let current = current_revision(&connection).unwrap();
        assert_eq!(current - baseline, 200);
        let batch = load_state_changes(&connection, "channel-a", baseline).unwrap();
        assert_eq!(batch.to_revision, current);
        assert_eq!(batch.changes.len(), 1);
        assert_eq!(batch.changes[0].payload["confirmedBytes"], 200);
        assert!(serde_json::to_vec(&batch).unwrap().len() < 2_048);

        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retained_cursor_gap_requests_one_snapshot_reset() {
        let (root, connection) = fixture();
        let stale_cursor = current_revision(&connection).unwrap();
        for revision in 0..3 {
            record_invalidation(
                &connection,
                "channel-a",
                "inventory",
                &format!("inventory-{revision}"),
                "invalidate",
            )
            .unwrap();
        }
        connection
            .execute(
                "DELETE FROM state_changes WHERE revision <= ?1",
                [stale_cursor.saturating_add(2) as i64],
            )
            .unwrap();
        let batch = load_state_changes(&connection, "channel-a", stale_cursor).unwrap();
        assert!(batch.reset_required);
        assert!(batch.changes.is_empty());
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }
}
