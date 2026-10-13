//! Reviewed playlist writes and provider-confirmed, retry-safe bulk receipts.
use super::*;

const CREATE_UNCERTAIN: &str = "YouTube may have created the playlist, but the app could not confirm the result. Refresh playlists and check for it before creating again.";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AddReceipt {
    channel_id: String,
    playlist_id: String,
    requested_video_ids: Vec<String>,
    added_video_ids: Vec<String>,
    skipped_video_ids: Vec<String>,
    remaining_video_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    uncertain: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AddProgress {
    channel_id: String,
    playlist_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    completed: usize,
    total: usize,
    added: usize,
    skipped: usize,
    current_title: String,
    status: String,
    detail: String,
}

struct WriteFailure {
    detail: String,
    uncertain: bool,
}

trait PlaylistProvider {
    fn ensure_scope(&mut self, channel_id: &str) -> Result<(), String>;
    fn authenticated_channel(&mut self) -> Result<String, String>;
    fn playlist_owner(&mut self, playlist_id: &str) -> Result<String, String>;
    fn video_channels(&mut self, video_ids: &[String]) -> Result<HashMap<String, String>, String>;
    fn membership(&mut self, playlist_id: &str) -> Result<HashSet<String>, String>;
    fn insert(&mut self, playlist_id: &str, video_id: &str) -> Result<(), WriteFailure>;
    fn create(&mut self, title: &str, privacy: &str) -> Result<serde_json::Value, WriteFailure>;
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn selected_ids(video_ids: Vec<String>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for id in video_ids {
        if !valid_identity(&id) {
            return Err("Select valid saved YouTube videos to add.".into());
        }
        if seen.insert(id.clone()) {
            result.push(id);
        }
    }
    if result.is_empty() || result.len() > 200 {
        return Err("Select between 1 and 200 saved YouTube videos to add.".into());
    }
    Ok(result)
}

fn progress(
    receipt: &AddReceipt,
    request_id: Option<&str>,
    title: &str,
    status: &str,
    detail: String,
) -> AddProgress {
    AddProgress {
        channel_id: receipt.channel_id.clone(),
        playlist_id: receipt.playlist_id.clone(),
        request_id: request_id.map(str::to_string),
        completed: receipt.added_video_ids.len() + receipt.skipped_video_ids.len(),
        total: receipt.requested_video_ids.len(),
        added: receipt.added_video_ids.len(),
        skipped: receipt.skipped_video_ids.len(),
        current_title: title.into(),
        status: status.into(),
        detail,
    }
}

fn run_bulk<P: PlaylistProvider>(
    provider: &mut P,
    channel_id: &str,
    playlist_id: &str,
    video_ids: Vec<String>,
    titles: &HashMap<String, String>,
    request_id: Option<&str>,
    mut emit: impl FnMut(AddProgress),
) -> AddReceipt {
    let mut receipt = AddReceipt {
        channel_id: channel_id.into(),
        playlist_id: playlist_id.into(),
        requested_video_ids: video_ids.clone(),
        added_video_ids: Vec::new(),
        skipped_video_ids: Vec::new(),
        remaining_video_ids: video_ids,
        error: None,
        uncertain: false,
    };
    emit(progress(
        &receipt,
        request_id,
        "",
        "running",
        "Checking playlist ownership and current membership.".into(),
    ));
    let membership = (|| {
        provider.ensure_scope(channel_id)?;
        if provider.authenticated_channel()? != channel_id {
            return Err("The playlist-management credential is for a different YouTube channel. Grant permission again for the connected channel.".into());
        }
        if provider.playlist_owner(playlist_id)? != channel_id {
            return Err("The selected playlist belongs to a different channel. Refresh playlists and choose one owned by the reviewed channel.".into());
        }
        let video_channels = provider.video_channels(&receipt.requested_video_ids)?;
        if receipt
            .requested_video_ids
            .iter()
            .any(|id| video_channels.get(id).map(String::as_str) != Some(channel_id))
        {
            return Err("One selected video is unavailable or belongs to another channel. Refresh the library and review the selection again.".into());
        }
        provider.membership(playlist_id)
    })();
    let mut membership = match membership {
        Ok(value) => value,
        Err(detail) => {
            receipt.error = Some(detail.clone());
            emit(progress(&receipt, request_id, "", "error", detail));
            return receipt;
        }
    };
    for (index, id) in receipt.requested_video_ids.clone().iter().enumerate() {
        let title = titles.get(id).map(String::as_str).unwrap_or(id);
        if let Err(detail) = provider.ensure_scope(channel_id) {
            receipt.error = Some(detail.clone());
            emit(progress(&receipt, request_id, title, "error", detail));
            return receipt;
        }
        if membership.contains(id) {
            receipt.skipped_video_ids.push(id.clone());
            receipt.remaining_video_ids = receipt.requested_video_ids[index + 1..].to_vec();
            emit(progress(
                &receipt,
                request_id,
                title,
                "running",
                "Already in this playlist; skipped.".into(),
            ));
            continue;
        }
        emit(progress(
            &receipt,
            request_id,
            title,
            "running",
            format!(
                "Adding video {} of {}.",
                index + 1,
                receipt.requested_video_ids.len()
            ),
        ));
        match provider.insert(playlist_id, id) {
            Ok(()) => {
                membership.insert(id.clone());
                receipt.added_video_ids.push(id.clone());
                receipt.remaining_video_ids = receipt.requested_video_ids[index + 1..].to_vec();
                emit(progress(
                    &receipt,
                    request_id,
                    title,
                    "running",
                    "YouTube confirmed this video was added.".into(),
                ));
            }
            Err(failure) => {
                receipt.uncertain = failure.uncertain;
                let detail = if failure.uncertain {
                    format!("{} This video may already have been added. Retry the remaining selection; current membership will be checked before any insertion.", failure.detail)
                } else {
                    failure.detail
                };
                receipt.error = Some(detail.clone());
                emit(progress(&receipt, request_id, title, "error", detail));
                return receipt;
            }
        }
    }
    emit(progress(
        &receipt,
        request_id,
        "",
        "completed",
        format!(
            "Added {} video(s); skipped {} already in the playlist.",
            receipt.added_video_ids.len(),
            receipt.skipped_video_ids.len()
        ),
    ));
    receipt
}

struct NativeProvider<'a> {
    state: &'a AppState,
    token: String,
    operation: PlaylistWrite,
    channel_id: String,
}

#[derive(Clone, Copy)]
enum PlaylistWrite {
    Add,
    Create,
}

impl PlaylistWrite {
    fn permission_guidance(self) -> &'static str {
        match self {
            Self::Add => "Grant video-management permission and enter its temporary 15-minute mode before adding videos to a playlist.",
            Self::Create => "Grant playlist-management permission and enter its temporary 15-minute mode before creating a playlist.",
        }
    }
}

