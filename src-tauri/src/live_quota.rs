//! Explicit, read-only Google Cloud quota reporting. The dedicated grant and
//! every token stay in the OS credential store; no Data API request is needed.
use super::*;
use chrono::TimeZone;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

const SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform.read-only https://www.googleapis.com/auth/monitoring.read";
const AUTH_LIFETIME: Duration = Duration::from_secs(600);
const MAX_BODY: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_BODY: usize = 32 * 1024 * 1024;
const MAX_PAGES: usize = 20;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const METRIC_LAG_SECONDS: u64 = 240;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LiveQuotaLimit {
    metric: String,
    display_name: String,
    unit: String,
    dimensions: BTreeMap<String, String>,
    effective_limit: Option<u64>,
    used: Option<u64>,
    remaining: Option<u64>,
    sample_through: Option<String>,
    state: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LiveQuotaSnapshot {
    fetched_at: String,
    period_start: String,
    period_end: String,
    metric_lag_seconds: u64,
    limits: Vec<LiveQuotaLimit>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LiveQuotaStatus {
    phase: String,
    channel_id: String,
    project_id: String,
    detail: String,
    attempt_id: Option<String>,
    snapshot: Option<LiveQuotaSnapshot>,
}

#[derive(Serialize, Deserialize)]
struct ProtectedGrant {
    generation: String,
    refresh_token: String,
    access_token: String,
    expires_at: String,
    scopes: String,
    // Verified by Resource Manager, not inferred from the OAuth client ID.
    #[serde(default)]
    project_number: Option<String>,
}

struct BoundContext {
    binding: SelectedYouTubeReadBinding,
    project: String,
    key: String,
}

impl BoundContext {
    fn capture(state: &AppState, expected: &str) -> Result<Self, String> {
        let binding = SelectedYouTubeReadBinding::capture(&database(state)?)?;
        if expected.is_empty() || binding.channel_id != expected {
            return Err("The selected YouTube channel changed. Open quota details again.".into());
        }
        let project = binding.project_id.clone().filter(|id| valid_project(id))
            .ok_or("Import the Desktop OAuth JSON containing this connection's Google Cloud project ID before connecting live quota.")?;
        let key = scoped_key(&state.database_path, &binding.cache_key()?);
        Ok(Self {
            binding,
            project,
            key,
        })
    }

    fn current(&self, state: &AppState) -> Result<(), String> {
        self.binding.require_current(&database(state)?)
    }

    fn entry(&self) -> Result<CredentialEntry, String> {
        CredentialEntry::new(
            SECURE_STORE_SERVICE,
            &format!("youtube-cloud-quota-{}", self.key),
        )
        .map_err(|_| "The protected Cloud quota connection is unavailable.".into())
    }
}

fn valid_project(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.' || c == b':')
}

fn scoped_key(path: &Path, binding: &str) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(path.to_string_lossy().as_bytes());
    hash.update(&[0]);
    hash.update(binding.as_bytes());
    hash.finalize().to_hex().to_string()
}

#[derive(Default)]
struct Runtime {
    attempt_id: Option<String>,
    deadline: Option<Instant>,
    detail: Option<String>,
    snapshot: Option<LiveQuotaSnapshot>,
}

fn runtime() -> &'static Mutex<HashMap<String, Runtime>> {
    static VALUE: OnceLock<Mutex<HashMap<String, Runtime>>> = OnceLock::new();
    VALUE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn operation_lock() -> &'static Mutex<()> {
    static VALUE: OnceLock<Mutex<()>> = OnceLock::new();
    VALUE.get_or_init(|| Mutex::new(()))
}

fn load_grant(context: &BoundContext) -> Result<Option<ProtectedGrant>, String> {
    match context.entry()?.get_password() {
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("The protected Cloud quota connection could not be read. Unlock this device's credential store and retry.".into()),
        Ok(value) if value.len() <= 64 * 1024 => {
            let grant: ProtectedGrant = serde_json::from_str(&value)
                .map_err(|_| "The protected Cloud quota connection is unreadable. Authorize it again.".to_string())?;
            if grant.refresh_token.is_empty() || !has_scopes(&grant.scopes) {
                return Err("The Cloud quota connection lacks its read-only permissions. Authorize it again.".into());
            }
            Ok(Some(grant))
        }
        Ok(_) => Err("The protected Cloud quota connection is invalid. Authorize it again.".into()),
    }
}

fn save_grant(context: &BoundContext, grant: &ProtectedGrant) -> Result<(), String> {
    let text = serde_json::to_string(grant)
        .map_err(|_| "Cloud authorization could not be saved.".to_string())?;
    if text.len() > 64 * 1024 {
        return Err("Google returned an oversized authorization response.".into());
    }
    context.entry()?.set_password(&text).map_err(|_| {
        "The read-only Cloud quota connection could not be saved in protected storage.".into()
    })
}

fn has_scopes(scope: &str) -> bool {
    SCOPES
        .split_whitespace()
        .all(|required| scope.split_whitespace().any(|actual| actual == required))
}

fn cleanup_attempt(state: &AppState, attempt: &str) {
    if let Ok(mut attempts) = state.oauth_attempts.lock() {
        if attempts.get(attempt) == Some(&OAuthAttemptKind::CloudQuota) {
            attempts.remove(attempt);
        }
    }
    let _ = clear_refresh_token(oauth_verifier_entry(attempt));
}

fn status_impl(state: &AppState, expected: &str) -> Result<LiveQuotaStatus, String> {
    let context = BoundContext::capture(state, expected)?;
    let _credentials = oauth_replacement::lock_credentials()?;
    let connected = load_grant(&context)?.is_some();
    let mut values = runtime().lock().map_err(user_error)?;
    let item = values.entry(context.key.clone()).or_default();
    if item
        .deadline
        .is_some_and(|deadline| Instant::now() >= deadline)
    {
        if let Some(attempt) = item.attempt_id.take() {
            cleanup_attempt(state, &attempt);
        }
        item.deadline = None;
        item.detail = Some("Google Cloud authorization expired. Retry when ready.".into());
    }
    context.current(state)?;
    Ok(LiveQuotaStatus {
        phase: if item.attempt_id.is_some() { "authorizing" } else if connected { "connected" } else if item.detail.is_some() { "failed" } else { "disconnected" }.into(),
        channel_id: context.binding.channel_id,
        project_id: context.project,
        detail: item.detail.clone().unwrap_or_else(|| if connected {
            "Read-only Google Cloud access is connected. Refresh to request current reported quota.".into()
        } else {
            "Authorize read-only Cloud access with a Google account that can view this project's quotas and monitoring metrics.".into()
        }),
        attempt_id: item.attempt_id.clone(),
        snapshot: item.snapshot.clone(),
    })
}

