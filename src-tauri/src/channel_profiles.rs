//! Device-local account profiles. Credentials are kept in independent protected
//! bundles; saved jobs keep their original channel identity when selection changes.
use super::*;

const MAX_PROFILES: i64 = 64;
const REVIEW_LIFETIME: Duration = Duration::from_secs(20 * 60);

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS channel_profiles (
  profile_id TEXT PRIMARY KEY NOT NULL,
  channel_id TEXT UNIQUE NOT NULL,
  channel_name TEXT NOT NULL,
  oauth_client_id TEXT NOT NULL,
  oauth_project_id TEXT,
  credential_bundle_id TEXT UNIQUE NOT NULL,
  management_authorized INTEGER NOT NULL DEFAULT 0 CHECK(management_authorized IN (0, 1)),
  credential_cleanup_pending INTEGER NOT NULL DEFAULT 0 CHECK(credential_cleanup_pending IN (0, 1)),
  removed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
"#;

pub(super) const STATE_TRIGGERS_SQL: &str = r#"
CREATE TRIGGER IF NOT EXISTS channel_profiles_state_insert
AFTER INSERT ON channel_profiles BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, created_at)
  VALUES(NEW.channel_id, 'connection', NEW.profile_id, 'invalidate', NEW.updated_at);
END;
CREATE TRIGGER IF NOT EXISTS channel_profiles_state_update
AFTER UPDATE ON channel_profiles BEGIN
  INSERT INTO state_changes(channel_id, surface, entity_id, event_kind, created_at)
  VALUES(NEW.channel_id, 'connection', NEW.profile_id, 'invalidate', NEW.updated_at);
END;
"#;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    channel_id: Option<String>,
    client_id: Option<String>,
    bundle_id: Option<String>,
    project_id: Option<String>,
}

fn binding(connection: &Connection) -> Result<Binding, String> {
    connection
        .query_row(
            "SELECT active_channel_id, oauth_client_id, oauth_credential_id, oauth_project_id FROM connection_settings WHERE singleton = 1",
            [],
            |row| Ok(Binding { channel_id: row.get(0)?, client_id: row.get(1)?, bundle_id: row.get(2)?, project_id: row.get(3)? }),
        )
        .optional()
        .map_err(user_error)
        .map(|value| value.unwrap_or(Binding { channel_id: None, client_id: None, bundle_id: None, project_id: None }))
}

fn require_expected(connection: &Connection, expected: Option<&str>) -> Result<(), String> {
    if binding(connection)?.channel_id.as_deref() == expected {
        Ok(())
    } else {
        Err("The selected channel changed. Review the account action again.".into())
    }
}

fn require_idle(connection: &Connection) -> Result<(), String> {
    let active: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM upload_items WHERE status IN ('dispatching', 'uploading')) OR EXISTS(SELECT 1 FROM deletion_requests WHERE status = 'executing')",
            [],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if active {
        Err("Wait for active uploads and video-management operations to finish before changing profiles. Saved jobs and checkpoints stay intact.".into())
    } else {
        Ok(())
    }
}

fn profile_audit(
    connection: &Connection,
    channel_id: &str,
    channel_name: &str,
    kind: &str,
    detail: &str,
) -> Result<(), String> {
    connection.execute(
        "INSERT INTO audit_events(id, item_id, channel_name, channel_id, kind, detail, created_at) VALUES(?1, NULL, ?2, ?3, ?4, ?5, ?6)",
        params![Uuid::new_v4().to_string(), channel_name, channel_id, kind, detail, now()],
    ).map_err(user_error)?;
    Ok(())
}

pub(super) fn credential_entry(bundle_id: &str, purpose: &str) -> Result<CredentialEntry, String> {
    if Uuid::parse_str(bundle_id).is_err()
        || !matches!(purpose, "refresh" | "secret" | "management")
    {
        return Err("The protected profile credential reference is invalid.".into());
    }
    CredentialEntry::new(
        SECURE_STORE_SERVICE,
        &format!("youtube-oauth-bundle-{bundle_id}-{purpose}"),
    )
    .map_err(user_error)
}

trait ProtectedStore {
    fn read(&self, bundle_id: Option<&str>, purpose: &str) -> Result<Option<String>, String>;
    fn write(&mut self, bundle_id: &str, purpose: &str, value: &str) -> Result<(), String>;
    fn erase(&mut self, bundle_id: Option<&str>, purpose: &str) -> Result<(), String>;
}

struct OsProtectedStore;

fn entry_for(bundle_id: Option<&str>, purpose: &str) -> Result<CredentialEntry, String> {
    match bundle_id {
        Some(id) => credential_entry(id, purpose),
        None => match purpose {
            "refresh" => refresh_token_entry(),
            "secret" => oauth_client_secret_entry(),
            "management" => deletion_refresh_token_entry(),
            _ => Err("The protected credential purpose is invalid.".into()),
        },
    }
}

impl ProtectedStore for OsProtectedStore {
    fn read(&self, bundle_id: Option<&str>, purpose: &str) -> Result<Option<String>, String> {
        match entry_for(bundle_id, purpose)?.get_password() {
            Ok(value) if !value.is_empty() => Ok(Some(value)),
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("The operating system could not read protected profile credentials. The saved connection is unchanged.".into()),
        }
    }

