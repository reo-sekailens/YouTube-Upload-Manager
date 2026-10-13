//! Stable watched-folder ownership. One worker reuses the existing bounded
//! original-source intake path independently for each configured root.
use super::*;

const MAX_MONITORS: i64 = 64;

pub(super) fn root_key(path: &str) -> String {
    #[cfg(windows)]
    {
        path.strip_prefix(r"\\?\")
            .unwrap_or(path)
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.trim_end_matches('/').to_string()
    }
}

pub(super) fn empty_settings() -> FolderMonitorSettings {
    FolderMonitorSettings {
        id: None,
        enabled: false,
        folder_path: None,
        channel_name: None,
        channel_id: None,
        visibility: "private".into(),
        made_for_kids: false,
        delete_source_after_upload: false,
        playlist_id: None,
        playlist_title: None,
        status: "disabled".into(),
        detail: "No watched folder is configured for this channel.".into(),
        last_scan_at: None,
        last_file_name: None,
    }
}

fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FolderMonitorSettings> {
    Ok(FolderMonitorSettings {
        id: Some(row.get(0)?),
        enabled: row.get::<_, i64>(1)? != 0,
        folder_path: row.get(2)?,
        channel_name: row.get(3)?,
        channel_id: row.get(4)?,
        visibility: row.get(5)?,
        made_for_kids: row.get::<_, i64>(6)? != 0,
        delete_source_after_upload: row.get::<_, i64>(7)? != 0,
        playlist_id: row.get(8)?,
        playlist_title: row.get(9)?,
        status: row.get(10)?,
        detail: row.get(11)?,
        last_scan_at: row.get(12)?,
        last_file_name: row.get(13)?,
    })
}

const SETTINGS_COLUMNS: &str = "id, enabled, folder_path, channel_name, channel_id, visibility, made_for_kids, delete_source_after_upload, playlist_id, playlist_title, status, detail, last_scan_at, last_file_name";

pub(super) fn settings_for(
    connection: &Connection,
    id: &str,
) -> Result<FolderMonitorSettings, String> {
    connection
        .query_row(
            &format!("SELECT {SETTINGS_COLUMNS} FROM folder_monitors WHERE id = ?1"),
            [id],
            from_row,
        )
        .map_err(|_| {
            "The selected watched folder is unavailable. Refresh the folder list.".to_string()
        })
}

fn active_channel(
    connection: &Connection,
    expected: Option<&str>,
) -> Result<Option<String>, String> {
    let active = connection_settings(connection)?.active_channel_id;
    if expected.is_some_and(|id| id.is_empty() || active.as_deref() != Some(id)) {
        return Err("The connected YouTube channel changed. Refresh the watched-folder list before continuing.".into());
    }
    Ok(active)
}

#[derive(Serialize)]
pub(super) struct List {
    pub(super) monitors: Vec<FolderMonitorSettings>,
}

pub(super) fn list(connection: &Connection, expected: Option<&str>) -> Result<List, String> {
    let Some(active) = active_channel(connection, expected)? else {
        return Ok(List {
            monitors: Vec::new(),
        });
    };
    let monitors = connection.prepare(&format!("SELECT {SETTINGS_COLUMNS} FROM folder_monitors WHERE channel_id = ?1 AND removed = 0 ORDER BY updated_at DESC, id LIMIT {MAX_MONITORS}"))
        .map_err(user_error)?.query_map([active], from_row).map_err(user_error)?.collect::<Result<_, _>>().map_err(user_error)?;
    Ok(List { monitors })
}

pub(super) fn resolve(
    connection: &Connection,
    id: Option<&str>,
    expected: Option<&str>,
    allow_removed: bool,
) -> Result<String, String> {
    let active = active_channel(connection, expected)?
        .ok_or("Connect YouTube before using its watched folders.")?;
    if let Some(id) = id {
        let matched: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM folder_monitors WHERE id = ?1 AND channel_id = ?2 AND (?3 OR removed = 0))", params![id, active, allow_removed], |row| row.get(0)).map_err(user_error)?;
        if !matched {
            return Err("The selected watched folder is no longer available for the connected channel. Refresh the folder list.".into());
        }
        return Ok(id.into());
    }
    let ids = connection.prepare("SELECT id FROM folder_monitors WHERE channel_id = ?1 AND removed = 0 ORDER BY id LIMIT 2").map_err(user_error)?.query_map([active], |row| row.get::<_, String>(0)).map_err(user_error)?.collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    if ids.len() != 1 {
        return Err(
            "Choose a specific watched folder before continuing. No other folder was changed."
                .into(),
        );
    }
    Ok(ids[0].clone())
}

// Retained only as an internal compatibility helper for the former one-folder
// engine/tests. Public commands always resolve within the active channel.
#[cfg(test)]
pub(super) fn legacy_settings(connection: &Connection) -> Result<FolderMonitorSettings, String> {
    let ids = connection
        .prepare("SELECT id FROM folder_monitors WHERE removed = 0 ORDER BY id LIMIT 2")
        .map_err(user_error)?
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    match ids.as_slice() {
        [] => Ok(empty_settings()),
        [id] => settings_for(connection, id),
        _ => Err("Choose a specific watched folder; more than one is configured.".into()),
    }
}