#[tauri::command]
pub(super) async fn load_live_quota_status(
    expected_channel_id: String,
    state: State<'_, AppState>,
) -> Result<LiveQuotaStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || status_impl(&state, &expected_channel_id))
        .await
        .map_err(|_| "Cloud quota status could not be loaded.".to_string())?
}

#[tauri::command]
pub(super) fn begin_live_quota_authorization(
    expected_channel_id: String,
    state: State<'_, AppState>,
) -> Result<OAuthStart, String> {
    let context = Arc::new(BoundContext::capture(&state, &expected_channel_id)?);
    let _credentials = oauth_replacement::lock_credentials()?;
    oauth_replacement::ensure_no_pending(&state)?;
    if !state.oauth_attempts.lock().map_err(user_error)?.is_empty() {
        return Err("Finish or cancel the current Google authorization first.".into());
    }
    if !secure_store_available() {
        return Err(
            "Unlock the device's secure credential store before connecting Cloud quota.".into(),
        );
    }
    context.current(&state)?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "A secure local Google callback port could not be opened.".to_string())?;
    let redirect = format!(
        "http://127.0.0.1:{}/oauth2/callback",
        listener.local_addr().map_err(user_error)?.port()
    );
    let attempt = Uuid::new_v4().simple().to_string();
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let authorization_url = url::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        &[
            ("client_id", context.binding.client_id.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("response_type", "code"),
            ("scope", SCOPES),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", attempt.as_str()),
        ],
    )
    .map_err(|_| "Google consent could not be prepared.".to_string())?
    .to_string();
    oauth_verifier_entry(&attempt)?
        .set_password(&verifier)
        .map_err(|_| "The protected Google consent verifier could not be saved.".to_string())?;
    {
        let mut values = runtime().lock().map_err(user_error)?;
        if values.len() >= 128 && !values.contains_key(&context.key) {
            values.retain(|_, value| value.attempt_id.is_some());
        }
        let item = values.entry(context.key.clone()).or_default();
        item.attempt_id = Some(attempt.clone());
        item.deadline = Some(Instant::now() + AUTH_LIFETIME);
        item.detail =
            Some("Waiting for read-only Google Cloud authorization in your browser.".into());
    }
    state
        .oauth_attempts
        .lock()
        .map_err(user_error)?
        .insert(attempt.clone(), OAuthAttemptKind::CloudQuota);
    let callback_state = state.inner().clone();
    let callback_attempt = attempt.clone();
    spawn_worker(move || {
        await_callback(
            callback_state,
            listener,
            context,
            redirect,
            callback_attempt,
        )
    });
    Ok(OAuthStart {
        authorization_url,
        attempt_id: attempt,
    })
}

#[tauri::command]
pub(super) async fn cancel_live_quota_authorization(
    expected_channel_id: String,
    attempt_id: String,
    state: State<'_, AppState>,
) -> Result<LiveQuotaStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let context = BoundContext::capture(&state, &expected_channel_id)?;
        {
            let _credentials = oauth_replacement::lock_credentials()?;
            let mut values = runtime().lock().map_err(user_error)?;
            let item = values
                .get_mut(&context.key)
                .ok_or("This Cloud authorization is no longer pending.")?;
            if item.attempt_id.as_deref() != Some(attempt_id.as_str()) {
                return Err("This Cloud authorization is no longer pending.".into());
            }
            item.attempt_id = None;
            item.deadline = None;
            item.detail = Some("Read-only Cloud authorization was cancelled.".into());
            cleanup_attempt(&state, &attempt_id);
        }
        status_impl(&state, &expected_channel_id)
    })
    .await
    .map_err(|_| "Cloud authorization could not be cancelled.".to_string())?
}

fn callback_live(state: &AppState, context: &BoundContext, attempt: &str) -> bool {
    context.current(state).is_ok()
        && state
            .oauth_attempts
            .lock()
            .ok()
            .is_some_and(|values| values.get(attempt) == Some(&OAuthAttemptKind::CloudQuota))
        && runtime().lock().ok().is_some_and(|values| {
            values.get(&context.key).is_some_and(|item| {
                item.attempt_id.as_deref() == Some(attempt)
                    && item
                        .deadline
                        .is_some_and(|deadline| Instant::now() < deadline)
            })
        })
}

fn finish_attempt(state: &AppState, context: &BoundContext, attempt: &str, detail: &str) {
    if let Ok(_credentials) = oauth_replacement::lock_credentials() {
        if let Ok(mut values) = runtime().lock() {
            if let Some(item) = values
                .get_mut(&context.key)
                .filter(|item| item.attempt_id.as_deref() == Some(attempt))
            {
                item.attempt_id = None;
                item.deadline = None;
                item.detail = Some(detail.into());
            }
        }
        cleanup_attempt(state, attempt);
    }
}