fn check_scope(state: &AppState, channel_id: &str, operation: PlaylistWrite) -> Result<(), String> {
    let connection = database(state)?;
    clear_expired_deletion_authorization(&connection)?;
    let settings = connection_settings(&connection)?;
    if !settings.connected || settings.active_channel_id.as_deref() != Some(channel_id) {
        return Err(
            "The connected YouTube channel changed. Review the playlist and selection again."
                .into(),
        );
    }
    if !settings.deletion_authorized || !settings.deletion_sudo_active {
        return Err(operation.permission_guidance().into());
    }
    Ok(())
}

/// Both playlist write methods require force-ssl. The ordinary upload/read-only
/// credential must never be selected, and permission must precede token access.
fn management_token_with(
    state: &AppState,
    channel_id: &str,
    operation: PlaylistWrite,
    refresh: impl FnOnce(provider_transport::AccessTokenKind) -> Result<String, String>,
) -> Result<String, String> {
    check_scope(state, channel_id, operation)?;
    refresh(provider_transport::AccessTokenKind::Deletion)
}

impl<'a> NativeProvider<'a> {
    fn for_write(
        state: &'a AppState,
        channel_id: &str,
        operation: PlaylistWrite,
    ) -> Result<Self, String> {
        let token = management_token_with(state, channel_id, operation, |kind| {
            refreshed_access_token_for(
                state,
                kind,
                channel_profiles::active_management_entry(&database(state)?),
                "This device no longer has a playlist-management credential; grant video-management permission again.",
            )
        })?;
        Ok(Self {
            state,
            token,
            operation,
            channel_id: channel_id.into(),
        })
    }
}

