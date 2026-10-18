//! A reviewed replacement stages protected credentials and promotes a pointer
//! only after Google proves that the immutable connected channel is unchanged.
use super::*;
use std::sync::MutexGuard;

const REVIEW_LIFETIME: Duration = Duration::from_secs(20 * 60);
const MAX_CLIENT_FILE_BYTES: u64 = 64 * 1024;

pub(super) struct ParsedClient {
    pub(super) client_id: String,
    pub(super) client_secret: String,
    pub(super) project_id: Option<String>,
}

pub(super) fn read_desktop_client(path: &Path) -> Result<ParsedClient, String> {
    let file = File::open(path)
        .map_err(|_| "The selected OAuth JSON file could not be read.".to_string())?;
    let metadata = file
        .metadata()
        .map_err(|_| "The selected OAuth JSON file could not be read.".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_CLIENT_FILE_BYTES {
        return Err("Choose a small downloaded Google Desktop OAuth JSON file.".into());
    }
    let mut contents = String::new();
    file.take(MAX_CLIENT_FILE_BYTES + 1)
        .read_to_string(&mut contents)
        .map_err(|_| "The selected OAuth JSON file is not readable text.".to_string())?;
    if contents.len() as u64 > MAX_CLIENT_FILE_BYTES {
        return Err("The OAuth JSON changed or is too large. Choose the downloaded Desktop client JSON again.".into());
    }
    parse_desktop_client(&contents)
}

fn parse_desktop_client(contents: &str) -> Result<ParsedClient, String> {
    let (client_id, client_secret) = desktop_oauth_client_from_file(contents)?;
    let parsed: DesktopOAuthClientFile = serde_json::from_str(contents)
        .map_err(|_| "Choose the downloaded JSON for a Google Desktop OAuth client.".to_string())?;
    let project_id = parsed
        .installed
        .project_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty());
    if project_id.as_ref().is_some_and(|id| {
        id.len() > 128
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == ':')
    }) {
        return Err("The Desktop OAuth JSON contains an invalid Cloud project identity.".into());
    }
    Ok(ParsedClient {
        client_id,
        client_secret,
        project_id,
    })
}

pub(super) fn bundle_entry(id: &str, purpose: &str) -> Result<CredentialEntry, String> {
    if Uuid::parse_str(id).is_err() || !matches!(purpose, "secret" | "refresh") {
        return Err("The protected credential reference is invalid.".into());
    }
    CredentialEntry::new(
        SECURE_STORE_SERVICE,
        &format!("youtube-oauth-bundle-{id}-{purpose}"),
    )
    .map_err(user_error)
}

pub(super) fn lock_credentials() -> Result<MutexGuard<'static, ()>, String> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().map_err(|_| {
        "The protected account connection is unavailable. Restart the app.".to_string()
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Ready,
    Authorizing,
    Completed,
    Failed,
    Cancelled,
}

impl Phase {
    fn holds_connection(self) -> bool {
        matches!(self, Self::Ready | Self::Authorizing | Self::Failed)
    }
}

#[derive(Clone)]
struct Candidate {
    id: String,
    channel_id: String,
    channel_name: String,
    client_id: String,
    project_id: Option<String>,
    has_secret: bool,
    original_client_id: String,
    original_bundle_id: Option<String>,
    original_project_id: Option<String>,
    phase: Phase,
    attempt_id: Option<String>,
    detail: Option<String>,
    expires_at: Instant,
}

type Reviews = HashMap<(PathBuf, String), Candidate>;
fn reviews() -> &'static Mutex<Reviews> {
    static REVIEWS: OnceLock<Mutex<Reviews>> = OnceLock::new();
    REVIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn erase_candidate_credentials(candidate: &Candidate) {
    for purpose in ["secret", "refresh"] {
        let _ = clear_refresh_token(bundle_entry(&candidate.id, purpose));
    }
    if let Some(attempt) = candidate.attempt_id.as_deref() {
        let _ = clear_refresh_token(oauth_verifier_entry(attempt));
    }
}

// Called under the credential guard. Expiry drops the dispatch hold and fences
// callbacks before removing staged credentials. No worker rotates credentials.
fn expire_reviews(state: &AppState) -> Result<(), String> {
    expire_reviews_with(state, erase_candidate_credentials)
}

fn expire_reviews_with(state: &AppState, mut erase: impl FnMut(&Candidate)) -> Result<(), String> {
    let mut reviews = reviews().lock().map_err(user_error)?;
    for ((database_path, _), candidate) in reviews.iter_mut() {
        if database_path == &state.database_path
            && candidate.phase.holds_connection()
            && Instant::now() >= candidate.expires_at
        {
            candidate.phase = Phase::Cancelled;
            candidate.detail = Some("The replacement review expired. Your existing connection and saved queue are unchanged.".into());
            if let Some(attempt) = candidate.attempt_id.as_ref() {
                state
                    .oauth_attempts
                    .lock()
                    .map_err(user_error)?
                    .remove(attempt);
            }
            erase(candidate);
            candidate.expires_at = Instant::now() + REVIEW_LIFETIME;
        }
    }
    reviews.retain(|_, candidate| {
        candidate.phase.holds_connection() || Instant::now() < candidate.expires_at
    });
    Ok(())
}

pub(super) fn ensure_no_pending(state: &AppState) -> Result<(), String> {
    channel_profiles::ensure_no_pending(state)?;
    ensure_no_replacement_pending(state)
}

/// Account-profile authorization owns its own staged review and therefore uses
/// this narrower gate. External credential/dispatch entry points use the full
/// guard above so no reviewed account mutation can be bypassed.
pub(super) fn ensure_no_replacement_pending(state: &AppState) -> Result<(), String> {
    expire_reviews(state)?;
    if reviews()
        .lock()
        .map_err(user_error)?
        .iter()
        .any(|((path, _), review)| path == &state.database_path && review.phase.holds_connection())
    {
        Err("Finish or cancel Replace OAuth JSON before using the saved account connection. Your queue stays saved.".into())
    } else {
        Ok(())
    }
}

fn ensure_review_binding(
    connection: &Connection,
    candidate: &Candidate,
    expected: &str,
) -> Result<(), String> {
    let settings = connection_settings(connection)?;
    if expected != candidate.channel_id
        || settings.active_channel_id.as_deref() != Some(candidate.channel_id.as_str())
        || configured_oauth_client_id(connection)? != candidate.original_client_id
        || active_credential_bundle(connection)? != candidate.original_bundle_id
        || configured_project_id(connection)? != candidate.original_project_id
    {
        return Err("The connected account changed. Cancel this replacement and review the current channel again.".into());
    }
    Ok(())
}

