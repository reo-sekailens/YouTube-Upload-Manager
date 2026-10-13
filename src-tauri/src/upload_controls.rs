//! Device-wide transfer pacing, local admission windows and aged scoped priorities.
//! Only upload bodies use the shared limiter. Google control reads stay unthrottled.
use super::*;
use chrono::{Datelike, Local, Timelike};
use std::collections::VecDeque;

const MAX_WINDOWS: usize = 14;
const MAX_PRIORITIES: usize = 200;
const MAX_RATE: u64 = 1024 * 1024 * 1024;
const READ_RESERVATION: usize = 64 * 1024;
const AGING_SECONDS: i64 = 5 * 60;

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS upload_transfer_controls (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1), settings_json TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0,
 reviewed_offset_minutes INTEGER NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS upload_priority_choices (
 channel_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('batch','folder')),
 target_id TEXT NOT NULL, priority INTEGER NOT NULL CHECK(priority BETWEEN 0 AND 5),
 updated_at TEXT NOT NULL, PRIMARY KEY(channel_id,kind,target_id)
);
"#;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LocalWindow {
    pub(super) days: Vec<u8>,
    pub(super) start: String,
    pub(super) end: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TimedBudget {
    pub(super) days: Vec<u8>,
    pub(super) start: String,
    pub(super) end: String,
    pub(super) bytes_per_second: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TransferSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) bytes_per_second: Option<u64>,
    #[serde(default)]
    pub(super) windows: Vec<LocalWindow>,
    #[serde(default)]
    pub(super) timed_limits: Vec<TimedBudget>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PriorityChoice {
    pub(super) kind: String,
    pub(super) id: String,
    pub(super) priority: u8,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PriorityTarget {
    kind: String,
    id: String,
    title: String,
    queued_items: u64,
    priority: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TransferOverview {
    channel_id: String,
    settings: TransferSettings,
    priorities: Vec<PriorityChoice>,
    targets: Vec<PriorityTarget>,
    time_zone: String,
    current_offset_minutes: i32,
    reviewed_offset_minutes: Option<i32>,
    timezone_changed: bool,
    updated_at: Option<String>,
    status: WindowStatus,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WindowStatus {
    pub(super) allowed: bool,
    pub(super) waiting_reason: Option<String>,
    pub(super) next_window_at: Option<String>,
    pub(super) effective_bytes_per_second: Option<u64>,
}

fn minute(value: &str) -> Result<u16, String> {
    let bytes = value.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 2 || c.is_ascii_digit())
    {
        return Err("Use a local time in HH:MM format.".into());
    }
    let hour = value[..2].parse::<u16>().map_err(user_error)?;
    let minute = value[3..].parse::<u16>().map_err(user_error)?;
    if hour >= 24 || minute >= 60 {
        return Err("Choose a valid local time between 00:00 and 23:59.".into());
    }
    Ok(hour * 60 + minute)
}

fn validate_window(days: &[u8], start: &str, end: &str) -> Result<(), String> {
    if days.is_empty()
        || days.len() > 7
        || days.iter().any(|day| *day > 6)
        || days.iter().collect::<HashSet<_>>().len() != days.len()
    {
        return Err("Choose each weekday at most once, with at least one selected day.".into());
    }
    if minute(start)? == minute(end)? {
        return Err(
            "Window start and end must differ. Remove all windows to allow uploads at any time."
                .into(),
        );
    }
    Ok(())
}

fn valid_rate(rate: u64) -> bool {
    (1024..=MAX_RATE).contains(&rate)
}

pub(super) fn validate(settings: &TransferSettings) -> Result<(), String> {
    if settings
        .bytes_per_second
        .is_some_and(|rate| !valid_rate(rate))
        || settings
            .timed_limits
            .iter()
            .any(|limit| !valid_rate(limit.bytes_per_second))
    {
        return Err("Choose an upload budget between 1 KiB/s and 1 GiB/s, or leave the global budget unlimited.".into());
    }
    if settings.windows.len() > MAX_WINDOWS || settings.timed_limits.len() > MAX_WINDOWS {
        return Err("Use at most 14 upload windows and 14 timed budgets.".into());
    }
    for window in &settings.windows {
        validate_window(&window.days, &window.start, &window.end)?;
    }
    for limit in &settings.timed_limits {
        validate_window(&limit.days, &limit.start, &limit.end)?;
    }
    Ok(())
}

/// An overnight window belongs to its selected start day. End is exclusive.
fn contains(days: &[u8], start: &str, end: &str, day: u8, current: u16) -> bool {
    let (Ok(start), Ok(end)) = (minute(start), minute(end)) else {
        return false;
    };
    if start < end {
        days.contains(&day) && current >= start && current < end
    } else {
        (days.contains(&day) && current >= start)
            || (days.contains(&((day + 6) % 7)) && current < end)
    }
}

pub(super) fn allowed_at_local(settings: &TransferSettings, day: u8, current: u16) -> bool {
    settings.windows.is_empty()
        || settings
            .windows
            .iter()
            .any(|window| contains(&window.days, &window.start, &window.end, day, current))
}

pub(super) fn rate_at_local(settings: &TransferSettings, day: u8, current: u16) -> Option<u64> {
    settings
        .timed_limits
        .iter()
        .filter(|limit| contains(&limit.days, &limit.start, &limit.end, day, current))
        .map(|limit| limit.bytes_per_second)
        .chain(settings.bytes_per_second)
        .min()
}

fn local_parts(time: DateTime<Utc>) -> (u8, u16) {
    let local = time.with_timezone(&Local);
    (
        local.weekday().num_days_from_sunday() as u8,
        (local.hour() * 60 + local.minute()) as u16,
    )
}

pub(super) fn window_status(settings: &TransferSettings, time: DateTime<Utc>) -> WindowStatus {
    let (day, current) = local_parts(time);
    let allowed = allowed_at_local(settings, day, current);
    let next_window_at = if allowed {
        None
    } else {
        let next_minute = time + ChronoDuration::seconds(60 - time.second() as i64);
        (0..=7 * 24 * 60)
            .map(|offset| next_minute + ChronoDuration::minutes(offset))
            .find(|candidate| {
                let (day, minute) = local_parts(*candidate);
                allowed_at_local(settings, day, minute)
            })
            .map(|time| time.to_rfc3339())
    };
    WindowStatus {
        allowed,
        waiting_reason: if allowed {
            None
        } else {
            Some("Waiting for approved local upload hours. An in-flight chunk finishes to its saved checkpoint before the next chunk waits.".into())
        },
        next_window_at,
        effective_bytes_per_second: rate_at_local(settings, day, current),
    }
}

fn require_channel(connection: &Connection, expected: &str) -> Result<(), String> {
    if expected.is_empty()
        || connection_settings(connection)?
            .active_channel_id
            .as_deref()
            != Some(expected)
    {
        return Err("The connected channel changed. Reopen upload controls before saving.".into());
    }
    Ok(())
}

pub(super) fn load_settings(connection: &Connection) -> Result<TransferSettings, String> {
    Ok(load_settings_revisioned(connection)?.0)
}

fn load_settings_revisioned(connection: &Connection) -> Result<(TransferSettings, u64), String> {
    let saved = connection
        .query_row(
            "SELECT settings_json,revision FROM upload_transfer_controls WHERE singleton=1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(user_error)?;
    let revision = saved.as_ref().map(|saved| saved.1).unwrap_or(0);
    let settings = saved
        .map(|(json, _)| {
            serde_json::from_str(&json).map_err(|_| {
                "Saved upload controls are invalid. Review new settings before transferring."
                    .to_string()
            })
        })
        .transpose()?
        .unwrap_or_default();
    validate(&settings)?;
    Ok((settings, revision))
}

pub(super) fn load_priorities(
    connection: &Connection,
    channel: &str,
) -> Result<Vec<PriorityChoice>, String> {
    require_channel(connection, channel)?;
    connection.prepare("SELECT kind,target_id,priority FROM upload_priority_choices AS choices WHERE channel_id=?1 AND priority>0 AND ((kind='folder' AND EXISTS(SELECT 1 FROM folder_monitors WHERE id=choices.target_id AND channel_id=?1 AND removed=0)) OR (kind='batch' AND EXISTS(SELECT 1 FROM upload_items WHERE upload_batch_id=choices.target_id AND channel_id=?1 AND status IN ('queued','dispatching','uploading','needs_reconciliation','failed')))) ORDER BY kind,target_id LIMIT 200").map_err(user_error)?.query_map([channel],|row|Ok(PriorityChoice{kind:row.get(0)?,id:row.get(1)?,priority:row.get(2)?})).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)
}

fn targets(
    connection: &Connection,
    channel: &str,
    priorities: &[PriorityChoice],
) -> Result<Vec<PriorityTarget>, String> {
    let mut result=connection.prepare("SELECT upload_batch_id,MIN(title),COUNT(*) FROM upload_items WHERE channel_id=?1 AND upload_batch_id IS NOT NULL AND status IN ('queued','dispatching','uploading','needs_reconciliation','failed') GROUP BY upload_batch_id ORDER BY MIN(created_at),upload_batch_id LIMIT 100").map_err(user_error)?.query_map([channel],|row|{let count=row.get::<_,i64>(2)?;Ok(PriorityTarget{kind:"batch".into(),id:row.get(0)?,title:if count>1{format!("{count} videos · {}",row.get::<_,String>(1)?)}else{row.get(1)?},queued_items:count as u64,priority:0})}).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    let folders=connection.prepare("SELECT monitors.id,monitors.folder_path,(SELECT COUNT(*) FROM upload_items WHERE channel_id=?1 AND folder_monitor_id=monitors.id AND status IN ('queued','dispatching','uploading','needs_reconciliation','failed')) FROM folder_monitors AS monitors WHERE channel_id=?1 AND removed=0 ORDER BY folder_path,id LIMIT 100").map_err(user_error)?.query_map([channel],|row|{let path=row.get::<_,String>(1)?;Ok(PriorityTarget{kind:"folder".into(),id:row.get(0)?,title:Path::new(&path).file_name().and_then(|name|name.to_str()).unwrap_or(&path).to_string(),queued_items:row.get::<_,i64>(2)? as u64,priority:0})}).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    result.extend(folders);
    for target in &mut result {
        target.priority = priorities
            .iter()
            .find(|choice| choice.kind == target.kind && choice.id == target.id)
            .map(|choice| choice.priority)
            .unwrap_or(0);
    }
    Ok(result)
}

fn overview(connection: &Connection, channel: &str) -> Result<TransferOverview, String> {
    require_channel(connection, channel)?;
    let settings = load_settings(connection)?;
    let priorities = load_priorities(connection, channel)?;
    let saved=connection.query_row("SELECT reviewed_offset_minutes,updated_at FROM upload_transfer_controls WHERE singleton=1",[],|row|Ok((row.get::<_,i32>(0)?,row.get::<_,String>(1)?))).optional().map_err(user_error)?;
    let offset = Local::now().offset().local_minus_utc() / 60;
    Ok(TransferOverview {
        channel_id: channel.into(),
        status: window_status(&settings, Utc::now()),
        targets: targets(connection, channel, &priorities)?,
        settings,
        priorities,
        time_zone: format!("Device local time · UTC{}", Local::now().format("%:z")),
        current_offset_minutes: offset,
        reviewed_offset_minutes: saved.as_ref().map(|saved| saved.0),
        timezone_changed: saved.as_ref().is_some_and(|saved| saved.0 != offset),
        updated_at: saved.map(|saved| saved.1),
    })
}

fn save(
    connection: &mut Connection,
    channel: &str,
    settings: TransferSettings,
    choices: Vec<PriorityChoice>,
    approved: bool,
) -> Result<TransferOverview, String> {
    if !approved {
        return Err("Review and approve upload controls before saving.".into());
    }
    validate(&settings)?;
    if choices.len() > MAX_PRIORITIES
        || choices.iter().any(|choice| {
            !matches!(choice.kind.as_str(), "batch" | "folder")
                || choice.id.is_empty()
                || choice.id.len() > 128
                || choice.priority > 5
        })
        || choices
            .iter()
            .map(|choice| (&choice.kind, &choice.id))
            .collect::<HashSet<_>>()
            .len()
            != choices.len()
    {
        return Err("Choose each scoped batch or folder once with a priority from 0 to 5.".into());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_channel(&transaction, channel)?;
    for choice in &choices {
        let exists=if choice.kind=="folder" {transaction.query_row("SELECT EXISTS(SELECT 1 FROM folder_monitors WHERE id=?1 AND channel_id=?2 AND removed=0)",params![choice.id,channel],|row|row.get::<_,bool>(0))}else{transaction.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE upload_batch_id=?1 AND channel_id=?2)",params![choice.id,channel],|row|row.get::<_,bool>(0))}.map_err(user_error)?;
        if !exists {
            return Err("A selected priority target no longer belongs to this channel. Refresh upload controls.".into());
        }
    }
    let updated = now();
    transaction.execute("INSERT INTO upload_transfer_controls(singleton,settings_json,revision,reviewed_offset_minutes,updated_at) VALUES(1,?1,1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET settings_json=excluded.settings_json,revision=upload_transfer_controls.revision+1,reviewed_offset_minutes=excluded.reviewed_offset_minutes,updated_at=excluded.updated_at",params![serde_json::to_string(&settings).map_err(user_error)?,Local::now().offset().local_minus_utc()/60,updated]).map_err(user_error)?;
    for choice in choices {
        if choice.priority == 0 {
            transaction.execute("DELETE FROM upload_priority_choices WHERE channel_id=?1 AND kind=?2 AND target_id=?3",params![channel,choice.kind,choice.id]).map_err(user_error)?;
        } else {
            transaction.execute("INSERT INTO upload_priority_choices(channel_id,kind,target_id,priority,updated_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(channel_id,kind,target_id) DO UPDATE SET priority=excluded.priority,updated_at=excluded.updated_at",params![channel,choice.kind,choice.id,choice.priority,updated]).map_err(user_error)?;
        }
    }
    transaction.execute("INSERT INTO audit_events(id,item_id,channel_id,channel_name,kind,detail,created_at) VALUES(?1,NULL,?2,(SELECT active_channel FROM connection_settings WHERE singleton=1 AND active_channel_id=?2),'upload_transfer_controls_saved','Operator approved device transfer budgets, local hours and scoped future-work priorities; claimed upload identity and checkpoints were retained',?3)",params![Uuid::new_v4().to_string(),channel,updated]).map_err(user_error)?;
    let revision = transaction
        .query_row(
            "SELECT revision FROM upload_transfer_controls WHERE singleton=1",
            [],
            |row| row.get::<_, u64>(0),
        )
        .map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    global_limiter().configure_revisioned(settings, revision)?;
    overview(connection, channel)
}

#[tauri::command]
pub(super) async fn load_upload_controls(
    state: State<'_, AppState>,
    expected_channel_id: String,
) -> Result<TransferOverview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || overview(&database(&state)?, &expected_channel_id))
        .await
        .map_err(|_| "Upload controls could not be loaded. Reopen the dialog.".to_string())?
}

#[tauri::command]
pub(super) async fn save_upload_controls(
    state: State<'_, AppState>,
    expected_channel_id: String,
    settings: TransferSettings,
    priorities: Vec<PriorityChoice>,
    approved: bool,
    expected_offset_minutes: Option<i32>,
) -> Result<TransferOverview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move||{
        if expected_offset_minutes!=Some(Local::now().offset().local_minus_utc()/60){return Err("The device timezone changed or was not reviewed. Reopen upload controls before saving local hours.".into());}
        let receipt = save(&mut database(&state)?,&expected_channel_id,settings,priorities,approved)?;
        let _ = ensure_quota_resume_worker(state.clone());
        let _ = start_queued_uploads_impl(&state);
        Ok(receipt)
    }).await.map_err(|_|"Saving upload controls stopped. Refresh before making further changes.".to_string())?
}