impl PlaylistProvider for NativeProvider<'_> {
    fn ensure_scope(&mut self, channel_id: &str) -> Result<(), String> {
        check_scope(self.state, channel_id, self.operation)
    }

    fn authenticated_channel(&mut self) -> Result<String, String> {
        active_channel_id(self.state, &self.token)
    }

    fn playlist_owner(&mut self, playlist_id: &str) -> Result<String, String> {
        let response = youtube_json(
            self.state,
            &self.token,
            "playlists",
            &[("part", "snippet"), ("id", playlist_id)],
        )?;
        response
            .get("items")
            .and_then(|items| items.as_array())
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("id").and_then(|id| id.as_str()) == Some(playlist_id))
            })
            .and_then(|item| item.pointer("/snippet/channelId"))
            .and_then(|id| id.as_str())
            .map(str::to_string)
            .ok_or_else(|| {
                "The selected playlist is no longer available. Refresh playlists and review again."
                    .into()
            })
    }

    fn video_channels(&mut self, video_ids: &[String]) -> Result<HashMap<String, String>, String> {
        let mut result = HashMap::new();
        for chunk in video_ids.chunks(50) {
            let ids = chunk.join(",");
            let response = youtube_json(
                self.state,
                &self.token,
                "videos",
                &[("part", "snippet"), ("id", &ids)],
            )?;
            for item in response
                .get("items")
                .and_then(|items| items.as_array())
                .ok_or("YouTube did not confirm the selected video identities.")?
            {
                if let (Some(id), Some(channel)) = (
                    item.get("id").and_then(|id| id.as_str()),
                    item.pointer("/snippet/channelId")
                        .and_then(|id| id.as_str()),
                ) {
                    result.insert(id.into(), channel.into());
                }
            }
        }
        Ok(result)
    }

    fn membership(&mut self, playlist_id: &str) -> Result<HashSet<String>, String> {
        let mut result = HashSet::new();
        let mut page: Option<String> = None;
        let mut seen_pages = HashSet::new();
        loop {
            let mut query = vec![
                ("part", "snippet"),
                ("playlistId", playlist_id),
                ("maxResults", "50"),
            ];
            if let Some(page) = page.as_deref() {
                query.push(("pageToken", page));
            }
            let response = youtube_json(self.state, &self.token, "playlistItems", &query)?;
            for item in response.get("items").and_then(|items| items.as_array()).ok_or("YouTube did not return readable playlist membership. No additional video was sent.")? {
                let id = item.pointer("/snippet/resourceId/videoId").and_then(|id| id.as_str()).filter(|id| !id.is_empty()).ok_or("YouTube returned incomplete playlist membership. Refresh and try again before adding videos.")?;
                result.insert(id.into());
            }
            page = response
                .get("nextPageToken")
                .and_then(|page| page.as_str())
                .map(str::to_string);
            match page.as_ref() {
                None => return Ok(result),
                Some(page) if !seen_pages.insert(page.clone()) => return Err("YouTube repeated a playlist page. Membership could not be confirmed; retry shortly.".into()),
                Some(_) => {}
            }
        }
    }

    fn insert(&mut self, playlist_id: &str, video_id: &str) -> Result<(), WriteFailure> {
        check_scope(self.state, &self.channel_id, self.operation).map_err(|detail| {
            WriteFailure {
                detail,
                uncertain: false,
            }
        })?;
        let quota_scope = database(self.state)
            .and_then(|connection| oauth_replacement::configured_quota_scope(&connection))
            .map_err(|detail| WriteFailure {
                detail,
                uncertain: false,
            })?;
        let client = provider_transport(self.state)
            .control_client()
            .map_err(|detail| WriteFailure {
                detail,
                uncertain: false,
            })?;
        let request = client.post("https://www.googleapis.com/youtube/v3/playlistItems")
            .query(&[("part", "id,snippet")]).bearer_auth(&self.token)
            .json(&serde_json::json!({"snippet": {"playlistId": playlist_id, "resourceId": {"kind": "youtube#video", "videoId": video_id}}}));
        let response =
            observed_youtube_request(self.state, "playlistItems.insert", || request.send())
                .map_err(|_| WriteFailure {
                    detail: "YouTube did not confirm the playlist addition. Check your connection."
                        .into(),
                    uncertain: true,
                })?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let mut body = String::new();
            let _ = response.take(64 * 1024).read_to_string(&mut body);
            if youtube_limits::classify(status, &body)
                == Some(youtube_limits::UploadLimitKind::ApiQuota)
            {
                let _ = oauth_replacement::record_scoped_api_pause(
                    self.state,
                    &self.channel_id,
                    &quota_scope,
                    None,
                    youtube_limits::includes_channel_limit(status, &body),
                );
            }
            let detail = match status {
                401 | 403 => "YouTube denied this playlist addition. Grant video-management permission again for the reviewed channel.",
                429 => "YouTube is rate-limiting playlist additions. Wait a little before retrying the remaining selection.",
                500..=599 => "YouTube is temporarily unavailable and did not confirm this addition.",
                _ => "YouTube rejected this playlist addition. Refresh the playlist and library before retrying the remaining selection.",
            };
            return Err(WriteFailure {
                detail: detail.into(),
                uncertain: status == 408 || status >= 500,
            });
        }
        let body: serde_json::Value = response.json().map_err(|_| WriteFailure {
            detail: "YouTube returned an unreadable playlist addition receipt.".into(),
            uncertain: true,
        })?;
        if body
            .get("id")
            .and_then(|id| id.as_str())
            .is_none_or(|id| id.is_empty())
            || body
                .pointer("/snippet/playlistId")
                .and_then(|id| id.as_str())
                != Some(playlist_id)
            || body
                .pointer("/snippet/resourceId/videoId")
                .and_then(|id| id.as_str())
                != Some(video_id)
        {
            return Err(WriteFailure {
                detail: "YouTube returned an incomplete playlist addition receipt.".into(),
                uncertain: true,
            });
        }
        Ok(())
    }

    fn create(&mut self, title: &str, privacy: &str) -> Result<serde_json::Value, WriteFailure> {
        check_scope(self.state, &self.channel_id, self.operation).map_err(|detail| {
            WriteFailure {
                detail,
                uncertain: false,
            }
        })?;
        let quota_scope = database(self.state)
            .and_then(|connection| oauth_replacement::configured_quota_scope(&connection))
            .map_err(|detail| WriteFailure {
                detail,
                uncertain: false,
            })?;
        let client = provider_transport(self.state)
            .control_client()
            .map_err(|detail| WriteFailure {
                detail,
                uncertain: false,
            })?;
        let request = client.post("https://www.googleapis.com/youtube/v3/playlists")
            .query(&[("part", "id,snippet,status")]).bearer_auth(&self.token)
            .json(&serde_json::json!({"snippet": {"title": title}, "status": {"privacyStatus": privacy}}));
        let response = observed_youtube_request(self.state, "playlists.insert", || request.send())
            .map_err(|_| WriteFailure {
                detail: CREATE_UNCERTAIN.into(),
                uncertain: true,
            })?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let mut body = String::new();
            let _ = response.take(64 * 1024).read_to_string(&mut body);
            if youtube_limits::classify(status, &body)
                == Some(youtube_limits::UploadLimitKind::ApiQuota)
            {
                let _ = oauth_replacement::record_scoped_api_pause(
                    self.state,
                    &self.channel_id,
                    &quota_scope,
                    None,
                    youtube_limits::includes_channel_limit(status, &body),
                );
            }
            return Err(WriteFailure {
                detail: youtube_playlist_creation_http_error(status),
                uncertain: status == 408 || status >= 500,
            });
        }
        response.json().map_err(|_| WriteFailure {
            detail: CREATE_UNCERTAIN.into(),
            uncertain: true,
        })
    }
}