pub(super) fn id_for_root(
    connection: &Connection,
    folder_path: &str,
    channel_name: &str,
) -> Result<String, String> {
    connection.query_row("SELECT id FROM folder_monitors WHERE folder_path = ?1 AND channel_name = ?2 AND removed = 0", params![folder_path, channel_name], |row| row.get(0)).map_err(|_| "This watched-folder authorization is no longer active.".into())
}

pub(super) fn audit_monitor(
    connection: &Connection,
    id: &str,
    kind: &str,
    detail: &str,
) -> Result<(), String> {
    connection.execute("INSERT INTO audit_events(id, item_id, channel_name, channel_id, folder_monitor_id, kind, detail, created_at) SELECT ?1, NULL, channel_name, COALESCE(channel_id, ''), id, ?2, ?3, ?4 FROM folder_monitors WHERE id = ?5", params![Uuid::new_v4().to_string(), kind, detail, now(), id]).map_err(user_error)?;
    Ok(())
}

pub(super) fn require_item_scope(
    connection: &Connection,
    id: &str,
    item_id: &str,
) -> Result<(), String> {
    let matched: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM folder_monitor_observations AS observation JOIN upload_items AS upload ON upload.id = observation.upload_item_id WHERE observation.monitor_id = ?1 AND observation.upload_item_id = ?2 AND upload.channel_id = observation.channel_id AND (upload.folder_monitor_id IS NULL OR upload.folder_monitor_id = ?1))", params![id, item_id], |row| row.get(0)).map_err(user_error)?;
    if matched {
        Ok(())
    } else {
        Err("This upload belongs to a different watched folder. Refresh the selected folder before continuing.".into())
    }
}