fn await_callback(
    state: AppState,
    listener: TcpListener,
    context: Arc<BoundContext>,
    redirect: String,
    attempt: String,
) {
    let deadline = Instant::now() + AUTH_LIFETIME;
    let _ = listener.set_nonblocking(true);
    while Instant::now() < deadline && callback_live(&state, &context, &attempt) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut bytes = [0u8; 8192];
                let count = stream.read(&mut bytes).unwrap_or(0);
                let text = String::from_utf8_lossy(&bytes[..count]);
                let mut parts = text.lines().next().unwrap_or("").split_whitespace();
                let method = parts.next();
                let target = parts.next().unwrap_or("");
                if !valid_oauth_callback_request(method, target, &attempt) {
                    respond_to_callback(&mut stream, "Google authorization did not complete.");
                    continue;
                }
                let result = if let Some(error) = callback_value(target, "error") {
                    Err(oauth_error_message(&error))
                } else if let Some(code) = callback_value(target, "code") {
                    exchange_consent(&state, &context, &redirect, &attempt, &code)
                } else {
                    Err("Google did not return an authorization code.".into())
                };
                let detail = match result {
                    Ok(()) => "Read-only Cloud access is connected. Refresh to request current reported quota.".into(),
                    Err(error) => error,
                };
                finish_attempt(&state, &context, &attempt, &detail);
                respond_to_callback(
                    &mut stream,
                    "Return to YouTube Upload Manager for Cloud quota status.",
                );
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(125))
            }
            Err(_) => break,
        }
    }
    finish_attempt(
        &state,
        &context,
        &attempt,
        "Google Cloud authorization expired or the selected connection changed. Retry when ready.",
    );
}

fn exchange_consent(
    state: &AppState,
    context: &BoundContext,
    redirect: &str,
    attempt: &str,
    code: &str,
) -> Result<(), String> {
    let _operation = operation_lock().lock().map_err(user_error)?;
    if !callback_live(state, context, attempt) {
        return Err("This Google Cloud authorization was cancelled or expired.".into());
    }
    let verifier = oauth_verifier_entry(attempt)?
        .get_password()
        .map_err(|_| "The protected Google consent verifier is unavailable.".to_string())?;
    let secret = configured_oauth_client_secret(state)?;
    let mut form = vec![
        ("code", code),
        ("client_id", context.binding.client_id.as_str()),
        ("code_verifier", verifier.as_str()),
        ("redirect_uri", redirect),
        ("grant_type", "authorization_code"),
    ];
    if let Some(secret) = secret.as_deref() {
        form.push(("client_secret", secret));
    }
    let response = provider_transport(state)
        .control_client()?
        .post("https://oauth2.googleapis.com/token")
        .form(&form)
        .send()
        .map_err(|_| {
            "Google Cloud authorization could not reach Google's token server.".to_string()
        })?;
    let success = response.status().is_success();
    let payload = bounded_json(response)?;
    if !success {
        return Err(token_error(&payload));
    }
    let grant = parse_grant(&payload, None, Utc::now())?;
    let _credentials = oauth_replacement::lock_credentials()?;
    if !callback_live(state, context, attempt) {
        return Err("This Google Cloud authorization was cancelled or expired.".into());
    }
    context.current(state)?;
    save_grant(context, &grant)?;
    // Promotion and attempt completion share the cancellation guard. A late
    // Cancel cannot report success after this grant has already been committed.
    if let Some(item) = runtime().lock().map_err(user_error)?.get_mut(&context.key) {
        item.attempt_id = None;
        item.deadline = None;
        item.detail = Some(
            "Read-only Cloud access is connected. Refresh to request current reported quota."
                .into(),
        );
    }
    cleanup_attempt(state, attempt);
    Ok(())
}

fn token_error(payload: &Value) -> String {
    match payload.get("error").and_then(Value::as_str) {
        Some("invalid_grant") => "Google rejected the saved Cloud authorization. Authorize read-only Cloud access again.",
        Some("invalid_client" | "unauthorized_client" | "deleted_client") => "Google rejected this Desktop OAuth client. Check its Cloud project configuration.",
        _ => "Google did not authorize read-only Cloud quota access. Check the OAuth consent setup and try again.",
    }.into()
}

fn parse_grant(
    payload: &Value,
    previous: Option<&ProtectedGrant>,
    at: DateTime<Utc>,
) -> Result<ProtectedGrant, String> {
    let scope = payload.get("scope").and_then(Value::as_str).or_else(|| previous.map(|grant| grant.scopes.as_str()))
        .filter(|scope| has_scopes(scope)).ok_or("Google did not grant both read-only Cloud quota and monitoring permissions. Authorize again and allow both permissions.")?;
    let access = payload
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 16 * 1024)
        .ok_or("Google did not return a valid Cloud access token.")?;
    let refresh = payload
        .get("refresh_token")
        .and_then(Value::as_str)
        .or_else(|| previous.map(|grant| grant.refresh_token.as_str()))
        .filter(|value| !value.is_empty() && value.len() <= 16 * 1024)
        .ok_or("Google did not return a Cloud refresh token. Retry the consent step.")?;
    let seconds = payload
        .get("expires_in")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0 && *value <= 86_400)
        .ok_or("Google did not return a valid Cloud token lifetime.")?;
    Ok(ProtectedGrant {
        generation: previous
            .map(|grant| grant.generation.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        refresh_token: refresh.into(),
        access_token: access.into(),
        expires_at: (at + chrono::Duration::seconds(seconds as i64)).to_rfc3339(),
        scopes: scope.into(),
        project_number: previous.and_then(|grant| grant.project_number.clone()),
    })
}

