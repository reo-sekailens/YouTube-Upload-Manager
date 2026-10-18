//! Device-local cache for YouTube inventory and playlist metadata only.
//!
//! Callers must supply an immutable account/channel key and fence provider results
//! before saving them. Credentials, cookies and signed media URLs do not belong
//! here. Missing, invalid or incompatible cached JSON is never provider evidence.

use rusqlite::{params, Connection, Error, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::io::{self, Write};

const TABLE: &str = "youtube_read_cache";
const MAX_KEY_BYTES: usize = 4096;
const MAX_JSON_BYTES: usize = 2 * 1024 * 1024;

const CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS youtube_read_cache (
    namespace TEXT NOT NULL CHECK(namespace IN ('inventory', 'playlists')),
    cache_key TEXT NOT NULL CHECK(length(cache_key) BETWEEN 1 AND 4096),
    payload BLOB NOT NULL CHECK(typeof(payload) = 'blob' AND length(payload) <= 2097152),
    PRIMARY KEY(namespace, cache_key)
) WITHOUT ROWID";

const UPSERT: &str = "INSERT INTO youtube_read_cache(namespace, cache_key, payload)
    VALUES(?1, ?2, ?3)
    ON CONFLICT(namespace, cache_key) DO UPDATE SET payload = excluded.payload";

fn validate_scope(namespace: &str, key: &str) -> Result<(), String> {
    if !matches!(namespace, "inventory" | "playlists") {
        return Err("The YouTube metadata cache namespace is invalid.".into());
    }
    if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_control) {
        return Err("The YouTube metadata cache account key is invalid.".into());
    }
    Ok(())
}

fn table_is_missing(error: &Error) -> bool {
    matches!(error, Error::SqliteFailure(_, Some(message)) if message == &format!("no such table: {TABLE}"))
}