fn scoped_audit(
    state: &AppState,
    channel_id: &str,
    kind: &str,
    detail: &str,
) -> Result<(), String> {
    database(state)?.execute("INSERT INTO audit_events (id, item_id, channel_id, kind, detail, created_at) VALUES (?1, NULL, ?2, ?3, ?4, ?5)", params![Uuid::new_v4().to_string(), channel_id, kind, detail, now()]).map_err(user_error)?;
    Ok(())
}

pub(super) fn add(
    state: &AppState,
    app: &AppHandle,
    playlist_id: String,
    video_ids: Vec<String>,
    expected_channel_id: Option<String>,
    request_id: Option<String>,
) -> Result<AddReceipt, String> {
    if !valid_identity(&playlist_id) {
        return Err("Choose a valid YouTube playlist before adding videos.".into());
    }
    if request_id.as_deref().is_some_and(|id| !valid_identity(id)) {
        return Err("The playlist request identity is invalid. Review the selection again.".into());
    }
    let video_ids = selected_ids(video_ids)?;
    let _guard = deletion_execution_lock(state).lock().map_err(|_| "Another playlist or video-management operation is unavailable. Restart the app and review again.".to_string())?;
    let connection = database(state)?;
    let channel_id = connection_settings(&connection)?
        .active_channel_id
        .ok_or("Connect YouTube before managing playlists.")?;
    if expected_channel_id
        .as_deref()
        .is_some_and(|expected| expected != channel_id)
    {
        return Err(
            "The connected YouTube channel changed. Review the playlist and selection again."
                .into(),
        );
    }
    check_scope(state, &channel_id, PlaylistWrite::Add)?;
    let mut titles = HashMap::new();
    for id in &video_ids {
        let title: String = connection.query_row("SELECT title FROM remote_videos WHERE video_id = ?1 AND channel_id = ?2", params![id, &channel_id], |row| row.get(0)).map_err(|_| "One selected video is no longer in the active channel's saved library. Refresh and review again.".to_string())?;
        titles.insert(id.clone(), title);
    }
    let mut provider = NativeProvider::for_write(state, &channel_id, PlaylistWrite::Add)?;
    let receipt = run_bulk(
        &mut provider,
        &channel_id,
        &playlist_id,
        video_ids,
        &titles,
        request_id.as_deref(),
        |event| {
            let _ = app.emit("playlist-add-progress", event);
        },
    );
    // A failed local audit must not discard known provider outcomes or suggest a full retry.
    let _ = scoped_audit(state, &channel_id, if receipt.error.is_some() { "youtube_playlist_addition_partial" } else { "youtube_playlist_videos_added" }, &format!("Reviewed playlist selection: {} confirmed added, {} already present, {} unresolved; uncertain current result: {}.", receipt.added_video_ids.len(), receipt.skipped_video_ids.len(), receipt.remaining_video_ids.len(), receipt.uncertain));
    Ok(receipt)
}