    fn write(&mut self, bundle_id: &str, purpose: &str, value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Err("Refusing to store an empty profile credential.".into());
        }
        credential_entry(bundle_id, purpose)?.set_password(value).map_err(|_| "The operating system could not protect the profile credentials. The saved connection is unchanged.".into())
    }

    fn erase(&mut self, bundle_id: Option<&str>, purpose: &str) -> Result<(), String> {
        match entry_for(bundle_id, purpose)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("The operating system has not confirmed removal of the protected profile credentials.".into()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProfileSummary {
    profile_id: String,
    channel_id: String,
    channel_name: String,
    project_id: Option<String>,
    selected: bool,
    management_authorized: bool,
    removed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Workspace {
    active_channel_id: Option<String>,
    profiles: Vec<ProfileSummary>,
    can_switch: bool,
    hold_reason: Option<String>,
}

fn workspace(connection: &Connection, hold_reason: Option<String>) -> Result<Workspace, String> {
    let active = binding(connection)?.channel_id;
    let profiles = connection.prepare(
        "SELECT profile_id, channel_id, channel_name, oauth_project_id, management_authorized FROM channel_profiles WHERE removed_at IS NULL ORDER BY channel_name COLLATE NOCASE, profile_id LIMIT 64",
    ).map_err(user_error)?.query_map([], |row| {
        let channel_id: String = row.get(1)?;
        Ok(ProfileSummary {
            profile_id: row.get(0)?, selected: active.as_deref() == Some(channel_id.as_str()), channel_id,
            channel_name: row.get(2)?, project_id: row.get(3)?, management_authorized: row.get::<_, i64>(4)? != 0, removed: false,
        })
    }).map_err(user_error)?.collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    Ok(Workspace {
        active_channel_id: active,
        profiles,
        can_switch: hold_reason.is_none(),
        hold_reason,
    })
}

fn cleanup_removed_with(
    connection: &Connection,
    store: &mut impl ProtectedStore,
) -> Result<(), String> {
    let pending = connection
        .prepare("SELECT profile_id, credential_bundle_id FROM channel_profiles WHERE removed_at IS NOT NULL AND credential_cleanup_pending = 1 ORDER BY updated_at, profile_id LIMIT 64")
        .map_err(user_error)?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    for (id, bundle) in pending {
        let mut complete = true;
        for purpose in ["refresh", "secret", "management"] {
            complete &= store.erase(Some(&bundle), purpose).is_ok();
        }
        if complete {
            connection
                .execute("UPDATE channel_profiles SET credential_cleanup_pending=0, updated_at=?1 WHERE profile_id=?2 AND removed_at IS NOT NULL AND credential_bundle_id=?3", params![now(), id, bundle])
                .map_err(user_error)?;
        }
    }
    Ok(())
}

/// Adopt an already verified legacy connection without reading media or rewriting
/// jobs. OS-to-OS credential copying is complete before the SQLite pointer commit.
fn adopt_with(connection: &mut Connection, store: &mut impl ProtectedStore) -> Result<(), String> {
    let original = binding(connection)?;
    let Some(channel_id) = original.channel_id.as_deref() else {
        return Ok(());
    };
    let Some(client_id) = original.client_id.as_deref() else {
        return Err(
            "The saved channel has no OAuth client identity. Reconnect it before saving a profile."
                .into(),
        );
    };
    require_idle(connection)?;
    let cleanup_pending: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM channel_profiles WHERE channel_id = ?1 AND removed_at IS NOT NULL AND credential_cleanup_pending = 1)", [channel_id], |row| row.get(0)).map_err(user_error)?;
    if cleanup_pending {
        return Err("This channel's removed profile still has protected credential cleanup pending. Open account profiles again after OS secure storage is available.".into());
    }
    if let Some(bundle_id) = original.bundle_id.as_deref() {
        let owner: Option<String> = connection.query_row("SELECT channel_id FROM channel_profiles WHERE credential_bundle_id = ?1 AND removed_at IS NULL", [bundle_id], |row| row.get(0)).optional().map_err(user_error)?;
        if owner.as_deref().is_some_and(|owner| owner != channel_id) {
            return Err("The saved protected credential belongs to another channel. Reconnect through Add account.".into());
        }
    }
    let existing: Option<(String, String)> = connection.query_row("SELECT profile_id, credential_bundle_id FROM channel_profiles WHERE channel_id = ?1 AND removed_at IS NULL", [channel_id], |row| Ok((row.get(0)?, row.get(1)?))).optional().map_err(user_error)?;
    let settings = connection_settings(connection)?;
    if existing
        .as_ref()
        .is_some_and(|(_, id)| Some(id.as_str()) == original.bundle_id.as_deref())
    {
        return synchronize_active_profile(connection);
    }
    let refresh = store.read(original.bundle_id.as_deref(), "refresh")?.ok_or("The saved refresh credential is unavailable. Reconnect this channel before switching profiles.")?;
    let secret = store.read(original.bundle_id.as_deref(), "secret")?;
    // Legacy management permission uses its separate protected global entry.
    let management: Option<String> = if settings.deletion_authorized {
        Some(store.read(None, "management")?.ok_or("The separately approved management credential is unavailable. Grant that permission again before saving this profile.")?)
    } else {
        None
    };
    let id = existing
        .as_ref()
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let bundle_id = original
        .bundle_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let write = (|| {
        store.write(&bundle_id, "refresh", &refresh)?;
        if let Some(secret) = secret.as_deref() {
            store.write(&bundle_id, "secret", secret)?;
        }
        if let Some(management) = management.as_deref() {
            store.write(&bundle_id, "management", management)?;
        }
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(user_error)?;
        if binding(&transaction)? != original {
            return Err(
                "The saved connection changed while its profile was being protected.".into(),
            );
        }
        require_idle(&transaction)?;
        let channel_name = settings.active_channel.as_deref().unwrap_or(channel_id);
        transaction.execute("INSERT INTO channel_profiles(profile_id, channel_id, channel_name, oauth_client_id, oauth_project_id, credential_bundle_id, management_authorized, removed_at, created_at, updated_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?8) ON CONFLICT(channel_id) DO UPDATE SET channel_name=excluded.channel_name, oauth_client_id=excluded.oauth_client_id, oauth_project_id=excluded.oauth_project_id, credential_bundle_id=excluded.credential_bundle_id, management_authorized=excluded.management_authorized, credential_cleanup_pending=0, removed_at=NULL, updated_at=excluded.updated_at", params![id, channel_id, channel_name, client_id, original.project_id, bundle_id, settings.deletion_authorized as i64, now()]).map_err(user_error)?;
        transaction.execute("UPDATE connection_settings SET oauth_credential_id = ?1, updated_at = ?2 WHERE singleton = 1", params![bundle_id, now()]).map_err(user_error)?;
        profile_audit(&transaction, channel_id, channel_name, "channel_profile_adopted", "Existing verified channel saved with independently protected credentials; upload identities, sources and checkpoints retained")?;
        transaction.commit().map_err(user_error)
    })();
    if write.is_err() && original.bundle_id.is_none() {
        for purpose in ["refresh", "secret", "management"] {
            let _ = store.erase(Some(&bundle_id), purpose);
        }
    }
    write?;
    if original.bundle_id.is_none() {
        let _ = store.erase(None, "refresh");
        let _ = store.erase(None, "secret");
    }
    if management.is_some() {
        let _ = store.erase(None, "management");
    }
    Ok(())
}

/// Call under the native credential/dispatch guards after a verified ordinary
/// connection. This is not a startup/schema hook: it accesses OS protected storage.
pub(super) fn adopt_current_connection(state: &AppState) -> Result<(), String> {
    adopt_with(&mut database(state)?, &mut OsProtectedStore)
}

/// DB-only hook after same-channel OAuth replacement has promoted its bundle.
/// Must run in that promotion transaction before the previous bundle is erased.
pub(super) fn synchronize_active_profile(connection: &Connection) -> Result<(), String> {
    let current = binding(connection)?;
    let (Some(channel), Some(client), Some(bundle)) = (
        current.channel_id.as_deref(),
        current.client_id.as_deref(),
        current.bundle_id.as_deref(),
    ) else {
        return Ok(());
    };
    let foreign: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM channel_profiles WHERE credential_bundle_id = ?1 AND channel_id != ?2 AND removed_at IS NULL)", params![bundle, channel], |row| row.get(0)).map_err(user_error)?;
    if foreign {
        return Err(
            "A protected profile credential cannot be reassigned to another channel.".into(),
        );
    }
    let settings = connection_settings(connection)?;
    connection.execute("UPDATE channel_profiles SET channel_name=?1, oauth_client_id=?2, oauth_project_id=?3, credential_bundle_id=?4, management_authorized=?5, updated_at=?6 WHERE channel_id=?7 AND removed_at IS NULL", params![settings.active_channel.as_deref().unwrap_or(channel), client, current.project_id, bundle, settings.deletion_authorized as i64, now(), channel]).map_err(user_error)?;
    Ok(())
}

pub(super) fn active_management_entry(connection: &Connection) -> Result<CredentialEntry, String> {
    let current = binding(connection)?;
    let protected: Option<String> = connection.query_row("SELECT credential_bundle_id FROM channel_profiles WHERE channel_id = ?1 AND credential_bundle_id = ?2 AND removed_at IS NULL", params![current.channel_id, current.bundle_id], |row| row.get(0)).optional().map_err(user_error)?;
    match protected {
        Some(id) => credential_entry(&id, "management"),
        None => deletion_refresh_token_entry(),
    }
}

/// Reconnecting the selected profile may refresh its credential, but ordinary
/// consent cannot overwrite a bundle already owned by a different saved channel.
pub(super) fn require_verified_channel(
    connection: &Connection,
    verified_channel: &str,
) -> Result<(), String> {
    let current = binding(connection)?;
    let owner: Option<String> = connection.query_row("SELECT channel_id FROM channel_profiles WHERE credential_bundle_id = ?1 AND removed_at IS NULL", params![current.bundle_id], |row| row.get(0)).optional().map_err(user_error)?;
    if owner
        .as_deref()
        .is_some_and(|owner| owner != verified_channel)
    {
        Err("Google authorized a different saved channel. Use Add account, then explicitly switch to its profile.".into())
    } else if owner.is_none()
        && connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM channel_profiles WHERE removed_at IS NULL)",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(user_error)?
    {
        Err("Use Add account to verify and review a new profile, or explicitly select an existing saved channel before reconnecting.".into())
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Ready,
    Authorizing,
    Verified,
    Completed,
    Failed,
    Cancelled,
}

impl Phase {
    fn holds(self) -> bool {
        matches!(
            self,
            Self::Ready | Self::Authorizing | Self::Verified | Self::Failed
        )
    }
}

#[derive(Clone)]
struct Candidate {
    id: String,
    original: Binding,
    client_id: String,
    project_id: Option<String>,
    has_secret: bool,
    phase: Phase,
    attempt_id: Option<String>,
    channel_id: Option<String>,
    channel_name: Option<String>,
    detail: Option<String>,
    expires: Instant,
}

type Reviews = HashMap<(PathBuf, String), Candidate>;
fn reviews() -> &'static Mutex<Reviews> {
    static REVIEWS: OnceLock<Mutex<Reviews>> = OnceLock::new();
    REVIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn erase_candidate(candidate: &Candidate) {
    let mut store = OsProtectedStore;
    for purpose in ["refresh", "secret", "management"] {
        let _ = store.erase(Some(&candidate.id), purpose);
    }
    if let Some(id) = candidate.attempt_id.as_deref() {
        let _ = clear_refresh_token(oauth_verifier_entry(id));
    }
}

fn expire_reviews(state: &AppState) -> Result<(), String> {
    let mut registry = reviews().lock().map_err(user_error)?;
    for ((path, _), candidate) in registry.iter_mut() {
        if path == &state.database_path
            && candidate.phase.holds()
            && Instant::now() >= candidate.expires
        {
            candidate.phase = Phase::Cancelled;
            candidate.detail = Some(
                "Account review expired. The selected profile and saved work are unchanged.".into(),
            );
            if let Some(id) = candidate.attempt_id.as_deref() {
                state.oauth_attempts.lock().map_err(user_error)?.remove(id);
            }
            erase_candidate(candidate);
            candidate.expires = Instant::now() + REVIEW_LIFETIME;
        }
    }
    registry.retain(|_, candidate| candidate.phase.holds() || Instant::now() < candidate.expires);
    Ok(())
}

fn pending(state: &AppState) -> Result<bool, String> {
    expire_reviews(state)?;
    Ok(reviews()
        .lock()
        .map_err(user_error)?
        .iter()
        .any(|((path, _), candidate)| path == &state.database_path && candidate.phase.holds()))
}

/// Root must include this alongside oauth_replacement::ensure_no_pending at all
/// dispatch, credential import/refresh, connection and management entry points.
pub(super) fn ensure_no_pending(state: &AppState) -> Result<(), String> {
    if pending(state)? {
        Err("Finish or cancel Add account before using the selected connection. Saved work stays on its original channel.".into())
    } else {
        Ok(())
    }
}

fn candidate_for(state: &AppState, id: &str) -> Result<Candidate, String> {
    if Uuid::parse_str(id).is_err() {
        return Err("The account review is invalid.".into());
    }
    reviews()
        .lock()
        .map_err(user_error)?
        .get(&(state.database_path.clone(), id.to_string()))
        .cloned()
        .ok_or("This account review is unavailable. Choose the OAuth JSON again.".into())
}

fn require_candidate_binding(
    connection: &Connection,
    candidate: &Candidate,
    expected: Option<&str>,
) -> Result<(), String> {
    require_expected(connection, expected)?;
    if binding(connection)? != candidate.original {
        return Err(
            "The saved account connection changed. Cancel this review and start Add account again."
                .into(),
        );
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Review {
    review_id: String,
    state: Phase,
    channel_id: Option<String>,
    channel_name: Option<String>,
    project_id: Option<String>,
    detail: Option<String>,
}

fn safe_review(candidate: &Candidate) -> Review {
    Review {
        review_id: candidate.id.clone(),
        state: candidate.phase,
        channel_id: candidate.channel_id.clone(),
        channel_name: candidate.channel_name.clone(),
        project_id: candidate.project_id.clone(),
        detail: candidate.detail.clone(),
    }
}

pub(super) fn load(state: &AppState, expected: Option<&str>) -> Result<Workspace, String> {
    let _dispatch = upload_scheduler(state).lock_dispatch()?;
    let management = deletion_execution_lock(state).try_lock();
    let _credentials = oauth_replacement::lock_credentials()?;
    let mut connection = database(state)?;
    require_expected(&connection, expected)?;
    let hold = if management.is_err() {
        Some("Wait for the current video-management operation to finish.".into())
    } else if !state.oauth_attempts.lock().map_err(user_error)?.is_empty() {
        Some("Finish or cancel the current Google authorization first.".into())
    } else if let Err(error) = oauth_replacement::ensure_no_pending(state) {
        Some(error)
    } else if let Err(error) = ensure_no_pending(state) {
        Some(error)
    } else if let Err(error) = require_idle(&connection) {
        Some(error)
    } else {
        cleanup_removed_with(&connection, &mut OsProtectedStore)?;
        adopt_with(&mut connection, &mut OsProtectedStore).err()
    };
    workspace(&connection, hold)
}

pub(super) fn prepare(
    state: &AppState,
    path: &Path,
    expected: Option<&str>,
) -> Result<Review, String> {
    let _dispatch = upload_scheduler(state).lock_dispatch()?;
    let _management = deletion_execution_lock(state).try_lock().map_err(|_| {
        "Wait for video-management operations to finish before adding an account.".to_string()
    })?;
    let _credentials = oauth_replacement::lock_credentials()?;
    oauth_replacement::ensure_no_pending(state)?;
    ensure_no_pending(state)?;
    if !state.oauth_attempts.lock().map_err(user_error)?.is_empty() {
        return Err("Finish or cancel the pending Google authorization first.".into());
    }
    if !secure_store_available() {
        return Err("The operating system's secure credential store is unavailable. An account cannot be saved safely.".into());
    }
    let mut connection = database(state)?;
    require_expected(&connection, expected)?;
    require_idle(&connection)?;
    cleanup_removed_with(&connection, &mut OsProtectedStore)?;
    adopt_with(&mut connection, &mut OsProtectedStore)?;
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM channel_profiles WHERE removed_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if count >= MAX_PROFILES {
        return Err("This device already has 64 saved profiles. Remove an unused profile before adding another.".into());
    }
    let client = oauth_replacement::read_desktop_client(path)?;
    let candidate = Candidate { id: Uuid::new_v4().to_string(), original: binding(&connection)?, client_id: client.client_id, project_id: client.project_id, has_secret: !client.client_secret.is_empty(), phase: Phase::Ready, attempt_id: None, channel_id: None, channel_name: None, detail: Some("Authorize the intended Google account, verify its YouTube channel, then review before saving. The current channel stays selected.".into()), expires: Instant::now() + REVIEW_LIFETIME };
    if candidate.has_secret {
        OsProtectedStore.write(&candidate.id, "secret", &client.client_secret)?;
    }
    let review = safe_review(&candidate);
    reviews().lock().map_err(user_error)?.insert(
        (state.database_path.clone(), candidate.id.clone()),
        candidate,
    );
    Ok(review)
}

pub(super) fn status(state: &AppState, id: &str) -> Result<Review, String> {
    let _credentials = oauth_replacement::lock_credentials()?;
    expire_reviews(state)?;
    let candidate = candidate_for(state, id)?;
    Ok(safe_review(&candidate))
}

pub(super) fn begin(
    state: &AppState,
    id: &str,
    expected: Option<&str>,
) -> Result<OAuthStart, String> {
    let _dispatch = upload_scheduler(state).lock_dispatch()?;
    let _management = deletion_execution_lock(state).try_lock().map_err(|_| {
        "Wait for video-management operations to finish before authorizing a profile.".to_string()
    })?;
    let _credentials = oauth_replacement::lock_credentials()?;
    let mut candidate = prepare_authorization(state, id, expected)?;
    if candidate.has_secret {
        OsProtectedStore
            .read(Some(&candidate.id), "secret")?
            .ok_or("The staged OAuth JSON is unavailable. Cancel and choose it again.")?;
    }
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "A secure local OAuth callback port could not be opened.".to_string())?;
    let redirect = format!(
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
            ("redirect_uri", redirect.as_str()),
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
        .map_err(|_| {
            "The operating system could not protect the authorization verifier.".to_string()
        })?;
    candidate.phase = Phase::Authorizing;
    candidate.attempt_id = Some(attempt_id.clone());
    candidate.channel_id = None;
    candidate.channel_name = None;
    candidate.detail = Some("Waiting for Google authorization. Return here to verify the channel before saving the profile.".into());
    candidate.expires = Instant::now() + REVIEW_LIFETIME;
    reviews().lock().map_err(user_error)?.insert(
        (state.database_path.clone(), candidate.id.clone()),
        candidate.clone(),
    );
    // This module owns its callback; generic connection cancellation cannot
    // accidentally promote it. Root may name this attempt kind Profile instead.
    state
        .oauth_attempts
        .lock()
        .map_err(user_error)?
        .insert(attempt_id.clone(), OAuthAttemptKind::Replacement);
    let callback_state = state.clone();
    let callback_attempt = attempt_id.clone();
    spawn_worker(move || {
        await_callback(
            callback_state,
            listener,
            candidate,
            redirect,
            callback_attempt,
        )
    });
    Ok(OAuthStart {
        authorization_url,
        attempt_id,
    })
}

// This read-only preflight is shared with the regression so the normal staged
// review is tested without opening consent or reading a real protected store.
fn prepare_authorization(
    state: &AppState,
    id: &str,
    expected: Option<&str>,
) -> Result<Candidate, String> {
    oauth_replacement::ensure_no_replacement_pending(state)?;
    expire_reviews(state)?;
    let candidate = candidate_for(state, id)?;
    if !matches!(candidate.phase, Phase::Ready | Phase::Failed) {
        return Err(
            "This account authorization is already pending or finished. Check the review status."
                .into(),
        );
    }
    if !state.oauth_attempts.lock().map_err(user_error)?.is_empty() {
        return Err("Finish or cancel the pending Google authorization first.".into());
    }
    let connection = database(state)?;
    require_candidate_binding(&connection, &candidate, expected)?;
    require_idle(&connection)?;
    Ok(candidate)
}

fn callback_live(state: &AppState, candidate: &Candidate, attempt: &str) -> bool {
    candidate_for(state, &candidate.id).is_ok_and(|current| {
        current.phase == Phase::Authorizing
            && current.attempt_id.as_deref() == Some(attempt)
            && Instant::now() < current.expires
    }) && state
        .oauth_attempts
        .lock()
        .is_ok_and(|attempts| attempts.contains_key(attempt))
}

fn failed(state: &AppState, candidate: &Candidate, attempt: &str, detail: &str) {
    let Ok(_credentials) = oauth_replacement::lock_credentials() else {
        return;
    };
    if !callback_live(state, candidate, attempt) {
        return;
    }
    if let Ok(mut registry) = reviews().lock() {
        if let Some(current) =
            registry.get_mut(&(state.database_path.clone(), candidate.id.clone()))
        {
            current.phase = Phase::Failed;
            current.detail = Some(format!(
                "{detail} The selected profile and saved work are unchanged."
            ));
        }
    }
    let _ = state
        .oauth_attempts
        .lock()
        .map(|mut attempts| attempts.remove(attempt));
    let _ = clear_refresh_token(oauth_verifier_entry(attempt));
    let _ = OsProtectedStore.erase(Some(&candidate.id), "refresh");
}

fn await_callback(
    state: AppState,
    listener: TcpListener,
    candidate: Candidate,
    redirect: String,
    attempt: String,
) {
    let _ = listener.set_nonblocking(true);
    let deadline = Instant::now() + Duration::from_secs(600);
    while Instant::now() < deadline {
        if !callback_live(&state, &candidate, &attempt) {
            return;
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut request = [0_u8; 8192];
                let bytes = stream.read(&mut request).unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..bytes]);
                let mut parts = text.lines().next().unwrap_or("").split_whitespace();
                let method = parts.next();
                let target = parts.next().unwrap_or("/");
                if !valid_oauth_callback_request(method, target, &attempt) {
                    respond_to_callback(&mut stream, "Connection did not complete.");
                    continue;
                }
                let result = if let Some(error) = callback_value(target, "error") {
                    Err(oauth_error_message(&error))
                } else if let Some(code) = callback_value(target, "code") {
                    exchange_and_verify(&state, &candidate, &redirect, &attempt, &code)
                } else {
                    Err("Google did not return an authorization code.".into())
                };
                match result {
                    Ok(()) => respond_to_callback(&mut stream, "YouTube channel verified. Return to the app to review and save its profile."),
                    Err(detail) => { failed(&state, &candidate, &attempt, &detail); respond_to_callback(&mut stream, "Account verification did not complete. Return to the app for details."); }
                }
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(125))
            }
            Err(_) => break,
        }
    }
    failed(
        &state,
        &candidate,
        &attempt,
        "Google authorization timed out. Retry when ready.",
    );
}