fn require_no_active_uploads(connection: &Connection) -> Result<(), String> {
    let active: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE status IN ('dispatching', 'uploading'))", [], |row| row.get(0)).map_err(user_error)?;
    if active {
        Err("Wait for active uploads to finish or cancel them before replacing the OAuth JSON. Saved queue and upload checkpoints stay intact.".into())
    } else {
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Review {
    review_id: String,
    channel_id: String,
    channel_name: String,
    cloud_quota_paused: bool,
    channel_upload_paused: bool,
}

pub(super) fn prepare(
    state: &AppState,
    path: &Path,
    expected_channel_id: &str,
) -> Result<Review, String> {
    let _dispatch_guard = upload_scheduler(state).lock_dispatch()?;
    let _management_guard = deletion_execution_lock(state).try_lock().map_err(|_| "Wait for the current video or playlist operation to finish before replacing the OAuth JSON.".to_string())?;
    let _credential_guard = lock_credentials()?;
    channel_profiles::ensure_no_pending(state)?;
    ensure_no_pending(state)?;
    if !state.oauth_attempts.lock().map_err(user_error)?.is_empty() {
        return Err(
            "Finish or cancel the current Google authorization before replacing the JSON.".into(),
        );
    }
    if !secure_store_available() {
        return Err("This device's secure credential store is unavailable; the OAuth JSON cannot be staged safely.".into());
    }
    let connection = database(state)?;
    let settings = connection_settings(&connection)?;
    if settings.active_channel_id.as_deref() != Some(expected_channel_id)
        || expected_channel_id.is_empty()
    {
        return Err(
            "The connected YouTube channel changed. Review the account before replacing the JSON."
                .into(),
        );
    }
    require_no_active_uploads(&connection)?;
    channel_profiles::require_verified_channel(&connection, expected_channel_id)?;
    let client = read_desktop_client(path)?;
    let candidate = Candidate {
        id: Uuid::new_v4().to_string(),
        channel_id: expected_channel_id.to_string(),
        channel_name: settings
            .active_channel
            .unwrap_or_else(|| "Connected channel".into()),
        client_id: client.client_id,
        project_id: client.project_id,
        has_secret: !client.client_secret.is_empty(),
        original_client_id: configured_oauth_client_id(&connection)?,
        original_bundle_id: active_credential_bundle(&connection)?,
        original_project_id: configured_project_id(&connection)?,
        phase: Phase::Ready,
        attempt_id: None,
        detail: None,
        expires_at: Instant::now() + REVIEW_LIFETIME,
    };
    let limits = limit_status(&connection, Some(expected_channel_id))?;
    let review = Review {
        review_id: candidate.id.clone(),
        channel_id: candidate.channel_id.clone(),
        channel_name: candidate.channel_name.clone(),
        cloud_quota_paused: limits.kind == "api_quota",
        channel_upload_paused: limits.channel_resumes_at.is_some(),
    };
    if candidate.has_secret {
        bundle_entry(&candidate.id, "secret")?
            .set_password(&client.client_secret)
            .map_err(user_error)?;
    }
    reviews().lock().map_err(user_error)?.insert(
        (state.database_path.clone(), candidate.id.clone()),
        candidate,
    );
    Ok(review)
}

fn candidate_for(state: &AppState, review_id: &str) -> Result<Candidate, String> {
    reviews()
        .lock()
        .map_err(user_error)?
        .get(&(state.database_path.clone(), review_id.to_string()))
        .cloned()
        .ok_or_else(|| {
            "This replacement review is unavailable. Choose the OAuth JSON again.".to_string()
        })
}

pub(super) fn begin(
    state: &AppState,
    review_id: &str,
    expected_channel_id: &str,
) -> Result<OAuthStart, String> {
    let _dispatch_guard = upload_scheduler(state).lock_dispatch()?;
    let _management_guard = deletion_execution_lock(state).try_lock().map_err(|_| {
        "Wait for the current video or playlist operation to finish before reconnecting."
            .to_string()
    })?;
    let _credential_guard = lock_credentials()?;
    channel_profiles::ensure_no_pending(state)?;
    expire_reviews(state)?;
    let mut candidate = candidate_for(state, review_id)?;
    if !matches!(candidate.phase, Phase::Ready | Phase::Failed) {
        return Err("This replacement is already pending or finished. Check its status.".into());
    }
    if !state.oauth_attempts.lock().map_err(user_error)?.is_empty() {
        return Err("Finish or cancel the pending Google authorization first.".into());
    }
    let connection = database(state)?;
    ensure_review_binding(&connection, &candidate, expected_channel_id)?;
    channel_profiles::require_verified_channel(&connection, expected_channel_id)?;
    require_no_active_uploads(&connection)?;
    // Check protected staging before exposing a consent link.
    if candidate.has_secret {
        let _ = bundle_entry(&candidate.id, "secret")?
            .get_password()
            .map_err(|_| {
                "The staged OAuth JSON is unavailable. Cancel and choose the file again."
                    .to_string()
            })?;
    }
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "A secure local OAuth callback port could not be opened.".to_string())?;
    let redirect_uri = format!(
        "http://127.0.0.1:{}/oauth2/callback",
        listener.local_addr().map_err(user_error)?.port()
    );
    let attempt_id = Uuid::new_v4().simple().to_string();
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let authorization_url = url::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", candidate.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", UPLOAD_OAUTH_SCOPES),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", attempt_id.as_str()),
        ],
    )
    .map_err(user_error)?
    .to_string();
    oauth_verifier_entry(&attempt_id)?
        .set_password(&verifier)
        .map_err(user_error)?;
    candidate.phase = Phase::Authorizing;
    candidate.attempt_id = Some(attempt_id.clone());
    candidate.detail = Some("Waiting for Google. Choose the same connected YouTube channel; the replacement becomes active only after verification.".into());
    candidate.expires_at = Instant::now() + REVIEW_LIFETIME;
    reviews().lock().map_err(user_error)?.insert(
        (state.database_path.clone(), candidate.id.clone()),
        candidate.clone(),
    );
    state
        .oauth_attempts
        .lock()
        .map_err(user_error)?
        .insert(attempt_id.clone(), OAuthAttemptKind::Replacement);
    let callback_state = state.clone();
    let callback_attempt = attempt_id.clone();
    spawn_worker(move || {
        await_replacement_callback(
            callback_state,
            listener,
            candidate,
            redirect_uri,
            callback_attempt,
        )
    });
    Ok(OAuthStart {
        authorization_url,
        attempt_id,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Status {
    state: Phase,
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settings: Option<ConnectionSettings>,
}

pub(super) fn status(state: &AppState, review_id: &str) -> Result<Status, String> {
    let _credential_guard = lock_credentials()?;
    expire_reviews(state)?;
    let candidate = candidate_for(state, review_id)?;
    Ok(Status {
        state: candidate.phase,
        detail: candidate.detail,
        settings: if candidate.phase == Phase::Completed {
            Some(connection_settings(&database(state)?)?)
        } else {
            None
        },
    })
}

pub(super) fn cancel(state: &AppState, review_id: &str) -> Result<ConnectionSettings, String> {
    let _dispatch_guard = upload_scheduler(state).lock_dispatch()?;
    let _management_guard = deletion_execution_lock(state).try_lock().map_err(|_| "Wait for the current video or playlist operation to finish before closing the replacement.".to_string())?;
    let _credential_guard = lock_credentials()?;
    let mut candidate = candidate_for(state, review_id)?;
    if candidate.phase != Phase::Completed {
        candidate.phase = Phase::Cancelled;
        candidate.expires_at = Instant::now() + REVIEW_LIFETIME;
        candidate.detail = Some("Replacement cancelled. The existing OAuth JSON, connected channel, queue and checkpoints are unchanged.".into());
        reviews().lock().map_err(user_error)?.insert(
            (state.database_path.clone(), candidate.id.clone()),
            candidate.clone(),
        );
        if let Some(attempt) = candidate.attempt_id.as_deref() {
            state
                .oauth_attempts
                .lock()
                .map_err(user_error)?
                .remove(attempt);
        }
        erase_candidate_credentials(&candidate);
    }
    let settings = connection_settings(&database(state)?)?;
    // Dispatch is restarted outside all mutation/claim locks by the command.
    Ok(settings)
}

fn callback_live(state: &AppState, candidate: &Candidate, attempt_id: &str) -> bool {
    candidate_for(state, &candidate.id).is_ok_and(|review| {
        review.phase == Phase::Authorizing
            && review.attempt_id.as_deref() == Some(attempt_id)
            && Instant::now() < review.expires_at
    })
}

fn finish_failed(state: &AppState, candidate: &Candidate, attempt_id: &str, detail: &str) {
    let Ok(_guard) = lock_credentials() else {
        return;
    };
    if !callback_live(state, candidate, attempt_id) {
        return;
    }
    if let Ok(mut registry) = reviews().lock() {
        if let Some(review) = registry.get_mut(&(state.database_path.clone(), candidate.id.clone()))
        {
            review.phase = Phase::Failed;
            review.detail = Some(format!(
                "{detail} Your existing connection and saved queue are unchanged."
            ));
        }
    }
    let _ = state
        .oauth_attempts
        .lock()
        .map(|mut attempts| attempts.remove(attempt_id));
    let _ = clear_refresh_token(oauth_verifier_entry(attempt_id));
    let _ = clear_refresh_token(bundle_entry(&candidate.id, "refresh"));
}

fn await_replacement_callback(
    state: AppState,
    listener: TcpListener,
    candidate: Candidate,
    redirect_uri: String,
    attempt_id: String,
) {
    let deadline = Instant::now() + Duration::from_secs(600);
    let _ = listener.set_nonblocking(true);
    while Instant::now() < deadline {
        if !callback_live(&state, &candidate, &attempt_id) {
            return;
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut request = [0_u8; 8192];
                let bytes = stream.read(&mut request).unwrap_or(0);
                let request_text = String::from_utf8_lossy(&request[..bytes]);
                let mut parts = request_text.lines().next().unwrap_or("").split_whitespace();
                let method = parts.next();
                let target = parts.next().unwrap_or("/");
                if !valid_oauth_callback_request(method, target, &attempt_id) {
                    respond_to_callback(&mut stream, "Connection did not complete.");
                    continue;
                }
                let result = if let Some(error) = callback_value(target, "error") {
                    Err(oauth_error_message(&error))
                } else if let Some(code) = callback_value(target, "code") {
                    exchange_and_promote(&state, &candidate, &redirect_uri, &attempt_id, &code)
                } else {
                    Err("Google did not return an authorization code.".into())
                };
                match result {
                    Ok(()) => {
                        respond_to_callback(
                            &mut stream,
                            "OAuth JSON replaced for the same YouTube channel.",
                        );
                        kick_automatic_upload_dispatch(state.clone());
                        let _ = ensure_upload_postprocess_worker(state.clone());
                    }
                    Err(detail) => {
                        finish_failed(&state, &candidate, &attempt_id, &detail);
                        respond_to_callback(
                            &mut stream,
                            "Replacement did not complete. Return to the app for details.",
                        );
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
    finish_failed(
        &state,
        &candidate,
        &attempt_id,
        "Google authorization timed out. Retry when ready.",
    );
}

fn exchange_and_promote(
    state: &AppState,
    candidate: &Candidate,
    redirect_uri: &str,
    attempt_id: &str,
    code: &str,
) -> Result<(), String> {
    {
        let _credential_guard = lock_credentials()?;
        channel_profiles::ensure_no_pending(state)?;
    }
    if !callback_live(state, candidate, attempt_id) {
        return Err("This replacement was cancelled or expired.".into());
    }
    let secret = if candidate.has_secret {
        bundle_entry(&candidate.id, "secret")?
            .get_password()
            .map_err(|_| "The staged OAuth JSON is unavailable.".to_string())?
    } else {
        String::new()
    };
    let verifier = oauth_verifier_entry(attempt_id)?
        .get_password()
        .map_err(|_| "The pending Google authorization is unavailable.".to_string())?;
    let mut form = vec![
        ("code", code),
        ("client_id", candidate.client_id.as_str()),
        ("code_verifier", verifier.as_str()),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    if !secret.is_empty() {
        form.push(("client_secret", secret.as_str()));
    }
    let response = provider_transport(state)
        .control_client()?
        .post("https://oauth2.googleapis.com/token")
        .form(&form)
        .send()
        .map_err(|_| "Google token exchange could not be reached.".to_string())?;
    let success = response.status().is_success();
    let tokens: serde_json::Value = response
        .json()
        .map_err(|_| "Google returned an unreadable authorization response.".to_string())?;
    if !success {
        return Err(oauth_token_error_message(&tokens, false));
    }
    let access_token = tokens
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("Google did not return an access token.")?;
    let refresh_token = tokens
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("Google did not return a refresh token. Retry the consent step.")?;
    let response = provider_transport(state)
        .control_client()?
        .get("https://www.googleapis.com/youtube/v3/channels")
        .query(&[("part", "id,snippet"), ("mine", "true")])
        .bearer_auth(access_token)
        .send()
        .map_err(|_| "YouTube channel verification could not be reached.".to_string())?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body = response.text().unwrap_or_default();
        if youtube_limits::classify(status, &body)
            == Some(youtube_limits::UploadLimitKind::ApiQuota)
        {
            return Err("Google Cloud API quota still blocks the candidate JSON. Check that project's quota before retrying.".into());
        }
        return Err(youtube_inventory_http_error(status));
    }
    let channels: serde_json::Value = response
        .json()
        .map_err(|_| "YouTube returned an unreadable channel response.".to_string())?;
    let items = channels
        .get("items")
        .and_then(serde_json::Value::as_array)
        .ok_or("YouTube did not return a channel identity.")?;
    if items
        .first()
        .and_then(|item| item.get("id"))
        .and_then(serde_json::Value::as_str)
        != Some(candidate.channel_id.as_str())
    {
        return Err("Google authorized a different YouTube channel. Choose the currently connected channel when retrying.".into());
    }
    let _dispatch_guard = upload_scheduler(state).lock_dispatch()?;
    let _management_guard = deletion_execution_lock(state).try_lock().map_err(|_| {
        "A video-management operation started. Retry after it finishes.".to_string()
    })?;
    let _credential_guard = lock_credentials()?;
    channel_profiles::ensure_no_pending(state)?;
    if !callback_live(state, candidate, attempt_id) {
        return Err("This replacement was cancelled or expired.".into());
    }
    let mut connection = database(state)?;
    ensure_review_binding(&connection, candidate, &candidate.channel_id)?;
    require_no_active_uploads(&connection)?;
    persist_refresh_token(refresh_token, bundle_entry(&candidate.id, "refresh"))?;
    let different_project = candidate_different_project(candidate);
    let detail = if different_project {
        "OAuth JSON replaced after same-channel verification. Google Cloud quota is checked with the new project; any channel upload pause still applies."
    } else {
        "OAuth JSON replaced after same-channel verification. The saved API quota pause remains because a different Cloud project could not be established; channel upload limits still apply."
    };
    // Acquire the status slot before the commit. Once the pointer is promoted,
    // cleanup failures must never reclassify success or delete active credentials.
    let mut registry = reviews().lock().map_err(user_error)?;
    let review = registry
        .get_mut(&(state.database_path.clone(), candidate.id.clone()))
        .ok_or("The replacement review is unavailable.")?;
    promote(&mut connection, candidate, detail)?;
    review.phase = Phase::Completed;
    review.expires_at = Instant::now() + REVIEW_LIFETIME;
    review.detail = Some(detail.into());
    drop(registry);
    provider_transport(state).invalidate_access_token(provider_transport::AccessTokenKind::Upload);
    provider_transport(state)
        .invalidate_access_token(provider_transport::AccessTokenKind::Deletion);
    provider_transport(state).cache_access_token(
        provider_transport::AccessTokenKind::Upload,
        access_token,
        Duration::from_secs(
            tokens
                .get("expires_in")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(45 * 60),
        ),
    );
    let _ = clear_refresh_token(deletion_refresh_token_entry());
    if let Some(old) = candidate.original_bundle_id.as_deref() {
        let _ = clear_refresh_token(bundle_entry(old, "refresh"));
        let _ = clear_refresh_token(bundle_entry(old, "secret"));
        let _ = clear_refresh_token(channel_profiles::credential_entry(old, "management"));
    } else {
        let _ = clear_refresh_token(refresh_token_entry());
        let _ = clear_oauth_client_secret();
    }
    let _ = clear_refresh_token(oauth_verifier_entry(attempt_id));
    let _ = state
        .oauth_attempts
        .lock()
        .map(|mut attempts| attempts.remove(attempt_id));
    startup_gate(state).quota_worker_wake.notify_all();
    Ok(())
}

fn different_project(old: Option<&str>, new: Option<&str>) -> bool {
    matches!((old, new), (Some(old), Some(new)) if !old.is_empty() && !new.is_empty() && old != new)
}

fn candidate_different_project(candidate: &Candidate) -> bool {
    candidate.original_client_id != candidate.client_id
        && different_project(
            candidate.original_project_id.as_deref(),
            candidate.project_id.as_deref(),
        )
}

fn promote(connection: &mut Connection, candidate: &Candidate, detail: &str) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    ensure_review_binding(&transaction, candidate, &candidate.channel_id)?;
    channel_profiles::require_verified_channel(&transaction, &candidate.channel_id)?;
    require_no_active_uploads(&transaction)?;
    let old_scope = quota_scope(
        &candidate.original_client_id,
        candidate.original_project_id.as_deref(),
    );
    let new_scope = quota_scope(&candidate.client_id, candidate.project_id.as_deref());
    if !candidate_different_project(candidate) && old_scope != new_scope {
        // Unknown/same-project metadata must not erase a saved quota hold simply
        // because another client ID was selected. Carry its deadline forward.
        transaction.execute("INSERT INTO api_quota_pauses(quota_scope, channel_id, pause_until, updated_at) SELECT ?1, channel_id, pause_until, ?2 FROM api_quota_pauses WHERE quota_scope = ?3 ON CONFLICT(quota_scope) DO UPDATE SET pause_until = MAX(pause_until, excluded.pause_until), updated_at = excluded.updated_at", params![new_scope, now(), old_scope]).map_err(user_error)?;
    }
    // Previous-project pauses remain recorded: switching back does not forget
    // their quota. Only the new active project may be admitted after verification.
    transaction.execute("UPDATE connection_settings SET oauth_client_id = ?1, oauth_credential_id = ?2, oauth_project_id = ?3, deletion_authorized = 0, deletion_sudo_until = NULL, connection_detail = ?4, updated_at = ?5 WHERE singleton = 1", params![candidate.client_id, candidate.id, candidate.project_id, detail, now()]).map_err(user_error)?;
    // Updating the selected profile is part of the same pointer-promotion
    // transaction. A failed profile fence rolls everything back before any
    // previous protected bundle is erased by the callback.
    channel_profiles::synchronize_active_profile(&transaction)?;
    audit_global(&transaction, "oauth_client_replaced_same_channel", "Operator replaced protected Desktop OAuth credentials after same-channel verification; saved uploads and checkpoints retained")?;
    transaction.commit().map_err(user_error)
}

pub(super) fn configured_project_id(connection: &Connection) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT oauth_project_id FROM connection_settings WHERE singleton = 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(user_error)
        .map(Option::flatten)
}

fn quota_scope(client_id: &str, project_id: Option<&str>) -> String {
    match project_id.filter(|id| !id.trim().is_empty()) {
        Some(project) => format!("project:{project}"),
        None => format!("client:{client_id}"),
    }
}

fn active_scope(connection: &Connection) -> Result<String, String> {
    Ok(quota_scope(
        &configured_oauth_client_id(connection)?,
        configured_project_id(connection)?.as_deref(),
    ))
}

/// Capture this scope before the provider request. Its project identity is shared
/// across saved channel profiles; a response must never recapture a new scope.
pub(super) fn configured_quota_scope(connection: &Connection) -> Result<String, String> {
    active_scope(connection)
}

/// Repair the former rolling 24-hour Cloud holds using their saved observation
/// time. Call only inside a native housekeeping/recording transaction, never a
/// projection. Keep updated_at unchanged so repeated passes cannot roll a reset
/// forward; malformed or already-shorter holds remain fail-closed.
pub(super) fn normalize_api_pause_deadlines(connection: &Connection) -> Result<usize, String> {
    let rows = connection
        .prepare("SELECT quota_scope,channel_id,pause_until,updated_at FROM api_quota_pauses")
        .map_err(user_error)?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    let mut changed = 0;
    for (scope, channel, saved, observed) in rows {
        let (Ok(until), Ok(observed)) = (
            DateTime::parse_from_rfc3339(&saved),
            DateTime::parse_from_rfc3339(&observed),
        ) else {
            continue;
        };
        if until <= observed {
            continue;
        }
        let Ok((_, reset)) = quota_planner::pacific_window(observed.with_timezone(&Utc)) else {
            continue;
        };
        let reset_time = DateTime::parse_from_rfc3339(&reset).map_err(user_error)?;
        if until <= reset_time {
            continue;
        }
        changed += connection.execute("UPDATE api_quota_pauses SET pause_until=?1 WHERE quota_scope=?2 AND pause_until=?3",params![reset,scope,saved]).map_err(user_error)?;
        connection.execute("INSERT INTO audit_events(id,item_id,channel_id,kind,detail,created_at) VALUES(?1,NULL,?2,'youtube_api_quota_deadline_corrected','Cloud quota retry was aligned to midnight Pacific using the saved quota observation. Channel upload holds, source references and protected checkpoints were retained.',?3)",params![Uuid::new_v4().to_string(),channel,now()]).map_err(user_error)?;
    }
    Ok(changed)
}

/// Record a Cloud quota response for a captured channel/project without changing
/// upload state. In particular, post-upload attachment/publication failures must
/// never turn a confirmed video receipt back into queued work.
///
/// `channel_limit` is true only for an exact structured mixed Cloud/channel
/// classification. Callers retain provider classification outside this recorder.
pub(super) fn record_scoped_api_pause(
    state: &AppState,
    expected_channel: &str,
    scope: &str,
    item_id: Option<&str>,
    channel_limit: bool,
) -> Result<String, String> {
    record_scoped_api_pause_at(
        state,
        expected_channel,
        scope,
        item_id,
        channel_limit,
        Utc::now(),
    )
}

fn record_scoped_api_pause_at(
    state: &AppState,
    expected_channel: &str,
    scope: &str,
    item_id: Option<&str>,
    channel_limit: bool,
    observed: DateTime<Utc>,
) -> Result<String, String> {
    let deadline = quota_planner::pacific_window(observed)?.1;
    let channel_deadline = (observed + ChronoDuration::hours(24)).to_rfc3339();
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let settings = connection_settings(&transaction)?;
    if expected_channel.is_empty()
        || settings.active_channel_id.as_deref() != Some(expected_channel)
        || configured_quota_scope(&transaction)? != scope
    {
        return Err("The quota response belongs to an account or Cloud project that is no longer selected. Saved upload receipts remain unchanged.".into());
    }
    if let Some(item) = item_id {
        let item_channel: Option<String> = transaction
            .query_row(
                "SELECT channel_id FROM upload_items WHERE id = ?1",
                [item],
                |row| row.get(0),
            )
            .optional()
            .map_err(user_error)?
            .flatten();
        if item_channel.as_deref() != Some(expected_channel) {
            return Err("The quota response does not match the saved upload's immutable channel. Saved upload receipts remain unchanged.".into());
        }
    }
    normalize_api_pause_deadlines(&transaction)?;
    transaction.execute("INSERT INTO api_quota_pauses(quota_scope, channel_id, pause_until, updated_at) VALUES(?1, ?2, ?3, ?4) ON CONFLICT(quota_scope) DO UPDATE SET pause_until = MAX(pause_until, excluded.pause_until), updated_at = excluded.updated_at", params![scope, expected_channel, deadline, observed.to_rfc3339()]).map_err(user_error)?;
    if channel_limit {
        transaction.execute("INSERT INTO upload_quota_pauses(channel_id, pause_until, updated_at) VALUES(?1, ?2, ?3) ON CONFLICT(channel_id) DO UPDATE SET pause_until = MAX(pause_until, excluded.pause_until), updated_at = excluded.updated_at", params![expected_channel, channel_deadline, observed.to_rfc3339()]).map_err(user_error)?;
    }
    let channel_name = settings.active_channel.as_deref();
    let scoped_audit = |kind: &str, detail: &str| -> Result<(), String> {
        transaction.execute("INSERT INTO audit_events(id, item_id, channel_name, channel_id, kind, detail, created_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![Uuid::new_v4().to_string(), item_id, channel_name, expected_channel, kind, detail, now()]).map_err(user_error)?;
        Ok(())
    };
    scoped_audit("youtube_api_quota_paused", "YouTube reported Google Cloud API quota for the captured project; confirmed upload receipts, source references and protected resumable checkpoints retained")?;
    if channel_limit {
        scoped_audit("youtube_daily_upload_limit_paused", "YouTube reported an exact mixed channel upload limit and Cloud API quota response; independent captured-scope pauses retained")?;
    }
    transaction.commit().map_err(user_error)?;
    // A coordinator failure cannot reclassify the committed pause. Its retained
    // deadline remains authoritative at the next explicit action or launch.
    startup_gate(state).quota_worker_wake.notify_all();
    #[cfg(not(test))]
    let _ = ensure_quota_resume_worker(state.clone());
    Ok(deadline)
}

pub(super) fn active_api_pause(connection: &Connection) -> Result<Option<String>, String> {
    let settings = connection_settings(connection)?;
    if !settings.connected || !settings.oauth_configured {
        return Ok(None);
    }
    let scope = active_scope(connection)?;
    let deadline: Option<String> = connection
        .query_row(
            "SELECT pause_until FROM api_quota_pauses WHERE quota_scope = ?1",
            [&scope],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?;
    let Some(deadline) = deadline else {
        return Ok(None);
    };
    let valid = DateTime::parse_from_rfc3339(&deadline).map_err(|_| {
        "The saved API quota pause deadline is invalid; the queue remains paused.".to_string()
    })?;
    if valid.with_timezone(&Utc) > Utc::now() {
        return Ok(Some(deadline));
    }
    connection
        .execute(
            "DELETE FROM api_quota_pauses WHERE quota_scope = ?1",
            [&scope],
        )
        .map_err(user_error)?;
    audit_global(connection, "youtube_api_quota_pause_elapsed", "The local API quota pause elapsed; the next explicit provider request determines quota availability")?;
    Ok(None)
}

pub(super) fn api_pause_detail(deadline: &str) -> String {
    format!("Google Cloud API quota was reached. Automatic retry is scheduled for {deadline}, the expected midnight Pacific reset. YouTube channel limits still apply; the next provider request confirms availability.")
}

pub(super) fn record_api_pause(state: &AppState, item_id: &str) -> Result<String, String> {
    record_api_pause_with_channel(state, item_id, false)
}

pub(super) fn record_api_pause_with_channel(
    state: &AppState,
    item_id: &str,
    channel_limit: bool,
) -> Result<String, String> {
    record_api_pause_with_channel_at(state, item_id, channel_limit, Utc::now())
}

fn record_api_pause_with_channel_at(
    state: &AppState,
    item_id: &str,
    channel_limit: bool,
    observed: DateTime<Utc>,
) -> Result<String, String> {
    let deadline = quota_planner::pacific_window(observed)?.1;
    let channel_deadline = (observed + ChronoDuration::hours(24)).to_rfc3339();
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let item_channel: String = transaction
        .query_row(
            "SELECT COALESCE(channel_id, '') FROM upload_items WHERE id = ?1",
            [item_id],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if connection_settings(&transaction)?
        .active_channel_id
        .as_deref()
        != Some(item_channel.as_str())
        || item_channel.is_empty()
    {
        return Err("The API quota result does not match the active upload channel.".into());
    }
    let scope = active_scope(&transaction)?;
    normalize_api_pause_deadlines(&transaction)?;
    if channel_limit {
        transaction.execute("INSERT INTO upload_quota_pauses(channel_id, pause_until, updated_at) VALUES(?1, ?2, ?3) ON CONFLICT(channel_id) DO UPDATE SET pause_until = MAX(pause_until, excluded.pause_until), updated_at = excluded.updated_at", params![item_channel, channel_deadline, observed.to_rfc3339()]).map_err(user_error)?;
        audit(&transaction, item_id, "youtube_daily_upload_limit_paused", "YouTube reported a channel upload limit alongside Cloud API quota; both saved pauses are retained")?;
    }
    transaction.execute("INSERT INTO api_quota_pauses(quota_scope, channel_id, pause_until, updated_at) VALUES(?1, ?2, ?3, ?4) ON CONFLICT(quota_scope) DO UPDATE SET pause_until = MAX(pause_until, excluded.pause_until), updated_at = excluded.updated_at", params![scope, item_channel, deadline, observed.to_rfc3339()]).map_err(user_error)?;
    transaction
        .execute(
            "UPDATE upload_items SET status = 'queued', detail = ?1, updated_at = ?2 WHERE id = ?3",
            params![api_pause_detail(&deadline), now(), item_id],
        )
        .map_err(user_error)?;
    audit(&transaction, item_id, "youtube_api_quota_paused", "YouTube reported Google Cloud API quota exhaustion; saved queue and protected resumable checkpoints retained")?;
    transaction.commit().map_err(user_error)?;
    #[cfg(not(test))]
    ensure_quota_resume_worker(state.clone())?;
    Ok(deadline)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LimitStatus {
    pub(super) kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) resumes_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) channel_resumes_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) detail: Option<String>,
}

pub(super) fn limit_status(
    connection: &Connection,
    expected_channel_id: Option<&str>,
) -> Result<LimitStatus, String> {
    let settings = connection_settings(connection)?;
    if expected_channel_id.is_some_and(|id| settings.active_channel_id.as_deref() != Some(id)) {
        return Err("The connected YouTube channel changed. Refresh account details.".into());
    }
    if !settings.connected {
        return Ok(LimitStatus {
            kind: "none",
            resumes_at: None,
            channel_resumes_at: None,
            detail: None,
        });
    }
    let channel_resumes_at = active_upload_quota_pause(connection)?;
    let api_resumes_at = active_api_pause(connection)?;
    let (kind, resumes_at, detail) = match api_resumes_at {
        Some(deadline) => (
            "api_quota",
            Some(deadline.clone()),
            Some(api_pause_detail(&deadline)),
        ),
        None => match channel_resumes_at.as_ref() {
            Some(deadline) => (
                "channel_upload",
                Some(deadline.clone()),
                Some(quota_pause_detail(deadline)),
            ),
            None => ("none", None, None),
        },
    };
    Ok(LimitStatus {
        kind,
        resumes_at,
        channel_resumes_at,
        detail,
    })
}

pub(super) fn dispatch_pause(connection: &Connection) -> Result<Option<(String, String)>, String> {
    let limits = limit_status(connection, None)?;
    Ok(limits.resumes_at.zip(limits.detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, AppState, Connection) {
        let root =
            std::env::temp_dir().join(format!("youtube-oauth-replacement-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton, oauth_client_id, oauth_project_id, active_channel, active_channel_id, deletion_authorized, deletion_sudo_until, updated_at) VALUES(1, 'old-client.apps.googleusercontent.com', 'old-project', 'Reviewed channel', 'channel-a', 1, ?1, ?2)", params![(Utc::now() + ChronoDuration::minutes(15)).to_rfc3339(), now()]).unwrap();
        connection.execute("INSERT INTO upload_items(id, title, file_name, channel_name, channel_id, source_path, workspace_path, size_bytes, status, confirmed_bytes, total_bytes, source_modified_key, resumable_session_uri, created_at, updated_at) VALUES('queued-a', 'Saved video', 'original.mp4', 'Reviewed channel', 'channel-a', 'original-source', 'original-source', 1000, 'queued', 400, 1000, 'saved-signature', 'legacy-protected-reference', ?1, ?1)", [now()]).unwrap();
        (root, state, connection)
    }

    fn candidate(connection: &Connection) -> Candidate {
        Candidate {
            id: Uuid::new_v4().to_string(),
            channel_id: "channel-a".into(),
            channel_name: "Reviewed channel".into(),
            client_id: "new-client.apps.googleusercontent.com".into(),
            project_id: Some("new-project".into()),
            has_secret: true,
            original_client_id: configured_oauth_client_id(connection).unwrap(),
            original_bundle_id: active_credential_bundle(connection).unwrap(),
            original_project_id: configured_project_id(connection).unwrap(),
            phase: Phase::Ready,
            attempt_id: None,
            detail: None,
            expires_at: Instant::now() + REVIEW_LIFETIME,
        }
    }

    fn deadlines(connection: &Connection) -> String {
        let deadline = (Utc::now() + ChronoDuration::hours(24)).to_rfc3339();
        connection.execute("INSERT INTO api_quota_pauses(quota_scope, channel_id, pause_until, updated_at) VALUES('project:old-project', 'channel-a', ?1, ?2)", params![deadline, now()]).unwrap();
        connection.execute("INSERT INTO upload_quota_pauses(channel_id, pause_until, updated_at) VALUES('channel-a', ?1, ?2)", params![deadline, now()]).unwrap();
        deadline
    }

    fn upload_evidence(connection: &Connection) -> (String, String, i64, String, String) {
        connection.query_row("SELECT status, workspace_path, confirmed_bytes, source_modified_key, resumable_session_uri FROM upload_items WHERE id = 'queued-a'", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))).unwrap()
    }

    fn saved_profile(connection: &Connection) -> (String, String) {
        let profile_id = Uuid::new_v4().to_string();
        let bundle_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "UPDATE connection_settings SET oauth_credential_id = ?1 WHERE singleton=1",
                [&bundle_id],
            )
            .unwrap();
        connection.execute("INSERT INTO channel_profiles(profile_id, channel_id, channel_name, oauth_client_id, oauth_project_id, credential_bundle_id, management_authorized, created_at, updated_at) VALUES(?1,'channel-a','Reviewed channel','old-client.apps.googleusercontent.com','old-project',?2,1,?3,?3)", params![profile_id, bundle_id, now()]).unwrap();
        (profile_id, bundle_id)
    }

    #[test]
    fn native_replacement_promotes_saved_profile_pointer_atomically_without_queue_rebinding() {
        let (root, _state, mut connection) = fixture();
        let (profile, old_bundle) = saved_profile(&connection);
        let evidence = upload_evidence(&connection);
        let replacement = candidate(&connection);
        deadlines(&connection);
        promote(&mut connection, &replacement, "Verified replacement").unwrap();
        let saved: (String, String, String, bool) = connection.query_row("SELECT credential_bundle_id, oauth_client_id, oauth_project_id, management_authorized FROM channel_profiles WHERE profile_id=?1", [&profile], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get::<_,i64>(3)? != 0))).unwrap();
        assert_eq!(saved.0, replacement.id);
        assert_ne!(saved.0, old_bundle);
        assert_eq!(saved.1, replacement.client_id);
        assert_eq!(saved.2, replacement.project_id.unwrap());
        assert!(!saved.3);
        assert_eq!(upload_evidence(&connection), evidence);
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM api_quota_pauses WHERE quota_scope='project:old-project'",
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
    fn native_replacement_profile_write_failure_rolls_back_active_pointer_and_queue_evidence() {
        let (root, _state, mut connection) = fixture();
        let (profile, old_bundle) = saved_profile(&connection);
        let evidence = upload_evidence(&connection);
        let replacement = candidate(&connection);
        connection.execute_batch("CREATE TRIGGER reject_profile_update BEFORE UPDATE ON channel_profiles BEGIN SELECT RAISE(ABORT, 'fixture profile write failure'); END;").unwrap();
        assert!(promote(&mut connection, &replacement, "Must roll back").is_err());
        assert_eq!(
            active_credential_bundle(&connection).unwrap().as_deref(),
            Some(old_bundle.as_str())
        );
        assert_eq!(
            configured_oauth_client_id(&connection).unwrap(),
            "old-client.apps.googleusercontent.com"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT credential_bundle_id FROM channel_profiles WHERE profile_id=?1",
                    [profile],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            old_bundle
        );
        assert_eq!(upload_evidence(&connection), evidence);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_replacement_requires_desktop_json_and_bounded_safe_metadata() {
        let client = parse_desktop_client(r#"{"installed":{"client_id":"123-client.apps.googleusercontent.com","client_secret":"fixture-secret","project_id":"project-name"}}"#).unwrap();
        assert_eq!(client.project_id.as_deref(), Some("project-name"));
        assert_eq!(client.client_secret, "fixture-secret");
        assert!(
            parse_desktop_client(r#"{"web":{"client_id":"123.apps.googleusercontent.com"}}"#)
                .is_err()
        );
        assert!(parse_desktop_client(
            r#"{"installed":{"client_id":"https://attacker.invalid","client_secret":"value"}}"#
        )
        .is_err());
        assert!(parse_desktop_client(r#"{"installed":{"client_id":"123.apps.googleusercontent.com","project_id":"../../invalid"}}"#).is_err());
        let root = std::env::temp_dir().join(format!("youtube-json-bound-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("oversize.json"),
            vec![b' '; MAX_CLIENT_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(read_desktop_client(&root.join("oversize.json")).is_err());
        assert!(read_desktop_client(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_replacement_promotes_pointer_preserving_queue_channel_pause_and_old_project_pause() {
        let (root, _state, mut connection) = fixture();
        let evidence = upload_evidence(&connection);
        let deadline = deadlines(&connection);
        let candidate = candidate(&connection);
        promote(&mut connection, &candidate, "Verified replacement").unwrap();
        assert_eq!(
            active_credential_bundle(&connection).unwrap().as_deref(),
            Some(candidate.id.as_str())
        );
        assert_eq!(
            configured_oauth_client_id(&connection).unwrap(),
            candidate.client_id
        );
        assert_eq!(upload_evidence(&connection), evidence);
        assert_eq!(
            active_upload_quota_pause(&connection).unwrap(),
            Some(deadline.clone())
        );
        assert_eq!(active_api_pause(&connection).unwrap(), None);
        assert_eq!(connection.query_row("SELECT pause_until FROM api_quota_pauses WHERE quota_scope = 'project:old-project'", [], |row| row.get::<_, String>(0)).unwrap(), deadline);
        let settings = connection_settings(&connection).unwrap();
        assert_eq!(settings.active_channel_id.as_deref(), Some("channel-a"));
        assert!(!settings.deletion_authorized);
        assert!(!settings.deletion_sudo_active);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_replacement_unknown_project_retains_api_hold_despite_a_new_client_id() {
        for new_project in [None, Some("new-project".to_string())] {
            let (root, _state, mut connection) = fixture();
            connection
                .execute("UPDATE connection_settings SET oauth_project_id = NULL", [])
                .unwrap();
            let deadline = (Utc::now() + ChronoDuration::hours(24)).to_rfc3339();
            connection.execute("INSERT INTO api_quota_pauses(quota_scope, channel_id, pause_until, updated_at) VALUES('client:old-client.apps.googleusercontent.com', 'channel-a', ?1, ?2)", params![deadline, now()]).unwrap();
            let mut candidate = candidate(&connection);
            candidate.project_id = new_project;
            let evidence = upload_evidence(&connection);
            promote(&mut connection, &candidate, "API hold retained").unwrap();
            assert_eq!(active_api_pause(&connection).unwrap(), Some(deadline));
            assert_eq!(limit_status(&connection, None).unwrap().kind, "api_quota");
            assert_eq!(upload_evidence(&connection), evidence);
            drop(connection);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn native_replacement_same_project_keeps_both_pauses_and_cloud_takes_precedence() {
        let (root, _state, mut connection) = fixture();
        let deadline = deadlines(&connection);
        let mut candidate = candidate(&connection);
        candidate.project_id = candidate.original_project_id.clone();
        promote(&mut connection, &candidate, "Same project").unwrap();
        let status = limit_status(&connection, Some("channel-a")).unwrap();
        assert_eq!(status.kind, "api_quota");
        assert_eq!(status.resumes_at, Some(deadline.clone()));
        assert_eq!(status.channel_resumes_at, Some(deadline));
        assert!(status.detail.unwrap().contains("Google Cloud"));
        assert!(limit_status(&connection, Some("channel-b")).is_err());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_replacement_same_client_cannot_clear_quota_by_changing_project_metadata() {
        let (root, _state, mut connection) = fixture();
        let deadline = deadlines(&connection);
        let mut candidate = candidate(&connection);
        candidate.client_id = candidate.original_client_id.clone();
        assert!(!candidate_different_project(&candidate));
        promote(&mut connection, &candidate, "Same client").unwrap();
        assert_eq!(active_api_pause(&connection).unwrap(), Some(deadline));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_replacement_final_binding_change_or_active_upload_blocks_promotion() {
        for mutation in [
            "UPDATE connection_settings SET active_channel_id = 'channel-b'",
            "UPDATE connection_settings SET oauth_client_id = 'changed.apps.googleusercontent.com'",
            "UPDATE connection_settings SET oauth_credential_id = 'changed-bundle'",
            "UPDATE connection_settings SET oauth_project_id = 'changed-project'",
            "UPDATE upload_items SET status = 'uploading' WHERE id = 'queued-a'",
        ] {
            let (root, _state, mut connection) = fixture();
            let candidate = candidate(&connection);
            connection.execute(mutation, []).unwrap();
            let evidence = upload_evidence(&connection);
            assert!(promote(&mut connection, &candidate, "Must not promote").is_err());
            assert_eq!(upload_evidence(&connection), evidence);
            assert_ne!(
                active_credential_bundle(&connection).unwrap().as_deref(),
                Some(candidate.id.as_str())
            );
            drop(connection);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn native_replacement_database_failure_rolls_back_pointer_and_retains_upload_evidence() {
        let (root, _state, mut connection) = fixture();
        let evidence = upload_evidence(&connection);
        let deadline = deadlines(&connection);
        let candidate = candidate(&connection);
        connection.execute_batch("CREATE TRIGGER fail_replacement_audit BEFORE INSERT ON audit_events WHEN NEW.kind = 'oauth_client_replaced_same_channel' BEGIN SELECT RAISE(ABORT, 'fixture commit failure'); END;").unwrap();
        assert!(promote(&mut connection, &candidate, "Must roll back").is_err());
        assert_eq!(
            configured_oauth_client_id(&connection).unwrap(),
            candidate.original_client_id
        );
        assert_eq!(active_credential_bundle(&connection).unwrap(), None);
        assert_eq!(upload_evidence(&connection), evidence);
        assert_eq!(active_api_pause(&connection).unwrap(), Some(deadline));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_replacement_cancelled_or_stale_callback_cannot_promote() {
        let (root, state, connection) = fixture();
        let mut candidate = candidate(&connection);
        candidate.phase = Phase::Authorizing;
        candidate.attempt_id = Some("current-attempt".into());
        let key = (state.database_path.clone(), candidate.id.clone());
        reviews()
            .lock()
            .unwrap()
            .insert(key.clone(), candidate.clone());
        assert!(callback_live(&state, &candidate, "current-attempt"));
        assert!(!callback_live(&state, &candidate, "old-attempt"));
        assert!(ensure_no_pending(&state).is_err());
        reviews().lock().unwrap().get_mut(&key).unwrap().phase = Phase::Cancelled;
        assert!(!callback_live(&state, &candidate, "current-attempt"));
        assert!(ensure_no_pending(&state).is_ok());
        assert_eq!(
            configured_oauth_client_id(&connection).unwrap(),
            candidate.original_client_id
        );
        reviews().lock().unwrap().remove(&key);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_replacement_expiry_fences_callback_erases_staging_and_releases_dispatch_hold() {
        let (root, state, connection) = fixture();
        let mut candidate = candidate(&connection);
        candidate.phase = Phase::Authorizing;
        candidate.attempt_id = Some("expired-attempt".into());
        candidate.expires_at = Instant::now() - Duration::from_secs(1);
        let key = (state.database_path.clone(), candidate.id.clone());
        reviews()
            .lock()
            .unwrap()
            .insert(key.clone(), candidate.clone());
        state
            .oauth_attempts
            .lock()
            .unwrap()
            .insert("expired-attempt".into(), OAuthAttemptKind::Replacement);
        let mut erased = Vec::new();
        expire_reviews_with(&state, |candidate| erased.push(candidate.id.clone())).unwrap();
        assert_eq!(erased, vec![candidate.id.clone()]);
        assert!(!callback_live(&state, &candidate, "expired-attempt"));
        assert!(state.oauth_attempts.lock().unwrap().is_empty());
        assert!(ensure_no_pending(&state).is_ok());
        assert_eq!(
            configured_oauth_client_id(&connection).unwrap(),
            candidate.original_client_id
        );
        assert!(active_credential_bundle(&connection).unwrap().is_none());
        reviews().lock().unwrap().remove(&key);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_api_pause_is_project_scoped_and_expires_without_changing_channel_pause() {
        let (root, _state, connection) = fixture();
        let deadline = deadlines(&connection);
        connection.execute("INSERT INTO api_quota_pauses(quota_scope, channel_id, pause_until, updated_at) VALUES('project:other-project', 'channel-b', ?1, ?2)", params![deadline, now()]).unwrap();
        assert_eq!(
            active_api_pause(&connection).unwrap(),
            Some(deadline.clone())
        );
        connection.execute("UPDATE api_quota_pauses SET pause_until = ?1 WHERE quota_scope = 'project:old-project'", [(Utc::now() - ChronoDuration::seconds(1)).to_rfc3339()]).unwrap();
        assert_eq!(active_api_pause(&connection).unwrap(), None);
        assert_eq!(
            active_upload_quota_pause(&connection).unwrap(),
            Some(deadline)
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM api_quota_pauses WHERE quota_scope = 'project:other-project'", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
        assert_eq!(
            limit_status(&connection, None).unwrap().kind,
            "channel_upload"
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_cloud_pause_ends_at_pacific_midnight_instead_of_rolling_24_hours() {
        use chrono::TimeZone;
        let (root, state, connection) = fixture();
        let deadline = record_api_pause(&state, "queued-a").unwrap();
        let observed: String = connection
            .query_row(
                "SELECT updated_at FROM api_quota_pauses WHERE quota_scope='project:old-project'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let local = DateTime::parse_from_rfc3339(&observed)
            .unwrap()
            .with_timezone(&chrono_tz::America::Los_Angeles);
        let expected = chrono_tz::America::Los_Angeles
            .from_local_datetime(
                &local
                    .date_naive()
                    .succ_opt()
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            DateTime::parse_from_rfc3339(&deadline)
                .unwrap()
                .with_timezone(&Utc),
            expected
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_cloud_recorders_share_daily_reset_across_seasons_and_midnight() {
        for (timestamp, expected) in [
            ("2026-01-13T07:59:59Z", "2026-01-13T08:00:00+00:00"),
            ("2026-01-13T08:00:00Z", "2026-01-14T08:00:00+00:00"),
            ("2026-03-08T07:59:59Z", "2026-03-08T08:00:00+00:00"),
            ("2026-03-08T10:00:00Z", "2026-03-09T07:00:00+00:00"),
            ("2026-11-01T06:59:59Z", "2026-11-01T07:00:00+00:00"),
            ("2026-11-01T09:00:00Z", "2026-11-02T08:00:00+00:00"),
            ("2026-10-18T22:09:14Z", "2026-10-19T07:00:00+00:00"),
        ] {
            let observed = DateTime::parse_from_rfc3339(timestamp)
                .unwrap()
                .with_timezone(&Utc);
            for scoped in [false, true] {
                let (root, state, connection) = fixture();
                let before = upload_evidence(&connection);
                // A legacy oversized hold must not win the recorder's MAX upsert.
                connection.execute("INSERT INTO api_quota_pauses(quota_scope,channel_id,pause_until,updated_at) VALUES('project:old-project','channel-a',?1,?2)",params![(observed+ChronoDuration::hours(24)).to_rfc3339(),observed.to_rfc3339()]).unwrap();
                let deadline = if scoped {
                    record_scoped_api_pause_at(
                        &state,
                        "channel-a",
                        "project:old-project",
                        Some("queued-a"),
                        true,
                        observed,
                    )
                    .unwrap()
                } else {
                    record_api_pause_with_channel_at(&state, "queued-a", true, observed).unwrap()
                };
                assert_eq!(deadline, expected, "{timestamp}, scoped={scoped}");
                let saved: (String,String) = connection.query_row("SELECT pause_until,updated_at FROM api_quota_pauses WHERE quota_scope='project:old-project'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
                assert_eq!(saved, (expected.into(), observed.to_rfc3339()));
                assert_eq!(connection.query_row("SELECT pause_until FROM upload_quota_pauses WHERE channel_id='channel-a'",[],|row|row.get::<_,String>(0)).unwrap(),(observed+ChronoDuration::hours(24)).to_rfc3339());
                assert_eq!(upload_evidence(&connection), before);
                drop(connection);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn native_cloud_legacy_deadline_repair_is_idempotent_and_preserves_channel_and_queue() {
        let (root, state, connection) = fixture();
        let before = upload_evidence(&connection);
        connection.execute_batch("INSERT INTO api_quota_pauses VALUES('project:old-project','channel-a','2026-10-19T22:09:14+00:00','2026-10-18T22:09:14+00:00');
            INSERT INTO api_quota_pauses VALUES('project:invalid','channel-b','unreadable','2026-10-18T22:09:14+00:00');
            INSERT INTO api_quota_pauses VALUES('project:short','channel-b','2026-10-18T23:09:14+00:00','2026-10-18T22:09:14+00:00');
            INSERT INTO upload_quota_pauses VALUES('channel-a','2026-10-19T22:09:14+00:00','2026-10-18T22:09:14+00:00');").unwrap();
        let transaction = connection.unchecked_transaction().unwrap();
        assert_eq!(normalize_api_pause_deadlines(&transaction).unwrap(), 1);
        assert_eq!(normalize_api_pause_deadlines(&transaction).unwrap(), 0);
        transaction.commit().unwrap();
        assert_eq!(connection.query_row("SELECT pause_until,updated_at FROM api_quota_pauses WHERE quota_scope='project:old-project'",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).unwrap(),("2026-10-19T07:00:00+00:00".into(),"2026-10-18T22:09:14+00:00".into()));
        assert_eq!(
            connection
                .query_row(
                    "SELECT pause_until FROM api_quota_pauses WHERE quota_scope='project:short'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "2026-10-18T23:09:14+00:00"
        );
        let reset = DateTime::parse_from_rfc3339("2026-10-19T07:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        expire_upload_admission_pauses(&connection, reset).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM api_quota_pauses WHERE quota_scope='project:old-project'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT pause_until FROM upload_quota_pauses WHERE channel_id='channel-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "2026-10-19T22:09:14+00:00"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT pause_until FROM api_quota_pauses WHERE quota_scope='project:invalid'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "unreadable"
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE kind='youtube_api_quota_deadline_corrected' AND channel_id='channel-a'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(upload_evidence(&connection), before);
        drop(connection);
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_cloud_deadline_repair_rolls_back_if_the_audit_receipt_fails() {
        let (root, state, connection) = fixture();
        connection.execute_batch("INSERT INTO api_quota_pauses VALUES('project:old-project','channel-a','2026-10-19T22:09:14+00:00','2026-10-18T22:09:14+00:00');
            CREATE TRIGGER reject_repair BEFORE INSERT ON audit_events WHEN NEW.kind='youtube_api_quota_deadline_corrected' BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END;").unwrap();
        let before = upload_evidence(&connection);
        assert!(expire_upload_admission_pauses(&connection, Utc::now()).is_err());
        assert_eq!(
            connection
                .query_row("SELECT pause_until FROM api_quota_pauses", [], |row| row
                    .get::<_, String>(
                    0
                ))
                .unwrap(),
            "2026-10-19T22:09:14+00:00"
        );
        assert_eq!(upload_evidence(&connection), before);
        drop(connection);
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_api_pause_records_structured_scope_and_retains_checkpoint_progress() {
        let (root, state, connection) = fixture();
        let before = upload_evidence(&connection);
        let deadline = record_api_pause(&state, "queued-a").unwrap();
        assert_eq!(active_api_pause(&connection).unwrap(), Some(deadline));
        assert_eq!(upload_evidence(&connection), before);
        assert!(connection
            .query_row(
                "SELECT detail FROM upload_items WHERE id = 'queued-a'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap()
            .contains("Google Cloud"));
        assert!(connection.query_row("SELECT EXISTS(SELECT 1 FROM state_changes WHERE surface = 'connection' AND channel_id = 'channel-a')", [], |row| row.get::<_, bool>(0)).unwrap());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_mixed_provider_limits_commit_both_holds_with_cloud_detail_and_preserve_progress() {
        let (root, state, connection) = fixture();
        connection
            .execute(
                "UPDATE upload_items SET status = 'uploading' WHERE id = 'queued-a'",
                [],
            )
            .unwrap();
        let deadline = record_api_pause_with_channel(&state, "queued-a", true).unwrap();
        let status = limit_status(&connection, None).unwrap();
        assert_eq!(status.kind, "api_quota");
        assert_eq!(status.resumes_at, Some(deadline.clone()));
        let observed: String = connection
            .query_row(
                "SELECT updated_at FROM api_quota_pauses WHERE quota_scope='project:old-project'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let channel_deadline = (DateTime::parse_from_rfc3339(&observed)
            .unwrap()
            .with_timezone(&Utc)
            + ChronoDuration::hours(24))
        .to_rfc3339();
        assert_eq!(status.channel_resumes_at, Some(channel_deadline));
        let item = upload_evidence(&connection);
        assert_eq!(item.0, "queued");
        assert_eq!(item.2, 400);
        assert_eq!(item.4, "legacy-protected-reference");
        assert!(connection
            .query_row(
                "SELECT detail FROM upload_items WHERE id = 'queued-a'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap()
            .contains("Google Cloud"));
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    fn postprocess_receipt(connection: &Connection) -> String {
        connection.query_row("SELECT json_object('id',id,'channel',channel_id,'status',status,'videoId',video_id,'confirmedBytes',confirmed_bytes,'totalBytes',total_bytes,'session',resumable_session_uri,'source',source_path,'workspace',workspace_path,'signature',source_modified_key,'detail',detail,'playlist',playlist_status,'cleanup',source_delete_status,'metadata',upload_metadata_json,'updatedAt',updated_at) FROM upload_items WHERE id='queued-a'", [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn native_scoped_postprocess_pause_preserves_confirmed_receipt_and_same_project_identity() {
        let (root, state, connection) = fixture();
        connection.execute("UPDATE upload_items SET status='uploaded',video_id='confirmed-video',confirmed_bytes=1000,playlist_status='completed',source_delete_status='retained',detail='Provider confirmed the upload',upload_metadata_json='{\"description\":\"approved\"}' WHERE id='queued-a'", []).unwrap();
        let before = postprocess_receipt(&connection);
        let scope = configured_quota_scope(&connection).unwrap();
        assert_eq!(scope, "project:old-project");
        // A client change in the same Cloud project cannot select fresh quota.
        connection.execute("UPDATE connection_settings SET oauth_client_id='same-project-client.apps.googleusercontent.com'", []).unwrap();
        let channel_pause = (Utc::now() + ChronoDuration::hours(72)).to_rfc3339();
        connection.execute("INSERT INTO upload_quota_pauses(channel_id,pause_until,updated_at) VALUES('channel-a',?1,?2)", params![channel_pause, now()]).unwrap();
        let deadline =
            record_scoped_api_pause(&state, "channel-a", &scope, Some("queued-a"), false).unwrap();
        assert_eq!(postprocess_receipt(&connection), before);
        assert_eq!(active_api_pause(&connection).unwrap(), Some(deadline));
        assert_eq!(
            active_upload_quota_pause(&connection).unwrap(),
            Some(channel_pause)
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE kind='youtube_api_quota_paused' AND channel_id='channel-a' AND item_id='queued-a'", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE kind='youtube_daily_upload_limit_paused'", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_scoped_postprocess_pause_rejects_stale_project_channel_and_item_scope_without_writes()
    {
        let (root, state, connection) = fixture();
        let scope = configured_quota_scope(&connection).unwrap();
        let before = postprocess_receipt(&connection);
        connection
            .execute(
                "UPDATE connection_settings SET oauth_project_id='changed-project'",
                [],
            )
            .unwrap();
        assert!(
            record_scoped_api_pause(&state, "channel-a", &scope, Some("queued-a"), true).is_err()
        );
        let fresh_scope = configured_quota_scope(&connection).unwrap();
        assert!(record_scoped_api_pause(&state, "channel-b", &fresh_scope, None, true).is_err());
        assert!(
            record_scoped_api_pause(&state, "channel-a", "project:unrelated", None, true).is_err()
        );
        assert!(record_scoped_api_pause(
            &state,
            "channel-a",
            &fresh_scope,
            Some("missing-item"),
            true
        )
        .is_err());
        assert_eq!(postprocess_receipt(&connection), before);
        connection
            .execute(
                "UPDATE upload_items SET channel_id='channel-b' WHERE id='queued-a'",
                [],
            )
            .unwrap();
        let foreign = postprocess_receipt(&connection);
        assert!(
            record_scoped_api_pause(&state, "channel-a", &fresh_scope, Some("queued-a"), true)
                .is_err()
        );
        assert_eq!(postprocess_receipt(&connection), foreign);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM api_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE kind IN ('youtube_api_quota_paused','youtube_daily_upload_limit_paused')", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_scoped_postprocess_pause_without_item_keeps_shared_project_hold_and_scopes_mixed_audit(
    ) {
        let (root, state, connection) = fixture();
        let original_deadline = deadlines(&connection);
        let before = postprocess_receipt(&connection);
        connection.execute("UPDATE connection_settings SET active_channel='Channel B',active_channel_id='channel-b',oauth_client_id='profile-b.apps.googleusercontent.com'", []).unwrap();
        let scope = configured_quota_scope(&connection).unwrap();
        let deadline = record_scoped_api_pause(&state, "channel-b", &scope, None, true).unwrap();
        assert_eq!(postprocess_receipt(&connection), before);
        assert_eq!(active_api_pause(&connection).unwrap(), Some(deadline));
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM api_quota_pauses WHERE quota_scope='project:old-project'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT pause_until FROM upload_quota_pauses WHERE channel_id='channel-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            original_deadline
        );
        let observed: String = connection
            .query_row(
                "SELECT updated_at FROM api_quota_pauses WHERE quota_scope='project:old-project'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let channel_deadline = (DateTime::parse_from_rfc3339(&observed)
            .unwrap()
            .with_timezone(&Utc)
            + ChronoDuration::hours(24))
        .to_rfc3339();
        assert_eq!(
            active_upload_quota_pause(&connection).unwrap(),
            Some(channel_deadline)
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE channel_id='channel-b' AND item_id IS NULL AND kind IN ('youtube_api_quota_paused','youtube_daily_upload_limit_paused')", [], |row| row.get::<_, i64>(0)).unwrap(), 2);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_scoped_postprocess_pause_audit_failure_rolls_back_both_holds_and_receipt() {
        let (root, state, connection) = fixture();
        let scope = configured_quota_scope(&connection).unwrap();
        let before = postprocess_receipt(&connection);
        connection.execute_batch("CREATE TRIGGER reject_scoped_quota_audit BEFORE INSERT ON audit_events WHEN NEW.kind='youtube_api_quota_paused' BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END;").unwrap();
        assert!(
            record_scoped_api_pause(&state, "channel-a", &scope, Some("queued-a"), true).is_err()
        );
        assert_eq!(postprocess_receipt(&connection), before);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM api_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_replacement_review_never_serializes_json_secrets_or_source_locators() {
        let review = Review {
            review_id: "opaque-review".into(),
            channel_id: "channel-a".into(),
            channel_name: "Reviewed channel".into(),
            cloud_quota_paused: true,
            channel_upload_paused: true,
        };
        let value = serde_json::to_value(review).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 5);
        assert_eq!(value["cloudQuotaPaused"], true);
        assert!(value.get("clientSecret").is_none());
        assert!(value.get("path").is_none());
        assert!(bundle_entry("unsafe/reference", "secret").is_err());
        assert!(bundle_entry(&Uuid::new_v4().to_string(), "unapproved-purpose").is_err());
    }
}