fn create_reviewed(
    provider: &mut impl PlaylistProvider,
    channel_id: &str,
    title: &str,
    privacy: &str,
) -> Result<YouTubePlaylist, String> {
    provider.ensure_scope(channel_id)?;
    if provider.authenticated_channel()? != channel_id {
        return Err("The playlist credential is for a different YouTube channel. Reconnect the reviewed channel before creating a playlist.".into());
    }
    provider.ensure_scope(channel_id)?;
    let response = provider.create(title, privacy).map_err(|failure| {
        if failure.uncertain {
            CREATE_UNCERTAIN.to_string()
        } else {
            failure.detail
        }
    })?;
    let id = response
        .get("id")
        .and_then(|id| id.as_str())
        .filter(|id| valid_identity(id))
        .ok_or(CREATE_UNCERTAIN)?;
    if response
        .pointer("/snippet/channelId")
        .and_then(|id| id.as_str())
        != Some(channel_id)
        || response
            .pointer("/status/privacyStatus")
            .and_then(|value| value.as_str())
            != Some(privacy)
    {
        return Err("YouTube returned a playlist, but its channel or visibility did not match the review. Refresh playlists and check it before creating again.".into());
    }
    Ok(YouTubePlaylist {
        id: id.into(),
        title: title.into(),
    })
}