/// Recheck before each new claim/session/chunk. The caller persists this exact
/// waiting reason and keeps protected checkpoints; a current body may finish.
pub(super) fn transfer_window_status(
    state: &AppState,
    time: DateTime<Utc>,
) -> Result<WindowStatus, String> {
    let (settings, revision) = load_settings_revisioned(&database(state)?)?;
    global_limiter().configure_revisioned(settings.clone(), revision)?;
    Ok(window_status(&settings, time))
}

pub(super) struct PriorityCandidate<'a> {
    pub(super) item_id: &'a str,
    pub(super) created_at: &'a str,
    pub(super) batch_id: Option<&'a str>,
    pub(super) folder_id: Option<&'a str>,
}

/// Waiting age is bounded but exceeds the maximum manual boost. Stable oldest
/// work eventually wins over a stream of newly boosted work on the same volume.
pub(super) fn priority_score(
    candidate: &PriorityCandidate<'_>,
    choices: &[PriorityChoice],
    time: DateTime<Utc>,
) -> i64 {
    let manual = choices
        .iter()
        .filter(|choice| {
            (choice.kind == "batch" && Some(choice.id.as_str()) == candidate.batch_id)
                || (choice.kind == "folder" && Some(choice.id.as_str()) == candidate.folder_id)
        })
        .map(|choice| choice.priority.min(5) as i64)
        .max()
        .unwrap_or(0);
    let age = DateTime::parse_from_rfc3339(candidate.created_at)
        .map(|created| (time - created.with_timezone(&Utc)).num_seconds().max(0) / AGING_SECONDS)
        .unwrap_or(0)
        .min(6);
    manual.max(age)
}