pub(super) fn validate_runtime_root(
    connection: &Connection,
    id: &str,
    folder: &Path,
) -> Result<(), String> {
    let canonical = folder.canonicalize().map_err(|_| {
        "The watched folder is unavailable; its saved queue is retained.".to_string()
    })?;
    let key = root_key(&canonical.to_string_lossy());
    let roots = connection
        .prepare("SELECT id, folder_path FROM folder_monitors WHERE removed = 0 AND id != ?1")
        .map_err(user_error)?
        .query_map([id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    for (_, path) in roots {
        if path
            .as_deref()
            .and_then(|path| Path::new(path).canonicalize().ok())
            .is_some_and(|root| root_key(&root.to_string_lossy()) == key)
        {
            return Err("This folder resolves to another configured watched root. Remove the duplicate entry before discovery continues.".into());
        }
    }
    let recorded: String = connection
        .query_row(
            "SELECT folder_key FROM folder_monitors WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if recorded != key {
        if id == "legacy-singleton" {
            connection
                .execute(
                    "UPDATE folder_monitors SET folder_key = ?1 WHERE id = ?2",
                    params![key, id],
                )
                .map_err(user_error)?;
        } else {
            return Err("The watched root now resolves to a different folder. Add that folder separately before allowing discovery.".into());
        }
    }
    Ok(())
}

pub(super) fn update_result(
    connection: &Connection,
    id: &str,
    status: &str,
    detail: &str,
    last_file_name: Option<&str>,
) -> Result<FolderMonitorSettings, String> {
    connection.execute("UPDATE folder_monitors SET status = ?1, detail = ?2, last_scan_at = ?3, last_file_name = COALESCE(?4, last_file_name), updated_at = ?3 WHERE id = ?5 AND enabled = 1 AND removed = 0", params![status, detail, now(), last_file_name, id]).map_err(user_error)?;
    settings_for(connection, id)
}

pub(super) fn configure(
    state: &AppState,
    path: String,
    visibility: String,
    made_for_kids: bool,
    delete_source_after_upload: bool,
    playlist_id: Option<String>,
    playlist_title: Option<String>,
    id: Option<String>,
    expected: Option<String>,
) -> Result<FolderMonitorSettings, String> {
    configure_with(
        state,
        path,
        visibility,
        made_for_kids,
        delete_source_after_upload,
        playlist_id,
        playlist_title,
        id,
        expected,
        |playlist, channel| upload_defaults::resolve_owned_playlist(state, playlist, channel),
    )
}

pub(super) fn configure_with(
    state: &AppState,
    path: String,
    visibility: String,
    made_for_kids: bool,
    delete_source_after_upload: bool,
    playlist_id: Option<String>,
    _playlist_title: Option<String>,
    id: Option<String>,
    expected: Option<String>,
    resolve_playlist: impl FnOnce(&str, &str) -> Result<String, String>,
) -> Result<FolderMonitorSettings, String> {
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "The folder monitor is unavailable.".to_string())?;
    if path.trim().is_empty() {
        return Err("Choose a folder to monitor.".into());
    }
    let selected_folder = PathBuf::from(&path);
    let folder = selected_folder
        .canonicalize()
        .map_err(|_| "Choose an existing local folder to monitor.".to_string())?;
    if !folder.is_dir() {
        return Err("Choose an existing local folder to monitor.".into());
    }
    let visibility = valid_folder_monitor_visibility(visibility.trim())?;
    let placeholder = playlist_id
        .as_ref()
        .map(|_| "Selected playlist".to_string());
    let (playlist_id, _) = valid_playlist_selection(playlist_id, placeholder)?;
    let mut connection = database(state)?;
    let channel = active_channel(&connection, expected.as_deref())?
        .ok_or("Connect the YouTube channel that should receive this folder's uploads first.")?;
    let name = connection_settings(&connection)?
        .active_channel
        .ok_or("The connected channel has no display name.")?;
    let folder_path = if selected_folder.is_absolute() {
        selected_folder
    } else {
        folder.clone()
    }
    .to_string_lossy()
    .to_string();
    let key = root_key(&folder.to_string_lossy());
    let id = match id {
        Some(id) => {
            resolve(&connection, Some(&id), Some(&channel), false)?;
            let (saved_key, saved_path): (String, Option<String>) = connection
                .query_row(
                    "SELECT folder_key, folder_path FROM folder_monitors WHERE id = ?1",
                    [&id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(user_error)?;
            let preserved_root = saved_path
                .as_deref()
                .and_then(|path| Path::new(path).canonicalize().ok())
                .is_some_and(|root| root_key(&root.to_string_lossy()) == key);
            if saved_key != key && !(id == "legacy-singleton" && preserved_root) {
                return Err("Add the new folder separately. A configured folder's root cannot be replaced while its history is retained.".into());
            }
            id
        }
        None => {
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM folder_monitors WHERE removed = 0",
                    [],
                    |row| row.get(0),
                )
                .map_err(user_error)?;
            if count >= MAX_MONITORS {
                return Err("Remove an unused watched folder before adding another. This device supports up to 64 configured folders.".into());
            }
            let exact = connection.query_row("SELECT id, folder_path FROM folder_monitors WHERE removed = 1 AND channel_id = ?1 AND folder_key = ?2 ORDER BY updated_at DESC, id LIMIT 1", params![channel, key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))).optional().map_err(user_error)?;
            let mut retained_id = None;
            if let Some((old_id, old_path)) = exact {
                let actual = old_path
                    .as_deref()
                    .and_then(|path| Path::new(path).canonicalize().ok())
                    .map(|root| root_key(&root.to_string_lossy()));
                if actual.as_deref() != Some(key.as_str()) {
                    return Err("Restore the retained folder's original path before resuming its saved history. Its queued uploads and checkpoints remain intact.".into());
                }
                retained_id = Some(old_id);
            } else {
                let archived = connection.prepare("SELECT id, folder_path FROM folder_monitors WHERE removed = 1 AND channel_id = ?1 ORDER BY updated_at DESC, id LIMIT 1025").map_err(user_error)?.query_map([&channel], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))).map_err(user_error)?.collect::<Result<Vec<_>, _>>().map_err(user_error)?;
                let truncated = archived.len() > 1024;
                for (old_id, old_path) in archived.into_iter().take(1024) {
                    let actual = old_path
                        .as_deref()
                        .and_then(|path| Path::new(path).canonicalize().ok())
                        .map(|root| root_key(&root.to_string_lossy()));
                    if actual.as_deref() == Some(key.as_str()) {
                        retained_id = Some(old_id);
                        break;
                    }
                }
                if retained_id.is_none() && truncated {
                    return Err("This device has too much retained folder history to safely verify a new path alias. Use the original configured path so its saved queue can be retained.".into());
                }
            }
            retained_id.unwrap_or_else(|| Uuid::new_v4().to_string())
        }
    };
    let duplicate: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM folder_monitors WHERE folder_key = ?1 AND id != ?2 AND removed = 0)", params![key, id], |row| row.get(0)).map_err(user_error)?;
    if duplicate {
        return Err("That folder is already configured. Resume or edit its existing entry instead of adding a duplicate.".into());
    }
    let live_roots = connection
        .prepare("SELECT folder_path FROM folder_monitors WHERE removed = 0 AND id != ?1")
        .map_err(user_error)?
        .query_map([&id], |row| row.get::<_, Option<String>>(0))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    if live_roots
        .iter()
        .filter_map(|path| path.as_deref())
        .filter_map(|path| Path::new(path).canonicalize().ok())
        .any(|root| root_key(&root.to_string_lossy()) == key)
    {
        return Err("That folder is already configured through another path. Resume or edit its existing entry instead.".into());
    }
    let playlist_title = playlist_id
        .as_deref()
        .map(|playlist| resolve_playlist(playlist, &channel))
        .transpose()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    active_channel(&transaction, Some(&channel))?;
    transaction.execute("INSERT INTO folder_monitors(id, enabled, folder_path, folder_key, channel_name, channel_id, visibility, made_for_kids, delete_source_after_upload, playlist_id, playlist_title, status, detail, updated_at) VALUES(?1, 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'watching', ?11, ?12) ON CONFLICT(id) DO UPDATE SET enabled = 1, removed = 0, folder_key = excluded.folder_key, channel_name = excluded.channel_name, visibility = excluded.visibility, made_for_kids = excluded.made_for_kids, delete_source_after_upload = excluded.delete_source_after_upload, playlist_id = excluded.playlist_id, playlist_title = excluded.playlist_title, status = excluded.status, detail = excluded.detail, updated_at = excluded.updated_at", params![id, folder_path, key, name, channel, visibility, made_for_kids, delete_source_after_upload, playlist_id, playlist_title, format!("Monitoring enabled; unchanged supported videos are prepared after two scans for {visibility} upload."), now()]).map_err(user_error)?;
    audit_monitor(
        &transaction,
        &id,
        "folder_monitor_enabled",
        "Operator enabled this folder's recurring uploads; existing queue choices remain unchanged",
    )?;
    transaction.commit().map_err(user_error)?;
    settings_for(&connection, &id)
}