fn verified_channel(payload: &serde_json::Value) -> Result<(String, String), String> {
    let channels = payload
        .get("items")
        .and_then(serde_json::Value::as_array)
        .ok_or("YouTube did not return a channel identity.")?;
    if channels.len() != 1 {
        return Err("YouTube did not return one unambiguous channel. Choose the intended channel during Google consent and retry.".into());
    }
    let id = channels[0]
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
        .ok_or("YouTube returned an invalid immutable channel identity.")?;
    let name = channels[0]
        .pointer("/snippet/title")
        .and_then(serde_json::Value::as_str)
        .filter(|name| {
            !name.trim().is_empty()
                && name.chars().count() <= 300
                && !name.chars().any(char::is_control)
        })
        .ok_or("YouTube returned an invalid channel name.")?;
    Ok((id.to_string(), name.to_string()))
}

fn exchange_and_verify(
    state: &AppState,
    candidate: &Candidate,
    redirect: &str,
    attempt: &str,
    code: &str,
) -> Result<(), String> {
    if !callback_live(state, candidate, attempt) {
        return Err("This account authorization was cancelled or expired.".into());
    }
    let secret = if candidate.has_secret {
        OsProtectedStore
            .read(Some(&candidate.id), "secret")?
            .ok_or("The staged OAuth JSON is unavailable.")?
    } else {
        String::new()
    };
    let verifier = oauth_verifier_entry(attempt)?
        .get_password()
        .map_err(|_| "The protected authorization verifier is unavailable.".to_string())?;
    let mut form = vec![
        ("code", code),
        ("client_id", candidate.client_id.as_str()),
        ("code_verifier", verifier.as_str()),
        ("redirect_uri", redirect),
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
    let access = tokens
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("Google did not return an access token.")?;
    let refresh = tokens
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("Google did not return a refresh token. Retry the consent step.")?;
    let response = provider_transport(state)
        .control_client()?
        .get("https://www.googleapis.com/youtube/v3/channels")
        .query(&[("part", "id,snippet"), ("mine", "true")])
        .bearer_auth(access)
        .send()
        .map_err(|_| "YouTube channel verification could not be reached.".to_string())?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(youtube_inventory_http_error(status));
    }
    let payload: serde_json::Value = response
        .json()
        .map_err(|_| "YouTube returned an unreadable channel response.".to_string())?;
    let (channel_id, channel_name) = verified_channel(&payload)?;
    let _dispatch = upload_scheduler(state).lock_dispatch()?;
    let _management = deletion_execution_lock(state).try_lock().map_err(|_| {
        "A video-management operation started. Retry after it finishes.".to_string()
    })?;
    let _credentials = oauth_replacement::lock_credentials()?;
    if !callback_live(state, candidate, attempt) {
        return Err("This account authorization was cancelled or expired.".into());
    }
    let connection = database(state)?;
    require_candidate_binding(
        &connection,
        candidate,
        candidate.original.channel_id.as_deref(),
    )?;
    require_idle(&connection)?;
    OsProtectedStore.write(&candidate.id, "refresh", refresh)?;
    let mut registry = reviews().lock().map_err(user_error)?;
    let current = registry
        .get_mut(&(state.database_path.clone(), candidate.id.clone()))
        .ok_or("The account review is unavailable.")?;
    current.channel_id = Some(channel_id);
    current.channel_name = Some(channel_name);
    current.phase = Phase::Verified;
    current.detail = Some("Channel verified by YouTube. Review its identity and Cloud project before saving; the currently selected channel stays active.".into());
    current.expires = Instant::now() + REVIEW_LIFETIME;
    drop(registry);
    let _ = state
        .oauth_attempts
        .lock()
        .map(|mut attempts| attempts.remove(attempt));
    let _ = clear_refresh_token(oauth_verifier_entry(attempt));
    // Candidate access tokens are deliberately not put in the selected channel cache.
    Ok(())
}