pub(super) fn compare_priority(
    left: &PriorityCandidate<'_>,
    right: &PriorityCandidate<'_>,
    choices: &[PriorityChoice],
    time: DateTime<Utc>,
) -> std::cmp::Ordering {
    priority_score(right, choices, time)
        .cmp(&priority_score(left, choices, time))
        .then_with(|| {
            let timestamp = |value: &str| {
                DateTime::parse_from_rfc3339(value)
                    .map(|time| time.timestamp_millis())
                    .unwrap_or(i64::MAX)
            };
            timestamp(left.created_at).cmp(&timestamp(right.created_at))
        })
        .then_with(|| left.item_id.cmp(right.item_id))
}

struct BudgetState {
    settings: TransferSettings,
    settings_revision: Option<u64>,
    tokens: f64,
    last: Instant,
    last_rate: Option<u64>,
    next_ticket: u64,
    queue: VecDeque<u64>,
    observed_bytes: u64,
}

pub(super) struct UploadRateLimiter {
    state: Mutex<BudgetState>,
    wake: Condvar,
}

impl UploadRateLimiter {
    pub(super) fn new(settings: TransferSettings) -> Self {
        Self {
            state: Mutex::new(BudgetState {
                settings,
                settings_revision: None,
                tokens: 0.0,
                last: Instant::now(),
                last_rate: None,
                next_ticket: 0,
                queue: VecDeque::new(),
                observed_bytes: 0,
            }),
            wake: Condvar::new(),
        }
    }
    pub(super) fn configure(&self, settings: TransferSettings) -> Result<(), String> {
        validate(&settings)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Upload bandwidth controls are unavailable.".to_string())?;
        state.settings = settings;
        state.settings_revision = None;
        state.tokens = 0.0;
        state.last = Instant::now();
        state.last_rate = None;
        self.wake.notify_all();
        Ok(())
    }
    pub(super) fn configure_revisioned(
        &self,
        settings: TransferSettings,
        revision: u64,
    ) -> Result<(), String> {
        validate(&settings)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Upload bandwidth controls are unavailable.".to_string())?;
        if state
            .settings_revision
            .is_some_and(|current| current >= revision)
        {
            return Ok(());
        }
        state.settings = settings;
        state.settings_revision = Some(revision);
        state.tokens = 0.0;
        state.last = Instant::now();
        state.last_rate = None;
        self.wake.notify_all();
        Ok(())
    }
    fn reserve(&self, wanted: usize, cancelled: &dyn Fn() -> bool) -> std::io::Result<usize> {
        if wanted == 0 {
            return Ok(0);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| std::io::Error::other("Upload bandwidth controls are unavailable."))?;
        let ticket = state.next_ticket;
        state.next_ticket = state.next_ticket.wrapping_add(1);
        state.queue.push_back(ticket);
        loop {
            if cancelled() {
                state.queue.retain(|queued| *queued != ticket);
                self.wake.notify_all();
                return Err(std::io::Error::other(UPLOAD_CANCELLED_MARKER));
            }
            let (day, minute) = local_parts(Utc::now());
            let rate = rate_at_local(&state.settings, day, minute);
            let current = Instant::now();
            if rate != state.last_rate {
                state.tokens = 0.0;
                state.last = current;
                state.last_rate = rate;
            }
            if let Some(rate) = rate {
                let burst = (rate as f64 / 10.0).clamp(1.0, READ_RESERVATION as f64);
                state.tokens = (state.tokens
                    + current.duration_since(state.last).as_secs_f64() * rate as f64)
                    .min(burst);
            }
            state.last = current;
            if state.queue.front() == Some(&ticket) {
                let available = rate
                    .map(|_| state.tokens.floor() as usize)
                    .unwrap_or(wanted);
                if available > 0 {
                    let count = wanted.min(READ_RESERVATION).min(available);
                    if rate.is_some() {
                        state.tokens -= count as f64;
                    }
                    state.queue.pop_front();
                    state.observed_bytes = state.observed_bytes.saturating_add(count as u64);
                    self.wake.notify_all();
                    return Ok(count);
                }
            }
            let (next, _) = self
                .wake
                .wait_timeout(state, Duration::from_millis(10))
                .map_err(|_| std::io::Error::other("Upload bandwidth controls are unavailable."))?;
            state = next;
        }
    }
}

