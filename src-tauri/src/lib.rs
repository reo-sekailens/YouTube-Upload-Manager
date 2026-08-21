use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use keyring::v1::Entry as CredentialEntry;
use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    database_path: PathBuf,
    media_directory: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadItem {
    id: String,
    title: String,
    file_name: String,
    size_bytes: u64,
    digest: Option<String>,
    status: String,
    confirmed_bytes: u64,
    total_bytes: u64,
    video_id: Option<String>,
    detail: Option<String>,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateCandidate {
    id: String,
    confidence: String,
    left_title: String,
    right_title: String,
    evidence: String,
    decision: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DashboardSnapshot {
    active_channel: Option<String>,
    items: Vec<UploadItem>,
    duplicates: Vec<DuplicateCandidate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionSettings {
    client_id: Option<String>,
    active_channel: Option<String>,
    connected: bool,
    secure_store_available: bool,
    deletion_authorized: bool,
    detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OAuthStart {
    authorization_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteVideo {
    video_id: String,
    title: String,
    duration: Option<String>,
    privacy_status: Option<String>,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeletionRequest {
    id: String,
    video_id: String,
    title: String,
    status: String,
    detail: String,
    updated_at: String,
}

fn user_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn database(state: &AppState) -> Result<Connection, String> {
    let connection = Connection::open(&state.database_path).map_err(user_error)?;
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS upload_items (
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
            CREATE INDEX IF NOT EXISTS upload_items_status_idx ON upload_items(status);
            CREATE TABLE IF NOT EXISTS connection_settings (
              singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
              oauth_client_id TEXT,
              active_channel TEXT,
              connection_detail TEXT,
              deletion_authorized INTEGER NOT NULL DEFAULT 0,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS remote_videos (
              video_id TEXT PRIMARY KEY NOT NULL,
              channel_name TEXT NOT NULL,
              title TEXT NOT NULL,
              duration TEXT,
              privacy_status TEXT,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS deletion_requests (
              id TEXT PRIMARY KEY NOT NULL,
              video_id TEXT NOT NULL UNIQUE,
              title TEXT NOT NULL,
              status TEXT NOT NULL,
              detail TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS audit_events (
              id TEXT PRIMARY KEY NOT NULL,
              item_id TEXT,
              kind TEXT NOT NULL,
              detail TEXT,
              created_at TEXT NOT NULL,
              FOREIGN KEY(item_id) REFERENCES upload_items(id)
            );
            ",
        )
        .map_err(user_error)?;
    // Existing local workspaces are upgraded in place; no queue data is discarded.
    for migration in [
        "ALTER TABLE upload_items ADD COLUMN partial_path TEXT",
        "ALTER TABLE upload_items ADD COLUMN imported_bytes INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE connection_settings ADD COLUMN connection_detail TEXT",
        "ALTER TABLE connection_settings ADD COLUMN deletion_authorized INTEGER NOT NULL DEFAULT 0",
    ] {
        let _ = connection.execute(migration, []);
    }
    Ok(connection)
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn connection_settings(connection: &Connection) -> Result<ConnectionSettings, String> {
    connection
        .query_row(
            "SELECT oauth_client_id, active_channel, connection_detail, deletion_authorized FROM connection_settings WHERE singleton = 1",
            [],
            |row| {
                Ok(ConnectionSettings {
                    client_id: row.get(0)?,
                    active_channel: row.get(1)?,
                    connected: row.get::<_, Option<String>>(1)?.is_some(),
                    secure_store_available: secure_store_available(),
                    detail: row.get(2)?,
                    deletion_authorized: row.get::<_, i64>(3)? != 0,
                })
            },
        )
        .or_else(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Ok(ConnectionSettings {
                client_id: None,
                active_channel: None,
                connected: false,
                secure_store_available: secure_store_available(),
                detail: None,
                deletion_authorized: false,
            }),
            other => Err(other),
        })
        .map_err(user_error)
}

fn secure_store_available() -> bool {
    CredentialEntry::store_status().is_ok()
}

fn refresh_token_entry() -> Result<CredentialEntry, String> {
    CredentialEntry::new("ph.furries.youtube-mass-uploader", "youtube-refresh-token")
        .map_err(user_error)
}

// OAuth callbacks use this boundary so credentials stay out of the database and webview.
fn persist_refresh_token(refresh_token: &str) -> Result<(), String> {
    if refresh_token.is_empty() {
        return Err("Refusing to store an empty refresh token.".into());
    }
    refresh_token_entry()?
        .set_password(refresh_token)
        .map_err(user_error)
}

fn clear_refresh_token() -> Result<(), String> {
    refresh_token_entry()?
        .delete_credential()
        .map_err(user_error)
}

fn upload_session_entry(item_id: &str) -> Result<CredentialEntry, String> {
    CredentialEntry::new(
        "ph.furries.youtube-mass-uploader",
        &format!("youtube-resumable-session-{item_id}"),
    )
    .map_err(user_error)
}

fn save_upload_session(item_id: &str, session_uri: &str) -> Result<(), String> {
    let url = url::Url::parse(session_uri).map_err(user_error)?;
    if url.scheme() != "https" {
        return Err("Refusing to persist a non-HTTPS YouTube upload session.".into());
    }
    upload_session_entry(item_id)?
        .set_password(session_uri)
        .map_err(user_error)
}

fn confirmed_offset_from_range(range: Option<&str>) -> Result<u64, String> {
    let Some(range) = range else { return Ok(0) };
    range
        .strip_prefix("bytes=0-")
        .ok_or_else(|| "YouTube returned an invalid resumable range.".to_string())?
        .parse::<u64>()
        .map(|value| value.saturating_add(1))
        .map_err(|_| "YouTube returned an invalid resumable range.".into())
}

fn oauth_verifier_entry(state: &str) -> Result<CredentialEntry, String> {
    CredentialEntry::new(
        "ph.furries.youtube-mass-uploader",
        &format!("youtube-oauth-pkce-{state}"),
    )
    .map_err(user_error)
}

fn set_connection_detail(
    state: &AppState,
    detail: &str,
    active_channel: Option<&str>,
) -> Result<(), String> {
    let connection = database(state)?;
    connection
        .execute(
            "INSERT INTO connection_settings (singleton, active_channel, connection_detail, updated_at) VALUES (1, ?1, ?2, ?3) ON CONFLICT(singleton) DO UPDATE SET active_channel = excluded.active_channel, connection_detail = excluded.connection_detail, updated_at = excluded.updated_at",
            params![active_channel, detail, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn oauth_error_message(error: &str) -> String {
    if error == "access_denied" {
        "Google authorization was cancelled.".into()
    } else {
        "Google authorization did not complete. Try connecting again.".into()
    }
}

fn callback_value(target: &str, key: &str) -> Option<String> {
    let callback = url::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    callback
        .query_pairs()
        .find_map(|(name, value)| (name == key).then(|| value.into_owned()))
}

fn respond_to_callback(stream: &mut TcpStream, text: &str) {
    let body = format!("<!doctype html><title>YouTube Mass Uploader</title><p>{text}</p><p>You may close this window and return to the app.</p>");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn complete_oauth_connection(
    state: &AppState,
    client_id: &str,
    redirect_uri: &str,
    verifier: &str,
    code: &str,
    deletion_authorized: bool,
) -> Result<String, String> {
    let token_response: serde_json::Value = reqwest::blocking::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .map_err(|_| "Google token exchange could not be reached.".to_string())?
        .error_for_status()
        .map_err(|_| "Google rejected the authorization response.".to_string())?
        .json()
        .map_err(|_| "Google returned an unreadable authorization response.".to_string())?;
    let access_token = token_response
        .get("access_token")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Google did not return an access token.".to_string())?;
    let refresh_token = token_response
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            "Google did not return a refresh token; revoke this app in Google and connect again."
                .to_string()
        })?;
    let channel_response: serde_json::Value = reqwest::blocking::Client::new()
        .get("https://www.googleapis.com/youtube/v3/channels")
        .query(&[("part", "snippet"), ("mine", "true")])
        .bearer_auth(access_token)
        .send()
        .map_err(|_| "YouTube channel verification could not be reached.".to_string())?
        .error_for_status()
        .map_err(|_| "YouTube rejected the channel verification request.".to_string())?
        .json()
        .map_err(|_| "YouTube returned an unreadable channel response.".to_string())?;
    let channel = channel_response
        .get("items")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.pointer("/snippet/title"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| "No YouTube channel was returned for this Google account.".to_string())?
        .to_string();
    persist_refresh_token(refresh_token)?;
    set_connection_detail(
        state,
        "Connected to YouTube on this device.",
        Some(&channel),
    )?;
    let connection = database(state)?;
    connection
        .execute(
            "UPDATE connection_settings SET deletion_authorized = ?1, updated_at = ?2 WHERE singleton = 1",
            params![deletion_authorized as i64, now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_connected",
        "YouTube channel connection verified",
    )?;
    Ok(channel)
}

fn refreshed_access_token(state: &AppState) -> Result<String, String> {
    let connection = database(state)?;
    let client_id = connection_settings(&connection)?
        .client_id
        .ok_or_else(|| "Google OAuth client ID is not configured.".to_string())?;
    let refresh_token = refresh_token_entry()?.get_password().map_err(|_| {
        "This device no longer has a YouTube credential; connect YouTube again.".to_string()
    })?;
    let token_response: serde_json::Value = reqwest::blocking::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(|_| "Google token refresh could not be reached.".to_string())?
        .error_for_status()
        .map_err(|_| "Google rejected the saved YouTube authorization. Connect again.".to_string())?
        .json()
        .map_err(|_| "Google returned an unreadable token refresh response.".to_string())?;
    token_response
        .get("access_token")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| "Google did not return a refreshed access token.".to_string())
}

fn stored_upload_session(item_id: &str) -> Result<Option<String>, String> {
    match upload_session_entry(item_id)?.get_password() {
        Ok(uri) => Ok(Some(uri)),
        Err(_) => Ok(None),
    }
}

fn mark_upload_state(
    state: &AppState,
    item_id: &str,
    status: &str,
    confirmed_bytes: u64,
    detail: &str,
    video_id: Option<&str>,
) -> Result<(), String> {
    let connection = database(state)?;
    connection
        .execute(
            "UPDATE upload_items SET status = ?1, confirmed_bytes = ?2, detail = ?3, video_id = COALESCE(?4, video_id), updated_at = ?5 WHERE id = ?6",
            params![status, confirmed_bytes as i64, detail, video_id, now(), item_id],
        )
        .map_err(user_error)?;
    Ok(())
}

fn establish_upload_session(
    state: &AppState,
    item_id: &str,
    title: &str,
    total_bytes: u64,
    access_token: &str,
) -> Result<String, String> {
    let response = reqwest::blocking::Client::new()
        .post("https://www.googleapis.com/upload/youtube/v3/videos")
        .query(&[("uploadType", "resumable"), ("part", "snippet,status")])
        .bearer_auth(access_token)
        .header("X-Upload-Content-Length", total_bytes)
        .header("X-Upload-Content-Type", "application/octet-stream")
        .json(&serde_json::json!({
            "snippet": { "title": title },
            "status": { "privacyStatus": "private" }
        }))
        .send()
        .map_err(|_| "YouTube could not start the resumable upload.".to_string())?;
    if !response.status().is_success() {
        return Err("YouTube rejected the upload setup request.".into());
    }
    let session_uri = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "YouTube did not return an upload session.".to_string())?;
    save_upload_session(item_id, session_uri)?;
    mark_upload_state(
        state,
        item_id,
        "uploading",
        0,
        "YouTube resumable session started.",
        None,
    )?;
    Ok(session_uri.to_string())
}

fn query_upload_session(session_uri: &str, total_bytes: u64) -> Result<Option<u64>, String> {
    let response = reqwest::blocking::Client::new()
        .put(session_uri)
        .header("Content-Length", "0")
        .header("Content-Range", format!("bytes */{total_bytes}"))
        .send()
        .map_err(|_| "YouTube upload-session reconciliation could not be reached.".to_string())?;
    if response.status().as_u16() == 308 {
        return confirmed_offset_from_range(
            response
                .headers()
                .get("range")
                .and_then(|value| value.to_str().ok()),
        )
        .map(Some);
    }
    if response.status().is_success() {
        return Ok(None);
    }
    Err("YouTube upload session expired or could not be safely reconciled.".into())
}

fn upload_item(state: &AppState, item_id: &str) -> Result<(), String> {
    let connection = database(state)?;
    let (title, workspace_path, total_bytes): (String, String, u64) = connection
        .query_row(
            "SELECT title, workspace_path, total_bytes FROM upload_items WHERE id = ?1 AND status IN ('queued', 'uploading', 'needs_reconciliation')",
            [item_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get::<_, i64>(2)? as u64)),
        )
        .map_err(|_| "This upload is no longer eligible to run.".to_string())?;
    if !Path::new(&workspace_path).is_file() {
        return Err("The managed local media file is missing; this upload cannot continue.".into());
    }
    let access_token = refreshed_access_token(state)?;
    let session_uri = match stored_upload_session(item_id)? {
        Some(uri) => match query_upload_session(&uri, total_bytes)? {
            Some(offset) => {
                mark_upload_state(
                    state,
                    item_id,
                    "uploading",
                    offset,
                    "Resuming from YouTube-confirmed byte range.",
                    None,
                )?;
                uri
            }
            None => {
                mark_upload_state(
                    state,
                    item_id,
                    "needs_reconciliation",
                    total_bytes,
                    "YouTube reports a completed upload; review the channel before any retry.",
                    None,
                )?;
                return Ok(());
            }
        },
        None => establish_upload_session(state, item_id, &title, total_bytes, &access_token)?,
    };
    let start_offset = database(state)?
        .query_row(
            "SELECT confirmed_bytes FROM upload_items WHERE id = ?1",
            [item_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(user_error)? as u64;
    let mut source = File::open(workspace_path).map_err(user_error)?;
    source
        .seek(SeekFrom::Start(start_offset))
        .map_err(user_error)?;
    let mut offset = start_offset;
    let mut buffer = vec![0_u8; 8 * 1024 * 1024];
    while offset < total_bytes {
        let bytes = source.read(&mut buffer).map_err(user_error)?;
        if bytes == 0 {
            return Err("Managed local media ended before the expected length.".into());
        }
        let end = offset + bytes as u64 - 1;
        let response = reqwest::blocking::Client::new()
            .put(&session_uri)
            .header("Content-Length", bytes)
            .header("Content-Type", "application/octet-stream")
            .header(
                "Content-Range",
                format!("bytes {offset}-{end}/{total_bytes}"),
            )
            .body(buffer[..bytes].to_vec())
            .send()
            .map_err(|_| {
                "Upload connection interrupted; the saved session will be reconciled on retry."
                    .to_string()
            })?;
        if response.status().as_u16() == 308 {
            offset = confirmed_offset_from_range(
                response
                    .headers()
                    .get("range")
                    .and_then(|value| value.to_str().ok()),
            )?;
            mark_upload_state(
                state,
                item_id,
                "uploading",
                offset,
                "YouTube confirmed upload bytes.",
                None,
            )?;
            continue;
        }
        if response.status().is_success() {
            let payload: serde_json::Value = response.json().map_err(|_| {
                "YouTube completed the upload but returned an unreadable result.".to_string()
            })?;
            let video_id = payload
                .get("id")
                .and_then(|value| value.as_str())
                .ok_or_else(|| "YouTube completed the upload without a video ID.".to_string())?;
            mark_upload_state(
                state,
                item_id,
                "uploaded",
                total_bytes,
                "Uploaded to YouTube; processing status can be checked from inventory.",
                Some(video_id),
            )?;
            let _ = upload_session_entry(item_id)
                .and_then(|entry| entry.delete_credential().map_err(user_error));
            let audit_connection = database(state)?;
            audit(
                &audit_connection,
                item_id,
                "upload_completed",
                "YouTube returned a video ID",
            )?;
            return Ok(());
        }
        return Err(
            "YouTube rejected an upload chunk; the session was preserved for reconciliation."
                .into(),
        );
    }
    Err("Upload finished without a final YouTube result; reconciliation is required.".into())
}

fn run_queued_uploads(state: AppState, item_ids: Vec<String>) {
    for item_id in item_ids {
        if let Err(error) = upload_item(&state, &item_id) {
            let confirmed_bytes = database(&state)
                .and_then(|connection| {
                    connection
                        .query_row(
                            "SELECT confirmed_bytes FROM upload_items WHERE id = ?1",
                            [&item_id],
                            |row| row.get::<_, i64>(0),
                        )
                        .map(|value| value as u64)
                        .map_err(user_error)
                })
                .unwrap_or(0);
            let _ = mark_upload_state(
                &state,
                &item_id,
                "needs_reconciliation",
                confirmed_bytes,
                &error,
                None,
            );
            if let Ok(connection) = database(&state) {
                let _ = audit(
                    &connection,
                    &item_id,
                    "upload_interrupted",
                    "Upload requires provider reconciliation",
                );
            }
        }
    }
}

fn youtube_json(
    access_token: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<serde_json::Value, String> {
    reqwest::blocking::Client::new()
        .get(format!("https://www.googleapis.com/youtube/v3/{path}"))
        .bearer_auth(access_token)
        .query(query)
        .send()
        .map_err(|_| "YouTube inventory sync could not be reached.".to_string())?
        .error_for_status()
        .map_err(|_| "YouTube rejected the inventory request.".to_string())?
        .json()
        .map_err(|_| "YouTube returned an unreadable inventory response.".to_string())
}

fn sync_channel_inventory_worker(state: &AppState) -> Result<usize, String> {
    let access_token = refreshed_access_token(state)?;
    let channel_response = youtube_json(
        &access_token,
        "channels",
        &[("part", "contentDetails,snippet"), ("mine", "true")],
    )?;
    let channel = channel_response
        .get("items")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .ok_or_else(|| "No YouTube channel was returned for this account.".to_string())?;
    let channel_name = channel
        .pointer("/snippet/title")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "YouTube did not return a channel name.".to_string())?;
    let uploads_playlist = channel
        .pointer("/contentDetails/relatedPlaylists/uploads")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "YouTube did not return an uploads playlist.".to_string())?;
    let connection = database(state)?;
    connection
        .execute("DELETE FROM remote_videos", [])
        .map_err(user_error)?;
    let mut next_page: Option<String> = None;
    let mut count = 0;
    loop {
        let mut query = vec![
            ("part", "contentDetails"),
            ("playlistId", uploads_playlist),
            ("maxResults", "50"),
        ];
        if let Some(page) = next_page.as_deref() {
            query.push(("pageToken", page));
        }
        let playlist = youtube_json(&access_token, "playlistItems", &query)?;
        let ids = playlist
            .get("items")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|item| {
                item.pointer("/contentDetails/videoId")
                    .and_then(|value| value.as_str())
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
        for ids in ids.chunks(50) {
            let joined = ids.join(",");
            let videos = youtube_json(
                &access_token,
                "videos",
                &[
                    ("part", "snippet,contentDetails,status"),
                    ("id", joined.as_str()),
                ],
            )?;
            for video in videos
                .get("items")
                .and_then(|value| value.as_array())
                .into_iter()
                .flatten()
            {
                let Some(video_id) = video.get("id").and_then(|value| value.as_str()) else {
                    continue;
                };
                let Some(title) = video
                    .pointer("/snippet/title")
                    .and_then(|value| value.as_str())
                else {
                    continue;
                };
                let duration = video
                    .pointer("/contentDetails/duration")
                    .and_then(|value| value.as_str());
                let privacy = video
                    .pointer("/status/privacyStatus")
                    .and_then(|value| value.as_str());
                connection.execute("INSERT INTO remote_videos (video_id, channel_name, title, duration, privacy_status, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![video_id, channel_name, title, duration, privacy, now()]).map_err(user_error)?;
                count += 1;
            }
        }
        next_page = playlist
            .get("nextPageToken")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        if next_page.is_none() {
            break;
        }
    }
    set_connection_detail(
        state,
        &format!("Synced {count} YouTube video records locally."),
        Some(channel_name),
    )?;
    audit_global(
        &connection,
        "youtube_inventory_synced",
        "Channel upload inventory synced locally",
    )?;
    Ok(count)
}

fn await_oauth_callback(
    state: AppState,
    listener: TcpListener,
    expected_state: String,
    client_id: String,
    redirect_uri: String,
    deletion_authorized: bool,
) {
    let deadline = Instant::now() + Duration::from_secs(600);
    let _ = listener.set_nonblocking(true);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut request = [0_u8; 8192];
                let bytes = match stream.read(&mut request) {
                    Ok(bytes) => bytes,
                    Err(_) => 0,
                };
                let request_line = String::from_utf8_lossy(&request[..bytes])
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                let target = request_line.split_whitespace().nth(1).unwrap_or("/");
                let received_state = callback_value(target, "state");
                let code = callback_value(target, "code");
                let result = if received_state.as_deref() != Some(&expected_state) {
                    Err("The Google authorization response could not be verified.".to_string())
                } else if let Some(error) = callback_value(target, "error") {
                    Err(oauth_error_message(&error))
                } else if let Some(code) = code {
                    let verifier = oauth_verifier_entry(&expected_state)
                        .and_then(|entry| entry.get_password().map_err(user_error));
                    verifier.and_then(|verifier| {
                        complete_oauth_connection(
                            &state,
                            &client_id,
                            &redirect_uri,
                            &verifier,
                            &code,
                            deletion_authorized,
                        )
                    })
                } else {
                    Err("The Google authorization response did not include a code.".to_string())
                };
                let _ = oauth_verifier_entry(&expected_state)
                    .and_then(|entry| entry.delete_credential().map_err(user_error));
                match result {
                    Ok(_) => respond_to_callback(&mut stream, "YouTube is connected."),
                    Err(error) => {
                        let _ = set_connection_detail(&state, &error, None);
                        respond_to_callback(&mut stream, "Connection did not complete.");
                    }
                }
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(125))
            }
            Err(_) => break,
        }
    }
    let _ = set_connection_detail(
        &state,
        "Google authorization timed out. Connect again when ready.",
        None,
    );
}

fn valid_google_client_id(client_id: &str) -> bool {
    let candidate = client_id.trim();
    candidate.len() > ".apps.googleusercontent.com".len()
        && candidate.ends_with(".apps.googleusercontent.com")
        && candidate.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '.'
        })
}

fn row_to_upload_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<UploadItem> {
    Ok(UploadItem {
        id: row.get("id")?,
        title: row.get("title")?,
        file_name: row.get("file_name")?,
        size_bytes: row.get::<_, i64>("size_bytes")? as u64,
        digest: row.get("digest")?,
        status: row.get("status")?,
        confirmed_bytes: row.get::<_, i64>("confirmed_bytes")? as u64,
        total_bytes: row.get::<_, i64>("total_bytes")? as u64,
        video_id: row.get("video_id")?,
        detail: row.get("detail")?,
        updated_at: row.get("updated_at")?,
    })
}

fn find_item(connection: &Connection, id: &str) -> Result<UploadItem, String> {
    connection
        .query_row(
            "SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, updated_at FROM upload_items WHERE id = ?1",
            [id],
            row_to_upload_item,
        )
        .map_err(user_error)
}

fn exact_local_duplicates(connection: &Connection) -> Result<Vec<DuplicateCandidate>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, digest FROM upload_items WHERE digest IS NOT NULL ORDER BY created_at ASC",
        )
        .map_err(user_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut by_digest: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for (id, title, digest) in rows {
        by_digest.entry(digest).or_default().push((id, title));
    }
    let mut candidates = Vec::new();
    for (digest, items) in by_digest {
        if items.len() < 2 {
            continue;
        }
        let (left_id, left_title) = &items[0];
        for (right_id, right_title) in items.iter().skip(1) {
            candidates.push(DuplicateCandidate {
                id: format!("local:{left_id}:{right_id}"),
                confidence: "exact_local".into(),
                left_title: left_title.clone(),
                right_title: right_title.clone(),
                evidence: format!("Matching managed-media SHA-256: {digest}"),
                decision: None,
            });
        }
    }
    Ok(candidates)
}

fn audit(connection: &Connection, item_id: &str, kind: &str, detail: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO audit_events (id, item_id, kind, detail, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![Uuid::new_v4().to_string(), item_id, kind, detail, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn audit_global(connection: &Connection, kind: &str, detail: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO audit_events (id, item_id, kind, detail, created_at) VALUES (?1, NULL, ?2, ?3, ?4)",
            params![Uuid::new_v4().to_string(), kind, detail, now()],
        )
        .map_err(user_error)?;
    Ok(())
}

fn copy_and_digest(source: &Path, destination_partial: &Path) -> Result<(u64, String), String> {
    let mut input = File::open(source).map_err(user_error)?;
    let existing_bytes = destination_partial
        .metadata()
        .map(|value| value.len())
        .unwrap_or(0);
    let source_bytes = input.metadata().map_err(user_error)?.len();
    if existing_bytes > source_bytes {
        return Err("The interrupted local copy is larger than its original source.".into());
    }
    input
        .seek(SeekFrom::Start(existing_bytes))
        .map_err(user_error)?;
    let mut output = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(destination_partial)
        .map_err(user_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];

    if existing_bytes > 0 {
        let mut prior = File::open(destination_partial).map_err(user_error)?;
        loop {
            let bytes = prior.read(&mut buffer).map_err(user_error)?;
            if bytes == 0 {
                break;
            }
            hasher.update(&buffer[..bytes]);
        }
    }
    let mut copied = existing_bytes;

    loop {
        let bytes = input.read(&mut buffer).map_err(user_error)?;
        if bytes == 0 {
            break;
        }
        output.write_all(&buffer[..bytes]).map_err(user_error)?;
        hasher.update(&buffer[..bytes]);
        copied += bytes as u64;
    }
    output.sync_all().map_err(user_error)?;
    Ok((copied, format!("{:x}", hasher.finalize())))
}

fn finish_import(
    connection: &Connection,
    id: &str,
    source: &Path,
    partial_path: &Path,
    workspace_path: &Path,
    expected_bytes: u64,
) -> Result<UploadItem, String> {
    let (copied, digest) = copy_and_digest(source, partial_path)?;
    if copied != expected_bytes {
        return Err("The selected source changed before its local copy finished.".into());
    }
    fs::rename(partial_path, workspace_path).map_err(user_error)?;
    connection
        .execute(
            "UPDATE upload_items SET digest = ?1, imported_bytes = ?2, status = 'draft', detail = 'Imported locally; ready for review', updated_at = ?3 WHERE id = ?4",
            params![digest, copied as i64, now(), id],
        )
        .map_err(user_error)?;
    audit(
        connection,
        id,
        "asset_import_completed",
        "Managed local asset verified with SHA-256",
    )?;
    find_item(connection, id)
}

fn initialize_state(app: &AppHandle) -> Result<AppState, String> {
    let root = app.path().app_data_dir().map_err(user_error)?;
    let media_directory = root.join("media");
    fs::create_dir_all(&media_directory).map_err(user_error)?;
    let state = AppState {
        database_path: root.join("queue.sqlite3"),
        media_directory,
    };
    database(&state)?;
    Ok(state)
}

#[tauri::command]
fn dashboard_snapshot(state: State<'_, AppState>) -> Result<DashboardSnapshot, String> {
    let connection = database(&state)?;
    let mut statement = connection
        .prepare("SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, updated_at FROM upload_items ORDER BY updated_at DESC")
        .map_err(user_error)?;
    let items = statement
        .query_map([], row_to_upload_item)
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(DashboardSnapshot {
        active_channel: connection_settings(&connection)?.active_channel,
        items,
        duplicates: exact_local_duplicates(&connection)?,
    })
}

#[tauri::command]
fn load_connection_settings(state: State<'_, AppState>) -> Result<ConnectionSettings, String> {
    connection_settings(&database(&state)?)
}

#[tauri::command]
fn save_oauth_client_id(
    oauth_client_id: String,
    state: State<'_, AppState>,
) -> Result<ConnectionSettings, String> {
    let client_id = oauth_client_id.trim();
    if !valid_google_client_id(client_id) {
        return Err("Enter a Google desktop or mobile OAuth client ID ending in .apps.googleusercontent.com.".into());
    }
    let connection = database(&state)?;
    connection
        .execute(
            "INSERT INTO connection_settings (singleton, oauth_client_id, updated_at) VALUES (1, ?1, ?2) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id = excluded.oauth_client_id, updated_at = excluded.updated_at",
            params![client_id, now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "oauth_client_configured",
        "OAuth client identifier saved locally",
    )?;
    connection_settings(&connection)
}

fn begin_oauth_connection(
    state: State<'_, AppState>,
    scope: &str,
    deletion_authorized: bool,
) -> Result<OAuthStart, String> {
    if !secure_store_available() {
        return Err("This device's secure credential store is unavailable; YouTube tokens cannot be stored safely.".into());
    }
    let connection = database(&state)?;
    let settings = connection_settings(&connection)?;
    let client_id = settings
        .client_id
        .ok_or_else(|| "Save a Google OAuth client ID before connecting YouTube.".to_string())?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "A secure local OAuth callback port could not be opened.".to_string())?;
    let port = listener.local_addr().map_err(user_error)?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth2/callback");
    let state_token = Uuid::new_v4().simple().to_string();
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    oauth_verifier_entry(&state_token)?
        .set_password(&verifier)
        .map_err(user_error)?;
    let authorization_url = url::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", scope),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state_token.as_str()),
        ],
    )
    .map_err(user_error)?
    .to_string();
    set_connection_detail(
        &state,
        "Waiting for Google authorization in your browser.",
        None,
    )?;
    let callback_state = state.inner().clone();
    thread::spawn(move || {
        await_oauth_callback(
            callback_state,
            listener,
            state_token,
            client_id,
            redirect_uri,
            deletion_authorized,
        )
    });
    Ok(OAuthStart { authorization_url })
}

#[tauri::command]
fn begin_youtube_connection(state: State<'_, AppState>) -> Result<OAuthStart, String> {
    begin_oauth_connection(
        state,
        "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly",
        false,
    )
}

#[tauri::command]
fn begin_deletion_authorization(state: State<'_, AppState>) -> Result<OAuthStart, String> {
    begin_oauth_connection(
        state,
        "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly https://www.googleapis.com/auth/youtube.force-ssl",
        true,
    )
}

#[tauri::command]
fn disconnect_youtube(state: State<'_, AppState>) -> Result<ConnectionSettings, String> {
    // Credential deletion is idempotent from the operator's perspective: an already-cleared
    // credential must not prevent the local connection record from being removed.
    let _ = clear_refresh_token();
    let connection = database(&state)?;
    connection
        .execute(
            "UPDATE connection_settings SET active_channel = NULL, deletion_authorized = 0, connection_detail = 'YouTube disconnected on this device.', updated_at = ?1 WHERE singleton = 1",
            params![now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_disconnected",
        "Local YouTube credential removed",
    )?;
    connection_settings(&connection)
}

#[tauri::command]
fn start_queued_uploads(state: State<'_, AppState>) -> Result<usize, String> {
    let connection = database(&state)?;
    let settings = connection_settings(&connection)?;
    if !settings.connected {
        return Err("Connect a YouTube channel before starting uploads.".into());
    }
    let item_ids = connection
        .prepare("SELECT id FROM upload_items WHERE status = 'queued' ORDER BY created_at ASC")
        .map_err(user_error)?
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    if item_ids.is_empty() {
        return Err("No reviewed uploads are waiting in the local queue.".into());
    }
    let total = item_ids.len();
    let worker_state = state.inner().clone();
    thread::spawn(move || run_queued_uploads(worker_state, item_ids));
    Ok(total)
}

#[tauri::command]
fn sync_channel_inventory(state: State<'_, AppState>) -> Result<usize, String> {
    let settings = connection_settings(&database(&state)?)?;
    if !settings.connected {
        return Err("Connect a YouTube channel before syncing its library.".into());
    }
    sync_channel_inventory_worker(&state)
}

fn row_to_remote_video(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemoteVideo> {
    Ok(RemoteVideo {
        video_id: row.get("video_id")?,
        title: row.get("title")?,
        duration: row.get("duration")?,
        privacy_status: row.get("privacy_status")?,
        updated_at: row.get("updated_at")?,
    })
}

fn row_to_deletion_request(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeletionRequest> {
    Ok(DeletionRequest {
        id: row.get("id")?,
        video_id: row.get("video_id")?,
        title: row.get("title")?,
        status: row.get("status")?,
        detail: row.get("detail")?,
        updated_at: row.get("updated_at")?,
    })
}

#[tauri::command]
fn list_remote_videos(state: State<'_, AppState>) -> Result<Vec<RemoteVideo>, String> {
    let connection = database(&state)?;
    let mut statement = connection
        .prepare("SELECT video_id, title, duration, privacy_status, updated_at FROM remote_videos ORDER BY updated_at DESC, title ASC")
        .map_err(user_error)?;
    let videos = statement
        .query_map([], row_to_remote_video)
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(videos)
}

#[tauri::command]
fn list_deletion_requests(state: State<'_, AppState>) -> Result<Vec<DeletionRequest>, String> {
    let connection = database(&state)?;
    let mut statement = connection
        .prepare("SELECT id, video_id, title, status, detail, updated_at FROM deletion_requests ORDER BY updated_at DESC")
        .map_err(user_error)?;
    let requests = statement
        .query_map([], row_to_deletion_request)
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(requests)
}

#[tauri::command]
fn request_video_deletion(
    video_id: String,
    confirmation: String,
    state: State<'_, AppState>,
) -> Result<DeletionRequest, String> {
    if confirmation.trim() != video_id {
        return Err("Type the exact YouTube video ID to create a deletion request.".into());
    }
    let connection = database(&state)?;
    let title: String = connection
        .query_row(
            "SELECT title FROM remote_videos WHERE video_id = ?1",
            [&video_id],
            |row| row.get(0),
        )
        .map_err(|_| {
            "Sync the active channel library and choose a current video before requesting deletion."
                .to_string()
        })?;
    let id = Uuid::new_v4().to_string();
    let detail =
        "Deletion requested locally. A separately re-authorized execution step is required.";
    connection
        .execute(
            "INSERT INTO deletion_requests (id, video_id, title, status, detail, created_at, updated_at) VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?5) ON CONFLICT(video_id) DO UPDATE SET status = 'pending', detail = excluded.detail, updated_at = excluded.updated_at",
            params![id, video_id, title, detail, now()],
        )
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_deletion_requested",
        "Operator created a local deletion request",
    )?;
    connection
        .query_row(
            "SELECT id, video_id, title, status, detail, updated_at FROM deletion_requests WHERE video_id = ?1",
            [&video_id],
            row_to_deletion_request,
        )
        .map_err(user_error)
}

#[tauri::command]
fn cancel_deletion_request(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let connection = database(&state)?;
    let affected = connection
        .execute(
            "UPDATE deletion_requests SET status = 'cancelled', detail = 'Operator cancelled this local deletion request.', updated_at = ?1 WHERE id = ?2 AND status = 'pending'",
            params![now(), id],
        )
        .map_err(user_error)?;
    if affected != 1 {
        return Err("Only a pending local deletion request can be cancelled.".into());
    }
    audit_global(
        &connection,
        "youtube_deletion_cancelled",
        "Operator cancelled a local deletion request",
    )?;
    Ok(())
}

fn active_channel_id(access_token: &str) -> Result<String, String> {
    youtube_json(
        access_token,
        "channels",
        &[("part", "id"), ("mine", "true")],
    )?
    .get("items")
    .and_then(|value| value.as_array())
    .and_then(|items| items.first())
    .and_then(|item| item.get("id"))
    .and_then(|value| value.as_str())
    .map(str::to_string)
    .ok_or_else(|| "YouTube did not return the active channel identity.".to_string())
}

#[tauri::command]
fn execute_deletion_request(
    id: String,
    confirmation: String,
    state: State<'_, AppState>,
) -> Result<DeletionRequest, String> {
    let connection = database(&state)?;
    let settings = connection_settings(&connection)?;
    if !settings.deletion_authorized {
        return Err(
            "Grant the separate YouTube deletion permission before executing a deletion request."
                .into(),
        );
    }
    let (video_id, status): (String, String) = connection
        .query_row(
            "SELECT video_id, status FROM deletion_requests WHERE id = ?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|_| "This local deletion request no longer exists.".to_string())?;
    if status != "pending" {
        return Err("Only a pending deletion request can be executed.".into());
    }
    if confirmation.trim() != video_id {
        return Err("Type the exact YouTube video ID again before permanent deletion.".into());
    }
    let access_token = refreshed_access_token(&state)?;
    let current_channel_id = active_channel_id(&access_token)?;
    let video = youtube_json(
        &access_token,
        "videos",
        &[("part", "snippet"), ("id", video_id.as_str())],
    )?;
    let verified_channel_id = video
        .get("items")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.pointer("/snippet/channelId"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            "The target video is unavailable; deletion was not attempted.".to_string()
        })?;
    if verified_channel_id != current_channel_id {
        return Err("The target video is not owned by the currently authorized channel.".into());
    }
    let response = reqwest::blocking::Client::new()
        .delete("https://www.googleapis.com/youtube/v3/videos")
        .query(&[("id", video_id.as_str())])
        .bearer_auth(&access_token)
        .send()
        .map_err(|_| {
            "YouTube deletion could not be reached; no local success was recorded.".to_string()
        })?;
    if response.status().as_u16() != 204 {
        return Err("YouTube rejected the deletion; the local request remains pending.".into());
    }
    connection
        .execute(
            "UPDATE deletion_requests SET status = 'deleted', detail = 'YouTube returned HTTP 204 after fresh channel-ownership validation.', updated_at = ?1 WHERE id = ?2",
            params![now(), id],
        )
        .map_err(user_error)?;
    connection
        .execute("DELETE FROM remote_videos WHERE video_id = ?1", [&video_id])
        .map_err(user_error)?;
    audit_global(
        &connection,
        "youtube_deletion_completed",
        "YouTube returned HTTP 204 for a confirmed deletion",
    )?;
    connection
        .query_row(
            "SELECT id, video_id, title, status, detail, updated_at FROM deletion_requests WHERE id = ?1",
            [&id],
            row_to_deletion_request,
        )
        .map_err(user_error)
}

#[tauri::command]
fn import_asset(path: String, state: State<'_, AppState>) -> Result<UploadItem, String> {
    let source = PathBuf::from(&path);
    let metadata = fs::metadata(&source).map_err(user_error)?;
    if !metadata.is_file() {
        return Err("Select a video file, not a directory.".into());
    }

    let id = Uuid::new_v4().to_string();
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "The selected file has no supported name.".to_string())?
        .to_string();
    let title = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled video")
        .to_string();
    let workspace_path = state.media_directory.join(format!("{id}.media"));
    let partial_path = state.media_directory.join(format!("{id}.partial"));
    let timestamp = now();
    let connection = database(&state)?;

    connection
        .execute(
            "INSERT INTO upload_items (id, title, file_name, source_path, workspace_path, partial_path, size_bytes, status, total_bytes, created_at, updated_at, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'importing', ?7, ?8, ?8, 'Importing into device-local workspace')",
            params![id, title, file_name, path, workspace_path.to_string_lossy(), partial_path.to_string_lossy(), metadata.len() as i64, timestamp],
        )
        .map_err(user_error)?;
    audit(
        &connection,
        &id,
        "asset_import_started",
        "Copying selected media to managed local workspace",
    )?;

    match finish_import(
        &connection,
        &id,
        &source,
        &partial_path,
        &workspace_path,
        metadata.len(),
    ) {
        Ok(item) => Ok(item),
        Err(error) => {
            connection
                .execute(
                    "UPDATE upload_items SET imported_bytes = ?1, status = 'importing', detail = ?2, updated_at = ?3 WHERE id = ?4",
                    params![partial_path.metadata().map(|value| value.len()).unwrap_or(0) as i64, format!("Local import paused: {error}"), now(), id],
                )
                .map_err(user_error)?;
            Err(error)
        }
    }
}

#[tauri::command]
fn queue_item(id: String, state: State<'_, AppState>) -> Result<UploadItem, String> {
    let connection = database(&state)?;
    let item = find_item(&connection, &id)?;
    if item.status != "draft" && item.status != "failed" {
        return Err("Only a reviewed draft or recoverable failed item can be queued.".into());
    }
    let workspace_path: String = connection
        .query_row(
            "SELECT workspace_path FROM upload_items WHERE id = ?1",
            [&id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if !Path::new(&workspace_path).is_file() {
        return Err(
            "This item has not finished its device-local import; repair it before queueing.".into(),
        );
    }
    connection
        .execute(
            "UPDATE upload_items SET status = 'queued', detail = 'Saved in local queue; waiting for YouTube connection', updated_at = ?1 WHERE id = ?2",
            params![now(), id],
        )
        .map_err(user_error)?;
    audit(&connection, &id, "item_queued", "Queue state saved locally")?;
    find_item(&connection, &id)
}

#[tauri::command]
fn reconcile_queue(state: State<'_, AppState>) -> Result<Vec<UploadItem>, String> {
    let connection = database(&state)?;
    let mut statement = connection
        .prepare("SELECT id, source_path, workspace_path, partial_path, size_bytes, status FROM upload_items WHERE status IN ('importing', 'uploading')")
        .map_err(user_error)?;
    let interrupted = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)? as u64,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;

    for (id, source_path, workspace_path, partial_path, expected_bytes, status) in interrupted {
        if status == "importing" && Path::new(&workspace_path).exists() {
            connection.execute("UPDATE upload_items SET status = 'draft', detail = 'Recovered completed local asset after restart', updated_at = ?1 WHERE id = ?2", params![now(), id]).map_err(user_error)?;
            audit(
                &connection,
                &id,
                "restart_reconciliation",
                "Recovered completed local asset after restart",
            )?;
            continue;
        }
        if status == "importing" {
            let resumed = match (source_path.as_deref(), partial_path.as_deref()) {
                (Some(source), Some(partial)) if Path::new(source).is_file() => finish_import(
                    &connection,
                    &id,
                    Path::new(source),
                    Path::new(partial),
                    Path::new(&workspace_path),
                    expected_bytes,
                ),
                _ => {
                    Err("The original source is no longer available at its saved location.".into())
                }
            };
            match resumed {
                Ok(_) => continue,
                Err(error) => {
                    let imported_bytes = partial_path
                        .as_deref()
                        .and_then(|value| fs::metadata(value).ok())
                        .map(|value| value.len())
                        .unwrap_or(0);
                    let detail = format!("Local import paused at {imported_bytes} bytes; select the original file to repair this entry. {error}");
                    connection.execute("UPDATE upload_items SET imported_bytes = ?1, status = 'failed', detail = ?2, updated_at = ?3 WHERE id = ?4", params![imported_bytes as i64, detail, now(), id]).map_err(user_error)?;
                    audit(
                        &connection,
                        &id,
                        "restart_reconciliation",
                        "Interrupted local import needs its original source repaired",
                    )?;
                    continue;
                }
            }
        }
        let (next_status, detail) = {
            (
                "needs_reconciliation",
                "App closed during upload; verify the YouTube resumable session before retrying",
            )
        };
        connection
            .execute(
                "UPDATE upload_items SET status = ?1, detail = ?2, updated_at = ?3 WHERE id = ?4",
                params![next_status, detail, now(), id],
            )
            .map_err(user_error)?;
        audit(&connection, &id, "restart_reconciliation", detail)?;
    }

    let mut all = connection
        .prepare("SELECT id, title, file_name, size_bytes, digest, status, confirmed_bytes, total_bytes, video_id, detail, updated_at FROM upload_items ORDER BY updated_at DESC")
        .map_err(user_error)?;
    let items = all
        .query_map([], row_to_upload_item)
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    Ok(items)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(
                initialize_state(app.handle())
                    .map_err(|error| Box::<dyn std::error::Error>::from(error))?,
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dashboard_snapshot,
            load_connection_settings,
            save_oauth_client_id,
            begin_youtube_connection,
            disconnect_youtube,
            start_queued_uploads,
            sync_channel_inventory,
            list_remote_videos,
            list_deletion_requests,
            request_video_deletion,
            cancel_deletion_request,
            begin_deletion_authorization,
            execute_deletion_request,
            import_asset,
            queue_item,
            reconcile_queue
        ])
        .run(tauri::generate_context!())
        .expect("error while running local application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_copy_resumes_and_keeps_the_original_digest() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source.mp4");
        let partial = root.join("managed.partial");
        let data = (0..2_600_000_u32)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(&source, &data).unwrap();
        fs::write(&partial, &data[..1_100_000]).unwrap();

        let (copied, digest) = copy_and_digest(&source, &partial).unwrap();
        let expected = format!("{:x}", Sha256::digest(&data));

        assert_eq!(copied, data.len() as u64);
        assert_eq!(digest, expected);
        assert_eq!(fs::read(&partial).unwrap(), data);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_matching_local_digests_are_exact_duplicate_candidates() {
        let root = std::env::temp_dir().join(format!("youtube-uploader-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
        };
        fs::create_dir_all(&state.media_directory).unwrap();
        let connection = database(&state).unwrap();
        for (id, title, digest) in [
            ("first", "First copy", "matching-digest"),
            ("second", "Second copy", "matching-digest"),
            ("third", "Different source", "other-digest"),
        ] {
            connection.execute("INSERT INTO upload_items (id, title, file_name, workspace_path, size_bytes, digest, status, total_bytes, created_at, updated_at) VALUES (?1, ?2, ?2, ?2, 1, ?3, 'draft', 1, ?4, ?4)", params![id, title, digest, now()]).unwrap();
        }

        let candidates = exact_local_duplicates(&connection).unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].confidence, "exact_local");
        assert_eq!(candidates[0].left_title, "First copy");
        assert_eq!(candidates[0].right_title, "Second copy");
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_only_google_client_identifiers() {
        assert!(valid_google_client_id(
            "12345-example.apps.googleusercontent.com"
        ));
        assert!(!valid_google_client_id("client-secret"));
        assert!(!valid_google_client_id("https://accounts.google.com"));
    }

    #[test]
    fn provider_range_uses_the_server_confirmed_next_byte() {
        assert_eq!(confirmed_offset_from_range(None).unwrap(), 0);
        assert_eq!(
            confirmed_offset_from_range(Some("bytes=0-524287")).unwrap(),
            524288
        );
        assert!(confirmed_offset_from_range(Some("bytes=2-4")).is_err());
        assert!(save_upload_session("item", "http://example.test/session").is_err());
    }
}
