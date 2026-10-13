//! Device-wide manual upload choices with channel-bound playlist preferences.
use super::*;

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ManualUploadDefaults {
    made_for_kids: bool,
    visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playlist_channel_id: Option<String>,
}

#[derive(Default)]
struct Patch {
    made_for_kids: Option<bool>,
    visibility: Option<String>,
    playlist_id: Option<String>,
    expected_channel_id: Option<String>,
}

fn valid_visibility(value: &str) -> bool {
    matches!(value, "private" | "unlisted" | "public")
}

fn valid_playlist_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn require_expected_channel(
    connection: &Connection,
    expected: Option<&str>,
) -> Result<Option<String>, String> {
    let channel_id = connection_settings(connection)?.active_channel_id;
    if expected
        .is_some_and(|expected| channel_id.as_deref() != Some(expected) || expected.is_empty())
    {
        return Err(
            "The connected YouTube channel changed. Refresh upload defaults before saving.".into(),
        );
    }
    Ok(channel_id)
}

pub(super) fn load(
    connection: &Connection,
    expected_channel_id: Option<&str>,
) -> Result<ManualUploadDefaults, String> {
    let channel_id = require_expected_channel(connection, expected_channel_id)?;
    let (made_for_kids, visibility) = connection.query_row(
        "SELECT manual_made_for_kids_default, manual_visibility_default FROM connection_settings WHERE singleton = 1", [],
        |row| Ok((row.get::<_, i64>(0)? != 0, row.get::<_, String>(1)?)),
    ).optional().map_err(user_error)?.unwrap_or((false, "private".into()));
    if !valid_visibility(&visibility) {
        return Err("The saved upload visibility is invalid. Choose a new default before preparing uploads.".into());
    }
    let playlist = channel_id.as_deref().map(|id| connection.query_row(
        "SELECT playlist_id, playlist_title FROM manual_playlist_defaults WHERE channel_id = ?1", [id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).optional().map_err(user_error)).transpose()?.flatten();
    let (playlist_id, playlist_title, playlist_channel_id) = match playlist {
        Some((id, title)) => (Some(id), Some(title), channel_id),
        None => (None, None, None),
    };
    Ok(ManualUploadDefaults {
        made_for_kids,
        visibility,
        playlist_id,
        playlist_title,
        playlist_channel_id,
    })
}

fn verified_playlist_title(
    payload: &serde_json::Value,
    playlist_id: &str,
    expected_channel_id: &str,
) -> Result<String, String> {
    let item = payload
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.get("id").and_then(serde_json::Value::as_str) == Some(playlist_id)
            })
        })
        .ok_or("The selected playlist is unavailable. Refresh playlists and choose again.")?;
    if item
        .pointer("/snippet/channelId")
        .and_then(serde_json::Value::as_str)
        != Some(expected_channel_id)
    {
        return Err("The selected playlist belongs to another YouTube channel. Choose a playlist owned by the connected channel.".into());
    }
    item.pointer("/snippet/title").and_then(serde_json::Value::as_str).filter(|title| !title.trim().is_empty() && title.len() <= 4096).map(str::to_string)
        .ok_or_else(|| "YouTube did not return a usable title for the selected playlist. Refresh playlists and choose again.".into())
}

pub(super) fn resolve_owned_playlist(
    state: &AppState,
    playlist_id: &str,
    channel_id: &str,
) -> Result<String, String> {
    let token = refreshed_access_token(state)?;
    if active_channel_id(state, &token)? != channel_id {
        return Err("The authorized YouTube channel differs from this reviewed account. Reconnect before using this playlist.".into());
    }
    let payload = youtube_json(
        state,
        &token,
        "playlists",
        &[("part", "snippet"), ("id", playlist_id)],
    )?;
    verified_playlist_title(&payload, playlist_id, channel_id)
}

pub(super) fn save(
    state: &AppState,
    made_for_kids: Option<bool>,
    visibility: Option<String>,
    playlist_id: Option<String>,
    expected_channel_id: Option<String>,
) -> Result<ManualUploadDefaults, String> {
    save_with(
        state,
        Patch {
            made_for_kids,
            visibility,
            playlist_id,
            expected_channel_id,
        },
        |id, channel| resolve_owned_playlist(state, id, channel),
    )
}