pub(super) fn create(
    state: &AppState,
    title: String,
    privacy: String,
    expected_channel_id: Option<String>,
) -> Result<YouTubePlaylist, String> {
    let title = valid_new_playlist_title(&title)?;
    let privacy = valid_playlist_privacy(&privacy)?;
    let settings = connection_settings(&database(state)?)?;
    if !settings.connected {
        return Err("Connect YouTube before creating a playlist.".into());
    }
    let channel_id = settings
        .active_channel_id
        .ok_or("Connect YouTube before creating a playlist.")?;
    if expected_channel_id
        .as_deref()
        .is_some_and(|expected| expected != channel_id)
    {
        return Err(
            "The connected YouTube channel changed. Review playlist creation again.".into(),
        );
    }
    let mut provider = NativeProvider::for_write(state, &channel_id, PlaylistWrite::Create)?;
    let result = create_reviewed(&mut provider, &channel_id, &title, privacy);
    let (kind, detail) = if result.is_ok() {
        (
            "youtube_playlist_created",
            format!("Confirmed a {privacy} YouTube playlist for the reviewed channel."),
        )
    } else {
        ("youtube_playlist_creation_failed", format!("Could not confirm the reviewed {privacy} YouTube playlist. Refresh the playlist list before repeating an uncertain creation."))
    };
    // Preserve a successful provider identity even if recording the local audit fails.
    let _ = scoped_audit(state, &channel_id, kind, &detail);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        channel: String,
        owner: String,
        video_channels: HashMap<String, String>,
        members: HashSet<String>,
        inserted: Vec<String>,
        fail_at: Option<usize>,
        failure_uncertain: bool,
        fail_after_acceptance: bool,
        membership_reads: usize,
        membership_failure: bool,
        scope_checks: usize,
        change_scope_at: Option<usize>,
        created: Vec<(String, String)>,
        create_failure: bool,
        create_response: Option<serde_json::Value>,
    }

    impl MockProvider {
        fn permitted() -> Self {
            Self {
                channel: "channel-a".into(),
                owner: "channel-a".into(),
                video_channels: ["video-c", "video-a", "video-b"]
                    .into_iter()
                    .map(|id| (id.into(), "channel-a".into()))
                    .collect(),
                members: HashSet::new(),
                inserted: Vec::new(),
                fail_at: None,
                failure_uncertain: false,
                fail_after_acceptance: false,
                membership_reads: 0,
                membership_failure: false,
                scope_checks: 0,
                change_scope_at: None,
                created: Vec::new(),
                create_failure: false,
                create_response: None,
            }
        }
    }

    impl PlaylistProvider for MockProvider {
        fn ensure_scope(&mut self, channel_id: &str) -> Result<(), String> {
            self.scope_checks += 1;
            if self.change_scope_at == Some(self.scope_checks) || channel_id != self.channel {
                return Err("The connected YouTube channel changed.".into());
            }
            Ok(())
        }
        fn authenticated_channel(&mut self) -> Result<String, String> {
            Ok(self.channel.clone())
        }
        fn playlist_owner(&mut self, _: &str) -> Result<String, String> {
            Ok(self.owner.clone())
        }
        fn video_channels(&mut self, _: &[String]) -> Result<HashMap<String, String>, String> {
            Ok(self.video_channels.clone())
        }
        fn membership(&mut self, _: &str) -> Result<HashSet<String>, String> {
            self.membership_reads += 1;
            if self.membership_failure {
                return Err("Membership unavailable.".into());
            }
            Ok(self.members.clone())
        }
        fn insert(&mut self, _: &str, video_id: &str) -> Result<(), WriteFailure> {
            if self.fail_at == Some(self.inserted.len()) {
                self.fail_at = None;
                if self.fail_after_acceptance {
                    self.inserted.push(video_id.into());
                    self.members.insert(video_id.into());
                }
                return Err(WriteFailure {
                    detail: "Safe provider failure.".into(),
                    uncertain: self.failure_uncertain,
                });
            }
            self.inserted.push(video_id.into());
            self.members.insert(video_id.into());
            Ok(())
        }
        fn create(
            &mut self,
            title: &str,
            privacy: &str,
        ) -> Result<serde_json::Value, WriteFailure> {
            self.created.push((title.into(), privacy.into()));
            if self.create_failure {
                return Err(WriteFailure {
                    detail: "Safe transport failure.".into(),
                    uncertain: true,
                });
            }
            Ok(self.create_response.clone().unwrap_or_else(|| serde_json::json!({"id": "playlist-a", "snippet": {"channelId": self.channel}, "status": {"privacyStatus": privacy}})))
        }
    }

    fn selection() -> Vec<String> {
        ["video-c", "video-a", "video-b"]
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    fn add_fixture(provider: &mut MockProvider, ids: Vec<String>) -> AddReceipt {
        run_bulk(
            provider,
            "channel-a",
            "playlist-a",
            ids,
            &HashMap::new(),
            Some("request-a"),
            |_| {},
        )
    }

    #[test]
    fn bulk_add_preserves_reviewed_order_and_reports_actual_item_progress() {
        let mut provider = MockProvider::permitted();
        let mut events = Vec::new();
        let titles = HashMap::from([("video-a".into(), "Title A".into())]);
        let receipt = run_bulk(
            &mut provider,
            "channel-a",
            "playlist-a",
            selection(),
            &titles,
            Some("request-a"),
            |event| events.push(event),
        );
        assert_eq!(provider.inserted, selection());
        assert_eq!(receipt.added_video_ids, selection());
        assert!(receipt.remaining_video_ids.is_empty());
        assert!(receipt.error.is_none());
        assert_eq!(
            events
                .iter()
                .map(|event| event.completed)
                .collect::<Vec<_>>(),
            vec![0, 0, 1, 1, 2, 2, 3, 3]
        );
        let event = events
            .iter()
            .find(|event| event.current_title == "Title A")
            .unwrap();
        let payload = serde_json::to_value(event).unwrap();
        assert_eq!(payload["channelId"], "channel-a");
        assert_eq!(payload["playlistId"], "playlist-a");
        assert_eq!(payload["requestId"], "request-a");
        assert_eq!(events.last().unwrap().status, "completed");
    }

    #[test]
    fn bulk_add_skips_fresh_membership_and_deduplicates_selection_in_order() {
        let mut provider = MockProvider::permitted();
        provider.members.insert("video-a".into());
        let ids = selected_ids(vec![
            "video-c".into(),
            "video-a".into(),
            "video-c".into(),
            "video-b".into(),
        ])
        .unwrap();
        assert_eq!(ids, selection());
        let receipt = add_fixture(&mut provider, ids);
        assert_eq!(provider.inserted, vec!["video-c", "video-b"]);
        assert_eq!(receipt.skipped_video_ids, vec!["video-a"]);
        assert_eq!(provider.membership_reads, 1);
        assert!(receipt.remaining_video_ids.is_empty());
    }

    #[test]
    fn bulk_add_partial_failure_keeps_only_unresolved_ids_for_retry() {
        let mut provider = MockProvider::permitted();
        provider.fail_at = Some(1);
        let receipt = add_fixture(&mut provider, selection());
        assert_eq!(receipt.added_video_ids, vec!["video-c"]);
        assert_eq!(receipt.remaining_video_ids, vec!["video-a", "video-b"]);
        assert_eq!(receipt.error.as_deref(), Some("Safe provider failure."));
        assert!(!receipt.uncertain);
        let retry = add_fixture(&mut provider, receipt.remaining_video_ids);
        assert_eq!(retry.added_video_ids, vec!["video-a", "video-b"]);
        assert_eq!(provider.inserted, selection());
        assert_eq!(provider.membership_reads, 2);
    }

    #[test]
    fn bulk_add_uncertain_accepted_result_is_reconciled_without_duplicate_insert() {
        let mut provider = MockProvider::permitted();
        provider.fail_at = Some(1);
        provider.failure_uncertain = true;
        provider.fail_after_acceptance = true;
        let receipt = add_fixture(&mut provider, selection());
        assert_eq!(receipt.added_video_ids, vec!["video-c"]);
        assert_eq!(receipt.remaining_video_ids, vec!["video-a", "video-b"]);
        assert!(receipt.uncertain);
        assert!(receipt
            .error
            .unwrap()
            .contains("may already have been added"));
        let retry = add_fixture(&mut provider, receipt.remaining_video_ids);
        assert_eq!(retry.skipped_video_ids, vec!["video-a"]);
        assert_eq!(retry.added_video_ids, vec!["video-b"]);
        assert_eq!(provider.inserted, selection());
        assert_eq!(provider.membership_reads, 2);
    }

    #[test]
    fn bulk_add_rechecks_channel_after_reads_and_before_every_write() {
        for (check, expected_added) in [(2, 0), (3, 1)] {
            let mut provider = MockProvider::permitted();
            provider.change_scope_at = Some(check);
            let receipt = add_fixture(&mut provider, selection());
            assert_eq!(provider.inserted.len(), expected_added);
            assert_eq!(receipt.added_video_ids.len(), expected_added);
            assert_eq!(receipt.remaining_video_ids, selection()[expected_added..]);
            assert!(receipt.error.unwrap().contains("channel changed"));
        }
    }

    #[test]
    fn bulk_add_rejects_other_owner_and_wrong_or_unavailable_selected_videos() {
        let mut wrong_owner = MockProvider::permitted();
        wrong_owner.owner = "channel-b".into();
        assert!(add_fixture(&mut wrong_owner, selection()).error.is_some());
        assert!(wrong_owner.inserted.is_empty());
        for channel in [Some("channel-b"), None] {
            let mut provider = MockProvider::permitted();
            if let Some(channel) = channel {
                provider
                    .video_channels
                    .insert("video-a".into(), channel.into());
            } else {
                provider.video_channels.remove("video-a");
            }
            let receipt = add_fixture(&mut provider, selection());
            assert!(receipt.error.is_some());
            assert_eq!(receipt.remaining_video_ids, selection());
            assert!(provider.inserted.is_empty());
            assert_eq!(provider.membership_reads, 0);
        }
    }

    #[test]
    fn bulk_add_membership_failure_sends_no_writes_and_retains_selection() {
        let mut provider = MockProvider::permitted();
        provider.membership_failure = true;
        let receipt = add_fixture(&mut provider, selection());
        assert!(provider.inserted.is_empty());
        assert_eq!(receipt.remaining_video_ids, selection());
        assert_eq!(receipt.error.as_deref(), Some("Membership unavailable."));
    }

    #[test]
    fn bulk_add_selection_limit_is_200_unique_valid_ids() {
        assert!(selected_ids(Vec::new()).is_err());
        assert!(selected_ids(vec![" ".into()]).is_err());
        assert!(selected_ids(vec!["unsafe/id".into()]).is_err());
        assert_eq!(
            selected_ids((0..200).map(|n| format!("video-{n}")).collect())
                .unwrap()
                .len(),
            200
        );
        assert!(selected_ids((0..201).map(|n| format!("video-{n}")).collect()).is_err());
    }

    #[test]
    fn playlist_creation_confirms_each_reviewed_privacy_and_channel() {
        for privacy in ["private", "unlisted", "public"] {
            let mut provider = MockProvider::permitted();
            let playlist =
                create_reviewed(&mut provider, "channel-a", "Reviewed title", privacy).unwrap();
            assert_eq!(playlist.id, "playlist-a");
            assert_eq!(playlist.title, "Reviewed title");
            assert_eq!(
                provider.created,
                vec![("Reviewed title".into(), privacy.into())]
            );
        }
    }

    #[test]
    fn playlist_creation_scope_change_prevents_provider_write() {
        let mut provider = MockProvider::permitted();
        provider.change_scope_at = Some(2);
        assert!(
            create_reviewed(&mut provider, "channel-a", "Reviewed title", "private")
                .err()
                .unwrap()
                .contains("channel changed")
        );
        assert!(provider.created.is_empty());
    }

    #[test]
    fn playlist_creation_uncertain_result_requires_refresh_before_repeating() {
        let mut provider = MockProvider::permitted();
        provider.create_failure = true;
        assert!(
            create_reviewed(&mut provider, "channel-a", "Reviewed title", "public")
                .err()
                .unwrap()
                .contains("Refresh playlists")
        );
        assert_eq!(provider.created.len(), 1);
        let mut provider = MockProvider::permitted();
        provider.create_response =
            Some(serde_json::json!({"status": {"privacyStatus": "private"}}));
        assert!(
            create_reviewed(&mut provider, "channel-a", "Reviewed title", "private")
                .err()
                .unwrap()
                .contains("before creating again")
        );
        let mut provider = MockProvider::permitted();
        provider.create_response = Some(
            serde_json::json!({"id": "playlist-a", "snippet": {"channelId": "channel-b"}, "status": {"privacyStatus": "public"}}),
        );
        assert!(
            create_reviewed(&mut provider, "channel-a", "Reviewed title", "private")
                .err()
                .unwrap()
                .contains("did not match")
        );
    }

    fn fixture_state(root: &Path) -> AppState {
        AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[test]
    fn native_playlist_scope_enforces_management_expiry_and_expected_channel_before_credentials() {
        let root = std::env::temp_dir().join(format!("playlist-bulk-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = fixture_state(&root);
        let connection = database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton, active_channel, active_channel_id, deletion_authorized, deletion_sudo_until, updated_at) VALUES(1, 'Channel A', 'channel-a', 1, ?1, ?2)", params![deletion_sudo_until(), now()]).unwrap();
        assert!(check_scope(&state, "channel-a", PlaylistWrite::Add).is_ok());
        assert!(check_scope(&state, "channel-b", PlaylistWrite::Add).is_err());
        connection
            .execute(
                "UPDATE connection_settings SET deletion_sudo_until = ?1",
                [(Utc::now() - ChronoDuration::minutes(1)).to_rfc3339()],
            )
            .unwrap();
        assert!(check_scope(&state, "channel-a", PlaylistWrite::Add)
            .unwrap_err()
            .contains("15-minute"));
        assert!(check_scope(&state, "channel-a", PlaylistWrite::Create)
            .unwrap_err()
            .contains("before creating a playlist"));
        assert!(create(
            &state,
            "Uploads".into(),
            "private".into(),
            Some("channel-b".into())
        )
        .err()
        .unwrap()
        .contains("channel changed"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_playlist_creation_without_management_permission_never_accesses_a_provider_token() {
        let root = std::env::temp_dir().join(format!("playlist-permission-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = fixture_state(&root);
        let connection = database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton, active_channel, active_channel_id, deletion_authorized, updated_at) VALUES(1, 'Channel A', 'channel-a', 0, ?1)", [now()]).unwrap();
        for authorized in [0, 1] {
            connection.execute("UPDATE connection_settings SET deletion_authorized = ?1, deletion_sudo_until = NULL", [authorized]).unwrap();
            let mut refresh_called = false;
            let result = management_token_with(&state, "channel-a", PlaylistWrite::Create, |_| {
                refresh_called = true;
                Err("Provider token access must not happen.".into())
            });
            assert_eq!(
                result.err().unwrap(),
                PlaylistWrite::Create.permission_guidance()
            );
            assert!(!refresh_called);
            assert_eq!(
                create(
                    &state,
                    "Reviewed title".into(),
                    "private".into(),
                    Some("channel-a".into())
                )
                .err()
                .unwrap(),
                PlaylistWrite::Create.permission_guidance()
            );
        }
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_playlist_writes_use_only_the_protected_management_token_kind() {
        let root = std::env::temp_dir().join(format!("playlist-token-kind-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = fixture_state(&root);
        let connection = database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton, active_channel, active_channel_id, deletion_authorized, deletion_sudo_until, updated_at) VALUES(1, 'Channel A', 'channel-a', 1, ?1, ?2)", params![deletion_sudo_until(), now()]).unwrap();
        assert!(!UPLOAD_OAUTH_SCOPES.contains("youtube.force-ssl"));
        assert!(DELETION_OAUTH_SCOPES.contains("youtube.force-ssl"));
        for operation in [PlaylistWrite::Create, PlaylistWrite::Add] {
            let mut refresh_called = false;
            let token = management_token_with(&state, "channel-a", operation, |kind| {
                refresh_called = true;
                assert!(matches!(
                    kind,
                    provider_transport::AccessTokenKind::Deletion
                ));
                Ok("mock-management-token".into())
            })
            .unwrap();
            assert!(refresh_called);
            assert_eq!(token, "mock-management-token");
        }
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_playlist_audit_is_immutable_channel_scoped_and_truthful_about_privacy() {
        let root = std::env::temp_dir().join(format!("playlist-audit-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let state = fixture_state(&root);
        let connection = database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton, active_channel, active_channel_id, updated_at) VALUES(1, 'Channel B', 'channel-b', ?1)", [now()]).unwrap();
        scoped_audit(
            &state,
            "channel-a",
            "youtube_playlist_created",
            "Confirmed a public YouTube playlist for the reviewed channel.",
        )
        .unwrap();
        let event: (String, String) = connection.query_row("SELECT channel_id, detail FROM audit_events WHERE kind = 'youtube_playlist_created'", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(event.0, "channel-a");
        assert!(event.1.contains("public"));
        assert!(!event.1.contains("private"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
}