fn save_verified(
    connection: &mut Connection,
    candidate: &Candidate,
    expected: Option<&str>,
    approval: bool,
) -> Result<(), String> {
    if !approval {
        return Err("Review and approve the verified channel before saving its profile.".into());
    }
    if candidate.phase != Phase::Verified {
        return Err("Verify this channel with Google before saving the profile.".into());
    }
    let channel = candidate
        .channel_id
        .as_deref()
        .ok_or("The verified channel identity is unavailable.")?;
    let name = candidate
        .channel_name
        .as_deref()
        .ok_or("The verified channel name is unavailable.")?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_candidate_binding(&transaction, candidate, expected)?;
    require_idle(&transaction)?;
    let existing: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM channel_profiles WHERE channel_id = ?1 AND removed_at IS NULL)", [channel], |row| row.get(0)).map_err(user_error)?;
    if existing {
        return Err("This channel already has a saved profile. Switch to it, or use Replace OAuth JSON to reconnect the same channel.".into());
    }
    let cleanup_pending: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM channel_profiles WHERE channel_id=?1 AND removed_at IS NOT NULL AND credential_cleanup_pending=1)", [channel], |row| row.get(0)).map_err(user_error)?;
    if cleanup_pending {
        return Err("This channel's previous profile still has protected credential cleanup pending. Retry after OS secure storage is available.".into());
    }
    let count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM channel_profiles WHERE removed_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if count >= MAX_PROFILES {
        return Err("This device already has 64 saved profiles.".into());
    }
    transaction.execute("INSERT INTO channel_profiles(profile_id, channel_id, channel_name, oauth_client_id, oauth_project_id, credential_bundle_id, management_authorized, credential_cleanup_pending, removed_at, created_at, updated_at) VALUES(?1, ?2, ?3, ?4, ?5, ?1, 0, 0, NULL, ?6, ?6) ON CONFLICT(channel_id) DO UPDATE SET profile_id=excluded.profile_id, channel_name=excluded.channel_name, oauth_client_id=excluded.oauth_client_id, oauth_project_id=excluded.oauth_project_id, credential_bundle_id=excluded.credential_bundle_id, management_authorized=0, credential_cleanup_pending=0, removed_at=NULL, updated_at=excluded.updated_at", params![candidate.id, channel, name, candidate.client_id, candidate.project_id, now()]).map_err(user_error)?;
    profile_audit(&transaction, channel, name, "channel_profile_added", "Operator saved an independently protected verified account; selection and existing upload bindings were retained")?;
    transaction.commit().map_err(user_error)
}