fn save_with(
    state: &AppState,
    patch: Patch,
    resolve: impl FnOnce(&str, &str) -> Result<String, String>,
) -> Result<ManualUploadDefaults, String> {
    if patch
        .visibility
        .as_deref()
        .is_some_and(|value| !valid_visibility(value))
    {
        return Err("Choose private, unlisted or public as the upload visibility default.".into());
    }
    if patch
        .playlist_id
        .as_deref()
        .is_some_and(|id| !id.is_empty() && !valid_playlist_id(id))
    {
        return Err("Choose a valid YouTube playlist as the upload default.".into());
    }
    let mut connection = database(state)?;
    let channel_id = require_expected_channel(&connection, patch.expected_channel_id.as_deref())?;
    let playlist_title = match patch.playlist_id.as_deref() {
        Some(id) => {
            let channel = channel_id
                .as_deref()
                .ok_or("Connect YouTube before choosing or clearing its playlist default.")?;
            if id.is_empty() {
                None
            } else {
                Some(resolve(id, channel)?)
            }
        }
        None => None,
    };
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let final_channel =
        require_expected_channel(&transaction, patch.expected_channel_id.as_deref())?;
    if final_channel != channel_id {
        return Err("The connected YouTube channel changed while checking the playlist. Review upload defaults again.".into());
    }
    if patch.made_for_kids.is_some() || patch.visibility.is_some() {
        transaction.execute(
            "INSERT INTO connection_settings(singleton, manual_made_for_kids_default, manual_visibility_default, updated_at) VALUES(1, COALESCE(?1, 0), COALESCE(?2, 'private'), ?3) ON CONFLICT(singleton) DO UPDATE SET manual_made_for_kids_default = COALESCE(?1, manual_made_for_kids_default), manual_visibility_default = COALESCE(?2, manual_visibility_default), updated_at = excluded.updated_at",
            params![patch.made_for_kids.map(i64::from), patch.visibility, now()],
        ).map_err(user_error)?;
    }
    if let Some(id) = patch.playlist_id.as_deref() {
        let channel = channel_id
            .as_deref()
            .ok_or("Connect YouTube before changing its playlist default.")?;
        if id.is_empty() {
            transaction
                .execute(
                    "DELETE FROM manual_playlist_defaults WHERE channel_id = ?1",
                    [channel],
                )
                .map_err(user_error)?;
        } else {
            let title = playlist_title
                .as_deref()
                .ok_or("The selected playlist could not be verified.")?;
            transaction.execute("INSERT INTO manual_playlist_defaults(channel_id, playlist_id, playlist_title, updated_at) VALUES(?1, ?2, ?3, ?4) ON CONFLICT(channel_id) DO UPDATE SET playlist_id = excluded.playlist_id, playlist_title = excluded.playlist_title, updated_at = excluded.updated_at", params![channel, id, title, now()]).map_err(user_error)?;
        }
    }
    if patch.made_for_kids.is_some() || patch.visibility.is_some() || patch.playlist_id.is_some() {
        audit_global(&transaction, "manual_upload_defaults_set", "Operator saved device-local manual upload defaults; playlist choice remains bound to its connected channel")?;
    }
    let defaults = load(&transaction, patch.expected_channel_id.as_deref())?;
    transaction.commit().map_err(user_error)?;
    Ok(defaults)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-upload-defaults-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        (root, state, connection)
    }

    fn connect(connection: &Connection, channel: &str) {
        connection.execute("INSERT INTO connection_settings(singleton, active_channel, active_channel_id, updated_at) VALUES(1, 'Fixture channel', ?1, ?2) ON CONFLICT(singleton) DO UPDATE SET active_channel_id = excluded.active_channel_id", params![channel, now()]).unwrap();
    }

    fn forbidden_provider(_: &str, _: &str) -> Result<String, String> {
        panic!("This defaults patch must not request provider access");
    }

    #[test]
    fn native_upload_defaults_start_private_and_preserve_missing_patch_fields() {
        let (root, state, connection) = fixture();
        assert_eq!(
            load(&connection, None).unwrap(),
            ManualUploadDefaults {
                made_for_kids: false,
                visibility: "private".into(),
                playlist_id: None,
                playlist_title: None,
                playlist_channel_id: None
            }
        );
        let first = save_with(
            &state,
            Patch {
                made_for_kids: Some(true),
                visibility: Some("unlisted".into()),
                ..Patch::default()
            },
            forbidden_provider,
        )
        .unwrap();
        assert!(first.made_for_kids);
        assert_eq!(first.visibility, "unlisted");
        let next = save_with(
            &state,
            Patch {
                visibility: Some("public".into()),
                ..Patch::default()
            },
            forbidden_provider,
        )
        .unwrap();
        assert!(next.made_for_kids);
        assert_eq!(next.visibility, "public");
        let boolean_only = save_with(
            &state,
            Patch {
                made_for_kids: Some(false),
                ..Patch::default()
            },
            forbidden_provider,
        )
        .unwrap();
        assert!(!boolean_only.made_for_kids);
        assert_eq!(boolean_only.visibility, "public");
        assert_eq!(
            save_with(&state, Patch::default(), forbidden_provider).unwrap(),
            boolean_only
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_upload_playlist_defaults_are_isolated_and_restored_per_channel() {
        let (root, state, connection) = fixture();
        connect(&connection, "channel-a");
        let patch = Patch {
            made_for_kids: Some(true),
            visibility: Some("unlisted".into()),
            playlist_id: Some("playlist-a".into()),
            expected_channel_id: Some("channel-a".into()),
        };
        let first = save_with(&state, patch, |id, channel| {
            assert_eq!((id, channel), ("playlist-a", "channel-a"));
            Ok("Provider-owned title A".into())
        })
        .unwrap();
        assert_eq!(
            first.playlist_title.as_deref(),
            Some("Provider-owned title A")
        );
        connect(&connection, "channel-b");
        let second = load(&connection, Some("channel-b")).unwrap();
        assert!(second.made_for_kids);
        assert_eq!(second.visibility, "unlisted");
        assert!(second.playlist_id.is_none());
        save_with(
            &state,
            Patch {
                playlist_id: Some("playlist-b".into()),
                expected_channel_id: Some("channel-b".into()),
                ..Patch::default()
            },
            |_, _| Ok("Provider-owned title B".into()),
        )
        .unwrap();
        connect(&connection, "channel-a");
        assert_eq!(load(&connection, Some("channel-a")).unwrap(), first);
        connection
            .execute(
                "UPDATE connection_settings SET active_channel = NULL, active_channel_id = NULL",
                [],
            )
            .unwrap();
        let disconnected = load(&connection, None).unwrap();
        assert!(disconnected.playlist_id.is_none());
        assert!(disconnected.playlist_title.is_none());
        assert!(disconnected.playlist_channel_id.is_none());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_upload_default_playlist_clear_is_local_and_missing_preserves_selection() {
        let (root, state, connection) = fixture();
        connect(&connection, "channel-a");
        save_with(
            &state,
            Patch {
                playlist_id: Some("playlist-a".into()),
                ..Patch::default()
            },
            |_, _| Ok("Owned title".into()),
        )
        .unwrap();
        let saved = save_with(
            &state,
            Patch {
                made_for_kids: Some(true),
                ..Patch::default()
            },
            forbidden_provider,
        )
        .unwrap();
        assert_eq!(saved.playlist_id.as_deref(), Some("playlist-a"));
        connection.execute("INSERT INTO manual_playlist_defaults(channel_id, playlist_id, playlist_title, updated_at) VALUES('channel-b', 'playlist-b', 'Other title', ?1)", [now()]).unwrap();
        let cleared = save_with(
            &state,
            Patch {
                playlist_id: Some(String::new()),
                expected_channel_id: Some("channel-a".into()),
                ..Patch::default()
            },
            forbidden_provider,
        )
        .unwrap();
        assert!(cleared.made_for_kids);
        assert!(cleared.playlist_id.is_none());
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM manual_playlist_defaults WHERE channel_id = 'channel-b'",
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
    fn native_upload_defaults_reject_bad_inputs_and_stale_channel_before_provider_reads() {
        let (root, state, connection) = fixture();
        connect(&connection, "channel-a");
        for patch in [
            Patch {
                made_for_kids: Some(true),
                visibility: Some("scheduled".into()),
                ..Patch::default()
            },
            Patch {
                playlist_id: Some("unsafe/id".into()),
                ..Patch::default()
            },
            Patch {
                playlist_id: Some("playlist-a".into()),
                expected_channel_id: Some("channel-b".into()),
                ..Patch::default()
            },
            Patch {
                made_for_kids: Some(true),
                expected_channel_id: Some("channel-b".into()),
                ..Patch::default()
            },
        ] {
            assert!(save_with(&state, patch, forbidden_provider).is_err());
        }
        assert!(load(&connection, Some("channel-b")).is_err());
        assert!(!load(&connection, Some("channel-a")).unwrap().made_for_kids);
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id = NULL",
                [],
            )
            .unwrap();
        assert!(save_with(
            &state,
            Patch {
                playlist_id: Some("playlist-a".into()),
                ..Patch::default()
            },
            forbidden_provider
        )
        .is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_upload_defaults_do_not_partially_save_when_provider_or_final_channel_check_fails() {
        let (root, state, connection) = fixture();
        connect(&connection, "channel-a");
        let patch = || Patch {
            made_for_kids: Some(true),
            visibility: Some("public".into()),
            playlist_id: Some("playlist-a".into()),
            expected_channel_id: Some("channel-a".into()),
        };
        assert!(save_with(&state, patch(), |_, _| Err(
            "Provider read unavailable".into()
        ))
        .is_err());
        assert!(!load(&connection, None).unwrap().made_for_kids);
        assert_eq!(load(&connection, None).unwrap().visibility, "private");
        assert!(save_with(&state, patch(), |_, _| {
            connect(&connection, "channel-b");
            Ok("Owned title".into())
        })
        .is_err());
        assert!(!load(&connection, None).unwrap().made_for_kids);
        assert_eq!(load(&connection, None).unwrap().visibility, "private");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM manual_playlist_defaults", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_upload_defaults_require_exact_provider_playlist_owner_and_native_title() {
        let payload = serde_json::json!({"items": [{"id": "playlist-a", "snippet": {"channelId": "channel-a", "title": "Actual provider title"}}]});
        assert_eq!(
            verified_playlist_title(&payload, "playlist-a", "channel-a").unwrap(),
            "Actual provider title"
        );
        assert!(verified_playlist_title(&payload, "playlist-a", "channel-b").is_err());
        assert!(verified_playlist_title(&payload, "missing-playlist", "channel-a").is_err());
        assert!(verified_playlist_title(&serde_json::json!({"items": [{"id": "playlist-a", "snippet": {"channelId": "channel-a"}}]}), "playlist-a", "channel-a").is_err());
    }

    #[test]
    fn native_upload_defaults_transaction_failure_leaves_preferences_and_queue_unchanged() {
        let (root, state, connection) = fixture();
        connect(&connection, "channel-a");
        connection.execute("INSERT INTO upload_items(id, title, file_name, workspace_path, channel_id, size_bytes, status, confirmed_bytes, total_bytes, resumable_session_uri, created_at, updated_at) VALUES('saved', 'Queued video', 'original.mp4', 'original-source', 'channel-a', 1000, 'queued', 400, 1000, 'legacy-protected-reference', ?1, ?1)", [now()]).unwrap();
        connection.execute_batch("CREATE TRIGGER reject_default_audit BEFORE INSERT ON audit_events WHEN NEW.kind = 'manual_upload_defaults_set' BEGIN SELECT RAISE(ABORT, 'fixture audit failure'); END;").unwrap();
        assert!(save_with(
            &state,
            Patch {
                made_for_kids: Some(true),
                visibility: Some("public".into()),
                playlist_id: Some("playlist-a".into()),
                expected_channel_id: Some("channel-a".into())
            },
            |_, _| Ok("Owned title".into())
        )
        .is_err());
        let defaults = load(&connection, None).unwrap();
        assert!(!defaults.made_for_kids);
        assert_eq!(defaults.visibility, "private");
        assert!(defaults.playlist_id.is_none());
        let evidence: (String, String, i64, String) = connection.query_row("SELECT status, workspace_path, confirmed_bytes, resumable_session_uri FROM upload_items WHERE id = 'saved'", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap();
        assert_eq!(
            evidence,
            (
                "queued".into(),
                "original-source".into(),
                400,
                "legacy-protected-reference".into()
            )
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
}