fn access_token(state: &AppState, context: &BoundContext) -> Result<String, String> {
    context.current(state)?;
    let grant = load_grant(context)?
        .ok_or("Connect read-only Google Cloud access before refreshing quota.")?;
    if DateTime::parse_from_rfc3339(&grant.expires_at)
        .ok()
        .is_some_and(|expires| expires > Utc::now() + chrono::Duration::seconds(60))
    {
        return Ok(grant.access_token);
    }
    let secret = configured_oauth_client_secret(state)?;
    let mut form = vec![
        ("client_id", context.binding.client_id.as_str()),
        ("refresh_token", grant.refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ];
    if let Some(secret) = secret.as_deref() {
        form.push(("client_secret", secret));
    }
    let response = provider_transport(state)
        .control_client()?
        .post("https://oauth2.googleapis.com/token")
        .form(&form)
        .send()
        .map_err(|_| "Google's Cloud token server could not be reached.".to_string())?;
    let success = response.status().is_success();
    let payload = bounded_json(response)?;
    if !success {
        return Err(token_error(&payload));
    }
    let renewed = parse_grant(&payload, Some(&grant), Utc::now())?;
    let _credentials = oauth_replacement::lock_credentials()?;
    context.current(state)?;
    if load_grant(context)?.as_ref().map(|value| &value.generation) != Some(&grant.generation) {
        return Err("The read-only Cloud connection changed. Refresh again.".into());
    }
    save_grant(context, &renewed)?;
    Ok(renewed.access_token)
}

fn bounded_json(response: reqwest::blocking::Response) -> Result<Value, String> {
    let mut bytes = Vec::new();
    response
        .take(MAX_BODY + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Google returned an unreadable Cloud quota response.".to_string())?;
    if bytes.len() as u64 > MAX_BODY {
        return Err("Google's Cloud quota response exceeded the safe size limit.".into());
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| "Google returned an unreadable Cloud quota response.".into())
}

fn cloud_error(status: u16, service: &str) -> String {
    match status {
        401 => "Google rejected Cloud authorization. Authorize read-only Cloud access again.".into(),
        403 if service == "Cloud Resource Manager" => "Google denied Cloud project lookup. Enable the Cloud Resource Manager API and give this Google account Monitoring Viewer (or equivalent resourcemanager.projects.get permission) for this project, then retry.".into(),
        403 => format!("Google denied {service} access. Enable the Service Usage and Cloud Monitoring APIs in this project and give this Google account Monitoring Viewer plus Service Usage Viewer (or equivalent quota and monitoring read permissions)."),
        404 => "Google could not find this Cloud project or its YouTube quota. Check the project in the imported OAuth JSON.".into(),
        429 => "Google is rate limiting Cloud quota reads. Wait before refreshing again.".into(),
        _ => "Google Cloud quota reporting is temporarily unavailable. Retry the manual refresh later.".into(),
    }
}

fn valid_project_number(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && !value.starts_with('0')
}

fn resolved_project_number(payload: &Value, expected_project: &str) -> Result<String, String> {
    if payload.get("projectId").and_then(Value::as_str) != Some(expected_project) {
        return Err(
            "Google returned a different Cloud project. No quota data was requested for it.".into(),
        );
    }
    if payload.get("lifecycleState").and_then(Value::as_str) != Some("ACTIVE") {
        return Err(
            "Google reports that this Cloud project is not active. Quota data is unavailable."
                .into(),
        );
    }
    let number = payload.get("projectNumber").and_then(Value::as_str)
        .filter(|value| valid_project_number(value))
        .ok_or("Google did not return a valid numeric Cloud project identity. Quota data is unavailable.")?;
    Ok(number.into())
}

fn project_number(state: &AppState, context: &BoundContext, token: &str) -> Result<String, String> {
    context.current(state)?;
    let grant = load_grant(context)?
        .ok_or("Connect read-only Google Cloud access before refreshing quota.")?;
    if let Some(number) = grant
        .project_number
        .as_deref()
        .filter(|number| valid_project_number(number))
    {
        return Ok(number.into());
    }
    let response = provider_transport(state)
        .control_client()?
        .get(format!(
            "https://cloudresourcemanager.googleapis.com/v1/projects/{}",
            context.project
        ))
        .bearer_auth(token)
        .send()
        .map_err(|_| {
            "Google Cloud Resource Manager could not be reached to verify this project.".to_string()
        })?;
    if !response.status().is_success() {
        return Err(cloud_error(
            response.status().as_u16(),
            "Cloud Resource Manager",
        ));
    }
    let number = resolved_project_number(&bounded_json(response)?, &context.project)?;
    let _credentials = oauth_replacement::lock_credentials()?;
    context.current(state)?;
    let mut current =
        load_grant(context)?.ok_or("The read-only Cloud connection changed. Refresh again.")?;
    if current.generation != grant.generation {
        return Err("The read-only Cloud connection changed. Refresh again.".into());
    }
    current.project_number = Some(number.clone());
    save_grant(context, &current)?;
    Ok(number)
}

fn fetch_pages(
    mut fetch: impl FnMut(&str) -> Result<Value, String>,
    collection: &str,
) -> Result<Vec<Value>, String> {
    let mut token = String::new();
    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut records = Vec::new();
    for _ in 0..MAX_PAGES {
        let payload = fetch(&token)?;
        total = total.saturating_add(serde_json::to_vec(&payload).map_err(user_error)?.len());
        if total > MAX_TOTAL_BODY {
            return Err("Google's Cloud quota result exceeded the safe size limit.".into());
        }
        for incomplete in ["executionErrors", "unreachable"] {
            if let Some(value) = payload.get(incomplete) {
                let values = value.as_array().ok_or("Google returned malformed Monitoring completeness data. Quota remaining is unavailable.")?;
                if !values.is_empty() {
                    return Err("Google returned incomplete Monitoring data. Quota remaining is unavailable; retry later.".into());
                }
            }
        }
        if let Some(values) = payload.get(collection) {
            let values = values
                .as_array()
                .ok_or("Google returned malformed Cloud quota data.")?;
            if records.len().saturating_add(values.len()) > 2_000 {
                return Err("Google returned too many Cloud quota series.".into());
            }
            records.extend(values.iter().cloned());
        }
        token = match payload.get("nextPageToken") {
            None => String::new(),
            Some(value) => value
                .as_str()
                .filter(|value| value.len() <= 4096)
                .ok_or("Google returned an invalid Cloud pagination token.")?
                .into(),
        };
        if token.is_empty() {
            return Ok(records);
        }
        if !seen.insert(token.clone()) {
            return Err("Google repeated a Cloud quota page. Refresh later.".into());
        }
    }
    Err("Google returned too many Cloud quota pages. Quota remaining is unavailable.".into())
}

fn daily_unit(unit: &str) -> bool {
    matches!(unit, "1/d/{project}" | "1/day/{project}")
}

fn quota_integer(value: &Value) -> Option<u64> {
    value
        .as_str()
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| value.as_u64())
        .filter(|value| *value <= MAX_SAFE_INTEGER)
}

fn parse_limits(metrics: &[Value]) -> Result<Vec<LiveQuotaLimit>, String> {
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for metric in metrics {
        let name = metric
            .get("metric")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 512)
            .ok_or("Google returned an invalid quota metric identity.")?;
        let display = metric
            .get("displayName")
            .and_then(Value::as_str)
            .filter(|value| value.len() <= 1024)
            .unwrap_or(name);
        let limits = metric
            .get("consumerQuotaLimits")
            .and_then(Value::as_array)
            .ok_or("Google did not return quota limits.")?;
        for limit in limits {
            let unit = limit
                .get("unit")
                .and_then(Value::as_str)
                .filter(|value| value.len() <= 256)
                .ok_or("Google returned an invalid quota unit.")?;
            if !unit.contains("/d/") && !unit.contains("/day/") {
                continue;
            }
            let buckets = limit
                .get("quotaBuckets")
                .and_then(Value::as_array)
                .ok_or("Google did not return effective quota buckets.")?;
            for bucket in buckets {
                let dimensions = match bucket.get("dimensions") {
                    None => BTreeMap::new(),
                    Some(value) => value
                        .as_object()
                        .ok_or("Google returned invalid quota dimensions.")?
                        .iter()
                        .map(|(key, value)| {
                            value
                                .as_str()
                                .filter(|value| key.len() <= 256 && value.len() <= 256)
                                .map(|value| (key.clone(), value.into()))
                                .ok_or("Google returned invalid quota dimensions.")
                        })
                        .collect::<Result<BTreeMap<String, String>, _>>()?,
                };
                if !seen.insert((name.to_string(), unit.to_string(), dimensions.clone())) {
                    return Err(
                        "Google repeated a daily quota bucket. Quota remaining is unavailable."
                            .into(),
                    );
                }
                if rows.len() >= 256 {
                    return Err("Google returned too many daily quota buckets.".into());
                }
                let effective_limit = bucket.get("effectiveLimit").and_then(quota_integer);
                let state = if !daily_unit(unit) || !dimensions.is_empty() {
                    "unsupported_bucket"
                } else if effective_limit.is_none() {
                    "limit_unavailable"
                } else {
                    "no_usage_data"
                };
                rows.push(LiveQuotaLimit {
                    metric: name.into(),
                    display_name: display.into(),
                    unit: unit.into(),
                    dimensions,
                    effective_limit,
                    used: None,
                    remaining: None,
                    sample_through: None,
                    state: state.into(),
                });
            }
        }
    }
    Ok(rows)
}

