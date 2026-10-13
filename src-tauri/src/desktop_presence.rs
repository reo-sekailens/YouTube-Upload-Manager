//! Opt-in desktop presence. Toasts contain counts and an opaque local route;
//! filenames, source locators, credentials and provider payloads never leave here.
use super::*;
use std::collections::BTreeMap;

pub(super) const ROUTE_EVENT: &str = "desktop-presence-route";
pub(super) const EXIT_EVENT: &str = "desktop-exit-requested";
pub(super) const SETTINGS_EVENT: &str = "desktop-presence-changed";
const TRAY_ID: &str = "upload-manager-presence";
static PREFERENCE_LOCK: Mutex<()> = Mutex::new(());
pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS desktop_presence_settings (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1), preferences_json TEXT NOT NULL,
 pause_new_work INTEGER NOT NULL DEFAULT 0, revision TEXT NOT NULL,
 startup_status TEXT NOT NULL DEFAULT 'unchanged', pending_route_id TEXT
);
CREATE TABLE IF NOT EXISTS desktop_presence_cursor (singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS desktop_item_status (channel_id TEXT NOT NULL,item_id TEXT NOT NULL,signature TEXT NOT NULL,PRIMARY KEY(channel_id,item_id));
CREATE TABLE IF NOT EXISTS desktop_notification_events (
 event_key TEXT PRIMARY KEY NOT NULL, channel_id TEXT NOT NULL, kind TEXT NOT NULL,
 item_ids_json TEXT NOT NULL, created_at TEXT NOT NULL, routed INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS desktop_notification_pending ON desktop_notification_events(routed,created_at);
CREATE TABLE IF NOT EXISTS desktop_notification_routes (
 id TEXT PRIMARY KEY NOT NULL, channel_id TEXT NOT NULL, kind TEXT NOT NULL,
 item_ids_json TEXT NOT NULL, count INTEGER NOT NULL, submitted_at TEXT,
 outcome TEXT NOT NULL DEFAULT 'pending', created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS desktop_notification_recent ON desktop_notification_routes(channel_id,created_at);
"#;
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Preferences {
    pub(super) batch_complete: bool,
    pub(super) needs_attention: bool,
    pub(super) quota_pause: bool,
    pub(super) tray_enabled: bool,
    pub(super) keep_running_on_close: bool,
    pub(super) run_on_login: bool,
}
impl Preferences {
    fn notifications(&self) -> bool {
        self.batch_complete || self.needs_attention || self.quota_pause
    }
    fn enabled(&self, kind: &str) -> bool {
        match kind {
            "batch_complete" => self.batch_complete,
            "needs_attention" => self.needs_attention,
            "quota_pause" => self.quota_pause,
            _ => false,
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Settings {
    preferences: Preferences,
    revision: String,
    pause_new_work: bool,
    desktop_supported: bool,
    tray_ready: bool,
    notification_permission: String,
    startup_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    startup_enabled: Option<bool>,
    click_support: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Route {
    route_id: String,
    channel_id: String,
    kind: String,
    item_ids: Vec<String>,
    count: usize,
    needs_channel_switch: bool,
}
#[derive(Clone, Debug)]
struct Notice {
    route_id: String,
    kind: String,
    count: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Activity {
    route_id: String,
    kind: String,
    count: usize,
    outcome: String,
    created_at: String,
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
fn desktop() -> bool {
    !cfg!(any(target_os = "android", target_os = "ios"))
}
fn saved(connection: &Connection) -> Result<(Preferences, String, bool, String), String> {
    let row:Option<(String,String,bool,String)>=connection.query_row("SELECT preferences_json,revision,pause_new_work,startup_status FROM desktop_presence_settings WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(user_error)?;
    match row {
        Some((json, revision, paused, startup)) => Ok((
            serde_json::from_str(&json).map_err(|_| {
                "Saved desktop preferences are unreadable; background behavior is disabled."
                    .to_string()
            })?,
            revision,
            paused,
            startup,
        )),
        None => Ok((
            Preferences::default(),
            "initial".into(),
            false,
            "unchanged".into(),
        )),
    }
}
pub(super) fn admission_paused(connection: &Connection) -> Result<bool, String> {
    saved(connection).map(|(_, _, paused, _)| paused)
}
trait Platform {
    fn supported(&self) -> bool;
    fn permission(&mut self) -> String;
    fn startup_enabled(&mut self) -> Result<bool, String>;
    fn set_startup(&mut self, enabled: bool) -> Result<(), String>;
    fn notify(&mut self, notice: &Notice) -> Result<(), String>;
    fn click_support(&self) -> String {
        "Notification clicks depend on the desktop notification server. Retained activity stays available in the app.".into()
    }
}
struct NativePlatform<'a> {
    app: &'a AppHandle,
}
impl Platform for NativePlatform<'_> {
    fn supported(&self) -> bool {
        desktop()
    }
    fn permission(&mut self) -> String {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            use tauri_plugin_notification::NotificationExt;
            return self
                .app
                .notification()
                .permission_state()
                .map(|state| {
                    match state {
                        tauri_plugin_notification::PermissionState::Granted => {
                            "desktop_os_controls_delivery"
                        }
                        tauri_plugin_notification::PermissionState::Denied => "denied",
                        _ => "unknown",
                    }
                    .to_string()
                })
                .unwrap_or_else(|_| "unknown".into());
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            "unsupported".into()
        }
    }
    fn startup_enabled(&mut self) -> Result<bool, String> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            use tauri_plugin_autostart::ManagerExt;
            self.app
                .autolaunch()
                .is_enabled()
                .map_err(|_| "The operating system's login setting could not be read.".into())
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            Err("Desktop login startup is unavailable on this platform.".into())
        }
    }
    fn set_startup(&mut self, enabled: bool) -> Result<(), String> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            use tauri_plugin_autostart::ManagerExt;
            let manager = self.app.autolaunch();
            (if enabled {manager.enable()} else {manager.disable()}).map_err(|_|"The operating system did not confirm the login-startup change. Review the saved setting before retrying.".into())
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = enabled;
            Err("Desktop login startup is unavailable on this platform.".into())
        }
    }
    fn notify(&mut self, notice: &Notice) -> Result<(), String> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            use tauri_plugin_notification::NotificationExt;
            self.app.notification().builder().title("YouTube Upload Manager").body(notice_body(&notice.kind,notice.count)).extra("routeId",&notice.route_id).show().map_err(|_|"The desktop notification could not be submitted; its activity receipt remains available in the app.".into())
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = notice;
            Err("Desktop notifications are unavailable on this platform.".into())
        }
    }
    fn click_support(&self) -> String {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if !self
            .app
            .try_state::<RuntimeState>()
            .is_some_and(|runtime| runtime.click_ready.load(Ordering::Acquire))
        {
            return "Notification navigation is unavailable in this session. Use retained activity in the app to open its saved channel and items.".into();
        }
        if cfg!(windows) {
            "Toast clicks are available while the notification is on screen. Windows Action Center clicks are not reported; use the app’s retained activity instead.".into()
        } else {
            "Notification clicks depend on the desktop notification server. Retained activity stays available in the app.".into()
        }
    }
}
fn notice_body(kind: &str, count: usize) -> String {
    match kind {"batch_complete"=>format!("{count} batch upload(s) finished. Review YouTube processing and follow-up steps in the app."),"needs_attention"=>format!("{count} upload outcome(s) need attention. Open the app to review their retained receipts."),_=>format!("{count} quota pause(s) need attention. Approved queues are retained; open the account view for details.")}
}
fn settings_with(
    connection: &Connection,
    platform: &mut impl Platform,
    tray_ready: bool,
) -> Result<Settings, String> {
    let (preferences, revision, pause_new_work, saved_status) = saved(connection)?;
    let startup_enabled = if platform.supported() {
        platform.startup_enabled().ok()
    } else {
        None
    };
    let startup_status =
        if startup_enabled.is_some_and(|enabled| enabled != preferences.run_on_login) {
            "needs_attention".into()
        } else {
            saved_status
        };
    Ok(Settings {
        preferences,
        revision,
        pause_new_work,
        desktop_supported: platform.supported(),
        tray_ready,
        notification_permission: platform.permission(),
        startup_status,
        startup_enabled,
        click_support: platform.click_support(),
    })
}
fn save_with(
    state: &AppState,
    preferences: Preferences,
    expected_revision: &str,
    approved: bool,
    startup_approved: bool,
    platform: &mut impl Platform,
) -> Result<(), String> {
    let _guard = PREFERENCE_LOCK.lock().map_err(user_error)?;
    if !approved {
        return Err("Review and approve the desktop choices before saving.".into());
    }
    if !platform.supported() {
        return Err(
            "Desktop tray and login-startup choices are unavailable on this platform.".into(),
        );
    }
    if preferences.keep_running_on_close && !preferences.tray_enabled {
        return Err(
            "Enable the tray before keeping this app running when its window closes.".into(),
        );
    }
    let mut connection = database(state)?;
    let (previous, revision, paused, _) = saved(&connection)?;
    if revision != expected_revision {
        return Err(
            "Desktop choices changed while this review was open. Refresh and review them again."
                .into(),
        );
    }
    let actual = platform.startup_enabled().ok();
    if preferences.run_on_login && actual != Some(true) && !startup_approved {
        return Err(
            "Separately approve launching this app when you sign in before enabling login startup."
                .into(),
        );
    }
    // Persist the exact approved startup intent before the separate OS operation.
    // On crash/restart, observe the OS state; never enable startup speculatively.
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    if saved(&transaction)?.1 != revision {
        return Err("Desktop choices changed before saving. Refresh this review.".into());
    }
    let latest: i64 = transaction
        .query_row(
            "SELECT COALESCE(MAX(revision),0) FROM state_changes",
            [],
            |r| r.get(0),
        )
        .map_err(user_error)?;
    let change_startup = actual.is_some_and(|enabled| enabled != preferences.run_on_login)
        || (actual.is_none() && previous.run_on_login != preferences.run_on_login);
    transaction.execute("INSERT INTO desktop_presence_settings(singleton,preferences_json,pause_new_work,revision,startup_status) VALUES(1,?1,?2,?3,?4) ON CONFLICT(singleton) DO UPDATE SET preferences_json=excluded.preferences_json,revision=excluded.revision,startup_status=excluded.startup_status",params![serde_json::to_string(&preferences).map_err(user_error)?,paused,Uuid::new_v4().to_string(),if change_startup{"pending"}else if actual.is_none(){"needs_attention"}else{"unchanged"}]).map_err(user_error)?;
    if !previous.notifications() && preferences.notifications() {
        transaction.execute("INSERT INTO desktop_presence_cursor(singleton,revision) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET revision=excluded.revision",[latest]).map_err(user_error)?;
        transaction
            .execute(
                "UPDATE desktop_notification_events SET routed=1 WHERE routed=0",
                [],
            )
            .map_err(user_error)?;
    }
    audit_global(&transaction,"desktop_preferences_reviewed","Operator reviewed device-wide desktop activity, tray and close behavior; login startup has a separate explicit approval.")?;
    transaction.commit().map_err(user_error)?;
    if change_startup {
        let result = platform.set_startup(preferences.run_on_login);
        let observed = platform.startup_enabled().ok();
        let connection = database(state)?;
        connection
            .execute(
                "UPDATE desktop_presence_settings SET startup_status=?1 WHERE singleton=1",
                [
                    if result.is_ok() && observed == Some(preferences.run_on_login) {
                        "confirmed"
                    } else {
                        "needs_attention"
                    },
                ],
            )
            .map_err(user_error)?;
        if result.is_err() || observed != Some(preferences.run_on_login) {
            return Err("Desktop choices were saved, but login startup is not confirmed. Refresh and review the operating-system setting before retrying.".into());
        }
    }
    Ok(())
}
pub(super) fn set_pause(state: &AppState, paused: bool) -> Result<(), String> {
    let _guard = PREFERENCE_LOCK.lock().map_err(user_error)?;
    let mut connection = database(state)?;
    let (preferences, _, _, startup) = saved(&connection)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    transaction.execute("INSERT INTO desktop_presence_settings(singleton,preferences_json,pause_new_work,revision,startup_status) VALUES(1,?1,?2,?3,?4) ON CONFLICT(singleton) DO UPDATE SET pause_new_work=excluded.pause_new_work,revision=excluded.revision",params![serde_json::to_string(&preferences).map_err(user_error)?,paused,Uuid::new_v4().to_string(),startup]).map_err(user_error)?;
    audit_global(&transaction,if paused{"desktop_new_work_paused"}else{"desktop_new_work_resumed"},"Operator changed admission for new uploads and folder discovery on this device; in-flight work, approvals, queues and checkpoints were retained.")?;
    transaction.commit().map_err(user_error)
}
fn enqueue(
    connection: &Connection,
    key: &str,
    channel: &str,
    kind: &str,
    ids: &[String],
) -> Result<(), String> {
    if !valid_id(channel)
        || !matches!(kind, "batch_complete" | "needs_attention" | "quota_pause")
        || ids.len() > 200
        || ids.iter().any(|id| !valid_id(id))
    {
        return Err("The desktop activity identity is invalid.".into());
    }
    for id in ids {
        let owned: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM upload_items WHERE id=?1 AND channel_id=?2)",
                params![id, channel],
                |r| r.get(0),
            )
            .map_err(user_error)?;
        if !owned {
            return Err("A desktop activity item belongs to another channel.".into());
        }
    }
    connection.execute("INSERT OR IGNORE INTO desktop_notification_events(event_key,channel_id,kind,item_ids_json,created_at) VALUES(?1,?2,?3,?4,?5)",params![blake3::hash(key.as_bytes()).to_hex().to_string(),channel,kind,serde_json::to_string(ids).map_err(user_error)?,now()]).map_err(user_error)?;
    Ok(())
}
/// Observe only authoritative, meaningful state transitions; never chunk/scan events.
pub(super) fn observe_changes(
    state: &AppState,
    changes: &[state_events::StateChange],
) -> Result<(), String> {
    let mut connection = database(state)?;
    let (preferences, _, _, _) = saved(&connection)?;
    if !preferences.notifications() {
        return Ok(());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let cursor: i64 = transaction
        .query_row(
            "SELECT revision FROM desktop_presence_cursor WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(user_error)?
        .unwrap_or(0);
    let mut latest = cursor;
    for change in changes
        .iter()
        .filter(|change| change.revision > cursor as u64)
    {
        latest = latest.max(change.revision as i64);
        let channel = &change.channel_id;
        if !valid_id(channel) {
            continue;
        }
        if matches!(change.surface.as_str(), "upload" | "completion" | "assets") {
            let row:Option<(String,String,Option<String>,Option<String>,Option<String>)>=transaction.query_row("SELECT status,playlist_status,source_delete_status,upload_batch_id,video_id FROM upload_items WHERE id=?1 AND channel_id=?2",params![change.entity_id,channel],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(user_error)?;
            let Some((status, playlist, cleanup, batch, video)) = row else {
                continue;
            };
            let processing: Option<String> = if table_present(
                &transaction,
                "upload_completion_checks",
            )? {
                transaction.query_row("SELECT processing_status FROM upload_completion_checks WHERE channel_id=?1 AND item_id=?2", params![channel, change.entity_id], |row| row.get(0)).optional().map_err(user_error)?
            } else {
                None
            };
            let assets: Vec<(String, String)> = if table_present(&transaction, "video_asset_jobs")?
            {
                transaction.prepare("SELECT id,status FROM video_asset_jobs WHERE channel_id=?1 AND item_id=?2 ORDER BY id").map_err(user_error)?.query_map(params![channel, change.entity_id], |row| Ok((row.get(0)?, row.get(1)?))).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?
            } else {
                Vec::new()
            };
            let mut attention = Vec::<String>::new();
            if matches!(status.as_str(), "failed" | "needs_reconciliation") {
                attention.push(format!("upload:{status}"));
            }
            if matches!(playlist.as_str(), "failed" | "needs_reconciliation") {
                attention.push(format!("playlist:{playlist}"));
            }
            if cleanup
                .as_deref()
                .is_some_and(|value| value.starts_with("retained"))
            {
                attention.push("source:retained".into());
            }
            if processing.as_deref() == Some("failed") {
                attention.push("processing:failed".into());
            }
            for (id, status) in &assets {
                if valid_id(id)
                    && matches!(
                        status.as_str(),
                        "failed" | "needs_reconciliation" | "conflict" | "waiting_permission"
                    )
                {
                    attention.push(format!("asset:{id}:{status}"));
                }
            }
            let signature = serde_json::to_string(&(&status, &attention)).map_err(user_error)?;
            let old: Option<String> = transaction
                .query_row(
                    "SELECT signature FROM desktop_item_status WHERE channel_id=?1 AND item_id=?2",
                    params![channel, change.entity_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(user_error)?;
            if old.as_deref() == Some(signature.as_str()) {
                continue;
            }
            transaction.execute("INSERT INTO desktop_item_status(channel_id,item_id,signature) VALUES(?1,?2,?3) ON CONFLICT(channel_id,item_id) DO UPDATE SET signature=excluded.signature",params![channel,change.entity_id,signature]).map_err(user_error)?;
            if preferences.needs_attention && !attention.is_empty() {
                enqueue(
                    &transaction,
                    &format!(
                        "attention:{channel}:{}:{}",
                        change.entity_id, change.revision
                    ),
                    channel,
                    "needs_attention",
                    &[change.entity_id.clone()],
                )?;
            }
            if preferences.batch_complete
                && status == "uploaded"
                && video.as_deref().is_some_and(valid_id)
            {
                if let Some(batch) = batch.filter(|batch| valid_id(batch)) {
                    let (total,unfinished):(usize,usize)=transaction.query_row("SELECT COUNT(*),COALESCE(SUM(CASE WHEN status!='uploaded' OR video_id IS NULL THEN 1 ELSE 0 END),0) FROM upload_items WHERE channel_id=?1 AND upload_batch_id=?2",params![channel,batch],|r|Ok((r.get(0)?,r.get(1)?))).map_err(user_error)?;
                    if total > 0 && unfinished == 0 {
                        let ids=transaction.prepare("SELECT id FROM upload_items WHERE channel_id=?1 AND upload_batch_id=?2 ORDER BY id LIMIT 200").map_err(user_error)?.query_map(params![channel,batch],|r|r.get::<_,String>(0)).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
                        enqueue(
                            &transaction,
                            &format!("complete:{channel}:{batch}"),
                            channel,
                            "batch_complete",
                            &ids,
                        )?;
                    }
                }
            }
        } else if preferences.quota_pause
            && matches!(change.surface.as_str(), "quota" | "api_quota")
            && change.event_kind != "delete"
        {
            let api = change.payload.get("kind").and_then(|value| value.as_str())
                == Some("api_quota")
                || change.surface == "api_quota";
            let pause: Option<(String, String)> = if api {
                transaction.query_row("SELECT quota_scope,pause_until FROM api_quota_pauses WHERE channel_id=?1 ORDER BY pause_until DESC LIMIT 1",[channel],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(user_error)?
            } else {
                transaction.query_row("SELECT channel_id,pause_until FROM upload_quota_pauses WHERE channel_id=?1",[channel],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(user_error)?
            };
            if let Some((scope, until)) = pause {
                if DateTime::parse_from_rfc3339(&until).is_ok_and(|deadline| deadline > Utc::now())
                {
                    enqueue(
                        &transaction,
                        &format!("quota:{channel}:{api}:{scope}:{until}"),
                        channel,
                        "quota_pause",
                        &[],
                    )?;
                }
            }
        }
    }
    transaction.execute("INSERT INTO desktop_presence_cursor(singleton,revision) VALUES(1,?1) ON CONFLICT(singleton) DO UPDATE SET revision=MAX(revision,excluded.revision)",[latest]).map_err(user_error)?;
    transaction.commit().map_err(user_error)
}
fn table_present(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )
        .map_err(user_error)
}
/// Read the durable device stream, rather than an active-channel webview cursor.
/// Coalesce entities inside one bounded page and retain its covering revision.
fn observe_backlog(state: &AppState) -> Result<(), String> {
    let connection = database(state)?;
    if !saved(&connection)?.0.notifications() {
        return Ok(());
    }
    let cursor: i64 = connection
        .query_row(
            "SELECT revision FROM desktop_presence_cursor WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?
        .unwrap_or(0);
    let rows = connection.prepare("SELECT revision,channel_id,surface,entity_id,event_kind,payload_json FROM state_changes WHERE revision>?1 ORDER BY revision LIMIT 512").map_err(user_error)?.query_map([cursor],|row|Ok(state_events::StateChange { revision:row.get::<_,i64>(0)?.max(0) as u64,channel_id:row.get(1)?,surface:row.get(2)?,entity_id:row.get(3)?,event_kind:row.get(4)?,payload:serde_json::from_str(&row.get::<_,String>(5)?).unwrap_or(serde_json::Value::Null) })).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    let Some(last) = rows.last() else {
        return Ok(());
    };
    let revision = last.revision;
    let mut coalesced = BTreeMap::new();
    for row in rows {
        coalesced.insert(
            (
                row.channel_id.clone(),
                row.surface.clone(),
                row.entity_id.clone(),
            ),
            row,
        );
    }
    let mut changes = coalesced.into_values().collect::<Vec<_>>();
    changes.sort_by_key(|change| change.revision);
    changes.push(state_events::StateChange {
        revision,
        channel_id: String::new(),
        surface: "cursor".into(),
        entity_id: String::new(),
        event_kind: "cover".into(),
        payload: serde_json::Value::Null,
    });
    drop(connection);
    observe_changes(state, &changes)
}
fn has_pending(connection: &Connection) -> Result<bool, String> {
    connection.query_row("SELECT EXISTS(SELECT 1 FROM desktop_notification_events WHERE routed=0) OR COALESCE((SELECT revision FROM desktop_presence_cursor WHERE singleton=1),0)<COALESCE((SELECT MAX(revision) FROM state_changes),0)",[],|row|row.get(0)).map_err(user_error)
}
fn flush_with(state: &AppState, platform: &mut impl Platform) -> Result<usize, String> {
    let mut connection = database(state)?;
    let (preferences, _, _, _) = saved(&connection)?;
    if !preferences.notifications() {
        return Ok(0);
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let pending=transaction.prepare("SELECT event_key,channel_id,kind,item_ids_json FROM desktop_notification_events WHERE routed=0 ORDER BY created_at,event_key LIMIT 512").map_err(user_error)?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    let mut groups = BTreeMap::<(String, String), (usize, BTreeSet<String>)>::new();
    for (key, channel, kind, json) in pending {
        transaction
            .execute(
                "UPDATE desktop_notification_events SET routed=1 WHERE event_key=?1",
                [key],
            )
            .map_err(user_error)?;
        if !preferences.enabled(&kind) {
            continue;
        }
        let ids: Vec<String> = serde_json::from_str(&json).map_err(user_error)?;
        let group = groups.entry((channel, kind)).or_default();
        group.0 += 1;
        group.1.extend(ids);
    }
    let mut notices = Vec::new();
    for ((channel, kind), (count, ids)) in groups {
        let recent:Option<(String,String)>=transaction.query_row("SELECT id,item_ids_json FROM desktop_notification_routes WHERE channel_id=?1 AND kind=?2 AND created_at>?3 ORDER BY created_at DESC LIMIT 1",params![channel,kind,(Utc::now()-ChronoDuration::seconds(60)).to_rfc3339()],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(user_error)?;
        let mut ids = ids;
        let (route, new) = if let Some((id, json)) = recent {
            ids.extend(serde_json::from_str::<Vec<String>>(&json).map_err(user_error)?);
            (id, false)
        } else {
            (Uuid::new_v4().to_string(), true)
        };
        let ids = ids.into_iter().take(200).collect::<Vec<_>>();
        transaction.execute("INSERT INTO desktop_notification_routes(id,channel_id,kind,item_ids_json,count,created_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET item_ids_json=excluded.item_ids_json,count=desktop_notification_routes.count+excluded.count",params![route,channel,kind,serde_json::to_string(&ids).map_err(user_error)?,count as i64,now()]).map_err(user_error)?;
        if new {
            notices.push(Notice {
                route_id: route,
                kind,
                count,
            });
        }
    }
    transaction.commit().map_err(user_error)?;
    let permission = platform.permission();
    for notice in &notices {
        // Claim before submission: a crash cannot blindly replay a toast whose
        // OS delivery outcome is inherently unobservable on desktop.
        let current = saved(&database(state)?)?.0;
        let status = if !current.enabled(&notice.kind) {
            "suppressed"
        } else if !platform.supported() {
            "unsupported"
        } else if permission == "denied" {
            "permission_denied"
        } else if permission == "unknown" {
            "permission_unknown"
        } else {
            "submission_unknown"
        };
        let connection = database(state)?;
        connection.execute("UPDATE desktop_notification_routes SET outcome=?1,submitted_at=?2 WHERE id=?3 AND outcome='pending'",params![status,now(),notice.route_id]).map_err(user_error)?;
        if status == "submission_unknown" {
            let outcome = if platform.notify(notice).is_ok() {
                "submitted"
            } else {
                "submission_failed"
            };
            connection
                .execute(
                    "UPDATE desktop_notification_routes SET outcome=?1 WHERE id=?2",
                    params![outcome, notice.route_id],
                )
                .map_err(user_error)?;
        }
    }
    Ok(notices.len())
}
fn activate(state: &AppState, route: &str) -> Result<(), String> {
    if Uuid::parse_str(route).is_err() {
        return Err("The local notification route is invalid.".into());
    }
    let connection = database(state)?;
    let present:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM desktop_notification_routes WHERE id=?1 AND created_at>?2)",params![route,(Utc::now()-ChronoDuration::days(7)).to_rfc3339()],|r|r.get(0)).map_err(user_error)?;
    if !present {
        return Err("This activity link expired; review retained uploads in the app.".into());
    }
    connection
        .execute(
            "UPDATE desktop_presence_settings SET pending_route_id=?1 WHERE singleton=1",
            [route],
        )
        .map_err(user_error)?;
    Ok(())
}
fn pending_route(connection: &Connection) -> Result<Option<Route>, String> {
    let route: Option<String> = connection
        .query_row(
            "SELECT pending_route_id FROM desktop_presence_settings WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(user_error)?
        .flatten();
    let Some(id) = route else { return Ok(None) };
    let row:Option<(String,String,String,usize)>=connection.query_row("SELECT channel_id,kind,item_ids_json,count FROM desktop_notification_routes WHERE id=?1 AND created_at>?2",params![id,(Utc::now()-ChronoDuration::days(7)).to_rfc3339()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(user_error)?;
    let Some((channel, kind, json, count)) = row else {
        connection.execute("UPDATE desktop_presence_settings SET pending_route_id=NULL WHERE singleton=1 AND pending_route_id=?1",[id]).map_err(user_error)?;
        return Ok(None);
    };
    let needs_channel_switch = connection_settings(connection)?
        .active_channel_id
        .as_deref()
        != Some(channel.as_str());
    let item_ids = if needs_channel_switch {
        Vec::new()
    } else {
        let ids: Vec<String> = serde_json::from_str(&json).map_err(user_error)?;
        let mut owned = Vec::new();
        for item in ids {
            if connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM upload_items WHERE id=?1 AND channel_id=?2)",
                    params![item, channel],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(user_error)?
            {
                owned.push(item);
            }
        }
        owned
    };
    Ok(Some(Route {
        route_id: id,
        channel_id: channel,
        kind,
        item_ids,
        count,
        needs_channel_switch,
    }))
}
#[cfg(not(any(target_os = "android", target_os = "ios")))]
struct RuntimeState {
    enabled: AtomicBool,
    worker: AtomicBool,
    click_ready: AtomicBool,
    pause_item: Mutex<Option<tauri::menu::CheckMenuItem<tauri::Wry>>>,
}
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn sync_tray(app: &AppHandle, state: &AppState) -> Result<(), String> {
    use tauri::{
        menu::{CheckMenuItem, Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    let (prefs, _, paused, _) = saved(&database(state)?)?;
    if !prefs.tray_enabled {
        show(app);
        drop(app.remove_tray_by_id(TRAY_ID));
        return Ok(());
    }
    if app.tray_by_id(TRAY_ID).is_some() {
        if let Some(runtime) = app.try_state::<RuntimeState>() {
            if let Some(item) = runtime.pause_item.lock().map_err(user_error)?.as_ref() {
                let _ = item.set_checked(paused);
            }
        }
        return Ok(());
    }
    let open =
        MenuItem::with_id(app, "show", "Show app", true, None::<&str>).map_err(user_error)?;
    let pause = CheckMenuItem::with_id(
        app,
        "pause_new",
        "Pause new uploads and monitoring (this device)",
        true,
        paused,
        None::<&str>,
    )
    .map_err(user_error)?;
    let exit =
        MenuItem::with_id(app, "exit_review", "Exit…", true, None::<&str>).map_err(user_error)?;
    let menu = Menu::with_items(app, &[&open, &pause, &exit]).map_err(user_error)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("YouTube Upload Manager")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show(app),
            "exit_review" => {
                show(app);
                let _ = app.emit(EXIT_EVENT, ());
            }
            "pause_new" => {
                let app = app.clone();
                let state = app.state::<AppState>().inner().clone();
                let _ = spawn_worker(move || {
                    if let Ok(connection) = database(&state) {
                        if let Ok(paused) = admission_paused(&connection) {
                            let _ = set_pause(&state, !paused);
                            let _ = sync_tray(&app, &state);
                            let _ = app.emit(SETTINGS_EVENT, ());
                            let _ = app.emit("desktop-presence-admission-changed", ());
                        }
                    }
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app).map_err(|_|"The desktop tray could not be created. Window close will continue to ask for exit confirmation.".to_string())?;
    if let Some(runtime) = app.try_state::<RuntimeState>() {
        *runtime.pause_item.lock().map_err(user_error)? = Some(pause);
    }
    Ok(())
}
/// Initialize after the app state/database and the desktop plugins exist. Does
/// not alter OS login startup or request notification consent automatically.
pub(super) fn bootstrap(app: &AppHandle, state: &AppState) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use tauri_plugin_notification::NotificationExt;
        if app.try_state::<RuntimeState>().is_some() {
            return Ok(());
        }
        mark_interrupted_submissions(&database(state)?)?;
        let preferences = saved(&database(state)?)?.0;
        app.manage(RuntimeState {
            enabled: AtomicBool::new(preferences.notifications()),
            worker: AtomicBool::new(false),
            click_ready: AtomicBool::new(false),
            pause_item: Mutex::new(None),
        });
        let handle = app.clone();
        let click_state = state.clone();
        let click_registered = app
            .notification()
            .on_action(move |action| {
                if action.action_id() != "tap" {
                    return;
                }
                if let Some(route) = action
                    .notification()
                    .and_then(|notification| notification.extra().get("routeId"))
                    .and_then(|value| value.as_str())
                {
                    if activate(&click_state, route).is_ok() {
                        show(&handle);
                        let _ = handle.emit(ROUTE_EVENT, serde_json::json!({"routeId":route}));
                    }
                }
            })
            .is_ok();
        if let Some(runtime) = app.try_state::<RuntimeState>() {
            runtime
                .click_ready
                .store(click_registered, Ordering::Release);
        }
        let _ = sync_tray(app, state);
        wake_notifications(app, state)?;
    }
    let _ = (app, state);
    Ok(())
}

#[tauri::command]
pub(super) fn load_desktop_activity(
    state: State<'_, AppState>,
    expected_channel_id: String,
) -> Result<Vec<Activity>, String> {
    let connection = database(&state)?;
    if connection_settings(&connection)?
        .active_channel_id
        .as_deref()
        != Some(expected_channel_id.as_str())
    {
        return Err("Choose the activity's saved channel before viewing its receipts.".into());
    }
    let rows=connection.prepare("SELECT id,kind,count,outcome,created_at FROM desktop_notification_routes WHERE channel_id=?1 AND created_at>?2 ORDER BY created_at DESC,id LIMIT 25").map_err(user_error)?.query_map(params![expected_channel_id,(Utc::now()-ChronoDuration::days(7)).to_rfc3339()],|row|Ok(Activity{route_id:row.get(0)?,kind:row.get(1)?,count:row.get(2)?,outcome:row.get(3)?,created_at:row.get(4)?})).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    Ok(rows)
}
#[tauri::command]
pub(super) fn open_desktop_activity(
    app: AppHandle,
    state: State<'_, AppState>,
    route_id: String,
    expected_channel_id: String,
) -> Result<(), String> {
    let connection = database(&state)?;
    if connection_settings(&connection)?
        .active_channel_id
        .as_deref()
        != Some(expected_channel_id.as_str())
    {
        return Err("The connected channel changed. Open its activity again.".into());
    }
    let matched:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM desktop_notification_routes WHERE id=?1 AND channel_id=?2)",params![route_id,expected_channel_id],|row|row.get(0)).map_err(user_error)?;
    if !matched {
        return Err("This activity receipt belongs to another channel.".into());
    }
    activate(&state, &route_id)?;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    show(&app);
    let _ = app.emit(ROUTE_EVENT, serde_json::json!({"routeId":route_id}));
    Ok(())
}
#[tauri::command]
pub(super) fn dismiss_desktop_route(
    state: State<'_, AppState>,
    route_id: String,
) -> Result<(), String> {
    if Uuid::parse_str(&route_id).is_err() {
        return Err("The local route is invalid.".into());
    }
    database(&state)?.execute("UPDATE desktop_presence_settings SET pending_route_id=NULL WHERE singleton=1 AND pending_route_id=?1",[route_id]).map_err(user_error)?;
    Ok(())
}
/// Call after a durable state-change batch. Only an opt-in, meaningful batch
/// starts a two-second coalescing worker; settled idle owns no periodic timer.
pub(super) fn on_changes(
    app: &AppHandle,
    state: &AppState,
    changes: &[state_events::StateChange],
) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let Some(runtime) = app.try_state::<RuntimeState>() else {
            return Ok(());
        };
        if !runtime.enabled.load(Ordering::Acquire) {
            return Ok(());
        }
        let meaningful = changes.iter().any(|change| {
            matches!(
                change.surface.as_str(),
                "quota" | "api_quota" | "completion" | "assets"
            ) || (change.surface == "upload"
                && change
                    .payload
                    .get("status")
                    .and_then(|value| value.as_str())
                    .is_some_and(|status| {
                        matches!(status, "uploaded" | "failed" | "needs_reconciliation")
                    }))
        });
        if !meaningful {
            return Ok(());
        }
        observe_backlog(state)?;
        wake_notifications(app, state)?;
    }
    let _ = (app, state, changes);
    Ok(())
}
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn wake_notifications(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let Some(runtime) = app.try_state::<RuntimeState>() else {
        return Ok(());
    };
    if !runtime.enabled.load(Ordering::Acquire) {
        return Ok(());
    }
    let pending = has_pending(&database(state)?)?;
    if !pending {
        return Ok(());
    }
    if runtime
        .worker
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let app = app.clone();
        let state = state.clone();
        spawn_worker(move || loop {
            thread::sleep(Duration::from_secs(2));
            let mut platform = NativePlatform { app: &app };
            let success = observe_backlog(&state)
                .and_then(|_| flush_with(&state, &mut platform))
                .is_ok();
            let Some(runtime) = app.try_state::<RuntimeState>() else {
                break;
            };
            runtime.worker.store(false, Ordering::Release);
            let pending = success
                && runtime.enabled.load(Ordering::Acquire)
                && database(&state)
                    .and_then(|connection| has_pending(&connection))
                    .unwrap_or(false);
            if !pending
                || runtime
                    .worker
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                break;
            }
        });
    }
    Ok(())
}
#[tauri::command]
pub(super) fn load_desktop_presence(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Settings, String> {
    let mut platform = NativePlatform { app: &app };
    settings_with(&database(&state)?, &mut platform, tray_ready(&app))
}
fn tray_ready(app: &AppHandle) -> bool {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        app.tray_by_id(TRAY_ID).is_some()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = app;
        false
    }
}
fn mark_interrupted_submissions(connection: &Connection) -> Result<(), String> {
    connection.execute("UPDATE desktop_notification_routes SET outcome='submission_unknown' WHERE outcome='pending'",[]).map_err(user_error)?;
    Ok(())
}
#[tauri::command]
pub(super) async fn save_desktop_presence(
    app: AppHandle,
    state: State<'_, AppState>,
    preferences: Preferences,
    expected_revision: String,
    approved: bool,
    startup_approved: bool,
) -> Result<Settings, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut platform = NativePlatform { app: &app };
        let result = save_with(
            &state,
            preferences,
            &expected_revision,
            approved,
            startup_approved,
            &mut platform,
        );
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            if let Some(runtime) = app.try_state::<RuntimeState>() {
                runtime.enabled.store(
                    saved(&database(&state)?)?.0.notifications(),
                    Ordering::Release,
                );
            }
            let _ = sync_tray(&app, &state);
            wake_notifications(&app, &state)?;
        }
        let _ = app.emit(SETTINGS_EVENT, ());
        result?;
        settings_with(&database(&state)?, &mut platform, tray_ready(&app))
    })
    .await
    .map_err(user_error)?
}
#[tauri::command]
pub(super) fn set_desktop_presence_pause(
    app: AppHandle,
    state: State<'_, AppState>,
    paused: bool,
) -> Result<(), String> {
    set_pause(&state, paused)?;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = sync_tray(&app, &state);
    let _ = app.emit(SETTINGS_EVENT, ());
    let _ = app.emit("desktop-presence-admission-changed", ());
    if !paused {
        let _ = ensure_folder_monitor_worker(state.inner().clone());
        let _ = ensure_quota_resume_worker(state.inner().clone());
        let _ = start_queued_uploads_impl(&state);
    }
    Ok(())
}
#[tauri::command]
pub(super) fn handle_desktop_close(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let preferences = saved(&database(&state)?)?.0;
    if should_hide(&preferences, desktop(), tray_ready(&app)) {
        if let Some(window) = app.get_webview_window("main") {
            window.hide().map_err(user_error)?;
            return Ok(true);
        }
    }
    Ok(false)
}
fn should_hide(preferences: &Preferences, supported: bool, tray_ready: bool) -> bool {
    supported && preferences.tray_enabled && preferences.keep_running_on_close && tray_ready
}
#[tauri::command]
pub(super) fn load_pending_desktop_route(
    state: State<'_, AppState>,
) -> Result<Option<Route>, String> {
    pending_route(&database(&state)?)
}
#[tauri::command]
pub(super) fn acknowledge_desktop_route(
    state: State<'_, AppState>,
    route_id: String,
    expected_channel_id: String,
) -> Result<(), String> {
    let connection = database(&state)?;
    let route = pending_route(&connection)?.ok_or("No desktop route is waiting.")?;
    if route.route_id != route_id
        || route.channel_id != expected_channel_id
        || route.needs_channel_switch
    {
        return Err(
            "Switch explicitly to the route's saved channel before opening its upload details."
                .into(),
        );
    }
    connection.execute("UPDATE desktop_presence_settings SET pending_route_id=NULL WHERE singleton=1 AND pending_route_id=?1",[route_id]).map_err(user_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        root: PathBuf,
        state: AppState,
        connection: Option<Connection>,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("youtube-desktop-presence-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("media")).unwrap();
            let state = AppState {
                database_path: root.join("queue.sqlite3"),
                media_directory: root.join("media"),
                folder_monitor_lock: Arc::new(Mutex::new(())),
                oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
            };
            let connection = initialize_database(&state).unwrap();
            connection.execute_batch(SCHEMA_SQL).unwrap();
            connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture channel','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET active_channel_id='channel-a'",[now()]).unwrap();
            connection.execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,status,total_bytes,confirmed_bytes,video_id,resumable_session_uri,playlist_status,created_at,updated_at) VALUES('item-a','Private title','private-name.mp4','channel-a','D:/private/original.mp4','D:/private/original.mp4',3,'uploaded',3,3,'video-a','protected-checkpoint','not_requested',?1,?1),('item-b','Foreign private title','foreign-name.mp4','channel-b','foreign-locator','foreign-locator',3,'uploaded',3,3,'video-b','foreign-checkpoint','not_requested',?1,?1)",[now()]).unwrap();
            Self {
                root,
                state,
                connection: Some(connection),
            }
        }
        fn db(&self) -> &Connection {
            self.connection.as_ref().unwrap()
        }
        fn approve(&self, preferences: Preferences, platform: &mut MockPlatform) {
            let revision = saved(self.db()).unwrap().1;
            save_with(&self.state, preferences, &revision, true, false, platform).unwrap();
        }
        fn change(
            &self,
            channel: &str,
            item: &str,
            surface: &str,
            offset: u64,
        ) -> state_events::StateChange {
            state_events::StateChange {
                revision: state_events::current_revision(self.db()).unwrap() + offset,
                channel_id: channel.into(),
                surface: surface.into(),
                entity_id: item.into(),
                event_kind: "update".into(),
                payload: serde_json::json!({"status":"uploaded"}),
            }
        }
        fn routes(&self) -> usize {
            self.db()
                .query_row(
                    "SELECT COUNT(*) FROM desktop_notification_routes",
                    [],
                    |row| row.get(0),
                )
                .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(self.connection.take());
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    struct MockPlatform {
        supported: bool,
        permission: String,
        startup: bool,
        startup_read_failed: bool,
        startup_write_failed: bool,
        notification_failed: bool,
        startup_changes: Vec<bool>,
        notices: Vec<Notice>,
    }
    impl Default for MockPlatform {
        fn default() -> Self {
            Self {
                supported: true,
                permission: "desktop_os_controls_delivery".into(),
                startup: false,
                startup_read_failed: false,
                startup_write_failed: false,
                notification_failed: false,
                startup_changes: Vec::new(),
                notices: Vec::new(),
            }
        }
    }
    impl Platform for MockPlatform {
        fn supported(&self) -> bool {
            self.supported
        }
        fn permission(&mut self) -> String {
            self.permission.clone()
        }
        fn startup_enabled(&mut self) -> Result<bool, String> {
            if self.startup_read_failed {
                Err("Startup unavailable".into())
            } else {
                Ok(self.startup)
            }
        }
        fn set_startup(&mut self, enabled: bool) -> Result<(), String> {
            self.startup_changes.push(enabled);
            if self.startup_write_failed {
                Err("OS rejected startup".into())
            } else {
                self.startup = enabled;
                Ok(())
            }
        }
        fn notify(&mut self, notice: &Notice) -> Result<(), String> {
            self.notices.push(notice.clone());
            if self.notification_failed {
                Err("Submission failed".into())
            } else {
                Ok(())
            }
        }
    }
    fn opted_in() -> Preferences {
        Preferences {
            batch_complete: true,
            needs_attention: true,
            quota_pause: true,
            ..Preferences::default()
        }
    }
    #[test]
    fn defaults_are_opt_in_and_reading_never_changes_os_startup() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        let choices = settings_with(fixture.db(), &mut platform, false).unwrap();
        assert_eq!(choices.preferences, Preferences::default());
        assert!(!choices.pause_new_work);
        assert!(!choices.tray_ready);
        assert!(platform.startup_changes.is_empty());
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "item-a", "upload", 1)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
        assert!(platform.notices.is_empty());
    }
    #[test]
    fn independent_startup_approval_is_required_and_disablement_is_reversible() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        let preferences = Preferences {
            tray_enabled: true,
            keep_running_on_close: true,
            run_on_login: true,
            ..opted_in()
        };
        assert!(save_with(
            &fixture.state,
            preferences.clone(),
            "initial",
            true,
            false,
            &mut platform
        )
        .is_err());
        assert_eq!(saved(fixture.db()).unwrap().1, "initial");
        assert!(platform.startup_changes.is_empty());
        save_with(
            &fixture.state,
            preferences.clone(),
            "initial",
            true,
            true,
            &mut platform,
        )
        .unwrap();
        assert_eq!(platform.startup_changes, vec![true]);
        assert_eq!(saved(fixture.db()).unwrap().0, preferences);
        assert_eq!(
            settings_with(fixture.db(), &mut platform, true)
                .unwrap()
                .startup_status,
            "confirmed"
        );
        fixture.approve(Preferences::default(), &mut platform);
        assert_eq!(platform.startup_changes, vec![true, false]);
    }
    #[test]
    fn stale_unapproved_unsupported_and_invisible_background_choices_are_rejected() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        for (revision, approved, preferences) in [
            ("old", true, opted_in()),
            ("initial", false, opted_in()),
            (
                "initial",
                true,
                Preferences {
                    keep_running_on_close: true,
                    ..Preferences::default()
                },
            ),
        ] {
            assert!(save_with(
                &fixture.state,
                preferences,
                revision,
                approved,
                false,
                &mut platform
            )
            .is_err());
        }
        platform.supported = false;
        assert!(save_with(
            &fixture.state,
            opted_in(),
            "initial",
            true,
            false,
            &mut platform
        )
        .is_err());
        assert_eq!(saved(fixture.db()).unwrap().0, Preferences::default());
        assert!(platform.startup_changes.is_empty());
    }
    #[test]
    fn denied_startup_retains_approved_choices_and_restart_does_not_retry_os_change() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform {
            startup_write_failed: true,
            ..MockPlatform::default()
        };
        let preferences = Preferences {
            run_on_login: true,
            ..opted_in()
        };
        assert!(save_with(
            &fixture.state,
            preferences.clone(),
            "initial",
            true,
            true,
            &mut platform
        )
        .is_err());
        assert_eq!(saved(fixture.db()).unwrap().0, preferences);
        assert_eq!(saved(fixture.db()).unwrap().3, "needs_attention");
        mark_interrupted_submissions(fixture.db()).unwrap();
        let observed = settings_with(fixture.db(), &mut platform, false).unwrap();
        assert_eq!(observed.startup_enabled, Some(false));
        assert_eq!(platform.startup_changes, vec![true]);
    }
    #[test]
    fn unavailable_startup_inspection_does_not_block_unrelated_notification_choices() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform {
            startup_read_failed: true,
            ..MockPlatform::default()
        };
        fixture.approve(opted_in(), &mut platform);
        assert_eq!(saved(fixture.db()).unwrap().0, opted_in());
        assert_eq!(saved(fixture.db()).unwrap().3, "needs_attention");
        assert!(platform.startup_changes.is_empty());
    }
    #[test]
    fn pause_is_device_wide_and_preserves_queue_approvals_receipts_and_checkpoints() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        fixture.approve(opted_in(), &mut platform);
        set_pause(&fixture.state, true).unwrap();
        assert!(admission_paused(fixture.db()).unwrap());
        assert_eq!(saved(fixture.db()).unwrap().0, opted_in());
        let retained:(String,String,i64,String,String)=fixture.db().query_row("SELECT channel_id,video_id,confirmed_bytes,resumable_session_uri,source_path FROM upload_items WHERE id='item-a'",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
        assert_eq!(
            retained,
            (
                "channel-a".into(),
                "video-a".into(),
                3,
                "protected-checkpoint".into(),
                "D:/private/original.mp4".into()
            )
        );
        set_pause(&fixture.state, false).unwrap();
        assert!(!admission_paused(fixture.db()).unwrap());
        assert!(platform.startup_changes.is_empty());
    }
    #[test]
    fn grouped_notices_are_channel_scoped_with_opaque_safe_routes_and_cooldown() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        fixture.approve(opted_in(), &mut platform);
        enqueue(
            fixture.db(),
            "event-a",
            "channel-a",
            "needs_attention",
            &["item-a".into()],
        )
        .unwrap();
        enqueue(
            fixture.db(),
            "event-a",
            "channel-a",
            "needs_attention",
            &["item-a".into()],
        )
        .unwrap();
        enqueue(
            fixture.db(),
            "event-a2",
            "channel-a",
            "needs_attention",
            &["item-a".into()],
        )
        .unwrap();
        enqueue(
            fixture.db(),
            "event-b",
            "channel-b",
            "needs_attention",
            &["item-b".into()],
        )
        .unwrap();
        assert!(enqueue(
            fixture.db(),
            "foreign",
            "channel-a",
            "needs_attention",
            &["item-b".into()]
        )
        .is_err());
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 2);
        assert_eq!(fixture.routes(), 2);
        assert_eq!(platform.notices.len(), 2);
        for notice in &platform.notices {
            assert!(Uuid::parse_str(&notice.route_id).is_ok());
            let body = notice_body(&notice.kind, notice.count);
            assert!(!body.contains("private"));
            assert!(!body.contains("D:/"));
            assert!(!body.contains("checkpoint"));
        }
        enqueue(
            fixture.db(),
            "event-a3",
            "channel-a",
            "needs_attention",
            &["item-a".into()],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
        assert_eq!(platform.notices.len(), 2);
        assert_eq!(
            fixture
                .db()
                .query_row(
                    "SELECT count FROM desktop_notification_routes WHERE channel_id='channel-a'",
                    [],
                    |row| row.get::<_, usize>(0)
                )
                .unwrap(),
            3
        );
        let route: String = fixture
            .db()
            .query_row(
                "SELECT id FROM desktop_notification_routes WHERE channel_id='channel-b'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        activate(&fixture.state, &route).unwrap();
        let pending = pending_route(fixture.db()).unwrap().unwrap();
        assert!(pending.needs_channel_switch);
        assert!(pending.item_ids.is_empty());
        assert_eq!(pending.channel_id, "channel-b");
        fixture
            .db()
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b' WHERE singleton=1",
                [],
            )
            .unwrap();
        let pending = pending_route(fixture.db()).unwrap().unwrap();
        assert!(!pending.needs_channel_switch);
        assert_eq!(pending.item_ids, vec!["item-b"]);
    }
    #[test]
    fn denied_unknown_failed_and_interrupted_notifications_keep_receipts_without_replay() {
        for (permission, failed, outcome, calls) in [
            ("denied", false, "permission_denied", 0),
            ("unknown", false, "permission_unknown", 0),
            ("desktop_os_controls_delivery", true, "submission_failed", 1),
        ] {
            let fixture = Fixture::new();
            let mut platform = MockPlatform {
                permission: permission.into(),
                notification_failed: failed,
                ..MockPlatform::default()
            };
            fixture.approve(opted_in(), &mut platform);
            enqueue(
                fixture.db(),
                "event",
                "channel-a",
                "needs_attention",
                &["item-a".into()],
            )
            .unwrap();
            flush_with(&fixture.state, &mut platform).unwrap();
            assert_eq!(platform.notices.len(), calls);
            assert_eq!(
                fixture
                    .db()
                    .query_row(
                        "SELECT outcome FROM desktop_notification_routes",
                        [],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                outcome
            );
            mark_interrupted_submissions(fixture.db()).unwrap();
            assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
            assert_eq!(platform.notices.len(), calls);
            fixture
                .db()
                .execute(
                    "UPDATE desktop_notification_routes SET outcome='pending'",
                    [],
                )
                .unwrap();
            mark_interrupted_submissions(fixture.db()).unwrap();
            assert_eq!(
                fixture
                    .db()
                    .query_row(
                        "SELECT outcome FROM desktop_notification_routes",
                        [],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                "submission_unknown"
            );
            assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
        }
    }
    #[test]
    fn batch_notification_waits_for_every_confirmed_video_and_suppresses_progress() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        fixture.approve(opted_in(), &mut platform);
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET upload_batch_id='batch-a' WHERE id='item-a'",
                [],
            )
            .unwrap();
        fixture.db().execute("INSERT INTO upload_items(id,title,file_name,channel_id,workspace_path,size_bytes,status,total_bytes,upload_batch_id,created_at,updated_at) VALUES('item-a2','Second','second.mp4','channel-a','original',3,'queued',3,'batch-a',?1,?1)",[now()]).unwrap();
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "item-a", "upload", 1)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET status='uploaded',video_id='video-a2' WHERE id='item-a2'",
                [],
            )
            .unwrap();
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "item-a2", "upload", 2)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 1);
        assert_eq!(platform.notices[0].kind, "batch_complete");
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "item-a2", "upload", 3)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
        assert_eq!(platform.notices.len(), 1);
    }
    #[test]
    fn restart_backlog_observes_authoritative_attention_without_replaying_processed_events() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        fixture.approve(opted_in(), &mut platform);
        fixture.db().execute("UPDATE upload_items SET status='failed',detail='private payload that is never projected' WHERE id='item-b'",[]).unwrap();
        assert!(has_pending(fixture.db()).unwrap());
        observe_backlog(&fixture.state).unwrap();
        flush_with(&fixture.state, &mut platform).unwrap();
        assert_eq!(fixture.routes(), 1);
        assert_eq!(platform.notices.len(), 1);
        assert!(!has_pending(fixture.db()).unwrap());
        let restart = database(&fixture.state).unwrap();
        mark_interrupted_submissions(&restart).unwrap();
        observe_backlog(&fixture.state).unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
        assert_eq!(platform.notices.len(), 1);
    }
    #[test]
    fn quota_pause_is_grouped_once_per_saved_limit_and_never_changes_the_limit() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        fixture.approve(opted_in(), &mut platform);
        let until = (Utc::now() + ChronoDuration::hours(24)).to_rfc3339();
        fixture.db().execute("INSERT INTO upload_quota_pauses(channel_id,pause_until,updated_at) VALUES('channel-a',?1,?2)",params![until,now()]).unwrap();
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "channel-a", "quota", 1)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 1);
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "channel-a", "quota", 2)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
        assert_eq!(
            fixture
                .db()
                .query_row("SELECT pause_until FROM upload_quota_pauses", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            until
        );
        assert_eq!(platform.notices[0].kind, "quota_pause");
    }
    #[test]
    fn processing_and_asset_failures_have_safe_signatures_without_progress_toasts() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        fixture.approve(opted_in(), &mut platform);
        fixture.db().execute_batch("CREATE TABLE IF NOT EXISTS upload_completion_checks(channel_id TEXT,item_id TEXT,processing_status TEXT,updated_at TEXT,PRIMARY KEY(channel_id,item_id)); CREATE TABLE IF NOT EXISTS video_asset_jobs(id TEXT PRIMARY KEY,item_id TEXT,channel_id TEXT,status TEXT);").unwrap();
        fixture.db().execute("INSERT INTO upload_completion_checks(channel_id,item_id,processing_status,updated_at) VALUES('channel-a','item-a','failed',?1)",[now()]).unwrap();
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "item-a", "completion", 1)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 1);
        let signature: String = fixture
            .db()
            .query_row(
                "SELECT signature FROM desktop_item_status WHERE item_id='item-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(signature.contains("processing:failed"));
        assert!(!signature.contains("private"));
        fixture.db().execute("UPDATE upload_completion_checks SET processing_status='waiting' WHERE item_id='item-a'",[]).unwrap();
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "item-a", "completion", 2)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
        // Only the broker's authoritative status projection is exercised here.
        let asset_table_sql: String = fixture
            .db()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name='video_asset_jobs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        if asset_table_sql.contains("source_path") {
            fixture.db().execute("INSERT INTO video_asset_jobs(id,item_id,channel_id,video_id,kind,source_path,file_name,source_size,source_modified,source_identity,source_digest,declaration_key,status,detail,created_at,updated_at) VALUES('asset-a','item-a','channel-a','video-a','thumbnail','private-locator','private-file.png',3,'fixture','fixture','fixture','fixture','needs_reconciliation','private provider detail',?1,?1)",[now()]).unwrap();
        } else {
            fixture.db().execute("INSERT INTO video_asset_jobs(id,item_id,channel_id,status) VALUES('asset-a','item-a','channel-a','needs_reconciliation')",[]).unwrap();
        }
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "item-a", "assets", 3)],
        )
        .unwrap();
        flush_with(&fixture.state, &mut platform).unwrap();
        let signature: String = fixture
            .db()
            .query_row(
                "SELECT signature FROM desktop_item_status WHERE item_id='item-a'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(signature.contains("asset:asset-a:needs_reconciliation"));
        assert!(!signature.contains("private"));
        observe_changes(
            &fixture.state,
            &[fixture.change("channel-a", "item-a", "assets", 4)],
        )
        .unwrap();
        assert_eq!(flush_with(&fixture.state, &mut platform).unwrap(), 0);
    }
    #[test]
    fn expired_pending_routes_clear_and_close_requires_an_approved_actual_tray() {
        let fixture = Fixture::new();
        let mut platform = MockPlatform::default();
        fixture.approve(opted_in(), &mut platform);
        enqueue(
            fixture.db(),
            "event",
            "channel-a",
            "needs_attention",
            &["item-a".into()],
        )
        .unwrap();
        flush_with(&fixture.state, &mut platform).unwrap();
        let route = &platform.notices[0].route_id;
        activate(&fixture.state, route).unwrap();
        fixture
            .db()
            .execute(
                "UPDATE desktop_notification_routes SET created_at=?1",
                [(Utc::now() - ChronoDuration::days(8)).to_rfc3339()],
            )
            .unwrap();
        assert!(pending_route(fixture.db()).unwrap().is_none());
        assert!(activate(&fixture.state, route).is_err());
        let approved = Preferences {
            tray_enabled: true,
            keep_running_on_close: true,
            ..Preferences::default()
        };
        assert!(should_hide(&approved, true, true));
        assert!(!should_hide(&approved, true, false));
        assert!(!should_hide(&approved, false, true));
        assert!(!should_hide(&Preferences::default(), true, true));
    }
}
