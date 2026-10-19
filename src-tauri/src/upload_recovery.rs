//! Durable holds for retrying an existing upload identity and its checkpoint.
//!
//! These records never authorize a new resumable session or modify checkpoints.
//! A changed credential revision permits another reconciliation attempt; it does
//! not make an earlier, uncertain upload safe to repeat.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, Error, OptionalExtension};

const TABLE: &str = "youtube_upload_recovery";
const MAX_KEY_BYTES: usize = 4096;
const READ_ERROR: &str = "The saved upload recovery wait could not be read safely.";
const WRITE_ERROR: &str = "The upload recovery wait could not be saved safely.";

const CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS youtube_upload_recovery (
    item_id TEXT NOT NULL CHECK(length(item_id) BETWEEN 1 AND 4096),
    channel_id TEXT NOT NULL CHECK(length(channel_id) BETWEEN 1 AND 4096),
    credential_revision TEXT NOT NULL CHECK(length(credential_revision) BETWEEN 1 AND 4096),
    failure_count INTEGER NOT NULL CHECK(typeof(failure_count) = 'integer' AND failure_count BETWEEN 1 AND 31),
    terminal INTEGER NOT NULL CHECK(typeof(terminal) = 'integer' AND terminal IN (0, 1)),
    retry_at_ms INTEGER CHECK(
        (terminal = 1 AND retry_at_ms IS NULL) OR
        (terminal = 0 AND typeof(retry_at_ms) = 'integer')
    ),
    PRIMARY KEY(item_id, channel_id, credential_revision)
) WITHOUT ROWID";

// An existing terminal response remains terminal for this credential revision.
// The single UPSERT avoids two workers losing the increment or shortening a
// provider's Retry-After. Its WHERE clause refuses malformed existing records.
const UPSERT: &str = "INSERT INTO youtube_upload_recovery(
        item_id, channel_id, credential_revision, failure_count, terminal, retry_at_ms
    ) VALUES(?1, ?2, ?3, 1, ?4, CASE WHEN ?4 = 1 THEN NULL ELSE ?5 END)
    ON CONFLICT(item_id, channel_id, credential_revision) DO UPDATE SET
        failure_count = MIN(31, youtube_upload_recovery.failure_count + 1),
        terminal = MAX(youtube_upload_recovery.terminal, excluded.terminal),
        retry_at_ms = CASE
            WHEN youtube_upload_recovery.terminal = 1 OR excluded.terminal = 1 THEN NULL
            ELSE MAX(
                ?6 + CASE youtube_upload_recovery.failure_count
                    WHEN 1 THEN 120000 WHEN 2 THEN 240000 WHEN 3 THEN 480000 ELSE 900000 END,
                ?7,
                youtube_upload_recovery.retry_at_ms
            ) END
    WHERE typeof(youtube_upload_recovery.failure_count) = 'integer'
        AND youtube_upload_recovery.failure_count BETWEEN 1 AND 31
        AND typeof(youtube_upload_recovery.terminal) = 'integer'
        AND youtube_upload_recovery.terminal IN (0, 1)
        AND (
            (youtube_upload_recovery.terminal = 1 AND youtube_upload_recovery.retry_at_ms IS NULL)
            OR (youtube_upload_recovery.terminal = 0
                AND typeof(youtube_upload_recovery.retry_at_ms) = 'integer'
                AND youtube_upload_recovery.retry_at_ms BETWEEN ?8 AND ?9)
        )
    RETURNING failure_count, terminal, retry_at_ms";

fn validate_key(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_KEY_BYTES || value.chars().any(char::is_control) {
        return Err("The upload recovery identity is invalid.".into());
    }
    Ok(())
}

fn validate_scope(item: &str, channel: &str, revision: &str) -> Result<(), String> {
    validate_key(item)?;
    validate_key(channel)?;
    validate_key(revision)
}

fn table_is_missing(error: &Error) -> bool {
    matches!(error, Error::SqliteFailure(_, Some(message)) if message == &format!("no such table: {TABLE}"))
}

fn decode_hold(
    failures: i64,
    terminal: i64,
    retry_at_ms: Option<i64>,
) -> Result<Option<DateTime<Utc>>, String> {
    if !(1..=31).contains(&failures) {
        return Err(READ_ERROR.into());
    }
    match (terminal, retry_at_ms) {
        (1, None) => Ok(None),
        (0, Some(value)) => DateTime::from_timestamp_millis(value)
            .map(Some)
            .ok_or_else(|| READ_ERROR.into()),
        _ => Err(READ_ERROR.into()),
    }
}