pub(super) fn commit(
    state: &AppState,
    id: &str,
    expected: Option<&str>,
    approval: bool,
) -> Result<Workspace, String> {
    let _dispatch = upload_scheduler(state).lock_dispatch()?;
    let _management = deletion_execution_lock(state)
        .try_lock()
        .map_err(|_| "Wait for the current video-management operation to finish.".to_string())?;
    let _credentials = oauth_replacement::lock_credentials()?;
    expire_reviews(state)?;
    let candidate = candidate_for(state, id)?;
    let mut connection = database(state)?;
    require_expected(&connection, expected)?;
    if candidate.phase == Phase::Completed {
        return workspace(&connection, None);
    }
    if OsProtectedStore
        .read(Some(&candidate.id), "refresh")?
        .is_none()
    {
        return Err("The verified protected credential is unavailable. Cancel and authorize this profile again.".into());
    }
    let mut registry = reviews().lock().map_err(user_error)?;
    let current = registry
        .get_mut(&(state.database_path.clone(), candidate.id.clone()))
        .ok_or("The account review is unavailable.")?;
    save_verified(&mut connection, &candidate, expected, approval)?;
    current.phase = Phase::Completed;
    current.detail = Some("Profile saved. Explicitly switch when ready; the selected channel and existing upload jobs are unchanged.".into());
    current.expires = Instant::now() + REVIEW_LIFETIME;
    drop(registry);
    workspace(&connection, None)
}

pub(super) fn cancel(state: &AppState, id: &str) -> Result<ConnectionSettings, String> {
    let _dispatch = upload_scheduler(state).lock_dispatch()?;
    let _credentials = oauth_replacement::lock_credentials()?;
    let mut candidate = candidate_for(state, id)?;
    if candidate.phase != Phase::Completed {
        candidate.phase = Phase::Cancelled;
        candidate.detail = Some("Add account cancelled. The selected profile, saved queue and checkpoints are unchanged.".into());
        candidate.expires = Instant::now() + REVIEW_LIFETIME;
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
        erase_candidate(&candidate);
    }
    connection_settings(&database(state)?)
}

fn switch_with(
    connection: &mut Connection,
    store: &mut impl ProtectedStore,
    id: &str,
    expected: Option<&str>,
    approval: bool,
) -> Result<(), String> {
    if !approval {
        return Err("Review and approve the destination channel before switching.".into());
    }
    if Uuid::parse_str(id).is_err() {
        return Err("The saved profile identity is invalid.".into());
    }
    require_expected(connection, expected)?;
    require_idle(connection)?;
    let row: (String, String, String, Option<String>, String, bool) = connection.query_row("SELECT channel_id, channel_name, oauth_client_id, oauth_project_id, credential_bundle_id, management_authorized FROM channel_profiles WHERE profile_id = ?1 AND removed_at IS NULL", [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get::<_, i64>(5)? != 0))).optional().map_err(user_error)?.ok_or("The saved profile is unavailable. Reconnect it through Add account.")?;
    store.read(Some(&row.4), "refresh")?.ok_or("This profile's protected credential is unavailable. Reconnect the same channel before switching.")?;
    let management_available = row.5 && store.read(Some(&row.4), "management")?.is_some();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_expected(&transaction, expected)?;
    require_idle(&transaction)?;
    let fresh: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM channel_profiles WHERE profile_id = ?1 AND channel_id = ?2 AND credential_bundle_id = ?3 AND removed_at IS NULL)", params![id, row.0, row.4], |row| row.get(0)).map_err(user_error)?;
    if !fresh {
        return Err("The saved profile changed during review. Select it again.".into());
    }
    transaction.execute("INSERT INTO connection_settings(singleton, oauth_client_id, oauth_credential_id, oauth_project_id, active_channel, active_channel_id, deletion_authorized, deletion_sudo_until, connection_detail, updated_at) VALUES(1, ?1, ?2, ?3, ?4, ?5, ?6, NULL, 'Selected a saved channel profile. Queue bindings and quota holds remain unchanged.', ?7) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id=excluded.oauth_client_id, oauth_credential_id=excluded.oauth_credential_id, oauth_project_id=excluded.oauth_project_id, active_channel=excluded.active_channel, active_channel_id=excluded.active_channel_id, deletion_authorized=excluded.deletion_authorized, deletion_sudo_until=NULL, connection_detail=excluded.connection_detail, updated_at=excluded.updated_at", params![row.2, row.4, row.3, row.1, row.0, management_available as i64, now()]).map_err(user_error)?;
    if row.5 && !management_available {
        transaction.execute("UPDATE channel_profiles SET management_authorized=0, updated_at=?1 WHERE profile_id=?2", params![now(), id]).map_err(user_error)?;
    }
    profile_audit(&transaction, &row.0, &row.1, "channel_profile_selected", "Operator explicitly selected the saved channel; retained jobs remain bound to their original channel, management mode reset and quota holds preserved")?;
    transaction.commit().map_err(user_error)
}