pub(super) fn pause(
    state: &AppState,
    id: &str,
    expected: Option<&str>,
    remove: bool,
) -> Result<FolderMonitorSettings, String> {
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "The folder monitor is unavailable.".to_string())?;
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    resolve(&transaction, Some(id), expected, false)?;
    transaction.execute("UPDATE folder_monitors SET enabled = 0, removed = ?1, status = ?2, detail = ?3, updated_at = ?4 WHERE id = ?5", params![remove, if remove { "removed" } else { "disabled" }, if remove { "Watched-folder entry removed. Existing source files, queue, checkpoints and history are retained." } else { "Discovery paused for this folder. Existing queued uploads and original sources are retained." }, now(), id]).map_err(user_error)?;
    audit_monitor(&transaction, id, if remove { "folder_monitor_removed" } else { "folder_monitor_disabled" }, "Operator stopped this folder's discovery; retained queue and source evidence were not deleted")?;
    transaction.commit().map_err(user_error)?;
    settings_for(&connection, id)
}

pub(super) fn scan_for(
    state: &AppState,
    id: &str,
    dispatch_uploads: bool,
) -> Result<FolderMonitorSettings, String> {
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "The folder monitor is unavailable.".to_string())?;
    scan_folder_monitor_locked_for(state, id, dispatch_uploads)
}

pub(super) fn scan_all(state: &AppState, dispatch: bool) -> Result<(), String> {
    let connection = database(state)?;
    let ids = connection.prepare(&format!("SELECT id FROM folder_monitors WHERE enabled = 1 AND removed = 0 AND channel_id = COALESCE((SELECT active_channel_id FROM connection_settings WHERE singleton = 1), '') ORDER BY id LIMIT {MAX_MONITORS}")).map_err(user_error)?.query_map([], |row| row.get::<_, String>(0)).map_err(user_error)?.collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    drop(connection);
    for id in ids {
        if scan_for(state, &id, dispatch).is_err() {
            if let Ok(connection) = database(state) {
                let _ = update_result(&connection, &id, "error", "This folder scan could not complete. Other configured folders continue normally; saved queue evidence is retained.", None);
            }
        }
    }
    Ok(())
}