/// A cache miss performs just one lookup and never changes the database schema.
pub(crate) fn load<T: DeserializeOwned>(
    connection: &Connection,
    namespace: &str,
    key: &str,
) -> Result<Option<T>, String> {
    validate_scope(namespace, key)?;
    let result = connection
        .query_row(
            "SELECT payload FROM youtube_read_cache
             WHERE namespace = ?1 AND cache_key = ?2
               AND typeof(payload) = 'blob' AND length(payload) <= ?3",
            params![namespace, key, MAX_JSON_BYTES as i64],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional();
    match result {
        Ok(Some(payload)) => Ok(serde_json::from_slice(&payload).ok()),
        Ok(None) => Ok(None),
        Err(error) if table_is_missing(&error) => Ok(None),
        Err(_) => Err("The local YouTube metadata cache could not be read.".into()),
    }
}

struct BoundedJson {
    bytes: Vec<u8>,
}

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_JSON_BYTES.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("YouTube metadata exceeds the cache limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The single UPSERT is atomic and also participates in a caller's transaction.
/// Schema DDL is needed only for the first save into a database.
pub(crate) fn save<T: Serialize>(
    connection: &Connection,
    namespace: &str,
    key: &str,
    value: &T,
) -> Result<(), String> {
    validate_scope(namespace, key)?;
    let mut payload = BoundedJson { bytes: Vec::new() };
    serde_json::to_writer(&mut payload, value)
        .map_err(|_| "The YouTube metadata is invalid or exceeds the local cache limit.")?;
    let write = || connection.execute(UPSERT, params![namespace, key, &payload.bytes]);
    match write() {
        Ok(_) => Ok(()),
        Err(error) if table_is_missing(&error) => {
            connection
                .execute_batch(CREATE_TABLE)
                .map_err(|_| "The local YouTube metadata cache could not be created.")?;
            write()
                .map(|_| ())
                .map_err(|_| "The local YouTube metadata cache could not be saved.".into())
        }
        Err(_) => Err("The local YouTube metadata cache could not be saved.".into()),
    }
}

pub(crate) fn invalidate(
    connection: &Connection,
    namespace: &str,
    key: &str,
) -> Result<(), String> {
    validate_scope(namespace, key)?;
    match connection.execute(
        "DELETE FROM youtube_read_cache WHERE namespace = ?1 AND cache_key = ?2",
        params![namespace, key],
    ) {
        Ok(_) => Ok(()),
        Err(error) if table_is_missing(&error) => Ok(()),
        Err(_) => Err("The local YouTube metadata cache could not be invalidated.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
    struct Video {
        id: String,
        title: String,
    }

    fn connection() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn absent_cache_reads_and_invalidation_do_not_create_schema() {
        let database = connection();
        assert_eq!(
            load::<Vec<Video>>(&database, "inventory", "channel-a").unwrap(),
            None
        );
        invalidate(&database, "inventory", "channel-a").unwrap();
        assert_eq!(
            database
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE name = 'youtube_read_cache'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn an_empty_result_is_a_cache_hit_and_upserts_replace_metadata() {
        let database = connection();
        save(&database, "inventory", "channel-a", &Vec::<Video>::new()).unwrap();
        assert_eq!(
            load::<Vec<Video>>(&database, "inventory", "channel-a").unwrap(),
            Some(vec![])
        );
        let videos = vec![Video {
            id: "video-a".into(),
            title: "Current title".into(),
        }];
        save(&database, "inventory", "channel-a", &videos).unwrap();
        assert_eq!(
            load::<Vec<Video>>(&database, "inventory", "channel-a").unwrap(),
            Some(videos)
        );
        assert_eq!(
            database
                .query_row("SELECT count(*) FROM youtube_read_cache", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn namespaces_accounts_and_databases_are_independent() {
        let first = connection();
        let second = connection();
        save(&first, "inventory", "channel-a", &vec!["video-a"]).unwrap();
        save(&first, "playlists", "channel-a", &vec!["playlist-a"]).unwrap();
        save(&first, "inventory", "channel-b", &vec!["video-b"]).unwrap();
        save(
            &second,
            "inventory",
            "channel-a",
            &vec!["other-profile-video"],
        )
        .unwrap();
        invalidate(&first, "inventory", "channel-a").unwrap();
        assert_eq!(
            load::<Vec<String>>(&first, "inventory", "channel-a").unwrap(),
            None
        );
        assert_eq!(
            load::<Vec<String>>(&first, "playlists", "channel-a").unwrap(),
            Some(vec!["playlist-a".into()])
        );
        assert_eq!(
            load::<Vec<String>>(&first, "inventory", "channel-b").unwrap(),
            Some(vec!["video-b".into()])
        );
        assert_eq!(
            load::<Vec<String>>(&second, "inventory", "channel-a").unwrap(),
            Some(vec!["other-profile-video".into()])
        );
    }

    #[test]
    fn malformed_incompatible_non_blob_and_oversized_payloads_are_misses() {
        let database = connection();
        save(&database, "inventory", "channel-a", &Vec::<Video>::new()).unwrap();
        for payload in [b"not json".as_slice(), b"[{}]".as_slice(), &[0xff, 0xfe]] {
            database
                .execute("UPDATE youtube_read_cache SET payload = ?1", [payload])
                .unwrap();
            assert_eq!(
                load::<Vec<Video>>(&database, "inventory", "channel-a").unwrap(),
                None
            );
        }
        database
            .execute_batch("PRAGMA ignore_check_constraints = ON")
            .unwrap();
        database
            .execute("UPDATE youtube_read_cache SET payload = ?1", ["[]"])
            .unwrap();
        assert_eq!(
            load::<Vec<Video>>(&database, "inventory", "channel-a").unwrap(),
            None
        );
        let oversized = vec![b' '; MAX_JSON_BYTES + 1];
        database
            .execute("UPDATE youtube_read_cache SET payload = ?1", [&oversized])
            .unwrap();
        assert_eq!(
            load::<Vec<Video>>(&database, "inventory", "channel-a").unwrap(),
            None
        );
    }

    #[test]
    fn oversized_writes_preserve_the_previous_entry() {
        let database = connection();
        save(&database, "inventory", "channel-a", &vec!["retained"]).unwrap();
        assert!(save(
            &database,
            "inventory",
            "channel-a",
            &"x".repeat(MAX_JSON_BYTES)
        )
        .is_err());
        assert_eq!(
            load::<Vec<String>>(&database, "inventory", "channel-a").unwrap(),
            Some(vec!["retained".into()])
        );
        // A JSON string's two quotes count toward the exact byte cap.
        save(
            &database,
            "playlists",
            "channel-a",
            &"x".repeat(MAX_JSON_BYTES - 2),
        )
        .unwrap();
        assert_eq!(
            load::<String>(&database, "playlists", "channel-a")
                .unwrap()
                .unwrap()
                .len(),
            MAX_JSON_BYTES - 2
        );
    }

    #[test]
    fn only_metadata_namespaces_and_bounded_account_keys_are_accepted() {
        let database = connection();
        for namespace in ["tokens", "cookies", "", "inventory; DROP TABLE ignored"] {
            assert!(save(&database, namespace, "channel-a", &Vec::<String>::new()).is_err());
            assert!(load::<Vec<String>>(&database, namespace, "channel-a").is_err());
            assert!(invalidate(&database, namespace, "channel-a").is_err());
        }
        for key in [
            String::new(),
            "x".repeat(MAX_KEY_BYTES + 1),
            "channel\nother".into(),
        ] {
            assert!(save(&database, "inventory", &key, &Vec::<String>::new()).is_err());
            assert!(load::<Vec<String>>(&database, "inventory", &key).is_err());
            assert!(invalidate(&database, "inventory", &key).is_err());
        }
        assert_eq!(
            database
                .query_row("SELECT count(*) FROM sqlite_master", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn cache_writes_and_invalidation_follow_the_callers_transaction() {
        let database = connection();
        save(&database, "inventory", "channel-a", &vec!["retained"]).unwrap();
        database.execute_batch("BEGIN IMMEDIATE").unwrap();
        save(&database, "inventory", "channel-a", &vec!["uncommitted"]).unwrap();
        save(&database, "playlists", "channel-a", &Vec::<String>::new()).unwrap();
        invalidate(&database, "inventory", "channel-a").unwrap();
        database.execute_batch("ROLLBACK").unwrap();
        assert_eq!(
            load::<Vec<String>>(&database, "inventory", "channel-a").unwrap(),
            Some(vec!["retained".into()])
        );
        assert_eq!(
            load::<Vec<String>>(&database, "playlists", "channel-a").unwrap(),
            None
        );
    }

    #[test]
    fn database_errors_are_distinct_from_cache_misses() {
        let database = connection();
        database
            .execute_batch("CREATE TABLE youtube_read_cache(unrelated TEXT)")
            .unwrap();
        assert!(load::<Vec<String>>(&database, "inventory", "channel-a").is_err());
        assert!(save(&database, "inventory", "channel-a", &Vec::<String>::new()).is_err());
        assert!(invalidate(&database, "inventory", "channel-a").is_err());
    }
}