pub(super) fn switch(
    state: &AppState,
    id: &str,
    expected: Option<&str>,
    approval: bool,
) -> Result<ConnectionSettings, String> {
    let _dispatch = upload_scheduler(state).lock_dispatch()?;
    let _management = deletion_execution_lock(state).try_lock().map_err(|_| {
        "Wait for the current video-management operation to finish before switching profiles."
            .to_string()
    })?;
    let _credentials = oauth_replacement::lock_credentials()?;
    oauth_replacement::ensure_no_pending(state)?;
    ensure_no_pending(state)?;
    if !state.oauth_attempts.lock().map_err(user_error)?.is_empty() {
        return Err(
            "Finish or cancel the current Google authorization before switching profiles.".into(),
        );
    }
    let mut connection = database(state)?;
    require_expected(&connection, expected)?;
    adopt_with(&mut connection, &mut OsProtectedStore)?;
    switch_with(
        &mut connection,
        &mut OsProtectedStore,
        id,
        expected,
        approval,
    )?;
    provider_transport(state).invalidate_access_token(provider_transport::AccessTokenKind::Upload);
    provider_transport(state)
        .invalidate_access_token(provider_transport::AccessTokenKind::Deletion);
    startup_gate(state).folder_worker_wake.notify_all();
    startup_gate(state).quota_worker_wake.notify_all();
    let settings = connection_settings(&connection)?;
    drop(connection);
    drop(_credentials);
    drop(_management);
    drop(_dispatch);
    let _ = ensure_folder_monitor_worker(state.clone());
    let _ = ensure_quota_resume_worker(state.clone());
    let _ = start_queued_uploads_impl(state);
    Ok(settings)
}