pub(super) fn global_limiter() -> Arc<UploadRateLimiter> {
    static LIMITER: OnceLock<Arc<UploadRateLimiter>> = OnceLock::new();
    LIMITER
        .get_or_init(|| Arc::new(UploadRateLimiter::new(TransferSettings::default())))
        .clone()
}

/// No additional full-chunk copy. Wrap the existing bounded body reader in
/// reqwest Body::sized(..., known_length), retaining one shared process budget.
pub(super) struct UploadBudgetReader<R> {
    inner: R,
    limiter: Arc<UploadRateLimiter>,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}
impl<R> UploadBudgetReader<R> {
    pub(super) fn new(inner: R, cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self {
            inner,
            limiter: global_limiter(),
            cancelled,
        }
    }
    #[cfg(test)]
    fn with_limiter(
        inner: R,
        limiter: Arc<UploadRateLimiter>,
        cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            inner,
            limiter,
            cancelled,
        }
    }
}
impl<R: Read> Read for UploadBudgetReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self
            .limiter
            .reserve(buffer.len(), self.cancelled.as_ref())?;
        self.inner.read(&mut buffer[..count])
    }
}

/// Low budgets must still let a provider-aligned chunk complete within the
/// existing 30-minute request timeout. Call only before a new range is claimed.
pub(super) fn recommended_chunk_bytes(
    settings: &TransferSettings,
    current: DateTime<Utc>,
    maximum: u64,
) -> u64 {
    let (day, minute) = local_parts(current);
    let Some(rate) = rate_at_local(settings, day, minute) else {
        return maximum;
    };
    let alignment = 256 * 1024;
    let target = (rate.saturating_mul(15 * 60) / alignment) * alignment;
    maximum.min(target.max(alignment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-upload-controls-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute_batch(SCHEMA_SQL).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture channel','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET active_channel='Fixture channel',active_channel_id='channel-a'",[now()]).unwrap();
        connection.execute("INSERT INTO upload_items(id,title,file_name,workspace_path,size_bytes,total_bytes,status,channel_id,upload_batch_id,confirmed_bytes,resumable_session_uri,created_at,updated_at) VALUES('a','Approved title','clip.mp4','original',2,2,'uploading','channel-a','batch-a',1,'protected-checkpoint',?1,?1),('b','Other channel private title','other.mp4','other-original',2,2,'queued','channel-b','batch-b',0,NULL,?1,?1)",[now()]).unwrap();
        connection.execute("INSERT INTO folder_monitors(id,channel_id,channel_name,folder_path,folder_key,enabled,visibility,made_for_kids,delete_source_after_upload,status,detail,updated_at) VALUES('folder-a','channel-a','Fixture channel','D:/Camera A','d:/camera a',1,'unlisted',0,0,'watching','Fixture',?1),('folder-b','channel-b','Other channel','D:/Private B','d:/private b',1,'unlisted',0,0,'watching','Fixture',?1)",[now()]).unwrap();
        (root, state, connection)
    }

    #[test]
    fn persisted_controls_scope_priorities_and_retain_claimed_source_and_checkpoint() {
        let (root, state, mut connection) = fixture();
        let choices = vec![
            PriorityChoice {
                kind: "batch".into(),
                id: "batch-a".into(),
                priority: 5,
            },
            PriorityChoice {
                kind: "folder".into(),
                id: "folder-a".into(),
                priority: 3,
            },
        ];
        let settings = TransferSettings {
            bytes_per_second: Some(128 * 1024),
            windows: vec![LocalWindow {
                days: vec![1, 2, 3, 4, 5],
                start: "09:00".into(),
                end: "17:00".into(),
            }],
            ..Default::default()
        };
        assert!(save(
            &mut connection,
            "channel-a",
            settings.clone(),
            choices.clone(),
            false
        )
        .is_err());
        let saved = save(
            &mut connection,
            "channel-a",
            settings.clone(),
            choices.clone(),
            true,
        )
        .unwrap();
        assert_eq!(saved.targets.len(), 2);
        assert!(saved.targets.iter().all(|target| target.id.ends_with('a')));
        let retained=connection.query_row("SELECT title,workspace_path,status,confirmed_bytes,resumable_session_uri FROM upload_items WHERE id='a'",[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?))).unwrap();
        assert_eq!(
            retained,
            (
                "Approved title".into(),
                "original".into(),
                "uploading".into(),
                1,
                "protected-checkpoint".into()
            )
        );
        assert!(save(
            &mut connection,
            "channel-a",
            TransferSettings::default(),
            vec![PriorityChoice {
                kind: "batch".into(),
                id: "batch-b".into(),
                priority: 5
            }],
            true
        )
        .is_err());
        assert_eq!(load_settings(&connection).unwrap(), settings);
        assert_eq!(load_priorities(&connection, "channel-a").unwrap(), choices);
        drop(connection);
        let restarted = database(&state).unwrap();
        assert_eq!(load_settings(&restarted).unwrap(), settings);
        assert_eq!(overview(&restarted, "channel-a").unwrap().targets.len(), 2);
        restarted
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        let other = overview(&restarted, "channel-b").unwrap();
        assert!(other.priorities.is_empty());
        assert!(other.targets.iter().all(|target| target.id.ends_with('b')));
        assert!(overview(&restarted, "channel-a").is_err());
        drop(restarted);
        global_limiter()
            .configure(TransferSettings::default())
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_saved_budget_snapshots_cannot_restore_an_older_higher_rate() {
        let limiter = UploadRateLimiter::new(TransferSettings::default());
        limiter
            .configure_revisioned(
                TransferSettings {
                    bytes_per_second: Some(256 * 1024),
                    ..Default::default()
                },
                1,
            )
            .unwrap();
        limiter
            .configure_revisioned(
                TransferSettings {
                    bytes_per_second: Some(64 * 1024),
                    ..Default::default()
                },
                2,
            )
            .unwrap();
        limiter
            .configure_revisioned(
                TransferSettings {
                    bytes_per_second: Some(256 * 1024),
                    ..Default::default()
                },
                1,
            )
            .unwrap();
        let state = limiter.state.lock().unwrap();
        assert_eq!(state.settings.bytes_per_second, Some(64 * 1024));
        assert_eq!(state.settings_revision, Some(2));
    }

    #[test]
    fn aged_work_remains_fifo_even_against_an_already_aged_urgent_backlog() {
        let time = DateTime::parse_from_rfc3339("2026-10-13T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let choices = vec![PriorityChoice {
            kind: "batch".into(),
            id: "urgent".into(),
            priority: 5,
        }];
        let old = PriorityCandidate {
            item_id: "old",
            created_at: "2026-10-13T10:00:00Z",
            batch_id: None,
            folder_id: None,
        };
        let urgent = PriorityCandidate {
            item_id: "new",
            created_at: "2026-10-13T11:00:00Z",
            batch_id: Some("urgent"),
            folder_id: None,
        };
        assert_eq!(priority_score(&old, &choices, time), 6);
        assert_eq!(priority_score(&urgent, &choices, time), 6);
        assert!(compare_priority(&old, &urgent, &choices, time).is_lt());
    }

    #[test]
    fn google_control_reads_are_responsive_while_a_video_reader_waits() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut header = Vec::new();
            let mut byte = [0];
            while !header.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
        });
        let limiter = Arc::new(UploadRateLimiter::new(TransferSettings {
            bytes_per_second: Some(1024),
            ..Default::default()
        }));
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let worker = thread::spawn(move || {
            let mut reader = UploadBudgetReader::with_limiter(
                Cursor::new(vec![1; 65536]),
                limiter,
                Arc::new(move || flag.load(Ordering::SeqCst)),
            );
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)
        });
        let started = Instant::now();
        let response = reqwest::blocking::Client::new()
            .get(format!("http://{address}/control"))
            .send()
            .unwrap();
        assert!(response.status().is_success());
        assert!(started.elapsed() < Duration::from_secs(1));
        cancelled.store(true, Ordering::SeqCst);
        assert!(worker.join().unwrap().is_err());
        server.join().unwrap();
    }

    #[test]
    fn local_windows_use_start_day_and_exclusive_end_including_overnight() {
        let settings = TransferSettings {
            windows: vec![LocalWindow {
                days: vec![1],
                start: "22:00".into(),
                end: "02:00".into(),
            }],
            ..Default::default()
        };
        assert!(!allowed_at_local(&settings, 1, 21 * 60 + 59));
        assert!(allowed_at_local(&settings, 1, 22 * 60));
        assert!(allowed_at_local(&settings, 2, 119));
        assert!(!allowed_at_local(&settings, 2, 120));
        assert!(!allowed_at_local(&settings, 3, 60));
        assert!(allowed_at_local(&TransferSettings::default(), 3, 60));
        for window in [
            LocalWindow {
                days: vec![1, 1],
                start: "09:00".into(),
                end: "17:00".into(),
            },
            LocalWindow {
                days: vec![1],
                start: "09:00".into(),
                end: "09:00".into(),
            },
            LocalWindow {
                days: vec![],
                start: "09:00".into(),
                end: "17:00".into(),
            },
        ] {
            assert!(validate(&TransferSettings {
                windows: vec![window],
                ..Default::default()
            })
            .is_err());
        }
    }

    #[test]
    fn overlapping_timed_budgets_apply_one_most_restrictive_device_rate() {
        let settings = TransferSettings {
            bytes_per_second: Some(256 * 1024),
            timed_limits: vec![
                TimedBudget {
                    days: vec![1],
                    start: "09:00".into(),
                    end: "18:00".into(),
                    bytes_per_second: 128 * 1024,
                },
                TimedBudget {
                    days: vec![1],
                    start: "12:00".into(),
                    end: "13:00".into(),
                    bytes_per_second: 64 * 1024,
                },
            ],
            ..Default::default()
        };
        assert_eq!(rate_at_local(&settings, 1, 12 * 60), Some(64 * 1024));
        assert_eq!(rate_at_local(&settings, 1, 18 * 60), Some(256 * 1024));
        assert_eq!(rate_at_local(&settings, 2, 12 * 60), Some(256 * 1024));
        assert!(validate(&TransferSettings {
            bytes_per_second: Some(0),
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn aged_priority_favors_urgent_work_but_does_not_starve_older_work() {
        let time = DateTime::parse_from_rfc3339("2026-10-13T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let choices = vec![PriorityChoice {
            kind: "batch".into(),
            id: "urgent".into(),
            priority: 5,
        }];
        let old = PriorityCandidate {
            item_id: "old",
            created_at: "2026-10-13T11:00:00Z",
            batch_id: Some("normal"),
            folder_id: None,
        };
        let urgent = PriorityCandidate {
            item_id: "new",
            created_at: "2026-10-13T12:00:00Z",
            batch_id: Some("urgent"),
            folder_id: None,
        };
        assert_eq!(priority_score(&old, &choices, time), 6);
        assert_eq!(priority_score(&urgent, &choices, time), 5);
        assert!(compare_priority(&old, &urgent, &choices, time).is_lt());
        let fresh = PriorityCandidate {
            item_id: "fresh",
            created_at: "2026-10-13T12:00:00Z",
            batch_id: Some("normal"),
            folder_id: None,
        };
        assert!(compare_priority(&fresh, &urgent, &choices, time).is_gt());
    }

    #[test]
    fn cancellation_wakes_waiting_read_and_keeps_other_workers_usable() {
        let limiter = Arc::new(UploadRateLimiter::new(TransferSettings {
            bytes_per_second: Some(1024),
            ..Default::default()
        }));
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let mut reader = UploadBudgetReader::with_limiter(
            Cursor::new(vec![1; 65536]),
            limiter.clone(),
            Arc::new(move || flag.load(Ordering::SeqCst)),
        );
        let worker = thread::spawn(move || {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes)
        });
        thread::sleep(Duration::from_millis(30));
        cancelled.store(true, Ordering::SeqCst);
        assert!(worker
            .join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains(UPLOAD_CANCELLED_MARKER));
        limiter.configure(TransferSettings::default()).unwrap();
        let mut reader = UploadBudgetReader::with_limiter(
            Cursor::new(vec![2; 100]),
            limiter,
            Arc::new(|| false),
        );
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, vec![2; 100]);
    }

    #[test]
    fn aggregate_budget_is_enforced_across_two_loopback_upload_bodies() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let limiter = Arc::new(UploadRateLimiter::new(TransferSettings {
            bytes_per_second: Some(256 * 1024),
            ..Default::default()
        }));
        let server = thread::spawn(move || {
            let workers=(0..2).map(|_|{let(mut stream,_)=listener.accept().unwrap();thread::spawn(move||{stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();let mut header=Vec::new();let mut byte=[0];while !header.ends_with(b"\r\n\r\n"){stream.read_exact(&mut byte).unwrap();header.push(byte[0]);}let mut received=0;let mut buffer=[0;8192];while received<65536{let read=stream.read(&mut buffer[..(65536-received).min(8192)]).unwrap();assert!(read>0,"Upload body ended early");received+=read;}stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok").unwrap();received})}).collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .sum::<usize>()
        });
        let started = Instant::now();
        let workers = (0..2)
            .map(|_| {
                let limiter = limiter.clone();
                thread::spawn(move || {
                    let reader = UploadBudgetReader::with_limiter(
                        Cursor::new(vec![1; 65536]),
                        limiter,
                        Arc::new(|| false),
                    );
                    let response = reqwest::blocking::Client::new()
                        .post(format!("http://{address}/upload"))
                        .body(reqwest::blocking::Body::sized(reader, 65536))
                        .send()
                        .unwrap();
                    assert!(response.status().is_success());
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(server.join().unwrap(), 131072);
        assert!(
            started.elapsed() >= Duration::from_millis(450),
            "Two workers exceeded their aggregate device budget"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn waiting_reason_and_next_window_are_truthful_at_boundary() {
        let now = Utc::now();
        let (day, current) = local_parts(now);
        let start = (current + 2) % 1440;
        let end = (start + 2) % 1440;
        let settings = TransferSettings {
            windows: vec![LocalWindow {
                days: vec![day],
                start: format!("{:02}:{:02}", start / 60, start % 60),
                end: format!("{:02}:{:02}", end / 60, end % 60),
            }],
            ..Default::default()
        };
        let status = window_status(&settings, now);
        assert!(!status.allowed);
        assert!(status.waiting_reason.unwrap().contains("checkpoint"));
        assert!(
            DateTime::parse_from_rfc3339(&status.next_window_at.unwrap())
                .unwrap()
                .with_timezone(&Utc)
                > now
        );
    }
}