#[derive(Default)]
struct Usage {
    count: u64,
    through: Option<DateTime<Utc>>,
    points: usize,
}

fn apply_usage(
    rows: &mut [LiveQuotaLimit],
    series: &[Value],
    project: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<(), String> {
    let mut usage: HashMap<String, Usage> = HashMap::new();
    let mut incomplete_metrics = HashSet::new();
    let mut seen_intervals: HashMap<String, HashSet<(DateTime<Utc>, DateTime<Utc>)>> =
        HashMap::new();
    let mut point_count = 0usize;
    for item in series {
        let resource = &item["resource"];
        if resource["type"].as_str() != Some("consumer_quota")
            || resource["labels"]["project_id"].as_str() != Some(project)
            || resource["labels"]["service"].as_str() != Some("youtube.googleapis.com")
        {
            return Err("Google returned quota data for another project or service. No remaining amount was calculated.".into());
        }
        if resource["labels"]["location"].as_str() != Some("global") {
            continue;
        }
        if item["metric"]["type"].as_str()
            != Some("serviceruntime.googleapis.com/quota/rate/net_usage")
            || item["metricKind"].as_str() != Some("DELTA")
            || item["valueType"].as_str() != Some("INT64")
        {
            return Err("Google returned unsupported quota measurements. No remaining amount was calculated.".into());
        }
        let metric = item["metric"]["labels"]["quota_metric"]
            .as_str()
            .ok_or("Google omitted the quota metric identity.")?;
        if !rows
            .iter()
            .any(|row| row.metric == metric && row.state == "no_usage_data")
        {
            continue;
        }
        let identity = serde_json::to_string(&(resource, &item["metric"])).map_err(user_error)?;
        let points = item["points"]
            .as_array()
            .ok_or("Google omitted quota measurements.")?;
        // FULL pagination may split one series across pages. Its intervals
        // remain unique across the entire result, rather than just one page.
        let intervals = seen_intervals.entry(identity).or_default();
        for point in points {
            point_count += 1;
            if point_count > 200_000 {
                return Err("Google returned too many quota measurements.".into());
            }
            let parse_time = |field| {
                point["interval"][field]
                    .as_str()
                    .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.with_timezone(&Utc))
            };
            let point_start = parse_time("startTime")
                .ok_or("Google returned an invalid quota measurement start time.")?;
            let point_end = parse_time("endTime")
                .ok_or("Google returned an invalid quota measurement end time.")?;
            if point_start >= point_end || point_end > end {
                return Err("Google returned an invalid quota measurement interval.".into());
            }
            // A DELTA interval crossing midnight cannot be apportioned safely.
            if point_start < start || point_end <= start {
                if point_end > start {
                    incomplete_metrics.insert(metric.to_string());
                }
                continue;
            }
            if !intervals.insert((point_start, point_end)) {
                return Err("Google repeated a quota measurement interval.".into());
            }
            let count = quota_integer(&point["value"]["int64Value"])
                .ok_or("Google returned an invalid quota usage amount.")?;
            let value = usage.entry(metric.into()).or_default();
            value.count = value
                .count
                .checked_add(count)
                .filter(|value| *value <= MAX_SAFE_INTEGER)
                .ok_or("Google quota usage exceeded the supported count range.")?;
            value.through = Some(
                value
                    .through
                    .map_or(point_end, |current| current.max(point_end)),
            );
            value.points += 1;
        }
    }
    for intervals in seen_intervals.values() {
        let mut sorted: Vec<_> = intervals.iter().collect();
        sorted.sort_unstable();
        if sorted.windows(2).any(|pair| pair[0].1 > pair[1].0) {
            return Err("Google returned overlapping quota measurements. No remaining amount was calculated.".into());
        }
    }
    for row in rows.iter_mut().filter(|row| row.state == "no_usage_data") {
        if incomplete_metrics.contains(&row.metric) {
            continue;
        }
        if let Some(value) = usage.get(&row.metric).filter(|value| value.points > 0) {
            row.used = Some(value.count);
            row.remaining = row
                .effective_limit
                .map(|limit| limit.saturating_sub(value.count));
            row.sample_through = value.through.map(|value| value.to_rfc3339());
            row.state = "reported".into();
        }
    }
    Ok(())
}