/// Record another failed attempt at reconciling this existing upload identity.
/// Transient failures wait 60, 120, 240, 480 and then 900 seconds. A terminal
/// failure returns None and remains blocked until explicit repair or a changed
/// credential revision. Only this write path creates the lazy table.
pub(crate) fn record_failure(
    connection: &Connection,
    item: &str,
    channel: &str,
    revision: &str,
    terminal: bool,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, String> {
    record_failure_with_retry_after(connection, item, channel, revision, terminal, now, None)
}

/// A provider Retry-After is a minimum and may exceed the local 900-second cap.
pub(crate) fn record_failure_with_retry_after(
    connection: &Connection,
    item: &str,
    channel: &str,
    revision: &str,
    terminal: bool,
    now: DateTime<Utc>,
    minimum_retry: Option<DateTime<Utc>>,
) -> Result<Option<DateTime<Utc>>, String> {
    validate_scope(item, channel, revision)?;
    // Check the largest addition before SQL so arithmetic cannot overflow into
    // SQLite REAL or create an unrepresentable timestamp.
    now.checked_add_signed(Duration::seconds(900))
        .ok_or_else(|| WRITE_ERROR.to_string())?;
    let first_retry = now
        .checked_add_signed(Duration::seconds(60))
        .ok_or_else(|| WRITE_ERROR.to_string())?;
    let minimum_ms = minimum_retry.unwrap_or(now).timestamp_millis();
    let first_retry_ms = first_retry.timestamp_millis().max(minimum_ms);
    let write = || {
        connection
            .query_row(
                UPSERT,
                params![
                    item,
                    channel,
                    revision,
                    i64::from(terminal),
                    first_retry_ms,
                    now.timestamp_millis(),
                    minimum_ms,
                    DateTime::<Utc>::MIN_UTC.timestamp_millis(),
                    DateTime::<Utc>::MAX_UTC.timestamp_millis()
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .optional()
    };
    let result = match write() {
        Ok(result) => result,
        Err(error) if table_is_missing(&error) => {
            connection
                .execute_batch(CREATE_TABLE)
                .map_err(|_| WRITE_ERROR.to_string())?;
            write().map_err(|_| WRITE_ERROR.to_string())?
        }
        Err(_) => return Err(WRITE_ERROR.into()),
    };
    let (failures, terminal, retry_at_ms) = result.ok_or_else(|| WRITE_ERROR.to_string())?;
    decode_hold(failures, terminal, retry_at_ms)
}

struct RecoveryHold {
    retry_at: Option<DateTime<Utc>>,
}

fn load_hold(
    connection: &Connection,
    item: &str,
    channel: &str,
    revision: &str,
) -> Result<Option<RecoveryHold>, String> {
    validate_scope(item, channel, revision)?;
    let result = connection
        .query_row(
            "SELECT failure_count, terminal, retry_at_ms FROM youtube_upload_recovery
             WHERE item_id = ?1 AND channel_id = ?2 AND credential_revision = ?3",
            params![item, channel, revision],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .optional();
    match result {
        Ok(Some((failures, terminal, retry))) => Ok(Some(RecoveryHold {
            retry_at: decode_hold(failures, terminal, retry)?,
        })),
        Ok(None) => Ok(None),
        Err(error) if table_is_missing(&error) => Ok(None),
        Err(_) => Err(READ_ERROR.into()),
    }
}

/// Whether this exact item/channel/revision has retained retry evidence. This
/// does not imply eligibility or authorize new upload/session creation.
pub(crate) fn recorded(
    connection: &Connection,
    item: &str,
    channel: &str,
    revision: &str,
) -> Result<bool, String> {
    Ok(load_hold(connection, item, channel, revision)?.is_some())
}

/// An absent table/record means no hold. All other read/shape errors propagate
/// so callers can fail closed instead of treating corrupt evidence as approval.
pub(crate) fn eligible(
    connection: &Connection,
    item: &str,
    channel: &str,
    revision: &str,
    now: DateTime<Utc>,
) -> Result<bool, String> {
    Ok(match load_hold(connection, item, channel, revision)? {
        Some(hold) => hold.retry_at.is_some_and(|retry| retry <= now),
        None => true,
    })
}

/// Earliest future transient deadline in the selected channel/revision. Terminal
/// rows have no automatic retry. Expired holds remain eligible without a write.
pub(crate) fn next_retry(
    connection: &Connection,
    channel: &str,
    revision: &str,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, String> {
    validate_key(channel)?;
    validate_key(revision)?;
    let mut statement = match connection.prepare(
        "SELECT failure_count, terminal, retry_at_ms FROM youtube_upload_recovery
         WHERE channel_id = ?1 AND credential_revision = ?2",
    ) {
        Ok(statement) => statement,
        Err(error) if table_is_missing(&error) => return Ok(None),
        Err(_) => return Err(READ_ERROR.into()),
    };
    let records = statement
        .query_map(params![channel, revision], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|_| READ_ERROR.to_string())?;
    let mut earliest: Option<DateTime<Utc>> = None;
    for record in records {
        let (failures, terminal, retry) = record.map_err(|_| READ_ERROR.to_string())?;
        if let Some(retry) = decode_hold(failures, terminal, retry)? {
            if retry > now && earliest.is_none_or(|current| retry < current) {
                earliest = Some(retry);
            }
        }
    }
    Ok(earliest)
}

/// Clear recovery holds after a confirmed receipt or explicitly authorized
/// repair. The queue row and encrypted resumable checkpoint are untouched.
pub(crate) fn clear(connection: &Connection, item: &str) -> Result<(), String> {
    validate_key(item)?;
    match connection.execute(
        "DELETE FROM youtube_upload_recovery WHERE item_id = ?1",
        params![item],
    ) {
        Ok(_) => Ok(()),
        Err(error) if table_is_missing(&error) => Ok(()),
        Err(_) => Err("The upload recovery wait could not be cleared safely.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-19T01:30:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn absent_reads_and_clear_never_create_a_table() {
        let database = Connection::open_in_memory().unwrap();
        assert!(eligible(&database, "item", "channel", "revision", now()).unwrap());
        assert!(!recorded(&database, "item", "channel", "revision").unwrap());
        assert_eq!(
            next_retry(&database, "channel", "revision", now()).unwrap(),
            None
        );
        clear(&database, "item").unwrap();
        assert_eq!(
            database
                .query_row("SELECT count(*) FROM sqlite_master", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn transient_hold_survives_a_reopened_database_and_expires_without_writes() {
        let path =
            std::env::temp_dir().join(format!("upload-recovery-{}.sqlite", uuid::Uuid::new_v4()));
        let deadline = now() + Duration::seconds(60);
        {
            let database = Connection::open(&path).unwrap();
            assert_eq!(
                record_failure(&database, "item", "channel", "revision", false, now()).unwrap(),
                Some(deadline)
            );
        }
        {
            let database = Connection::open(&path).unwrap();
            database.execute_batch("PRAGMA query_only = ON").unwrap();
            assert!(!eligible(&database, "item", "channel", "revision", now()).unwrap());
            assert_eq!(
                next_retry(&database, "channel", "revision", now()).unwrap(),
                Some(deadline)
            );
            assert!(eligible(&database, "item", "channel", "revision", deadline).unwrap());
            assert_eq!(
                next_retry(&database, "channel", "revision", deadline).unwrap(),
                None
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn recorded_requires_valid_persisted_evidence_in_the_exact_scope() {
        let path = std::env::temp_dir().join(format!(
            "upload-recovery-recorded-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        {
            let database = Connection::open(&path).unwrap();
            assert!(!recorded(&database, "item", "channel", "revision").unwrap());
            record_failure(&database, "item", "channel", "revision", false, now()).unwrap();
        }
        {
            let database = Connection::open(&path).unwrap();
            assert!(recorded(&database, "item", "channel", "revision").unwrap());
            assert!(!recorded(&database, "other", "channel", "revision").unwrap());
            assert!(!recorded(&database, "item", "other", "revision").unwrap());
            assert!(!recorded(&database, "item", "channel", "other").unwrap());
            database.execute_batch("PRAGMA ignore_check_constraints = ON; UPDATE youtube_upload_recovery SET retry_at_ms = 'broken'").unwrap();
            assert!(recorded(&database, "item", "channel", "revision").is_err());
            database
                .execute_batch(
                    "UPDATE youtube_upload_recovery SET terminal = 1, retry_at_ms = NULL",
                )
                .unwrap();
            assert!(recorded(&database, "item", "channel", "revision").unwrap());
            clear(&database, "item").unwrap();
            assert!(!recorded(&database, "item", "channel", "revision").unwrap());
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn holds_fence_item_channel_revision_and_device() {
        let database = Connection::open_in_memory().unwrap();
        let other_device = Connection::open_in_memory().unwrap();
        record_failure(&database, "item", "channel", "old", true, now()).unwrap();
        assert!(!eligible(&database, "item", "channel", "old", now()).unwrap());
        assert!(eligible(&database, "next-item", "channel", "old", now()).unwrap());
        assert!(eligible(&database, "item", "other-channel", "old", now()).unwrap());
        assert!(eligible(&database, "item", "channel", "new", now()).unwrap());
        assert!(eligible(&other_device, "item", "channel", "old", now()).unwrap());
        assert_eq!(
            next_retry(&database, "channel", "new", now()).unwrap(),
            None
        );
        assert_eq!(
            database
                .query_row("SELECT count(*) FROM youtube_upload_recovery", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
    }

    #[test]
    fn transient_backoff_is_bounded_and_provider_minimum_is_preserved() {
        let database = Connection::open_in_memory().unwrap();
        let mut current = now();
        for delay in [60, 120, 240, 480, 900, 900, 900] {
            let deadline = current + Duration::seconds(delay);
            assert_eq!(
                record_failure(&database, "item", "channel", "revision", false, current).unwrap(),
                Some(deadline)
            );
            current = deadline;
        }
        let provider_deadline = current + Duration::seconds(3600);
        assert_eq!(
            record_failure_with_retry_after(
                &database,
                "item",
                "channel",
                "revision",
                false,
                current,
                Some(provider_deadline)
            )
            .unwrap(),
            Some(provider_deadline)
        );
        assert_eq!(
            record_failure(&database, "item", "channel", "revision", false, current).unwrap(),
            Some(provider_deadline)
        );
    }

    #[test]
    fn terminal_recovery_requires_changed_revision_or_explicit_clear() {
        let database = Connection::open_in_memory().unwrap();
        record_failure(&database, "item", "channel", "revision", false, now()).unwrap();
        assert_eq!(
            record_failure(&database, "item", "channel", "revision", true, now()).unwrap(),
            None
        );
        assert_eq!(
            record_failure(&database, "item", "channel", "revision", false, now()).unwrap(),
            None
        );
        let later = now() + Duration::days(365);
        assert!(!eligible(&database, "item", "channel", "revision", later).unwrap());
        assert!(eligible(&database, "item", "channel", "new-revision", later).unwrap());
        assert_eq!(
            next_retry(&database, "channel", "revision", now()).unwrap(),
            None
        );
        clear(&database, "item").unwrap();
        assert!(eligible(&database, "item", "channel", "revision", now()).unwrap());
    }

    #[test]
    fn earliest_future_retry_excludes_due_terminal_and_other_scopes() {
        let database = Connection::open_in_memory().unwrap();
        record_failure(&database, "early", "channel", "revision", false, now()).unwrap();
        record_failure(
            &database,
            "later",
            "channel",
            "revision",
            false,
            now() + Duration::seconds(30),
        )
        .unwrap();
        record_failure(&database, "blocked", "channel", "revision", true, now()).unwrap();
        record_failure(
            &database,
            "other",
            "other-channel",
            "revision",
            false,
            now() - Duration::seconds(30),
        )
        .unwrap();
        assert_eq!(
            next_retry(&database, "channel", "revision", now()).unwrap(),
            Some(now() + Duration::seconds(60))
        );
        assert_eq!(
            next_retry(
                &database,
                "channel",
                "revision",
                now() + Duration::seconds(60)
            )
            .unwrap(),
            Some(now() + Duration::seconds(90))
        );
    }

    #[test]
    fn malformed_records_and_database_errors_fail_closed() {
        let database = Connection::open_in_memory().unwrap();
        record_failure(&database, "item", "channel", "revision", false, now()).unwrap();
        database.execute_batch("PRAGMA ignore_check_constraints = ON; UPDATE youtube_upload_recovery SET retry_at_ms = 'broken'").unwrap();
        assert!(eligible(&database, "item", "channel", "revision", now()).is_err());
        assert!(next_retry(&database, "channel", "revision", now()).is_err());
        assert!(record_failure(&database, "item", "channel", "revision", false, now()).is_err());
        database
            .execute_batch("UPDATE youtube_upload_recovery SET retry_at_ms = 9223372036854775807")
            .unwrap();
        assert!(eligible(&database, "item", "channel", "revision", now()).is_err());
        assert!(next_retry(&database, "channel", "revision", now()).is_err());
        assert!(record_failure(&database, "item", "channel", "revision", false, now()).is_err());

        let wrong_schema = Connection::open_in_memory().unwrap();
        wrong_schema
            .execute_batch("CREATE TABLE youtube_upload_recovery(item_id TEXT)")
            .unwrap();
        assert!(eligible(&wrong_schema, "item", "channel", "revision", now()).is_err());
        assert!(next_retry(&wrong_schema, "channel", "revision", now()).is_err());
        assert!(
            record_failure(&wrong_schema, "item", "channel", "revision", false, now()).is_err()
        );
    }

    #[test]
    fn invalid_identity_and_unwritable_database_cannot_authorize_work() {
        let database = Connection::open_in_memory().unwrap();
        assert!(eligible(&database, "item", "", "revision", now()).is_err());
        assert!(next_retry(&database, "channel", "revision\n", now()).is_err());
        database.execute_batch("PRAGMA query_only = ON").unwrap();
        assert!(record_failure(&database, "item", "channel", "revision", false, now()).is_err());
    }
}