pub(super) fn reconcile(
    state: &AppState,
    id: &str,
    expected: Option<&str>,
) -> Result<usize, String> {
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "The folder monitor is unavailable.".to_string())?;
    let connection = database(state)?;
    resolve(&connection, Some(id), expected, false)?;
    let channel = settings_for(&connection, id)?
        .channel_id
        .ok_or("The selected folder has no channel identity.")?;
    let items = connection.prepare("SELECT upload.id, upload.video_id FROM upload_items AS upload JOIN folder_monitor_observations AS observation ON observation.upload_item_id = upload.id WHERE observation.monitor_id = ?1 AND observation.channel_id = ?2 AND upload.channel_id = ?2 AND (upload.folder_monitor_id IS NULL OR upload.folder_monitor_id = ?1) AND upload.status = 'needs_reconciliation' ORDER BY upload.updated_at, upload.id LIMIT 200").map_err(user_error)?.query_map(params![id, channel], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))).map_err(user_error)?.collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    if items.is_empty() {
        return Ok(0);
    }
    let token = refreshed_access_token(state)?;
    if active_channel_id(state, &token)? != channel {
        return Err(
            "The authorized YouTube channel changed. Reconnect before reconciling this folder."
                .into(),
        );
    }
    let ids = items
        .iter()
        .filter_map(|(_, id)| id.as_deref())
        .collect::<Vec<_>>();
    let mut confirmed = HashMap::new();
    for chunk in ids.chunks(50) {
        let joined = chunk.join(",");
        let payload = youtube_json(
            state,
            &token,
            "videos",
            &[("part", "id,snippet,status"), ("id", joined.as_str())],
        )?;
        let rows = payload
            .get("items")
            .and_then(serde_json::Value::as_array)
            .ok_or(
            "YouTube returned an unreadable video-verification result. No uploads were retried.",
        )?;
        for row in rows {
            let video_id = row
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or("YouTube returned an invalid video identity. No uploads were retried.")?;
            if !chunk.contains(&video_id)
                || row
                    .pointer("/snippet/channelId")
                    .and_then(serde_json::Value::as_str)
                    != Some(channel.as_str())
            {
                return Err("YouTube did not confirm the reviewed video and folder channel. No uploads were retried.".into());
            }
            confirmed.insert(video_id.to_string(), row.clone());
        }
    }
    drop(connection);
    let retried = commit_reconciliation(state, id, &channel, &items, &confirmed)?;
    if retried > 0 {
        let _ = start_queued_uploads_impl(state);
    }
    Ok(retried)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-multi-monitors-{}", Uuid::new_v4()));
        for folder in ["media", "first", "second", "third"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connect(&connection, "channel-a");
        (root, state, connection)
    }

    fn connect(connection: &Connection, channel: &str) {
        connection.execute("INSERT INTO connection_settings(singleton, active_channel, active_channel_id, updated_at) VALUES(1, 'Fixture channel', ?1, ?2) ON CONFLICT(singleton) DO UPDATE SET active_channel_id = excluded.active_channel_id", params![channel, now()]).unwrap();
    }

    fn add(state: &AppState, path: &Path, visibility: &str, kids: bool) -> FolderMonitorSettings {
        configure(
            state,
            path.to_string_lossy().to_string(),
            visibility.into(),
            kids,
            false,
            None,
            None,
            None,
            Some("channel-a".into()),
        )
        .unwrap()
    }

    fn item(connection: &Connection, id: &str) -> String {
        connection.query_row("SELECT upload_item_id FROM folder_monitor_observations WHERE monitor_id = ?1 AND upload_item_id IS NOT NULL", [id], |row| row.get(0)).unwrap()
    }

    #[test]
    fn native_multi_folder_discovers_existing_sources_independently_without_copies_and_restarts() {
        let (root, state, connection) = fixture();
        fs::write(
            root.join("first").join("same-name.mp4"),
            b"first fixture video",
        )
        .unwrap();
        fs::write(
            root.join("second").join("same-name.mp4"),
            b"second fixture video",
        )
        .unwrap();
        let first = add(&state, &root.join("first"), "private", false);
        let second = add(&state, &root.join("second"), "unlisted", true);
        assert_ne!(first.id, second.id);
        assert_eq!(
            list(&connection, Some("channel-a")).unwrap().monitors.len(),
            2
        );
        assert!(resolve(&connection, None, Some("channel-a"), false).is_err());
        scan_all(&state, false).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let relaunched = state.clone();
        scan_all(&relaunched, false).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM upload_items WHERE status = 'queued'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
        for settings in [first, second] {
            let id = settings.id.unwrap();
            let choices: (String, bool, String, String) = connection.query_row("SELECT visibility, made_for_kids, workspace_path, source_path FROM upload_items WHERE folder_monitor_id = ?1", [&id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap();
            assert_eq!(choices.0, settings.visibility);
            assert_eq!(choices.1, settings.made_for_kids);
            assert_eq!(choices.2, choices.3);
            assert!(Path::new(&choices.3).exists());
            assert!(require_item_scope(&connection, &id, &item(&connection, &id)).is_ok());
        }
        assert_eq!(fs::read_dir(&state.media_directory).unwrap().count(), 0);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_failure_does_not_stop_other_roots_and_channel_list_is_isolated() {
        let (root, state, connection) = fixture();
        let first = add(&state, &root.join("first"), "private", false);
        let second = add(&state, &root.join("second"), "private", false);
        fs::remove_dir(root.join("first")).unwrap();
        fs::write(root.join("second").join("second.mp4"), b"second video").unwrap();
        scan_all(&state, false).unwrap();
        scan_all(&state, false).unwrap();
        assert_eq!(
            settings_for(&connection, first.id.as_deref().unwrap())
                .unwrap()
                .status,
            "error"
        );
        assert_eq!(
            settings_for(&connection, second.id.as_deref().unwrap())
                .unwrap()
                .status,
            "watching"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM upload_items WHERE folder_monitor_id = ?1",
                    [second.id.as_deref().unwrap()],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        connect(&connection, "channel-b");
        assert!(list(&connection, Some("channel-b"))
            .unwrap()
            .monitors
            .is_empty());
        assert!(resolve(&connection, first.id.as_deref(), Some("channel-b"), false).is_err());
        assert!(pause(
            &state,
            second.id.as_deref().unwrap(),
            Some("channel-a"),
            false
        )
        .is_err());
        assert!(
            settings_for(&connection, second.id.as_deref().unwrap())
                .unwrap()
                .enabled
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_duplicate_root_and_edits_preserve_old_queue_choices() {
        let (root, state, connection) = fixture();
        let first = add(&state, &root.join("first"), "private", false);
        assert!(configure(
            &state,
            root.join("first").join(".").to_string_lossy().to_string(),
            "private".into(),
            false,
            false,
            None,
            None,
            None,
            Some("channel-a".into())
        )
        .is_err());
        fs::write(root.join("first").join("first.mp4"), b"first video").unwrap();
        scan_all(&state, false).unwrap();
        scan_all(&state, false).unwrap();
        let id = first.id.unwrap();
        assert!(configure(
            &state,
            root.join("second").to_string_lossy().to_string(),
            "unlisted".into(),
            true,
            false,
            None,
            None,
            Some(id.clone()),
            Some("channel-a".into())
        )
        .is_err());
        configure(
            &state,
            root.join("first").to_string_lossy().to_string(),
            "unlisted".into(),
            true,
            false,
            None,
            None,
            Some(id.clone()),
            Some("channel-a".into()),
        )
        .unwrap();
        let choices: (String, bool) = connection
            .query_row(
                "SELECT visibility, made_for_kids FROM upload_items WHERE folder_monitor_id = ?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(choices, ("private".into(), false));
        assert_eq!(
            list(&connection, Some("channel-a")).unwrap().monitors.len(),
            1
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_pause_remove_and_late_scans_preserve_other_folders_and_evidence() {
        let (root, state, connection) = fixture();
        let first = add(&state, &root.join("first"), "private", false);
        let second = add(&state, &root.join("second"), "private", false);
        fs::write(root.join("first").join("first.mp4"), b"first video").unwrap();
        fs::write(root.join("second").join("second.mp4"), b"second video").unwrap();
        scan_all(&state, false).unwrap();
        scan_all(&state, false).unwrap();
        let first_id = first.id.unwrap();
        let second_id = second.id.unwrap();
        let first_item = item(&connection, &first_id);
        connection.execute("UPDATE upload_items SET confirmed_bytes = 4, resumable_session_uri = 'legacy-protected-reference' WHERE id = ?1", [&first_item]).unwrap();
        pause(&state, &first_id, Some("channel-a"), false).unwrap();
        fs::write(
            root.join("first").join("new-after-pause.mp4"),
            b"retained file",
        )
        .unwrap();
        scan_for(&state, &first_id, false).unwrap();
        assert_eq!(
            settings_for(&connection, &first_id).unwrap().status,
            "disabled"
        );
        assert!(settings_for(&connection, &second_id).unwrap().enabled);
        pause(&state, &first_id, Some("channel-a"), true).unwrap();
        scan_for(&state, &first_id, false).unwrap();
        assert_eq!(
            settings_for(&connection, &first_id).unwrap().status,
            "removed"
        );
        assert_eq!(
            list(&connection, Some("channel-a")).unwrap().monitors.len(),
            1
        );
        let evidence: (String, i64, String) = connection.query_row("SELECT status, confirmed_bytes, resumable_session_uri FROM upload_items WHERE id = ?1", [&first_item], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
        assert_eq!(
            evidence,
            ("queued".into(), 4, "legacy-protected-reference".into())
        );
        assert!(root.join("first").join("first.mp4").exists());
        assert!(root.join("first").join("new-after-pause.mp4").exists());
        assert!(connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM audit_events WHERE folder_monitor_id = ?1)",
                [&first_id],
                |row| row.get::<_, bool>(0)
            )
            .unwrap());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_retry_and_reconciliation_cannot_mutate_another_folder() {
        let (root, state, connection) = fixture();
        let first = add(&state, &root.join("first"), "private", false);
        let second = add(&state, &root.join("second"), "private", false);
        fs::write(root.join("first").join("first.mp4"), b"first video").unwrap();
        fs::write(root.join("second").join("second.mp4"), b"second video").unwrap();
        scan_all(&state, false).unwrap();
        scan_all(&state, false).unwrap();
        let first_id = first.id.unwrap();
        let second_id = second.id.unwrap();
        let first_item = item(&connection, &first_id);
        let second_item = item(&connection, &second_id);
        assert!(require_item_scope(&connection, &first_id, &second_item).is_err());
        connection
            .execute("UPDATE upload_items SET status = 'cancelled'", [])
            .unwrap();
        assert!(requeue_cancelled_folder_monitor_files_for(
            &state,
            &first_id,
            vec![second_item.clone()],
            false
        )
        .is_err());
        assert_eq!(
            requeue_cancelled_folder_monitor_files_for(
                &state,
                &first_id,
                vec![first_item.clone()],
                false
            )
            .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id = ?1",
                    [&second_item],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "cancelled"
        );
        connection.execute("UPDATE upload_items SET status = 'needs_reconciliation', resumable_session_uri = 'legacy-protected-reference'", []).unwrap();
        assert_eq!(
            commit_reconciliation(
                &state,
                &first_id,
                "channel-a",
                &[(first_item.clone(), None)],
                &HashMap::new()
            )
            .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id = ?1",
                    [&second_item],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "needs_reconciliation"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT resumable_session_uri FROM upload_items WHERE id = ?1",
                    [&first_item],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "legacy-protected-reference"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_remove_then_add_reuses_history_without_reingesting_queued_originals() {
        let (root, state, connection) = fixture();
        let first = add(&state, &root.join("first"), "private", false);
        let first_id = first.id.unwrap();
        fs::write(root.join("first").join("first.mp4"), b"first video").unwrap();
        scan_all(&state, false).unwrap();
        scan_all(&state, false).unwrap();
        let existing_item = item(&connection, &first_id);
        connection.execute("UPDATE upload_items SET confirmed_bytes = 4, resumable_session_uri = 'legacy-protected-reference' WHERE id = ?1", [&existing_item]).unwrap();
        pause(&state, &first_id, Some("channel-a"), true).unwrap();
        let restored = add(&state, &root.join("first"), "unlisted", true);
        assert_eq!(restored.id.as_deref(), Some(first_id.as_str()));
        scan_all(&state, false).unwrap();
        scan_all(&state, false).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let retained: (String, bool, i64, String) = connection.query_row("SELECT visibility, made_for_kids, confirmed_bytes, resumable_session_uri FROM upload_items WHERE id = ?1", [&existing_item], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap();
        assert_eq!(
            retained,
            (
                "private".into(),
                false,
                4,
                "legacy-protected-reference".into()
            )
        );
        fs::write(root.join("first").join("later.mp4"), b"later video").unwrap();
        scan_all(&state, false).unwrap();
        scan_all(&state, false).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT visibility FROM upload_items WHERE file_name = 'later.mp4'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "unlisted"
        );
        assert_eq!(fs::read_dir(&state.media_directory).unwrap().count(), 0);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_scoped_local_cleanup_deletes_only_the_reviewed_original() {
        let (root, state, connection) = fixture();
        let first = add(&state, &root.join("first"), "private", false);
        let second = add(&state, &root.join("second"), "private", false);
        fs::write(root.join("first").join("first.mp4"), b"first video").unwrap();
        fs::write(root.join("second").join("second.mp4"), b"second video").unwrap();
        scan_all(&state, false).unwrap();
        scan_all(&state, false).unwrap();
        let first_id = first.id.unwrap();
        let second_id = second.id.unwrap();
        let first_item = item(&connection, &first_id);
        let second_item = item(&connection, &second_id);
        let digest = blake3::hash(b"first video").to_hex().to_string();
        connection.execute("UPDATE upload_items SET digest = ?1, background_hash_status = 'complete', status = 'uploaded', video_id = 'first-video', confirmed_bytes = total_bytes WHERE id = ?2", params![digest, first_item]).unwrap();
        connection.execute("INSERT INTO remote_videos(video_id, channel_name, channel_id, title, upload_status, updated_at) VALUES('first-video', 'Fixture channel', 'channel-a', 'First video', 'uploaded', ?1)", [now()]).unwrap();
        assert!(require_item_scope(&connection, &first_id, &second_item).is_err());
        assert!(delete_folder_monitor_uploaded_source_for(
            &state,
            &second_item,
            "second.mp4",
            Some(&first_id)
        )
        .is_err());
        assert!(root.join("second").join("second.mp4").exists());
        delete_folder_monitor_uploaded_source_for(
            &state,
            &first_item,
            "first.mp4",
            Some(&first_id),
        )
        .unwrap();
        assert!(!root.join("first").join("first.mp4").exists());
        assert!(root.join("second").join("second.mp4").exists());
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id = ?1",
                    [&second_item],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "queued"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_paused_legacy_alias_resumes_without_changing_source_locators() {
        let (root, state, connection) = fixture();
        let alias = root
            .join("second")
            .join("..")
            .join("first")
            .to_string_lossy()
            .to_string();
        fs::write(root.join("first").join("first.mp4"), b"legacy video").unwrap();
        connection.execute("INSERT INTO folder_monitors(id, enabled, folder_path, folder_key, channel_name, channel_id, status, updated_at) VALUES('legacy-singleton', 0, ?1, ?2, 'Fixture channel', 'channel-a', 'disabled', ?3)", params![alias, root_key(&alias), now()]).unwrap();
        let original_path = Path::new(&alias)
            .join("first.mp4")
            .to_string_lossy()
            .to_string();
        connection.execute("INSERT INTO folder_monitor_observations(monitor_id, channel_id, channel_name, file_path, size_bytes, modified_key, state, first_seen_at, updated_at) VALUES('legacy-singleton', 'channel-a', 'Fixture channel', ?1, 12, 'retained-signature', 'baseline', ?2, ?2)", params![original_path, now()]).unwrap();
        let resumed = configure(
            &state,
            root.join("first").to_string_lossy().to_string(),
            "private".into(),
            false,
            false,
            None,
            None,
            Some("legacy-singleton".into()),
            Some("channel-a".into()),
        )
        .unwrap();
        assert!(resumed.enabled);
        assert_eq!(resumed.folder_path.as_deref(), Some(alias.as_str()));
        assert_eq!(connection.query_row("SELECT file_path FROM folder_monitor_observations WHERE monitor_id = 'legacy-singleton'", [], |row| row.get::<_, String>(0)).unwrap(), original_path);
        assert_eq!(
            connection
                .query_row(
                    "SELECT folder_key FROM folder_monitors WHERE id = 'legacy-singleton'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            root_key(&root.join("first").canonicalize().unwrap().to_string_lossy())
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_delayed_scan_cannot_resurrect_a_removed_monitor() {
        let (root, state, connection) = fixture();
        let first = add(&state, &root.join("first"), "private", false);
        let second = add(&state, &root.join("second"), "private", false);
        let first_id = first.id.unwrap();
        fs::write(root.join("first").join("first.mp4"), b"first video").unwrap();
        let blocker = state.folder_monitor_lock.lock().unwrap();
        let scan_state = state.clone();
        let scan_id = first_id.clone();
        let delayed_scan = thread::spawn(move || scan_for(&scan_state, &scan_id, false).unwrap());
        let remove_state = state.clone();
        let remove_id = first_id.clone();
        let delayed_remove = thread::spawn(move || {
            pause(&remove_state, &remove_id, Some("channel-a"), true).unwrap()
        });
        drop(blocker);
        delayed_scan.join().unwrap();
        delayed_remove.join().unwrap();
        let removed = settings_for(&connection, &first_id).unwrap();
        assert!(!removed.enabled);
        assert_eq!(removed.status, "removed");
        scan_for(&state, &first_id, false).unwrap();
        assert_eq!(
            settings_for(&connection, &first_id).unwrap().status,
            "removed"
        );
        assert!(
            settings_for(&connection, second.id.as_deref().unwrap())
                .unwrap()
                .enabled
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_playlist_approval_resolves_native_title_and_rejects_a_stale_channel() {
        let (root, state, connection) = fixture();
        let first = configure_with(
            &state,
            root.join("first").to_string_lossy().to_string(),
            "private".into(),
            false,
            false,
            Some("playlist-a".into()),
            Some("Untrusted webview title".into()),
            None,
            Some("channel-a".into()),
            |id, channel| {
                assert_eq!((id, channel), ("playlist-a", "channel-a"));
                Ok("Owner-verified native title".into())
            },
        )
        .unwrap();
        assert_eq!(
            first.playlist_title.as_deref(),
            Some("Owner-verified native title")
        );
        assert!(configure_with(
            &state,
            root.join("second").to_string_lossy().to_string(),
            "private".into(),
            false,
            false,
            Some("other-playlist".into()),
            None,
            None,
            Some("channel-a".into()),
            |_, _| Err("Playlist belongs to another channel".into())
        )
        .is_err());
        assert!(configure_with(
            &state,
            root.join("second").to_string_lossy().to_string(),
            "private".into(),
            false,
            false,
            Some("playlist-a".into()),
            None,
            None,
            Some("channel-a".into()),
            |_, _| {
                connect(&connection, "channel-b");
                Ok("Verified before channel switched".into())
            }
        )
        .is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM folder_monitors", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_multi_folder_worker_handoff_rechecks_an_enable_after_the_empty_snapshot() {
        let (root, state, connection) = fixture();
        startup_gate(&state)
            .folder_worker_started
            .store(true, Ordering::Release);
        add(&state, &root.join("first"), "private", false);
        assert!(restart_folder_worker_after_empty_check(&state));
        assert!(startup_gate(&state)
            .folder_worker_started
            .load(Ordering::Acquire));
        startup_gate(&state)
            .folder_worker_started
            .store(false, Ordering::Release);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
}

fn commit_reconciliation(
    state: &AppState,
    id: &str,
    channel: &str,
    items: &[(String, Option<String>)],
    confirmed: &HashMap<String, serde_json::Value>,
) -> Result<usize, String> {
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    resolve(&transaction, Some(id), Some(channel), false)?;
    let mut retried = 0;
    for (item_id, video_id) in items {
        require_item_scope(&transaction, id, item_id)?;
        let live = video_id.as_deref().and_then(|id| confirmed.get(id));
        if let Some(video) = live {
            let title = video
                .pointer("/snippet/title")
                .and_then(serde_json::Value::as_str)
                .ok_or("YouTube did not return a video title; reconciliation was retained.")?;
            let privacy = video
                .pointer("/status/privacyStatus")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("private");
            let upload_status = video
                .pointer("/status/uploadStatus")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("uploaded");
            transaction.execute("INSERT INTO remote_videos(video_id, channel_name, channel_id, title, privacy_status, upload_status, updated_at) VALUES(?1, (SELECT channel_name FROM folder_monitors WHERE id = ?2), ?3, ?4, ?5, ?6, ?7) ON CONFLICT(video_id) DO UPDATE SET title = excluded.title, privacy_status = excluded.privacy_status, upload_status = excluded.upload_status, updated_at = excluded.updated_at WHERE remote_videos.channel_id = excluded.channel_id", params![video_id, id, channel, title, privacy, upload_status, now()]).map_err(user_error)?;
            transaction.execute("UPDATE upload_items SET status = 'uploaded', detail = 'YouTube confirmed the prior upload for this watched folder; no retry was sent.', updated_at = ?1 WHERE id = ?2 AND channel_id = ?3 AND status = 'needs_reconciliation'", params![now(), item_id, channel]).map_err(user_error)?;
            audit(&transaction, item_id, "folder_monitor_reconciliation_confirmed", "Owner-authorized YouTube verification confirmed this selected folder's prior video")?;
        } else {
            retried += transaction.execute("UPDATE upload_items SET status = 'queued', video_id = NULL, detail = 'Owner-authorized YouTube verification did not contain the prior video ID; this selected folder upload is queued for provider-session reconciliation.', updated_at = ?1 WHERE id = ?2 AND channel_id = ?3 AND status = 'needs_reconciliation'", params![now(), item_id, channel]).map_err(user_error)?;
            audit(&transaction, item_id, "folder_monitor_reconciliation_requeued", "Operator-approved retry retained this selected folder's original source and protected checkpoint")?;
        }
    }
    transaction.execute("INSERT INTO channel_generations(channel_id, inventory_generation, upload_generation, updated_at) VALUES(?1, 1, 1, ?2) ON CONFLICT(channel_id) DO UPDATE SET inventory_generation = inventory_generation + 1, upload_generation = upload_generation + 1, updated_at = excluded.updated_at", params![channel, now()]).map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    Ok(retried)
}