fn pacific_start(at: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let date = at.with_timezone(&chrono_tz::US::Pacific).date_naive();
    chrono_tz::US::Pacific
        .from_local_datetime(
            &date
                .and_hms_opt(0, 0, 0)
                .ok_or("The Pacific quota date is invalid.")?,
        )
        .single()
        .map(|value| value.with_timezone(&Utc))
        .ok_or("The Pacific quota window is unavailable.".into())
}

fn refresh_impl(state: &AppState, expected: &str) -> Result<LiveQuotaStatus, String> {
    let _operation = operation_lock().lock().map_err(user_error)?;
    let context = BoundContext::capture(state, expected)?;
    let token = access_token(state, &context)?;
    let project_number = project_number(state, &context, &token)?;
    let end = Utc::now();
    let start = pacific_start(end)?;
    let client = provider_transport(state).control_client()?;
    let quota_url = format!("https://serviceusage.googleapis.com/v1beta1/projects/{project_number}/services/youtube.googleapis.com/consumerQuotaMetrics");
    let quota = fetch_pages(
        |page| {
            context.current(state)?;
            let response = client
                .get(&quota_url)
                .query(&[("view", "FULL"), ("pageSize", "100"), ("pageToken", page)])
                .bearer_auth(&token)
                .send()
                .map_err(|_| "Google Service Usage could not be reached.".to_string())?;
            if !response.status().is_success() {
                return Err(cloud_error(response.status().as_u16(), "Service Usage"));
            }
            let result = bounded_json(response)?;
            context.current(state)?;
            Ok(result)
        },
        "metrics",
    )?;
    let mut limits = parse_limits(&quota)?;
    let filter = format!("metric.type=\"serviceruntime.googleapis.com/quota/rate/net_usage\" AND resource.type=\"consumer_quota\" AND resource.labels.service=\"youtube.googleapis.com\" AND resource.labels.project_id=\"{}\"",context.project);
    let monitoring_url = format!(
        "https://monitoring.googleapis.com/v3/projects/{}/timeSeries",
        context.project
    );
    let start_text = start.to_rfc3339();
    let end_text = end.to_rfc3339();
    let series = fetch_pages(
        |page| {
            context.current(state)?;
            let response = client
                .get(&monitoring_url)
                .query(&[
                    ("filter", filter.as_str()),
                    ("interval.startTime", start_text.as_str()),
                    ("interval.endTime", end_text.as_str()),
                    ("view", "FULL"),
                    ("pageSize", "10000"),
                    ("pageToken", page),
                ])
                .bearer_auth(&token)
                .send()
                .map_err(|_| "Google Cloud Monitoring could not be reached.".to_string())?;
            if !response.status().is_success() {
                return Err(cloud_error(response.status().as_u16(), "Cloud Monitoring"));
            }
            let result = bounded_json(response)?;
            context.current(state)?;
            Ok(result)
        },
        "timeSeries",
    )?;
    apply_usage(&mut limits, &series, &context.project, start, end)?;
    context.current(state)?;
    let snapshot = LiveQuotaSnapshot {
        fetched_at: now(),
        period_start: start_text,
        period_end: end_text,
        metric_lag_seconds: METRIC_LAG_SECONDS,
        limits,
    };
    let mut values = runtime().lock().map_err(user_error)?;
    let item = values.entry(context.key).or_default();
    item.snapshot = Some(snapshot);
    item.detail = Some("Fetched effective limits and reported quota consumption directly from Google Cloud. Monitoring is delayed; remaining reflects reported usage, not an instantaneous guarantee.".into());
    drop(values);
    status_impl(state, expected)
}