fn remove_with(
    connection: &mut Connection,
    store: &mut impl ProtectedStore,
    id: &str,
    expected: Option<&str>,
    approval: bool,
) -> Result<(), String> {
    if !approval {
        return Err("Review and approve removal of this device's saved connection.".into());
    }
    if Uuid::parse_str(id).is_err() {
        return Err("The saved profile identity is invalid.".into());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_expected(&transaction, expected)?;
    require_idle(&transaction)?;
    let row: (String, String, String, Option<String>) = transaction.query_row("SELECT channel_id, channel_name, credential_bundle_id, removed_at FROM channel_profiles WHERE profile_id = ?1", [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).optional().map_err(user_error)?.ok_or("The saved profile is unavailable.")?;
    let selected = binding(&transaction)?.channel_id.as_deref() == Some(row.0.as_str());
    if selected {
        transaction.execute("UPDATE connection_settings SET active_channel=NULL, active_channel_id=NULL, oauth_client_id=NULL, oauth_credential_id=NULL, oauth_project_id=NULL, deletion_authorized=0, deletion_sudo_until=NULL, connection_detail='Removed the selected profile from this device. Saved jobs, original files and recovery checkpoints remain retained.', updated_at=?1 WHERE singleton=1", [now()]).map_err(user_error)?;
    }
    if row.3.is_none() {
        transaction.execute("UPDATE channel_profiles SET removed_at=?1, management_authorized=0, credential_cleanup_pending=1, updated_at=?1 WHERE profile_id=?2", params![now(), id]).map_err(user_error)?;
        profile_audit(&transaction, &row.0, &row.1, "channel_profile_removed", "Operator removed this device's saved connection; queue items, original sources, monitor approvals, upload checkpoints, history and quota holds retained")?;
    }
    transaction.commit().map_err(user_error)?;
    let mut complete = true;
    for purpose in ["refresh", "secret", "management"] {
        complete &= store.erase(Some(&row.2), purpose).is_ok();
    }
    connection.execute("UPDATE channel_profiles SET credential_cleanup_pending=?1, updated_at=?2 WHERE profile_id=?3", params![(!complete) as i64, now(), id]).map_err(user_error)?;
    if !complete {
        profile_audit(connection, &row.0, &row.1, "channel_profile_credential_cleanup_pending", "Profile detached safely; OS credential removal is not yet confirmed and may be retried locally")?;
        if selected {
            connection.execute("UPDATE connection_settings SET connection_detail='Profile removed from selection. The operating system has not confirmed removal of its protected credentials; saved jobs and checkpoints remain retained.', updated_at=?1 WHERE singleton=1", [now()]).map_err(user_error)?;
        }
    }
    Ok(())
}

pub(super) fn remove(
    state: &AppState,
    id: &str,
    expected: Option<&str>,
    approval: bool,
) -> Result<ConnectionSettings, String> {
    let _dispatch = upload_scheduler(state).lock_dispatch()?;
    let _management = deletion_execution_lock(state).try_lock().map_err(|_| {
        "Wait for the current video-management operation to finish before removing profiles."
            .to_string()
    })?;
    let _credentials = oauth_replacement::lock_credentials()?;
    oauth_replacement::ensure_no_pending(state)?;
    ensure_no_pending(state)?;
    if !state.oauth_attempts.lock().map_err(user_error)?.is_empty() {
        return Err(
            "Finish or cancel the current Google authorization before removing profiles.".into(),
        );
    }
    let mut connection = database(state)?;
    require_expected(&connection, expected)?;
    adopt_with(&mut connection, &mut OsProtectedStore)?;
    remove_with(
        &mut connection,
        &mut OsProtectedStore,
        id,
        expected,
        approval,
    )?;
    provider_transport(state).invalidate_access_token(provider_transport::AccessTokenKind::Upload);
    provider_transport(state)
        .invalidate_access_token(provider_transport::AccessTokenKind::Deletion);
    startup_gate(state).folder_worker_wake.notify_all();
    startup_gate(state).quota_worker_wake.notify_all();
    connection_settings(&connection)
}

/// Disconnect only deselects a saved profile. Removal of protected credentials is
/// the separate reviewed action above; resumable-session entries are never touched.
pub(super) fn disconnect_selected(connection: &Connection) -> Result<(), String> {
    connection.execute("UPDATE connection_settings SET active_channel=NULL, active_channel_id=NULL, oauth_client_id=NULL, oauth_credential_id=NULL, oauth_project_id=NULL, deletion_authorized=0, deletion_sudo_until=NULL, connection_detail='No channel selected. Protected profiles and saved upload recovery evidence remain on this device.', updated_at=?1 WHERE singleton=1", [now()]).map_err(user_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemoryStore {
        values: HashMap<(Option<String>, String), String>,
        fail_write: bool,
        fail_erase: bool,
    }
    impl ProtectedStore for MemoryStore {
        fn read(&self, id: Option<&str>, purpose: &str) -> Result<Option<String>, String> {
            Ok(self
                .values
                .get(&(id.map(str::to_string), purpose.to_string()))
                .cloned())
        }
        fn write(&mut self, id: &str, purpose: &str, value: &str) -> Result<(), String> {
            if self.fail_write {
                return Err("protected write failed".into());
            }
            self.values.insert(
                (Some(id.to_string()), purpose.to_string()),
                value.to_string(),
            );
            Ok(())
        }
        fn erase(&mut self, id: Option<&str>, purpose: &str) -> Result<(), String> {
            if self.fail_erase {
                return Err("protected erase failed".into());
            }
            self.values
                .remove(&(id.map(str::to_string), purpose.to_string()));
            Ok(())
        }
    }

    fn fixture() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        initialize_fixture(&connection);
        connection
    }

    fn initialize_fixture(connection: &Connection) {
        connection.execute_batch("CREATE TABLE connection_settings(singleton INTEGER PRIMARY KEY, oauth_client_id TEXT, oauth_credential_id TEXT, oauth_project_id TEXT, active_channel TEXT, active_channel_id TEXT, connection_detail TEXT, deletion_authorized INTEGER NOT NULL DEFAULT 0, deletion_sudo_until TEXT, manual_visibility_default TEXT DEFAULT 'unlisted', updated_at TEXT NOT NULL); CREATE TABLE upload_items(id TEXT PRIMARY KEY, status TEXT, channel_id TEXT, workspace_path TEXT, confirmed_bytes INTEGER, digest TEXT); CREATE TABLE deletion_requests(id TEXT PRIMARY KEY, status TEXT); CREATE TABLE audit_events(id TEXT, item_id TEXT, channel_name TEXT, channel_id TEXT, kind TEXT, detail TEXT, created_at TEXT); CREATE TABLE upload_quota_pauses(channel_id TEXT PRIMARY KEY, pause_until TEXT); CREATE TABLE api_quota_pauses(quota_scope TEXT PRIMARY KEY, channel_id TEXT, pause_until TEXT); CREATE TABLE checkpoints(item_id TEXT PRIMARY KEY, protected_ref TEXT); CREATE TABLE folder_monitors(id TEXT PRIMARY KEY, channel_id TEXT, enabled INTEGER);").unwrap();
        connection.execute_batch(SCHEMA_SQL).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton, oauth_client_id, oauth_project_id, active_channel, active_channel_id, updated_at) VALUES(1,'client-a.apps.googleusercontent.com','project-one','Channel A','channel-a',?1)", [now()]).unwrap();
        connection.execute_batch("INSERT INTO upload_items VALUES('saved-a','queued','channel-a','original-a.mp4',123,'digest-a'); INSERT INTO upload_items VALUES('saved-b','needs_reconciliation','channel-b','original-b.mp4',456,'digest-b'); INSERT INTO checkpoints VALUES('saved-a','protected-session-a'); INSERT INTO checkpoints VALUES('saved-b','protected-session-b'); INSERT INTO folder_monitors VALUES('monitor-a','channel-a',1); INSERT INTO upload_quota_pauses VALUES('channel-a','2099-01-01T00:00:00Z'); INSERT INTO api_quota_pauses VALUES('project:project-one','channel-a','2099-01-01T00:00:00Z');").unwrap();
    }

    fn store() -> MemoryStore {
        let mut value = MemoryStore::default();
        value
            .values
            .insert((None, "refresh".into()), "legacy-refresh-a".into());
        value
            .values
            .insert((None, "secret".into()), "legacy-secret-a".into());
        value
    }

    fn add_profile(
        connection: &Connection,
        store: &mut MemoryStore,
        channel: &str,
        project: &str,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        connection.execute("INSERT INTO channel_profiles(profile_id,channel_id,channel_name,oauth_client_id,oauth_project_id,credential_bundle_id,created_at,updated_at) VALUES(?1,?2,?2,'client-b.apps.googleusercontent.com',?3,?1,?4,?4)", params![id, channel, project, now()]).unwrap();
        store
            .write(&id, "refresh", &format!("refresh-{channel}"))
            .unwrap();
        id
    }

    fn saved_evidence(connection: &Connection) -> Vec<(String, String, String, i64, String)> {
        connection.prepare("SELECT id, channel_id, workspace_path, confirmed_bytes, digest FROM upload_items ORDER BY id").unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap()
    }

    #[test]
    fn legacy_adoption_protects_independent_profile_and_retains_queue_evidence() {
        let mut connection = fixture();
        let mut protected = store();
        let before = saved_evidence(&connection);
        adopt_with(&mut connection, &mut protected).unwrap();
        let current = binding(&connection).unwrap();
        let id = current.bundle_id.unwrap();
        assert_eq!(
            protected.read(Some(&id), "refresh").unwrap().as_deref(),
            Some("legacy-refresh-a")
        );
        assert_eq!(
            protected.read(Some(&id), "secret").unwrap().as_deref(),
            Some("legacy-secret-a")
        );
        assert!(protected.read(None, "refresh").unwrap().is_none());
        assert_eq!(saved_evidence(&connection), before);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM checkpoints", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        let summary = serde_json::to_string(&workspace(&connection, None).unwrap()).unwrap();
        assert!(!summary.contains("refresh"));
        assert!(!summary.contains("credential"));
        assert!(!summary.contains("client-a"));
        adopt_with(&mut connection, &mut protected).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM channel_profiles", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn failed_protected_adoption_does_not_promote_or_touch_legacy_evidence() {
        let mut connection = fixture();
        let original = binding(&connection).unwrap();
        let before = saved_evidence(&connection);
        let mut protected = store();
        protected.fail_write = true;
        assert!(adopt_with(&mut connection, &mut protected).is_err());
        assert_eq!(binding(&connection).unwrap(), original);
        assert_eq!(saved_evidence(&connection), before);
        assert_eq!(
            protected.read(None, "refresh").unwrap().as_deref(),
            Some("legacy-refresh-a")
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM channel_profiles", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn explicit_switch_retains_identity_session_and_shared_project_channel_holds() {
        let mut connection = fixture();
        let mut protected = store();
        adopt_with(&mut connection, &mut protected).unwrap();
        let before = saved_evidence(&connection);
        let a = workspace(&connection, None)
            .unwrap()
            .profiles
            .remove(0)
            .profile_id;
        let b = add_profile(&connection, &mut protected, "channel-b", "project-one");
        switch_with(&mut connection, &mut protected, &b, Some("channel-a"), true).unwrap();
        assert_eq!(
            binding(&connection).unwrap().channel_id.as_deref(),
            Some("channel-b")
        );
        assert_eq!(saved_evidence(&connection), before);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM checkpoints", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM api_quota_pauses WHERE quota_scope='project:project-one'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
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
        switch_with(&mut connection, &mut protected, &a, Some("channel-b"), true).unwrap();
        assert_eq!(
            binding(&connection).unwrap().channel_id.as_deref(),
            Some("channel-a")
        );
    }

    #[test]
    fn stale_unapproved_active_or_missing_credential_switches_are_rejected() {
        let mut connection = fixture();
        let mut protected = store();
        adopt_with(&mut connection, &mut protected).unwrap();
        let b = add_profile(&connection, &mut protected, "channel-b", "project-two");
        let original = binding(&connection).unwrap();
        assert!(switch_with(
            &mut connection,
            &mut protected,
            &b,
            Some("channel-a"),
            false
        )
        .is_err());
        assert!(switch_with(&mut connection, &mut protected, &b, Some("channel-x"), true).is_err());
        connection
            .execute(
                "UPDATE upload_items SET status='uploading' WHERE id='saved-a'",
                [],
            )
            .unwrap();
        assert!(switch_with(&mut connection, &mut protected, &b, Some("channel-a"), true).is_err());
        connection
            .execute(
                "UPDATE upload_items SET status='queued' WHERE id='saved-a'",
                [],
            )
            .unwrap();
        protected.erase(Some(&b), "refresh").unwrap();
        assert!(switch_with(&mut connection, &mut protected, &b, Some("channel-a"), true).is_err());
        assert_eq!(binding(&connection).unwrap(), original);
    }

    #[test]
    fn management_permission_is_scoped_and_temporary_mode_resets_on_switch() {
        let mut connection = fixture();
        let mut protected = store();
        protected
            .values
            .insert((None, "management".into()), "management-a".into());
        connection.execute("UPDATE connection_settings SET deletion_authorized=1,deletion_sudo_until='2099-01-01T00:00:00Z'",[]).unwrap();
        adopt_with(&mut connection, &mut protected).unwrap();
        let a = workspace(&connection, None)
            .unwrap()
            .profiles
            .remove(0)
            .profile_id;
        let b = add_profile(&connection, &mut protected, "channel-b", "project-one");
        switch_with(&mut connection, &mut protected, &b, Some("channel-a"), true).unwrap();
        assert!(
            !connection_settings(&connection)
                .unwrap()
                .deletion_authorized
        );
        switch_with(&mut connection, &mut protected, &a, Some("channel-b"), true).unwrap();
        let settings = connection_settings(&connection).unwrap();
        assert!(settings.deletion_authorized);
        assert!(!settings.deletion_sudo_active);
        assert!(protected.read(None, "management").unwrap().is_none());
        assert_eq!(
            protected
                .read(
                    binding(&connection).unwrap().bundle_id.as_deref(),
                    "management"
                )
                .unwrap()
                .as_deref(),
            Some("management-a")
        );
    }

    #[test]
    fn remove_selected_profile_detaches_credentials_but_preserves_jobs_monitors_and_holds() {
        let mut connection = fixture();
        let mut protected = store();
        adopt_with(&mut connection, &mut protected).unwrap();
        let before = saved_evidence(&connection);
        let a = workspace(&connection, None)
            .unwrap()
            .profiles
            .remove(0)
            .profile_id;
        let bundle = binding(&connection).unwrap().bundle_id.unwrap();
        remove_with(&mut connection, &mut protected, &a, Some("channel-a"), true).unwrap();
        assert!(binding(&connection).unwrap().channel_id.is_none());
        assert!(protected.read(Some(&bundle), "refresh").unwrap().is_none());
        assert_eq!(saved_evidence(&connection), before);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM checkpoints", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT enabled FROM folder_monitors WHERE id='monitor-a'",
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
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(workspace(&connection, None).unwrap().profiles.is_empty());
    }

    #[test]
    fn inactive_removal_does_not_change_selected_credentials_and_cleanup_is_retryable() {
        let mut connection = fixture();
        let mut protected = store();
        adopt_with(&mut connection, &mut protected).unwrap();
        let original = binding(&connection).unwrap();
        let b = add_profile(&connection, &mut protected, "channel-b", "project-two");
        protected.fail_erase = true;
        remove_with(&mut connection, &mut protected, &b, Some("channel-a"), true).unwrap();
        assert_eq!(binding(&connection).unwrap(), original);
        assert_eq!(
            connection
                .query_row(
                    "SELECT credential_cleanup_pending FROM channel_profiles WHERE profile_id=?1",
                    [&b],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert!(protected.read(Some(&b), "refresh").unwrap().is_some());
        protected.fail_erase = false;
        remove_with(&mut connection, &mut protected, &b, Some("channel-a"), true).unwrap();
        assert!(protected.read(Some(&b), "refresh").unwrap().is_none());
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE kind='channel_profile_removed' AND channel_id='channel-b'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
    }

    fn verified_candidate(connection: &Connection) -> Candidate {
        Candidate {
            id: Uuid::new_v4().to_string(),
            original: binding(connection).unwrap(),
            client_id: "client-b.apps.googleusercontent.com".into(),
            project_id: Some("project-one".into()),
            has_secret: false,
            phase: Phase::Verified,
            attempt_id: None,
            channel_id: Some("channel-b".into()),
            channel_name: Some("Channel B".into()),
            detail: None,
            expires: Instant::now() + REVIEW_LIFETIME,
        }
    }

    #[test]
    fn verified_save_requires_final_approval_and_preserves_selection_and_existing_evidence() {
        let mut connection = fixture();
        let candidate = verified_candidate(&connection);
        let original = binding(&connection).unwrap();
        let before = saved_evidence(&connection);
        assert!(save_verified(&mut connection, &candidate, Some("channel-a"), false).is_err());
        let mut unverified = candidate.clone();
        unverified.phase = Phase::Authorizing;
        assert!(save_verified(&mut connection, &unverified, Some("channel-a"), true).is_err());
        save_verified(&mut connection, &candidate, Some("channel-a"), true).unwrap();
        assert_eq!(binding(&connection).unwrap(), original);
        assert_eq!(saved_evidence(&connection), before);
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE kind='channel_profile_added' AND channel_id='channel-b'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
        assert!(save_verified(&mut connection, &candidate, Some("channel-a"), true).is_err());
    }

    #[test]
    fn changed_binding_or_ambiguous_provider_identity_cannot_save_a_profile() {
        let mut connection = fixture();
        let candidate = verified_candidate(&connection);
        connection.execute("UPDATE connection_settings SET oauth_client_id='changed.apps.googleusercontent.com'",[]).unwrap();
        assert!(save_verified(&mut connection, &candidate, Some("channel-a"), true).is_err());
        assert!(verified_channel(&serde_json::json!({"items":[]})).is_err());
        assert!(verified_channel(&serde_json::json!({"items":[{"id":"channel-a","snippet":{"title":"A"}},{"id":"channel-b","snippet":{"title":"B"}}]})).is_err());
        assert!(verified_channel(
            &serde_json::json!({"items":[{"id":"../bad","snippet":{"title":"A"}}]})
        )
        .is_err());
        assert_eq!(
            verified_channel(
                &serde_json::json!({"items":[{"id":"channel-b","snippet":{"title":"Channel B"}}]})
            )
            .unwrap(),
            ("channel-b".into(), "Channel B".into())
        );
    }

    #[test]
    fn ordinary_reconnect_cannot_overwrite_another_profiles_protected_bundle() {
        let mut connection = fixture();
        let mut protected = store();
        adopt_with(&mut connection, &mut protected).unwrap();
        require_verified_channel(&connection, "channel-a").unwrap();
        assert!(require_verified_channel(&connection, "channel-b").is_err());
        let foreign = add_profile(&connection, &mut protected, "channel-b", "project-two");
        connection
            .execute(
                "UPDATE connection_settings SET oauth_credential_id=?1",
                [foreign],
            )
            .unwrap();
        assert!(synchronize_active_profile(&connection).is_err());
        disconnect_selected(&connection).unwrap();
        assert!(require_verified_channel(&connection, "channel-b").is_err());
    }

    #[test]
    fn saved_profiles_and_selected_binding_survive_database_reopen() {
        let path =
            std::env::temp_dir().join(format!("youtube-profile-fixture-{}.sqlite", Uuid::new_v4()));
        let mut connection = Connection::open(&path).unwrap();
        initialize_fixture(&connection);
        let mut protected = store();
        adopt_with(&mut connection, &mut protected).unwrap();
        let before = saved_evidence(&connection);
        let b = add_profile(&connection, &mut protected, "channel-b", "project-one");
        switch_with(&mut connection, &mut protected, &b, Some("channel-a"), true).unwrap();
        drop(connection);
        let connection = Connection::open(&path).unwrap();
        assert_eq!(
            binding(&connection).unwrap().channel_id.as_deref(),
            Some("channel-b")
        );
        let saved = workspace(&connection, None).unwrap();
        assert_eq!(saved.profiles.len(), 2);
        assert_eq!(
            saved
                .profiles
                .iter()
                .filter(|profile| profile.selected)
                .count(),
            1
        );
        assert_eq!(saved_evidence(&connection), before);
        assert_eq!(
            protected.read(Some(&b), "refresh").unwrap().as_deref(),
            Some("refresh-channel-b")
        );
        drop(connection);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn cleanup_failure_blocks_readding_until_protected_removal_is_confirmed() {
        let mut connection = fixture();
        let mut protected = store();
        adopt_with(&mut connection, &mut protected).unwrap();
        let b = add_profile(&connection, &mut protected, "channel-b", "project-one");
        protected.fail_erase = true;
        remove_with(&mut connection, &mut protected, &b, Some("channel-a"), true).unwrap();
        let candidate = verified_candidate(&connection);
        assert!(save_verified(&mut connection, &candidate, Some("channel-a"), true).is_err());
        cleanup_removed_with(&connection, &mut protected).unwrap();
        assert!(save_verified(&mut connection, &candidate, Some("channel-a"), true).is_err());
        protected.fail_erase = false;
        cleanup_removed_with(&connection, &mut protected).unwrap();
        save_verified(&mut connection, &candidate, Some("channel-a"), true).unwrap();
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM channel_profiles WHERE channel_id='channel-b' AND removed_at IS NULL", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
        assert!(protected.read(Some(&b), "refresh").unwrap().is_none());
    }

    #[test]
    fn staged_ready_profile_can_begin_its_own_authorization_while_external_credentials_stay_held() {
        let path = std::env::temp_dir().join(format!(
            "youtube-profile-authorization-{}.sqlite",
            Uuid::new_v4()
        ));
        let state = AppState {
            database_path: path.clone(),
            media_directory: path.with_extension("unused-media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton, oauth_client_id, active_channel, active_channel_id, updated_at) VALUES(1,'client-a.apps.googleusercontent.com','Channel A','channel-a',?1)", [now()]).unwrap();
        let mut candidate = verified_candidate(&connection);
        candidate.phase = Phase::Ready;
        candidate.channel_id = None;
        candidate.channel_name = None;
        let key = (state.database_path.clone(), candidate.id.clone());
        reviews()
            .lock()
            .unwrap()
            .insert(key.clone(), candidate.clone());
        let _credentials = oauth_replacement::lock_credentials().unwrap();
        assert!(oauth_replacement::ensure_no_pending(&state).is_err());
        assert!(oauth_replacement::ensure_no_replacement_pending(&state).is_ok());
        assert_eq!(
            prepare_authorization(&state, &candidate.id, Some("channel-a"))
                .unwrap()
                .id,
            candidate.id
        );
        connection.execute("UPDATE connection_settings SET oauth_client_id='changed-client.apps.googleusercontent.com'", []).unwrap();
        assert!(prepare_authorization(&state, &candidate.id, Some("channel-a")).is_err());
        reviews().lock().unwrap().remove(&key);
        drop(connection);
        std::fs::remove_file(path).unwrap();
    }
}