#[tauri::command]
pub(super) async fn refresh_live_quota(
    expected_channel_id: String,
    state: State<'_, AppState>,
) -> Result<LiveQuotaStatus, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || refresh_impl(&state, &expected_channel_id))
        .await
        .map_err(|_| "Google Cloud quota refresh stopped unexpectedly.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }
    fn limits() -> Vec<LiveQuotaLimit> {
        parse_limits(&[json!({"metric":"youtube.googleapis.com/default","displayName":"Daily units","consumerQuotaLimits":[
            {"unit":"1/min/{project}","quotaBuckets":[{"effectiveLimit":"100"}]},
            {"unit":"1/d/{project}","quotaBuckets":[{"effectiveLimit":"10000"}]}
        ]})]).unwrap()
    }
    fn series(method: &str, count: &str) -> Value {
        json!({"resource":{"type":"consumer_quota","labels":{"project_id":"project-a","service":"youtube.googleapis.com","location":"global"}},
            "metric":{"type":"serviceruntime.googleapis.com/quota/rate/net_usage","labels":{"quota_metric":"youtube.googleapis.com/default","method":method}},
            "metricKind":"DELTA","valueType":"INT64","points":[{"interval":{"startTime":"2026-10-19T07:00:00Z","endTime":"2026-10-19T07:01:00Z"},"value":{"int64Value":count}}]})
    }
    #[test]
    fn live_quota_daily_limits_and_delta_methods_produce_reported_remaining() {
        let mut rows = limits();
        assert_eq!(rows.len(), 1);
        apply_usage(
            &mut rows,
            &[series("list", "9958"), series("insert", "2")],
            "project-a",
            at("2026-10-19T07:00:00Z"),
            at("2026-10-19T07:05:00Z"),
        )
        .unwrap();
        assert_eq!((rows[0].used, rows[0].remaining), (Some(9960), Some(40)));
        assert_eq!(
            rows[0].sample_through.as_deref(),
            Some("2026-10-19T07:01:00+00:00")
        );
    }
    #[test]
    fn live_quota_no_series_is_unknown_but_actual_zero_is_reported() {
        let mut rows = limits();
        let start = at("2026-10-19T07:00:00Z");
        let end = at("2026-10-19T07:05:00Z");
        apply_usage(&mut rows, &[], "project-a", start, end).unwrap();
        assert_eq!((rows[0].used, rows[0].remaining), (None, None));
        assert_eq!(rows[0].state, "no_usage_data");
        apply_usage(&mut rows, &[series("list", "0")], "project-a", start, end).unwrap();
        assert_eq!((rows[0].used, rows[0].remaining), (Some(0), Some(10000)));
    }
    #[test]
    fn live_quota_foreign_project_gauge_duplicate_and_invalid_measurements_fail_closed() {
        let original = series("list", "10");
        let mut foreign = original.clone();
        foreign["resource"]["labels"]["project_id"] = json!("project-b");
        let mut gauge = original.clone();
        gauge["metricKind"] = json!("GAUGE");
        let mut negative = original.clone();
        negative["points"][0]["value"]["int64Value"] = json!("-1");
        for series in [
            vec![foreign],
            vec![gauge],
            vec![negative],
            vec![original.clone(), original],
        ] {
            let mut rows = limits();
            assert!(apply_usage(
                &mut rows,
                &series,
                "project-a",
                at("2026-10-19T07:00:00Z"),
                at("2026-10-19T07:05:00Z")
            )
            .is_err());
            assert_eq!(rows[0].remaining, None);
        }
    }
    #[test]
    fn live_quota_dimensioned_and_unlimited_buckets_remain_unknown() {
        let rows = parse_limits(&[
            json!({"metric":"metric","consumerQuotaLimits":[{"unit":"1/d/{project}","quotaBuckets":[
                {"effectiveLimit":"-1"},{"effectiveLimit":"100","dimensions":{"region":"us-east1"}}
            ]}]}),
        ])
        .unwrap();
        assert_eq!(rows[0].state, "limit_unavailable");
        assert_eq!(rows[1].state, "unsupported_bucket");
        assert!(rows.iter().all(|row| row.remaining.is_none()));
    }
    #[test]
    fn live_quota_midnight_crossing_is_not_apportioned_or_invented() {
        let mut item = series("list", "100");
        item["points"][0]["interval"]["startTime"] = json!("2026-10-19T06:59:00Z");
        let mut rows = limits();
        apply_usage(
            &mut rows,
            &[item],
            "project-a",
            at("2026-10-19T07:00:00Z"),
            at("2026-10-19T07:05:00Z"),
        )
        .unwrap();
        assert_eq!((rows[0].used, rows[0].remaining), (None, None));
    }
    #[test]
    fn live_quota_full_pagination_can_split_one_series_without_double_counting() {
        let first = series("list", "40");
        let mut second = series("list", "2");
        second["points"][0]["interval"]["startTime"] = json!("2026-10-19T07:01:00Z");
        second["points"][0]["interval"]["endTime"] = json!("2026-10-19T07:02:00Z");
        let mut rows = limits();
        apply_usage(
            &mut rows,
            &[first, second],
            "project-a",
            at("2026-10-19T07:00:00Z"),
            at("2026-10-19T07:05:00Z"),
        )
        .unwrap();
        assert_eq!((rows[0].used, rows[0].remaining), (Some(42), Some(9958)));
    }

    #[test]
    fn live_quota_partial_midnight_and_overlapping_intervals_do_not_inflate_remaining() {
        let mut crossing = series("list", "100");
        crossing["points"][0]["interval"]["startTime"] = json!("2026-10-19T06:59:00Z");
        let first = series("list", "40");
        let mut rows = limits();
        apply_usage(
            &mut rows,
            &[crossing, first.clone()],
            "project-a",
            at("2026-10-19T07:00:00Z"),
            at("2026-10-19T07:05:00Z"),
        )
        .unwrap();
        assert_eq!(rows[0].remaining, None);
        let mut overlap = series("list", "2");
        overlap["points"][0]["interval"]["startTime"] = json!("2026-10-19T07:00:30Z");
        overlap["points"][0]["interval"]["endTime"] = json!("2026-10-19T07:02:00Z");
        assert!(apply_usage(
            &mut limits(),
            &[first, overlap],
            "project-a",
            at("2026-10-19T07:00:00Z"),
            at("2026-10-19T07:05:00Z")
        )
        .is_err());
    }
    #[test]
    fn live_quota_pagination_preserves_pages_and_rejects_cycles_partial_errors() {
        let mut pages = vec![
            json!({"metrics":[1],"nextPageToken":"next"}),
            json!({"metrics":[2]}),
        ]
        .into_iter();
        assert_eq!(
            fetch_pages(|_| Ok(pages.next().unwrap()), "metrics").unwrap(),
            vec![json!(1), json!(2)]
        );
        assert!(fetch_pages(|_| Ok(json!({"nextPageToken":"same"})), "metrics").is_err());
        assert!(fetch_pages(|_| Ok(json!({"executionErrors":[{}]})), "timeSeries").is_err());
        assert!(fetch_pages(|_| Ok(json!({"unreachable":["global"]})), "timeSeries").is_err());
        assert!(fetch_pages(|_| Ok(json!({"executionErrors":"malformed"})), "timeSeries").is_err());
        assert!(fetch_pages(|_| Ok(json!({"unreachable":false})), "timeSeries").is_err());
    }
    #[test]
    fn live_quota_scope_validation_and_refresh_preserve_verified_dedicated_grant() {
        let payload = json!({"scope":SCOPES,"access_token":"access","refresh_token":"refresh","expires_in":3600});
        let mut grant = parse_grant(&payload, None, at("2026-10-19T07:00:00Z")).unwrap();
        grant.project_number = Some("123456789012".into());
        assert!(!has_scopes(
            "https://www.googleapis.com/auth/youtube.readonly"
        ));
        assert!(parse_grant(&json!({"scope":"https://www.googleapis.com/auth/monitoring.read","access_token":"access","refresh_token":"refresh","expires_in":3600}),None,Utc::now()).is_err());
        assert!(parse_grant(
            &json!({"access_token":"access","refresh_token":"refresh","expires_in":3600}),
            None,
            Utc::now()
        )
        .is_err());
        let renewed = parse_grant(
            &json!({"access_token":"new","expires_in":3600}),
            Some(&grant),
            Utc::now(),
        )
        .unwrap();
        assert_eq!(renewed.generation, grant.generation);
        assert_eq!(renewed.refresh_token, "refresh");
        assert_eq!(renewed.access_token, "new");
        assert_eq!(renewed.project_number.as_deref(), Some("123456789012"));
    }
    #[test]
    fn live_quota_pacific_day_handles_standard_summer_and_dst_reset() {
        assert_eq!(
            pacific_start(at("2026-10-19T12:00:00Z")).unwrap(),
            at("2026-10-19T07:00:00Z")
        );
        assert_eq!(
            pacific_start(at("2026-01-19T12:00:00Z")).unwrap(),
            at("2026-01-19T08:00:00Z")
        );
        assert_eq!(
            pacific_start(at("2026-11-01T12:00:00Z")).unwrap(),
            at("2026-11-01T07:00:00Z")
        );
    }
    #[test]
    fn live_quota_credentials_are_scoped_by_device_database_and_binding() {
        assert_ne!(
            scoped_key(Path::new("one.sqlite3"), "binding"),
            scoped_key(Path::new("two.sqlite3"), "binding")
        );
        assert_ne!(
            scoped_key(Path::new("one.sqlite3"), "binding-a"),
            scoped_key(Path::new("one.sqlite3"), "binding-b")
        );
        assert!(!cloud_error(403, "Service Usage").contains("access_token"));
        assert!(!valid_project("project\" OR true"));
    }

    #[test]
    fn live_quota_project_lookup_requires_exact_active_project_and_numeric_number() {
        let payload = json!({"projectId":"project-a","projectNumber":"123456789012","lifecycleState":"ACTIVE"});
        assert_eq!(
            resolved_project_number(&payload, "project-a").unwrap(),
            "123456789012"
        );
        assert!(resolved_project_number(&payload, "project-b").is_err());
        let mut inactive = payload.clone();
        inactive["lifecycleState"] = json!("DELETE_REQUESTED");
        assert!(resolved_project_number(&inactive, "project-a").is_err());
        for number in [
            "",
            "0",
            "0123",
            "project-a",
            "123/other",
            "-123",
            "123\" OR true",
        ] {
            let mut malformed = payload.clone();
            malformed["projectNumber"] = json!(number);
            assert!(resolved_project_number(&malformed, "project-a").is_err());
        }
        assert!(resolved_project_number(
            &json!({"projectId":"project-a","lifecycleState":"ACTIVE"}),
            "project-a"
        )
        .is_err());
    }

    #[test]
    fn live_quota_legacy_protected_grant_resolves_project_number_without_losing_tokens() {
        let legacy = json!({"generation":"grant-one","access_token":"access","refresh_token":"refresh",
            "expires_at":"2026-10-19T09:00:00Z","scopes":SCOPES});
        let grant: ProtectedGrant = serde_json::from_value(legacy).unwrap();
        assert_eq!(grant.project_number, None);
        assert_eq!(grant.refresh_token, "refresh");
        assert!(cloud_error(403, "Cloud Resource Manager").contains("resourcemanager.projects.get"));
        assert!(cloud_error(403, "Cloud Resource Manager").contains("Cloud Resource Manager API"));
    }

    #[test]
    fn live_quota_authorization_fences_attempt_kind_expiry_channel_and_project() {
        let root = std::env::temp_dir().join(format!("youtube-live-quota-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,oauth_client_id,oauth_credential_id,oauth_project_id,active_channel,active_channel_id,connection_detail,updated_at) VALUES(1,'fixture-client','fixture-bundle','project-a','Channel A','channel-a','Existing detail',?1) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id='fixture-client',oauth_credential_id='fixture-bundle',oauth_project_id='project-a',active_channel='Channel A',active_channel_id='channel-a'", [now()]).unwrap();
        let context = BoundContext::capture(&state, "channel-a").unwrap();
        assert!(BoundContext::capture(&state, "channel-b").is_err());
        let attempt = "synthetic-live-quota-attempt";
        state
            .oauth_attempts
            .lock()
            .unwrap()
            .insert(attempt.into(), OAuthAttemptKind::CloudQuota);
        runtime().lock().unwrap().insert(
            context.key.clone(),
            Runtime {
                attempt_id: Some(attempt.into()),
                deadline: Some(Instant::now() + AUTH_LIFETIME),
                ..Runtime::default()
            },
        );
        assert!(callback_live(&state, &context, attempt));
        state
            .oauth_attempts
            .lock()
            .unwrap()
            .insert(attempt.into(), OAuthAttemptKind::Connection);
        assert!(!callback_live(&state, &context, attempt));
        state
            .oauth_attempts
            .lock()
            .unwrap()
            .insert(attempt.into(), OAuthAttemptKind::CloudQuota);
        runtime()
            .lock()
            .unwrap()
            .get_mut(&context.key)
            .unwrap()
            .deadline = Some(Instant::now() - Duration::from_secs(1));
        assert!(!callback_live(&state, &context, attempt));
        runtime()
            .lock()
            .unwrap()
            .get_mut(&context.key)
            .unwrap()
            .deadline = Some(Instant::now() + AUTH_LIFETIME);
        connection
            .execute(
                "UPDATE connection_settings SET oauth_project_id='project-b' WHERE singleton=1",
                [],
            )
            .unwrap();
        assert!(!callback_live(&state, &context, attempt));
        assert_ne!(
            context.key,
            BoundContext::capture(&state, "channel-a").unwrap().key
        );
        connection.execute("UPDATE connection_settings SET oauth_project_id='project-a', active_channel_id='channel-b' WHERE singleton=1", []).unwrap();
        assert!(!callback_live(&state, &context, attempt));
        runtime().lock().unwrap().remove(&context.key);
        drop(connection);
        assert!(root.is_absolute());
        assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("youtube-live-quota-"));
        fs::remove_dir_all(root).unwrap();
    }
}
