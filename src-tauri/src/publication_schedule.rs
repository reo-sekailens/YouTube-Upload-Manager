//! Reviewed UTC publication jobs. Uploads stay private until an owned, processed
//! video receives a separately authorized and confirmed server-side schedule.
use super::*;
use chrono::{LocalResult, NaiveDateTime, TimeZone};
use chrono_tz::Tz;

const MAX_FILES: usize = 512;
const PREVIEW_MARGIN_SECONDS: i64 = 5 * 60;
const WRITE_MARGIN_SECONDS: i64 = 2 * 60;
const MAX_ERROR_BYTES: u64 = 64 * 1024;

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS publication_schedule_jobs (
 item_id TEXT PRIMARY KEY, channel_id TEXT NOT NULL, requested_publish_at TEXT NOT NULL,
 reviewed_time_zone TEXT NOT NULL, reviewed_local_time TEXT NOT NULL, tzdb_version TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'waiting_upload', video_id TEXT,
 provider_confirmed_publish_at TEXT, ever_public INTEGER NOT NULL DEFAULT 0,
 detail TEXT NOT NULL, attempt_count INTEGER NOT NULL DEFAULT 0,
 next_check_at TEXT, checked_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
 FOREIGN KEY(item_id) REFERENCES upload_items(id)
);
CREATE INDEX IF NOT EXISTS publication_schedule_pending ON publication_schedule_jobs(channel_id,state,next_check_at);
CREATE TABLE IF NOT EXISTS publication_schedule_history (
 id TEXT PRIMARY KEY, item_id TEXT NOT NULL, channel_id TEXT NOT NULL,
 previous_publish_at TEXT, requested_publish_at TEXT NOT NULL,
 reviewed_time_zone TEXT NOT NULL, tzdb_version TEXT NOT NULL, kind TEXT NOT NULL, created_at TEXT NOT NULL
);
"#;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicationContext {
    pub(super) time_zone: String,
    pub(super) tzdb_version: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TimeCandidate {
    choice: String,
    publish_at: String,
    local_time: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CalendarRow {
    index: usize,
    local_time: String,
    state: String,
    publish_at: Option<String>,
    candidates: Vec<TimeCandidate>,
    detail: Option<String>,
    preserved: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CalendarPreview {
    channel_id: String,
    time_zone: String,
    tzdb_version: String,
    rows: Vec<CalendarRow>,
    ready: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicationJob {
    pub(super) item_id: String,
    pub(super) channel_id: String,
    pub(super) title: String,
    pub(super) file_name: String,
    pub(super) publish_at: String,
    pub(super) time_zone: String,
    pub(super) local_time: String,
    pub(super) tzdb_version: String,
    pub(super) state: String,
    pub(super) video_id: Option<String>,
    pub(super) provider_confirmed_publish_at: Option<String>,
    pub(super) ever_public: bool,
    pub(super) detail: String,
    pub(super) updated_at: String,
    pub(super) checked_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PublicationPage {
    channel_id: String,
    jobs: Vec<PublicationJob>,
    total: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PublicationEdit {
    item_id: String,
    expected_publish_at: String,
    expected_updated_at: String,
    publish_at: String,
    context: PublicationContext,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn require_channel(connection: &Connection, channel: &str) -> Result<(), String> {
    if !valid_id(channel)
        || connection_settings(connection)?
            .active_channel_id
            .as_deref()
            != Some(channel)
    {
        return Err("The connected channel changed. Reopen the publication calendar.".into());
    }
    Ok(())
}
fn utc(value: &str) -> Result<DateTime<Utc>, String> {
    if value.len() > 64 {
        return Err("Use a valid publication timestamp with a timezone.".into());
    }
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| "Use a valid publication timestamp with a timezone.".into())
}
fn timezone(context: &PublicationContext) -> Result<Tz, String> {
    if context.tzdb_version != chrono_tz::IANA_TZDB_VERSION {
        return Err(
            "Timezone rules changed since the review. Resolve the dates again before saving."
                .into(),
        );
    }
    context.time_zone.parse::<Tz>().map_err(|_| {
        "Choose a known IANA timezone, such as Asia/Singapore or America/New_York.".into()
    })
}
fn local(value: &str) -> Result<NaiveDateTime, String> {
    if value.len() != 16 {
        return Err("Choose a local date and time with minute precision.".into());
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .map_err(|_| "Choose a valid local date and time.".into())
}
fn future(time: DateTime<Utc>, now: DateTime<Utc>, margin: i64) -> Result<(), String> {
    if time <= now + ChronoDuration::seconds(margin) {
        return Err(format!("Choose a publication time more than {} minutes ahead. Late times are held for review and never shifted automatically.",margin/60));
    }
    Ok(())
}

fn resolve_row(
    index: usize,
    value: &str,
    zone: Tz,
    choice: Option<&str>,
    now: DateTime<Utc>,
) -> CalendarRow {
    let mut row = CalendarRow {
        index,
        local_time: value.into(),
        state: "invalid".into(),
        publish_at: None,
        candidates: vec![],
        detail: None,
        preserved: false,
    };
    let naive = match local(value) {
        Ok(time) => time,
        Err(error) => {
            row.detail = Some(error);
            return row;
        }
    };
    let candidates = match zone.from_local_datetime(&naive) {
        LocalResult::None => {
            row.state = "gap".into();
            row.detail=Some("This local time does not exist because the clock moves forward. Choose another local time; the app will not move it automatically.".into());
            return row;
        }
        LocalResult::Single(time) => vec![("only", time)],
        LocalResult::Ambiguous(first, second) => {
            let (mut first, mut second) = (first, second);
            if first > second {
                std::mem::swap(&mut first, &mut second);
            }
            vec![("earlier", first), ("later", second)]
        }
    };
    row.candidates = candidates
        .iter()
        .map(|(choice, time)| TimeCandidate {
            choice: (*choice).into(),
            publish_at: time.with_timezone(&Utc).to_rfc3339(),
            local_time: time.to_rfc3339(),
        })
        .collect();
    let selected = if candidates.len() == 1 {
        Some(candidates[0].1)
    } else {
        candidates
            .iter()
            .find(|(candidate, _)| Some(*candidate) == choice)
            .map(|(_, time)| *time)
    };
    let Some(selected) = selected else {
        row.state = "ambiguous".into();
        row.detail=Some("The clock repeats this local time. Choose the earlier or later UTC instant explicitly.".into());
        return row;
    };
    let time = selected.with_timezone(&Utc);
    if let Err(error) = future(time, now, PREVIEW_MARGIN_SECONDS) {
        row.state = "past".into();
        row.detail = Some(error);
        return row;
    }
    row.state = "ready".into();
    row.publish_at = Some(time.to_rfc3339());
    row
}

fn preview(
    channel: &str,
    zone: &str,
    start: &str,
    spacing: u64,
    count: usize,
    overrides: &HashMap<usize, String>,
    choices: &HashMap<usize, String>,
    preserved: &HashMap<usize, String>,
    now: DateTime<Utc>,
) -> Result<CalendarPreview, String> {
    if count == 0
        || count > MAX_FILES
        || spacing > 7 * 24 * 60
        || overrides
            .keys()
            .chain(choices.keys())
            .chain(preserved.keys())
            .any(|index| *index >= count)
    {
        return Err("Resolve 1–512 selected videos with spacing of at most one week and only selected item indices.".into());
    }
    if choices
        .values()
        .any(|choice| !matches!(choice.as_str(), "earlier" | "later"))
    {
        return Err("Choose earlier or later for repeated local times.".into());
    }
    let zone = zone
        .parse::<Tz>()
        .map_err(|_| "Choose a known IANA timezone.".to_string())?;
    let base = local(start)?;
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        if let Some(value) = preserved
            .get(&index)
            .filter(|_| !overrides.contains_key(&index))
        {
            let time = utc(value)?;
            let mut row=CalendarRow{index,local_time:time.with_timezone(&zone).format("%Y-%m-%dT%H:%M").to_string(),state:"ready".into(),publish_at:Some(time.to_rfc3339()),candidates:vec![],detail:Some("The existing approved UTC instant is unchanged; this timezone changes its display only.".into()),preserved:true};
            if let Err(error) = future(time, now, PREVIEW_MARGIN_SECONDS) {
                row.state = "past".into();
                row.detail = Some(error);
                row.publish_at = None;
            }
            rows.push(row);
            continue;
        }
        let value = match overrides.get(&index) {
            Some(value) => value.clone(),
            None => base
                .checked_add_signed(ChronoDuration::minutes(spacing as i64 * index as i64))
                .ok_or("The chosen start and spacing exceed the supported calendar range.")?
                .format("%Y-%m-%dT%H:%M")
                .to_string(),
        };
        rows.push(resolve_row(
            index,
            &value,
            zone,
            choices.get(&index).map(String::as_str),
            now,
        ));
    }
    Ok(CalendarPreview {
        channel_id: channel.into(),
        time_zone: zone.name().into(),
        tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        ready: rows.iter().all(|row| row.state == "ready"),
        rows,
    })
}

#[tauri::command]
pub(super) async fn resolve_publication_calendar(
    state: State<'_, AppState>,
    expected_channel_id: String,
    time_zone: String,
    start_local: String,
    spacing_minutes: u64,
    count: usize,
    item_local_times: Option<HashMap<usize, String>>,
    ambiguous_choices: Option<HashMap<usize, String>>,
    preserved_times: Option<HashMap<usize, String>>,
) -> Result<CalendarPreview, String> {
    require_channel(&database(&state)?, &expected_channel_id)?;
    let result = preview(
        &expected_channel_id,
        &time_zone,
        &start_local,
        spacing_minutes,
        count,
        &item_local_times.unwrap_or_default(),
        &ambiguous_choices.unwrap_or_default(),
        &preserved_times.unwrap_or_default(),
        Utc::now(),
    )?;
    require_channel(&database(&state)?, &expected_channel_id)?;
    Ok(result)
}

#[tauri::command]
pub(super) fn list_publication_timezones() -> Vec<String> {
    chrono_tz::TZ_VARIANTS
        .iter()
        .map(|zone| zone.name().to_string())
        .collect()
}

pub(super) fn capture_requested_job(
    connection: &Connection,
    item: &str,
    channel: &str,
    publish_at: &str,
) -> Result<(), String> {
    capture_with_context(connection, item, channel, publish_at, None)
}

pub(super) fn capture_with_context(
    connection: &Connection,
    item: &str,
    channel: &str,
    publish_at: &str,
    context: Option<&PublicationContext>,
) -> Result<(), String> {
    require_channel(connection, channel)?;
    let time = utc(publish_at)?;
    future(time, Utc::now(), PREVIEW_MARGIN_SECONDS)?;
    let context = context.cloned().unwrap_or(PublicationContext {
        time_zone: "UTC".into(),
        tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
    });
    let zone = timezone(&context)?;
    let eligible:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE id=?1 AND channel_id=?2 AND visibility='private' AND video_id IS NULL AND folder_monitor_id IS NULL AND status IN ('draft','importing','queued'))",params![item,channel],|row|row.get(0)).map_err(user_error)?;
    if !eligible {
        return Err("A publication request requires a newly reviewed private manual upload. Existing public/unlisted videos and automatic watched uploads are not scheduled silently.".into());
    }
    let timestamp = now();
    let canonical = time.to_rfc3339();
    let local = time.with_timezone(&zone).to_rfc3339();
    connection.execute("INSERT INTO publication_schedule_jobs(item_id,channel_id,requested_publish_at,reviewed_time_zone,reviewed_local_time,tzdb_version,state,detail,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,'waiting_upload','Private upload and YouTube processing must complete before a separately authorized publication schedule is applied.',?7,?7)",params![item,channel,canonical,context.time_zone,local,context.tzdb_version,timestamp]).map_err(|_|"A publication request already exists for this upload. Review its retained calendar entry instead.".to_string())?;
    record_history(
        connection,
        item,
        channel,
        None,
        &canonical,
        &context,
        "requested",
    )?;
    audit(connection,item,"publication_requested","Operator approved an immutable UTC publication time; upload stays private until a separate confirmed schedule job succeeds")?;
    Ok(())
}

fn record_history(
    connection: &Connection,
    item: &str,
    channel: &str,
    previous: Option<&str>,
    requested: &str,
    context: &PublicationContext,
    kind: &str,
) -> Result<(), String> {
    connection.execute("INSERT INTO publication_schedule_history(id,item_id,channel_id,previous_publish_at,requested_publish_at,reviewed_time_zone,tzdb_version,kind,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![Uuid::new_v4().to_string(),item,channel,previous,requested,context.time_zone,context.tzdb_version,kind,now()]).map_err(user_error)?;
    Ok(())
}

fn load_job(connection: &Connection, item: &str, channel: &str) -> Result<PublicationJob, String> {
    connection.query_row("SELECT jobs.item_id,jobs.channel_id,uploads.title,uploads.file_name,jobs.requested_publish_at,jobs.reviewed_time_zone,jobs.reviewed_local_time,jobs.tzdb_version,jobs.state,jobs.video_id,jobs.provider_confirmed_publish_at,jobs.ever_public,jobs.detail,jobs.updated_at,jobs.checked_at FROM publication_schedule_jobs AS jobs JOIN upload_items AS uploads ON uploads.id=jobs.item_id AND uploads.channel_id=jobs.channel_id WHERE jobs.item_id=?1 AND jobs.channel_id=?2",params![item,channel],|row|Ok(PublicationJob{item_id:row.get(0)?,channel_id:row.get(1)?,title:row.get(2)?,file_name:row.get(3)?,publish_at:row.get(4)?,time_zone:row.get(5)?,local_time:row.get(6)?,tzdb_version:row.get(7)?,state:row.get(8)?,video_id:row.get(9)?,provider_confirmed_publish_at:row.get(10)?,ever_public:row.get::<_,i64>(11)?!=0,detail:row.get(12)?,updated_at:row.get(13)?,checked_at:row.get(14)?})).map_err(|_|"This publication request is unavailable for the connected channel.".to_string())
}

fn page(
    connection: &Connection,
    channel: &str,
    item_ids: Option<&[String]>,
    offset: usize,
) -> Result<PublicationPage, String> {
    require_channel(connection, channel)?;
    if item_ids
        .is_some_and(|ids| ids.is_empty() || ids.len() > 200 || ids.iter().any(|id| !valid_id(id)))
    {
        return Err("Choose 1–200 retained publication requests.".into());
    }
    let selected = item_ids
        .map(|ids| serde_json::to_string(ids).map_err(user_error))
        .transpose()?;
    let total=connection.query_row("SELECT COUNT(*) FROM publication_schedule_jobs WHERE channel_id=?1 AND (?2 IS NULL OR item_id IN (SELECT value FROM json_each(?2)))",params![channel,selected],|row|row.get::<_,i64>(0)).map_err(user_error)? as usize;
    let ids=connection.prepare("SELECT item_id FROM publication_schedule_jobs WHERE channel_id=?1 AND (?2 IS NULL OR item_id IN (SELECT value FROM json_each(?2))) ORDER BY requested_publish_at,item_id LIMIT 50 OFFSET ?3").map_err(user_error)?.query_map(params![channel,selected,offset as i64],|row|row.get::<_,String>(0)).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    let jobs = ids
        .into_iter()
        .map(|id| load_job(connection, &id, channel))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PublicationPage {
        channel_id: channel.into(),
        jobs,
        total,
    })
}

#[tauri::command]
pub(super) fn load_publication_calendar(
    state: State<'_, AppState>,
    expected_channel_id: String,
    item_ids: Option<Vec<String>>,
    offset: Option<usize>,
) -> Result<PublicationPage, String> {
    page(
        &database(&state)?,
        &expected_channel_id,
        item_ids.as_deref(),
        offset.unwrap_or(0),
    )
}

fn edit_requests(
    connection: &mut Connection,
    channel: &str,
    changes: &[PublicationEdit],
    approved: bool,
) -> Result<Vec<PublicationJob>, String> {
    if !approved
        || changes.is_empty()
        || changes.len() > 200
        || changes
            .iter()
            .map(|change| &change.item_id)
            .collect::<HashSet<_>>()
            .len()
            != changes.len()
    {
        return Err("Review and approve 1–200 distinct publication changes.".into());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_channel(&transaction, channel)?;
    for change in changes {
        let current = load_job(&transaction, &change.item_id, channel)?;
        let time = utc(&change.publish_at)?;
        future(time, Utc::now(), PREVIEW_MARGIN_SECONDS)?;
        let zone = timezone(&change.context)?;
        if current.publish_at != change.expected_publish_at
            || current.updated_at != change.expected_updated_at
            || current.ever_public
            || matches!(
                current.state.as_str(),
                "applying" | "uncertain" | "published"
            )
        {
            return Err("A schedule changed, was published, or has an unknown provider outcome. Reconcile its receipt before editing.".into());
        }
        let canonical = time.to_rfc3339();
        let same = time == utc(&current.publish_at)?;
        transaction.execute("UPDATE publication_schedule_jobs SET requested_publish_at=?1,reviewed_time_zone=?2,reviewed_local_time=?3,tzdb_version=?4,state=CASE WHEN ?5 THEN state WHEN video_id IS NULL THEN 'waiting_upload' ELSE 'pending' END,detail=CASE WHEN ?5 THEN detail ELSE 'Operator reviewed a replacement UTC time. The existing private video and prior confirmed schedule are retained until fresh owner verification.' END,next_check_at=NULL,updated_at=?6 WHERE item_id=?7 AND channel_id=?8",params![canonical,change.context.time_zone,time.with_timezone(&zone).to_rfc3339(),change.context.tzdb_version,same,now(),change.item_id,channel]).map_err(user_error)?;
        record_history(
            &transaction,
            &change.item_id,
            channel,
            Some(&current.publish_at),
            &canonical,
            &change.context,
            if same {
                "display_zone_reviewed"
            } else {
                "time_reviewed"
            },
        )?;
        audit(&transaction,&change.item_id,"publication_time_reviewed","Operator explicitly reviewed the publication time; source, upload identity, video receipt and checkpoints were retained")?;
    }
    transaction.commit().map_err(user_error)?;
    changes
        .iter()
        .map(|change| load_job(connection, &change.item_id, channel))
        .collect()
}

#[tauri::command]
pub(super) async fn edit_publication_calendar(
    state: State<'_, AppState>,
    expected_channel_id: String,
    changes: Vec<PublicationEdit>,
    approved: bool,
) -> Result<Vec<PublicationJob>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        edit_requests(
            &mut database(&state)?,
            &expected_channel_id,
            &changes,
            approved,
        )
    })
    .await
    .map_err(|_| {
        "Calendar editing stopped. Refresh the retained requests before retrying.".to_string()
    })?
}

pub(super) fn recover_interrupted(connection: &Connection) -> Result<(), String> {
    connection.execute("UPDATE publication_schedule_jobs SET state='uncertain',detail='Publication write was interrupted. Reconcile the current owned video before any retry.',next_check_at=NULL,updated_at=?1 WHERE state='applying'",[now()]).map_err(user_error)?;
    Ok(())
}

struct ProviderSnapshot {
    video: serde_json::Value,
    server_time: DateTime<Utc>,
}
trait ScheduleProvider {
    fn read(&mut self, channel: &str, video: &str) -> Result<ProviderSnapshot, String>;
    fn write(
        &mut self,
        channel: &str,
        video: &str,
        payload: &serde_json::Value,
        etag: &str,
    ) -> Result<(), (String, bool)>;
}
struct NativeProvider<'a> {
    state: &'a AppState,
    clock: Option<(DateTime<Utc>, Instant)>,
}

fn bounded_error_body(response: impl Read) -> String {
    let mut bytes = Vec::new();
    if response
        .take(MAX_ERROR_BYTES)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return String::new();
    }
    String::from_utf8(bytes).unwrap_or_default()
}

fn record_response_quota(
    state: &AppState,
    channel: &str,
    scope: &str,
    status: u16,
    body: &str,
) -> Result<Option<String>, String> {
    if youtube_limits::classify(status, body) != Some(youtube_limits::UploadLimitKind::ApiQuota) {
        return Ok(None);
    }
    let deadline = oauth_replacement::record_scoped_api_pause(
        state,
        channel,
        scope,
        None,
        youtube_limits::includes_channel_limit(status, body),
    )?;
    Ok(Some(format!("Google Cloud API quota is paused until {deadline}. The publication job and confirmed private upload receipt are retained; no upload was restarted.")))
}

fn publication_write_error(
    state: &AppState,
    channel: &str,
    scope: &str,
    status: u16,
    body: &str,
) -> (String, bool) {
    match record_response_quota(state, channel, scope, status, body) {
        Ok(Some(detail)) => return (detail, false),
        Err(detail) => return (detail, false),
        Ok(None) => {}
    }
    (match status {
        412 => "YouTube metadata changed after its fresh read. Review the current video before changing its publication time.",
        500..=599 => "YouTube is temporarily unavailable and the publication write has an unknown outcome. Reconcile the existing owned video before retrying.",
        _ => "YouTube rejected the publication request. The reviewed time and private upload receipt were retained.",
    }.into(), (500..=599).contains(&status))
}
impl NativeProvider<'_> {
    fn ensure(&self, channel: &str) -> Result<(), String> {
        require_channel(&database(self.state)?, channel)
    }
    fn provider_now(&self) -> Result<DateTime<Utc>, String> {
        let (time, observed) = self
            .clock
            .ok_or("Google's current clock could not be verified; the video remains private.")?;
        Ok(time + ChronoDuration::from_std(observed.elapsed()).map_err(user_error)?)
    }
}
impl ScheduleProvider for NativeProvider<'_> {
    fn read(&mut self, channel: &str, video: &str) -> Result<ProviderSnapshot, String> {
        self.ensure(channel)?;
        let token = refreshed_access_token(self.state)?;
        let credential_guard = oauth_replacement::lock_credentials()?;
        oauth_replacement::ensure_no_pending(self.state)?;
        self.ensure(channel)?;
        let scope = oauth_replacement::configured_quota_scope(&database(self.state)?)?;
        if active_channel_id(self.state, &token)? != channel {
            return Err("Google's credential does not confirm the publication channel.".into());
        }
        self.ensure(channel)?;
        let observed = Instant::now();
        let request = provider_transport(self.state)
            .control_client()?
            .get("https://www.googleapis.com/youtube/v3/videos")
            .query(&[("part", "snippet,status,processingDetails"), ("id", video)])
            .header(reqwest::header::CACHE_CONTROL, "no-cache")
            .bearer_auth(token);
        let response = observed_youtube_request(self.state, "videos.list", || request.send())
            .map_err(|_| "YouTube could not be read for publication verification.".to_string())?;
        drop(credential_guard);
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = bounded_error_body(response);
            return Err(record_response_quota(self.state,channel,&scope,status,&body)?.unwrap_or_else(||"YouTube did not return the owned video for publication verification. Its retained publication request remains unchanged.".into()));
        }
        let date = response
            .headers()
            .get(reqwest::header::DATE)
            .and_then(|date| date.to_str().ok())
            .ok_or(
                "YouTube did not return a current server clock; no publication time was changed.",
            )?;
        let server_time = DateTime::parse_from_rfc2822(date)
            .map_err(|_| "YouTube's server clock could not be verified.".to_string())?
            .with_timezone(&Utc);
        let payload: serde_json::Value = response
            .json()
            .map_err(|_| "YouTube's publication verification response was invalid.".to_string())?;
        let video = payload
            .get("items")
            .and_then(serde_json::Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("id").and_then(serde_json::Value::as_str) == Some(video))
            })
            .cloned()
            .ok_or("The uploaded video is unavailable; its publication request was retained.")?;
        self.clock = Some((server_time, observed));
        self.ensure(channel)?;
        Ok(ProviderSnapshot { video, server_time })
    }
    fn write(
        &mut self,
        channel: &str,
        _video: &str,
        payload: &serde_json::Value,
        etag: &str,
    ) -> Result<(), (String, bool)> {
        let check = || -> Result<(), String> {
            self.ensure(channel)?;
            let connection = database(self.state)?;
            clear_expired_deletion_authorization(&connection)?;
            let settings = connection_settings(&connection)?;
            if !settings.deletion_authorized || !settings.deletion_sudo_active {
                return Err("Publication is waiting for separately granted video-management permission and its temporary mode.".into());
            }
            Ok(())
        };
        check().map_err(|detail| (detail, false))?;
        let token =
            refreshed_deletion_access_token(self.state).map_err(|detail| (detail, false))?;
        let credential_guard =
            oauth_replacement::lock_credentials().map_err(|detail| (detail, false))?;
        oauth_replacement::ensure_no_pending(self.state).map_err(|detail| (detail, false))?;
        check().map_err(|detail| (detail, false))?;
        let scope = oauth_replacement::configured_quota_scope(
            &database(self.state).map_err(|detail| (detail, false))?,
        )
        .map_err(|detail| (detail, false))?;
        if active_channel_id(self.state, &token).map_err(|detail| (detail, false))? != channel {
            return Err((
                "The management credential belongs to another channel.".into(),
                false,
            ));
        }
        check().map_err(|detail| (detail, false))?;
        let requested = payload
            .pointer("/status/publishAt")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ("The reviewed publication time is missing.".into(), false))?;
        future(
            utc(requested).map_err(|detail| (detail, false))?,
            self.provider_now().map_err(|detail| (detail, false))?,
            WRITE_MARGIN_SECONDS,
        )
        .map_err(|detail| (detail, false))?;
        let request = provider_transport(self.state)
            .control_client()
            .map_err(|detail| (detail, false))?
            .put("https://www.googleapis.com/youtube/v3/videos")
            .query(&[("part", "status")])
            .header(reqwest::header::IF_MATCH, etag)
            .bearer_auth(token)
            .json(payload);
        let response=observed_youtube_request(self.state,"videos.update",||request.send()).map_err(|_|("The publication write has an unknown outcome. Reconcile the owned video before retrying.".into(),true))?;
        drop(credential_guard);
        if response.status().is_success() {
            Ok(())
        } else {
            let code = response.status().as_u16();
            let body = bounded_error_body(response);
            Err(publication_write_error(
                self.state, channel, &scope, code, &body,
            ))
        }
    }
}

fn owned(snapshot: &ProviderSnapshot, channel: &str, video: &str) -> Result<(), String> {
    if snapshot.video.get("id").and_then(serde_json::Value::as_str) != Some(video)
        || snapshot
            .video
            .pointer("/snippet/channelId")
            .and_then(serde_json::Value::as_str)
            != Some(channel)
    {
        return Err("Publication verification returned a different video or channel.".into());
    }
    Ok(())
}
fn same_time(left: &str, right: &str) -> bool {
    utc(left)
        .ok()
        .zip(utc(right).ok())
        .is_some_and(|(left, right)| left == right)
}
fn persist_state(
    state: &AppState,
    job: &PublicationJob,
    status: &str,
    detail: &str,
    confirmed: Option<&str>,
    ever_public: bool,
    next_check: Option<String>,
) -> Result<(), String> {
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_channel(&transaction, &job.channel_id)?;
    let changed=transaction.execute("UPDATE publication_schedule_jobs SET state=?1,detail=?2,provider_confirmed_publish_at=COALESCE(?3,provider_confirmed_publish_at),ever_public=MAX(ever_public,?4),checked_at=?5,next_check_at=?6,updated_at=?5 WHERE item_id=?7 AND channel_id=?8 AND requested_publish_at=?9 AND updated_at=?10",params![status,detail,confirmed,ever_public,now(),next_check,job.item_id,job.channel_id,job.publish_at,job.updated_at]).map_err(user_error)?;
    if changed != 1 {
        return Err("The publication request changed during verification; its retained state must be refreshed.".into());
    }
    audit(
        &transaction,
        &job.item_id,
        &format!("publication_{status}"),
        detail,
    )?;
    transaction.commit().map_err(user_error)?;
    Ok(())
}

fn run_with(
    state: &AppState,
    item: &str,
    channel: &str,
    permitted: bool,
    explicit_retry: bool,
    provider: &mut impl ScheduleProvider,
) -> Result<(), String> {
    let connection = database(state)?;
    require_channel(&connection, channel)?;
    let mut job = load_job(&connection, item, channel)?;
    if job.ever_public || job.state == "published" {
        return Ok(());
    }
    let upload=connection.query_row("SELECT status,video_id,visibility,folder_monitor_id FROM upload_items WHERE id=?1 AND channel_id=?2",params![item,channel],|row|Ok((row.get::<_,String>(0)?,row.get::<_,Option<String>>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?))).map_err(user_error)?;
    if upload.2 != "private" || upload.3.is_some() {
        return persist_state(state,&job,"needs_attention","This request is not a newly reviewed private manual upload; no publication was changed.",None,false,None);
    }
    let Some(video) = upload.1.filter(|video| valid_id(video)) else {
        return Ok(());
    };
    if upload.0 != "uploaded" {
        return Ok(());
    }
    if job.video_id.as_deref().is_some_and(|saved| saved != video) {
        return Err(
            "The saved publication video differs from the confirmed upload receipt.".into(),
        );
    }
    connection.execute("UPDATE publication_schedule_jobs SET video_id=?1 WHERE item_id=?2 AND channel_id=?3 AND video_id IS NULL",params![video,item,channel]).map_err(user_error)?;
    job.video_id = Some(video.clone());
    drop(connection);
    if matches!(
        job.state.as_str(),
        "waiting_upload"
            | "pending"
            | "waiting_processing"
            | "waiting_permission"
            | "waiting_verification"
    ) && utc(&job.publish_at)? <= Utc::now()
    {
        return persist_state(state,&job,"needs_attention","The reviewed publication time passed before its schedule was confirmed. The existing upload stays private and the time requires a new explicit review.",None,false,None);
    }
    let snapshot = match provider.read(channel, &video) {
        Ok(snapshot) => snapshot,
        Err(detail) => {
            return persist_state(
                state,
                &job,
                if matches!(job.state.as_str(), "uncertain" | "applying") {
                    "uncertain"
                } else {
                    "waiting_verification"
                },
                &detail,
                None,
                false,
                if matches!(job.state.as_str(), "uncertain" | "applying") {
                    None
                } else {
                    Some((Utc::now() + ChronoDuration::seconds(60)).to_rfc3339())
                },
            )
        }
    };
    if let Err(error) = owned(&snapshot, channel, &video) {
        return persist_state(state, &job, "needs_attention", &error, None, false, None);
    }
    let privacy = snapshot
        .video
        .pointer("/status/privacyStatus")
        .and_then(serde_json::Value::as_str)
        .ok_or("YouTube did not return the video's privacy for scheduling.")?;
    let current_schedule = snapshot
        .video
        .pointer("/status/publishAt")
        .and_then(serde_json::Value::as_str);
    if privacy != "private" {
        let accepted = job
            .provider_confirmed_publish_at
            .as_deref()
            .is_some_and(|time| same_time(time, &job.publish_at))
            || matches!(job.state.as_str(), "uncertain" | "applying");
        let actual = snapshot
            .video
            .pointer("/snippet/publishedAt")
            .and_then(serde_json::Value::as_str);
        let published = privacy == "public"
            && accepted
            && utc(&job.publish_at)? <= snapshot.server_time
            && actual.is_some_and(|time| same_time(time, &job.publish_at));
        return persist_state(
            state,
            &job,
            if published {
                "published"
            } else {
                "needs_attention"
            },
            if published {
                "YouTube confirms the approved video is public with the exact reviewed publication timestamp."
            } else {
                "The video is no longer private, or its published timestamp differs from the review. Its request was retained without changing YouTube visibility."
            },
            if published {
                Some(&job.publish_at)
            } else {
                None
            },
            true,
            None,
        );
    }
    if current_schedule.is_some_and(|time| same_time(time, &job.publish_at)) {
        if utc(&job.publish_at)? <= snapshot.server_time {
            return persist_state(state,&job,"needs_attention","The approved schedule has passed but YouTube still reports this video as private. Review the retained request; the app will not move its time.",Some(&job.publish_at),false,None);
        }
        return persist_state(state,&job,"accepted","YouTube confirms this exact server-side publication time. Publication no longer depends on the app staying open.",Some(&job.publish_at),false,Some((utc(&job.publish_at)?+ChronoDuration::seconds(60)).to_rfc3339()));
    }
    if let Err(error) = future(
        utc(&job.publish_at)?,
        snapshot.server_time,
        WRITE_MARGIN_SECONDS,
    ) {
        return persist_state(state, &job, "needs_attention", &error, None, false, None);
    }
    if current_schedule.is_some_and(|time| {
        !job.provider_confirmed_publish_at
            .as_deref()
            .is_some_and(|expected| same_time(time, expected))
    }) {
        return persist_state(state,&job,"needs_attention","YouTube has a different publication schedule. Review it explicitly before replacing it.",None,false,None);
    }
    if matches!(job.state.as_str(), "uncertain" | "applying") && !explicit_retry {
        return persist_state(state,&job,"needs_attention","The prior write was not confirmed as the reviewed schedule. A deliberate repair review is required before another write.",None,false,None);
    }
    let processing = snapshot
        .video
        .pointer("/processingDetails/processingStatus")
        .and_then(serde_json::Value::as_str);
    if processing != Some("succeeded") {
        return persist_state(state,&job,if matches!(processing,Some("failed"|"terminated")){"needs_attention"}else{"waiting_processing"},"YouTube processing must succeed before this publication request is applied; the upload stays private.",None,false,if matches!(processing,Some("failed"|"terminated")){None}else{Some((Utc::now()+ChronoDuration::seconds(60)).to_rfc3339())});
    }
    if !permitted {
        return persist_state(state,&job,"waiting_permission","Grant video-management permission and enter its temporary mode before this publication request can be applied.",None,false,None);
    }
    let etag=snapshot.video.get("etag").and_then(serde_json::Value::as_str).filter(|etag|!etag.is_empty()&&etag.len()<=256&& !etag.chars().any(char::is_control)).ok_or("YouTube did not return an update version; the schedule remains private until a fresh read can protect other metadata.")?;
    let payload = upload_metadata::merge_update_payload(
        &snapshot.video,
        &upload_metadata::UploadMetadata {
            publish_at: Some(job.publish_at.clone()),
            ..Default::default()
        },
    )?;
    persist_state(state,&job,"applying","Applying the reviewed publication time with a fresh owned-video version and retained unrelated status metadata.",None,false,None)?;
    job = load_job(&database(state)?, item, channel)?;
    let result = provider.write(channel, &video, &payload, etag);
    if let Err((detail, uncertain)) = result {
        return persist_state(
            state,
            &job,
            if uncertain {
                "uncertain"
            } else {
                "needs_attention"
            },
            &detail,
            None,
            false,
            None,
        );
    }
    let confirmation=match provider.read(channel,&video){Ok(snapshot)=>snapshot,Err(_)=>return persist_state(state,&job,"uncertain","YouTube accepted a publication write but its exact time could not be confirmed. Reconcile before retrying.",None,false,None)};
    if let Err(error) = owned(&confirmation, channel, &video) {
        return persist_state(state, &job, "uncertain", &error, None, false, None);
    }
    if confirmation
        .video
        .pointer("/status/privacyStatus")
        .and_then(serde_json::Value::as_str)
        == Some("private")
        && confirmation
            .video
            .pointer("/status/publishAt")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|time| same_time(time, &job.publish_at))
    {
        persist_state(state,&job,"accepted","YouTube confirms this exact server-side publication time. Publication no longer depends on the app staying open.",Some(&job.publish_at),false,Some((utc(&job.publish_at)?+ChronoDuration::seconds(60)).to_rfc3339()))
    } else {
        persist_state(state,&job,"uncertain","The provider did not confirm the exact private publication schedule. Review the owned video before further changes.",None,false,None)
    }
}

pub(super) fn process_requested_job(state: &AppState, item: &str) -> Result<(), String> {
    let _guard = deletion_execution_lock(state)
        .lock()
        .map_err(|_| "Publication management is unavailable.".to_string())?;
    {
        let _credential = oauth_replacement::lock_credentials()?;
        oauth_replacement::ensure_no_pending(state)?;
    }
    let connection = database(state)?;
    let settings = connection_settings(&connection)?;
    let Some(channel) = settings.active_channel_id else {
        return Ok(());
    };
    let permitted = settings.deletion_authorized && settings.deletion_sudo_active;
    drop(connection);
    let mut provider = NativeProvider { state, clock: None };
    run_automatic_with(state, item, &channel, permitted, Utc::now(), &mut provider)
}

fn run_automatic_with(
    state: &AppState,
    item: &str,
    channel: &str,
    permitted: bool,
    time: DateTime<Utc>,
    provider: &mut impl ScheduleProvider,
) -> Result<(), String> {
    let connection = database(state)?;
    require_channel(&connection, channel)?;
    // Other follow-up steps can wake this item's shared worker. They must not
    // turn a missing publication job or its retained cooldown into provider I/O.
    let due = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM publication_schedule_jobs AS jobs JOIN upload_items AS uploads ON uploads.id=jobs.item_id AND uploads.channel_id=jobs.channel_id WHERE jobs.item_id=?1 AND jobs.channel_id=?2 AND uploads.status='uploaded' AND uploads.video_id IS NOT NULL AND jobs.ever_public=0 AND ((jobs.state IN ('waiting_upload','pending','waiting_processing','waiting_verification','accepted') AND (jobs.next_check_at IS NULL OR jobs.next_check_at<=?3)) OR (jobs.state='waiting_permission' AND ?4)))",
        params![item, channel, time.to_rfc3339(), permitted],
        |row| row.get::<_, bool>(0),
    ).map_err(user_error)?;
    if !due {
        return Ok(());
    }
    drop(connection);
    run_with(state, item, channel, permitted, false, provider)
}

/// Completion repair already holds the shared provider execution lock. This
/// adapter repairs only the existing job and exact saved upload receipt.
pub(super) fn repair_completion(
    state: &AppState,
    item: &str,
    channel: &str,
    expected_video: &str,
    expected_publish_at: &str,
    expected_updated_at: &str,
) -> Result<(), String> {
    let connection = database(state)?;
    require_channel(&connection, channel)?;
    let job = load_job(&connection, item, channel)?;
    let eligible: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE id=?1 AND channel_id=?2 AND video_id=?3 AND status='uploaded')",params![item,channel,expected_video],|row|row.get(0)).map_err(user_error)?;
    if !eligible
        || job.video_id.as_deref() != Some(expected_video)
        || job.publish_at != expected_publish_at
        || job.updated_at != expected_updated_at
    {
        return Err("The existing publication job does not match this reviewed upload receipt. Refresh its completion checklist.".into());
    }
    let settings = connection_settings(&connection)?;
    drop(connection);
    let mut provider = NativeProvider { state, clock: None };
    run_with(
        state,
        item,
        channel,
        settings.deletion_authorized && settings.deletion_sudo_active,
        true,
        &mut provider,
    )
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CompletionSnapshot {
    pub(super) job_id: String,
    pub(super) publish_at: String,
    pub(super) updated_at: String,
    pub(super) checked_at: Option<String>,
    pub(super) outcome: String,
    pub(super) detail: String,
    pub(super) repairable: bool,
}

pub(super) fn completion_snapshot(
    connection: &Connection,
    item: &str,
    channel: &str,
) -> Result<Option<CompletionSnapshot>, String> {
    let exists=connection.query_row("SELECT EXISTS(SELECT 1 FROM publication_schedule_jobs WHERE item_id=?1 AND channel_id=?2)",params![item,channel],|row|row.get::<_,bool>(0)).map_err(user_error)?;
    if !exists {
        return Ok(None);
    }
    let job = load_job(connection, item, channel)?;
    let (outcome, detail, repairable) = job_outcome(&job);
    Ok(Some(CompletionSnapshot {
        job_id: item.into(),
        publish_at: job.publish_at,
        updated_at: job.updated_at,
        checked_at: job.checked_at,
        outcome,
        detail,
        repairable,
    }))
}

#[tauri::command]
pub(super) async fn repair_publication_job(
    state: State<'_, AppState>,
    expected_channel_id: String,
    item_id: String,
    approved: bool,
) -> Result<PublicationJob, String> {
    if !approved {
        return Err("Review this existing private video and its retained publication time before repairing.".into());
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = deletion_execution_lock(&state)
            .lock()
            .map_err(|_| "Publication management is unavailable.".to_string())?;
        {
            let _credential = oauth_replacement::lock_credentials()?;
            oauth_replacement::ensure_no_pending(&state)?;
        }
        let connection = database(&state)?;
        require_channel(&connection, &expected_channel_id)?;
        let settings = connection_settings(&connection)?;
        let mut provider = NativeProvider {
            state: &state,
            clock: None,
        };
        run_with(
            &state,
            &item_id,
            &expected_channel_id,
            settings.deletion_authorized && settings.deletion_sudo_active,
            true,
            &mut provider,
        )?;
        load_job(&database(&state)?, &item_id, &expected_channel_id)
    })
    .await
    .map_err(|_| {
        "Publication repair stopped. Reconcile the current owned video before retrying.".to_string()
    })?
}

pub(super) fn pending_items(
    connection: &Connection,
    channel: &str,
    time: DateTime<Utc>,
    permission_ready: bool,
) -> Result<Vec<String>, String> {
    require_channel(connection, channel)?;
    connection.prepare("SELECT jobs.item_id FROM publication_schedule_jobs AS jobs JOIN upload_items AS uploads ON uploads.id=jobs.item_id AND uploads.channel_id=jobs.channel_id WHERE jobs.channel_id=?1 AND uploads.status='uploaded' AND uploads.video_id IS NOT NULL AND jobs.ever_public=0 AND ((jobs.state IN ('waiting_upload','pending','waiting_processing','waiting_verification','accepted') AND (jobs.next_check_at IS NULL OR jobs.next_check_at<=?2)) OR (jobs.state='waiting_permission' AND ?3)) ORDER BY jobs.requested_publish_at,jobs.item_id LIMIT 10").map_err(user_error)?.query_map(params![channel,time.to_rfc3339(),permission_ready],|row|row.get::<_,String>(0)).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)
}

pub(super) fn next_wake(
    connection: &Connection,
    channel: &str,
) -> Result<Option<DateTime<Utc>>, String> {
    let time=connection.query_row("SELECT MIN(next_check_at) FROM publication_schedule_jobs WHERE channel_id=?1 AND state IN ('waiting_processing','waiting_verification','accepted')",[channel],|row|row.get::<_,Option<String>>(0)).map_err(user_error)?;
    time.map(|time| utc(&time)).transpose()
}

pub(super) fn completion_outcome(
    connection: &Connection,
    item: &str,
    channel: &str,
) -> Result<Option<(String, String, bool)>, String> {
    let exists=connection.query_row("SELECT EXISTS(SELECT 1 FROM publication_schedule_jobs WHERE item_id=?1 AND channel_id=?2)",params![item,channel],|row|row.get::<_,bool>(0)).map_err(user_error)?;
    if !exists {
        return Ok(None);
    }
    let job = load_job(connection, item, channel)?;
    Ok(Some(job_outcome(&job)))
}

fn job_outcome(job: &PublicationJob) -> (String, String, bool) {
    let outcome = match job.state.as_str() {
        "accepted" | "published" => "succeeded",
        "needs_attention" => "failed",
        "uncertain" | "applying" => "unknown",
        _ => "waiting",
    };
    let repairable = matches!(
        job.state.as_str(),
        "needs_attention" | "uncertain" | "waiting_permission"
    ) && !job.ever_public
        && utc(&job.publish_at)
            .is_ok_and(|time| time > Utc::now() + ChronoDuration::seconds(PREVIEW_MARGIN_SECONDS));
    (outcome.into(), job.detail.clone(), repairable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fixture {
        root: PathBuf,
        state: AppState,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("youtube-publication-test-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("media")).unwrap();
            let state = AppState {
                database_path: root.join("queue.sqlite3"),
                media_directory: root.join("media"),
                folder_monitor_lock: Arc::new(Mutex::new(())),
                oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
            };
            let connection = initialize_database(&state).unwrap();
            connection.execute_batch(SCHEMA_SQL).unwrap();
            connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET active_channel='Fixture',active_channel_id='channel-a'", [now()]).unwrap();
            Self { root, state }
        }
        fn db(&self) -> Connection {
            database(&self.state).unwrap()
        }
        fn seed(&self, item: &str) -> String {
            let time = (Utc::now() + ChronoDuration::days(1)).to_rfc3339();
            self.db().execute("INSERT INTO upload_items(id,title,file_name,source_path,workspace_path,size_bytes,total_bytes,status,visibility,channel_id,digest,confirmed_bytes,resumable_session_uri,upload_metadata_json,created_at,updated_at) VALUES(?1,'Approved original','clip.mp4','original-source','original-source',10,10,'queued','private','channel-a','retained-digest',4,'protected-checkpoint','{\"description\":\"Approved\"}',?2,?2)", params![item,now()]).unwrap();
            capture_requested_job(&self.db(), item, "channel-a", &time).unwrap();
            time
        }
        fn receipt(&self, item: &str) {
            self.db()
                .execute(
                    "UPDATE upload_items SET status='uploaded',video_id='video-a' WHERE id=?1",
                    [item],
                )
                .unwrap();
        }
        fn job(&self, item: &str) -> PublicationJob {
            load_job(&self.db(), item, "channel-a").unwrap()
        }
        fn retained(&self, item: &str) -> (String, String, String, i64, String, String) {
            self.db().query_row("SELECT source_path,workspace_path,digest,confirmed_bytes,resumable_session_uri,upload_metadata_json FROM upload_items WHERE id=?1",[item],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    struct FakeProvider {
        video: serde_json::Value,
        server_time: DateTime<Utc>,
        writes: Vec<serde_json::Value>,
        read_count: usize,
        read_error: bool,
        fail_confirmation: bool,
        write_failure: Option<(String, bool)>,
    }
    impl FakeProvider {
        fn new() -> Self {
            Self {
                video: json!({"id":"video-a","etag":"fresh-version","snippet":{"channelId":"channel-a","title":"Keep title","publishedAt":"2026-01-01T00:00:00Z"},"status":{"privacyStatus":"private","license":"creativeCommon","embeddable":false,"selfDeclaredMadeForKids":true,"containsSyntheticMedia":true},"processingDetails":{"processingStatus":"succeeded"}}),
                server_time: Utc::now(),
                writes: vec![],
                read_count: 0,
                read_error: false,
                fail_confirmation: false,
                write_failure: None,
            }
        }
    }
    impl ScheduleProvider for FakeProvider {
        fn read(&mut self, _channel: &str, _video: &str) -> Result<ProviderSnapshot, String> {
            self.read_count += 1;
            if self.read_error || self.fail_confirmation && self.read_count > 1 {
                return Err("Fixture read unavailable".into());
            }
            Ok(ProviderSnapshot {
                video: self.video.clone(),
                server_time: self.server_time,
            })
        }
        fn write(
            &mut self,
            channel: &str,
            video: &str,
            payload: &serde_json::Value,
            etag: &str,
        ) -> Result<(), (String, bool)> {
            assert_eq!(channel, "channel-a");
            assert_eq!(video, "video-a");
            assert_eq!(etag, "fresh-version");
            self.writes.push(payload.clone());
            if let Some(error) = &self.write_failure {
                return Err(error.clone());
            }
            self.video["status"] = payload["status"].clone();
            Ok(())
        }
    }
    fn run(fixture: &Fixture, item: &str, provider: &mut FakeProvider) {
        run_with(&fixture.state, item, "channel-a", true, false, provider).unwrap();
    }
    fn context(zone: &str) -> PublicationContext {
        PublicationContext {
            time_zone: zone.into(),
            tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        }
    }
    fn change(job: &PublicationJob, time: String, zone: &str) -> PublicationEdit {
        PublicationEdit {
            item_id: job.item_id.clone(),
            expected_publish_at: job.publish_at.clone(),
            expected_updated_at: job.updated_at.clone(),
            publish_at: time,
            context: context(zone),
        }
    }

    #[test]
    fn iana_time_resolution_discloses_gaps_and_both_repeated_instants() {
        let now = utc("2026-01-01T00:00:00Z").unwrap();
        let zone = "America/New_York".parse::<Tz>().unwrap();
        let gap = resolve_row(0, "2027-03-14T02:30", zone, None, now);
        assert_eq!(gap.state, "gap");
        assert!(gap.publish_at.is_none());
        let fold = resolve_row(0, "2026-11-01T01:30", zone, None, now);
        assert_eq!(fold.state, "ambiguous");
        assert_eq!(fold.candidates.len(), 2);
        assert_eq!(fold.candidates[0].publish_at, "2026-11-01T05:30:00+00:00");
        assert_eq!(fold.candidates[1].publish_at, "2026-11-01T06:30:00+00:00");
        assert_eq!(
            resolve_row(0, "2026-11-01T01:30", zone, Some("later"), now)
                .publish_at
                .as_deref(),
            Some("2026-11-01T06:30:00+00:00")
        );
        let singapore = resolve_row(
            0,
            "2026-11-01T14:30",
            "Asia/Singapore".parse().unwrap(),
            None,
            now,
        );
        assert_eq!(
            singapore.publish_at.as_deref(),
            Some("2026-11-01T06:30:00+00:00")
        );
    }

    #[test]
    fn preview_preserves_utc_in_zone_changes_and_rejects_unbounded_or_invalid_input() {
        let now = utc("2026-01-01T00:00:00Z").unwrap();
        let preserved = HashMap::from([(0, "2026-11-01T06:30:00Z".into())]);
        let resolved = preview(
            "channel-a",
            "Asia/Singapore",
            "2026-11-01T01:30",
            30,
            1,
            &HashMap::new(),
            &HashMap::new(),
            &preserved,
            now,
        )
        .unwrap();
        assert!(resolved.ready);
        assert!(resolved.rows[0].preserved);
        assert_eq!(resolved.rows[0].local_time, "2026-11-01T14:30");
        assert!(same_time(
            resolved.rows[0].publish_at.as_deref().unwrap(),
            &preserved[&0]
        ));
        for (zone, start, spacing, count) in [
            ("Unknown/Zone", "2026-11-01T01:30", 0, 1),
            ("UTC", "not-a-date", 0, 1),
            ("UTC", "2026-11-01T01:30", 10081, 1),
            ("UTC", "2026-11-01T01:30", 0, 513),
            ("UTC", "2026-11-01T01:30", 0, 0),
        ] {
            assert!(preview(
                "channel-a",
                zone,
                start,
                spacing,
                count,
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                now
            )
            .is_err());
        }
        assert!(preview(
            "channel-a",
            "UTC",
            "2026-11-01T01:30",
            0,
            1,
            &HashMap::from([(2, "2026-11-01T01:30".into())]),
            &HashMap::new(),
            &HashMap::new(),
            now
        )
        .is_err());
        assert!(timezone(&PublicationContext {
            time_zone: "UTC".into(),
            tzdb_version: "old-rules".into()
        })
        .is_err());
        assert_eq!(
            resolve_row(0, "2026-01-01T00:01", chrono_tz::UTC, None, now).state,
            "past"
        );
    }

    #[test]
    fn capture_is_transactional_channel_scoped_and_only_for_new_private_manual_uploads() {
        let fixture = Fixture::new();
        let time = fixture.seed("item-a");
        let job = fixture.job("item-a");
        assert_eq!(job.time_zone, "UTC");
        assert_eq!(job.tzdb_version, chrono_tz::IANA_TZDB_VERSION);
        assert!(capture_requested_job(&fixture.db(), "item-a", "channel-b", &time).is_err());
        assert!(page(&fixture.db(), "channel-b", None, 0).is_err());
        fixture.db().execute("INSERT INTO upload_items(id,title,file_name,workspace_path,size_bytes,total_bytes,status,visibility,channel_id,created_at,updated_at) VALUES('item-b','Original','clip.mp4','original',1,1,'queued','unlisted','channel-a',?1,?1)",[now()]).unwrap();
        assert!(capture_requested_job(&fixture.db(), "item-b", "channel-a", &time).is_err());
        fixture.db().execute("UPDATE upload_items SET visibility='private',folder_monitor_id='watched' WHERE id='item-b'",[]).unwrap();
        assert!(capture_requested_job(&fixture.db(), "item-b", "channel-a", &time).is_err());
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET folder_monitor_id=NULL WHERE id='item-b'",
                [],
            )
            .unwrap();
        let mut connection = fixture.db();
        {
            let tx = connection.transaction().unwrap();
            capture_with_context(
                &tx,
                "item-b",
                "channel-a",
                &time,
                Some(&context("Asia/Singapore")),
            )
            .unwrap();
            tx.rollback().unwrap();
        }
        assert!(load_job(&connection, "item-b", "channel-a").is_err());
        fixture
            .db()
            .execute(
                "UPDATE upload_items SET video_id='existing-video' WHERE id='item-b'",
                [],
            )
            .unwrap();
        assert!(capture_requested_job(&fixture.db(), "item-b", "channel-a", &time).is_err());
    }

    #[test]
    fn successful_job_waits_for_receipt_then_preserves_unrelated_status_and_is_idempotent() {
        let fixture = Fixture::new();
        let requested = fixture.seed("item-a");
        let retained = fixture.retained("item-a");
        let mut provider = FakeProvider::new();
        run(&fixture, "item-a", &mut provider);
        assert_eq!(provider.read_count, 0);
        fixture.receipt("item-a");
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "accepted");
        assert_eq!(provider.writes.len(), 1);
        let payload = &provider.writes[0];
        assert_eq!(payload["status"]["privacyStatus"], "private");
        assert_eq!(payload["status"]["publishAt"], requested);
        assert_eq!(payload["status"]["license"], "creativeCommon");
        assert_eq!(payload["status"]["embeddable"], false);
        assert_eq!(payload["status"]["selfDeclaredMadeForKids"], true);
        assert_eq!(payload["status"]["containsSyntheticMedia"], true);
        assert!(payload.get("snippet").is_none());
        run(&fixture, "item-a", &mut provider);
        assert_eq!(provider.writes.len(), 1);
        assert_eq!(fixture.retained("item-a"), retained);
        assert_eq!(
            completion_outcome(&fixture.db(), "item-a", "channel-a")
                .unwrap()
                .unwrap()
                .0,
            "succeeded"
        );
        assert!(next_wake(&fixture.db(), "channel-a").unwrap().is_some());
    }

    #[test]
    fn automatic_followup_without_a_due_publication_never_contacts_provider() {
        let fixture = Fixture::new();
        fixture.seed("item-a");
        let retained = fixture.retained("item-a");
        let mut provider = FakeProvider::new();
        let current = Utc::now();
        run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            current,
            &mut provider,
        )
        .unwrap();
        fixture.receipt("item-a");
        for status in ["published", "needs_attention", "uncertain", "applying"] {
            fixture.db().execute("UPDATE publication_schedule_jobs SET state=?1,next_check_at=NULL WHERE item_id='item-a'",[status]).unwrap();
            run_automatic_with(
                &fixture.state,
                "item-a",
                "channel-a",
                true,
                current,
                &mut provider,
            )
            .unwrap();
            assert_eq!(fixture.job("item-a").state, status);
        }
        fixture.db().execute("UPDATE publication_schedule_jobs SET state='pending',ever_public=1,next_check_at=NULL WHERE item_id='item-a'",[]).unwrap();
        run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            current,
            &mut provider,
        )
        .unwrap();
        fixture
            .db()
            .execute(
                "DELETE FROM publication_schedule_jobs WHERE item_id='item-a'",
                [],
            )
            .unwrap();
        run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            current,
            &mut provider,
        )
        .unwrap();
        assert_eq!(provider.read_count, 0);
        assert!(provider.writes.is_empty());
        assert_eq!(fixture.retained("item-a"), retained);
    }

    #[test]
    fn unrelated_followup_wakes_preserve_publication_cooldowns_and_accepted_schedule() {
        let fixture = Fixture::new();
        let requested = fixture.seed("item-a");
        fixture.receipt("item-a");
        let retained = fixture.retained("item-a");
        let mut provider = FakeProvider::new();
        let current = Utc::now();
        let future = (current + ChronoDuration::minutes(5)).to_rfc3339();
        for status in [
            "waiting_upload",
            "pending",
            "waiting_processing",
            "waiting_verification",
            "accepted",
        ] {
            fixture.db().execute("UPDATE publication_schedule_jobs SET state=?1,next_check_at=?2 WHERE item_id='item-a'",params![status,future]).unwrap();
            let updated = fixture.job("item-a").updated_at;
            for _ in 0..3 {
                run_automatic_with(
                    &fixture.state,
                    "item-a",
                    "channel-a",
                    true,
                    current,
                    &mut provider,
                )
                .unwrap();
            }
            assert_eq!(fixture.job("item-a").state, status);
            assert_eq!(fixture.job("item-a").updated_at, updated);
        }
        assert_eq!(provider.read_count, 0);
        assert!(provider.writes.is_empty());
        assert_eq!(fixture.job("item-a").publish_at, requested);
        assert_eq!(fixture.retained("item-a"), retained);
    }

    #[test]
    fn due_automatic_publication_runs_once_then_only_reads_at_its_deadline() {
        let fixture = Fixture::new();
        let requested = fixture.seed("item-a");
        fixture.receipt("item-a");
        let mut provider = FakeProvider::new();
        run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            Utc::now(),
            &mut provider,
        )
        .unwrap();
        assert_eq!(fixture.job("item-a").state, "accepted");
        assert_eq!(provider.read_count, 2);
        assert_eq!(provider.writes.len(), 1);
        run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            Utc::now(),
            &mut provider,
        )
        .unwrap();
        assert_eq!(provider.read_count, 2);
        provider.server_time = utc(&requested).unwrap() + ChronoDuration::seconds(60);
        provider.video["status"]["privacyStatus"] = json!("public");
        provider.video["snippet"]["publishedAt"] = json!(requested);
        run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            provider.server_time,
            &mut provider,
        )
        .unwrap();
        assert_eq!(fixture.job("item-a").state, "published");
        assert_eq!(provider.read_count, 3);
        assert_eq!(provider.writes.len(), 1);
    }

    #[test]
    fn automatic_due_gate_retains_channel_receipt_and_permission_guards() {
        let fixture = Fixture::new();
        fixture.seed("item-a");
        fixture.receipt("item-a");
        let mut provider = FakeProvider::new();
        assert!(run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-b",
            true,
            Utc::now(),
            &mut provider,
        )
        .is_err());
        fixture.db().execute("UPDATE publication_schedule_jobs SET state='waiting_permission' WHERE item_id='item-a'",[]).unwrap();
        run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            false,
            Utc::now(),
            &mut provider,
        )
        .unwrap();
        assert_eq!(provider.read_count, 0);
        run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            Utc::now(),
            &mut provider,
        )
        .unwrap();
        assert_eq!(provider.writes.len(), 1);
        fixture.db().execute("UPDATE publication_schedule_jobs SET state='pending',video_id='different-video',next_check_at=NULL WHERE item_id='item-a'",[]).unwrap();
        let reads = provider.read_count;
        assert!(run_automatic_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            Utc::now(),
            &mut provider,
        )
        .is_err());
        assert_eq!(provider.read_count, reads);
        assert_eq!(provider.writes.len(), 1);
    }

    #[test]
    fn explicit_publication_repair_still_reads_fresh_inside_automatic_cooldown() {
        let fixture = Fixture::new();
        fixture.seed("item-a");
        fixture.receipt("item-a");
        fixture.db().execute("UPDATE publication_schedule_jobs SET state='needs_attention',next_check_at=?1 WHERE item_id='item-a'",[(Utc::now()+ChronoDuration::hours(1)).to_rfc3339()]).unwrap();
        let mut provider = FakeProvider::new();
        run_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            true,
            &mut provider,
        )
        .unwrap();
        assert_eq!(provider.read_count, 2);
        assert_eq!(provider.writes.len(), 1);
        assert_eq!(fixture.job("item-a").state, "accepted");
    }

    #[test]
    fn processing_and_management_permission_gate_writes_without_new_uploads() {
        let fixture = Fixture::new();
        fixture.seed("item-a");
        fixture.receipt("item-a");
        let mut provider = FakeProvider::new();
        provider.video["processingDetails"]["processingStatus"] = json!("processing");
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "waiting_processing");
        assert!(provider.writes.is_empty());
        provider.video["processingDetails"]["processingStatus"] = json!("succeeded");
        run_with(
            &fixture.state,
            "item-a",
            "channel-a",
            false,
            false,
            &mut provider,
        )
        .unwrap();
        assert_eq!(fixture.job("item-a").state, "waiting_permission");
        assert!(pending_items(&fixture.db(), "channel-a", Utc::now(), false)
            .unwrap()
            .is_empty());
        assert_eq!(
            pending_items(&fixture.db(), "channel-a", Utc::now(), true).unwrap(),
            vec!["item-a"]
        );
        run(&fixture, "item-a", &mut provider);
        assert_eq!(provider.writes.len(), 1);
    }

    #[test]
    fn failed_processing_wrong_owner_and_changed_schedule_hold_without_any_put() {
        for mode in ["failed", "owner", "schedule", "public"] {
            let fixture = Fixture::new();
            fixture.seed("item-a");
            fixture.receipt("item-a");
            let mut provider = FakeProvider::new();
            match mode {
                "failed" => {
                    provider.video["processingDetails"]["processingStatus"] = json!("failed")
                }
                "owner" => provider.video["snippet"]["channelId"] = json!("channel-b"),
                "schedule" => {
                    provider.video["status"]["publishAt"] =
                        json!((Utc::now() + ChronoDuration::days(2)).to_rfc3339())
                }
                _ => provider.video["status"]["privacyStatus"] = json!("public"),
            };
            run(&fixture, "item-a", &mut provider);
            assert_eq!(fixture.job("item-a").state, "needs_attention");
            assert!(provider.writes.is_empty());
            assert_eq!(fixture.job("item-a").ever_public, mode == "public");
        }
    }

    #[test]
    fn provider_clock_ahead_and_late_private_upload_never_move_the_requested_date() {
        let fixture = Fixture::new();
        let requested = fixture.seed("item-a");
        fixture.receipt("item-a");
        let mut provider = FakeProvider::new();
        provider.server_time = utc(&requested).unwrap() + ChronoDuration::seconds(1);
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "needs_attention");
        assert_eq!(fixture.job("item-a").publish_at, requested);
        assert!(provider.writes.is_empty());
        fixture.db().execute("UPDATE publication_schedule_jobs SET state='waiting_upload',requested_publish_at=?1 WHERE item_id='item-a'",[(Utc::now()-ChronoDuration::minutes(1)).to_rfc3339()]).unwrap();
        provider.read_count = 0;
        run(&fixture, "item-a", &mut provider);
        assert_eq!(provider.read_count, 0);
        assert!(provider.writes.is_empty());
        assert_eq!(fixture.job("item-a").state, "needs_attention");
    }

    #[test]
    fn unknown_write_and_restart_need_fresh_reconciliation_before_explicit_retry() {
        let fixture = Fixture::new();
        fixture.seed("item-a");
        fixture.receipt("item-a");
        let mut provider = FakeProvider::new();
        provider.write_failure = Some(("Unknown fixture outcome".into(), true));
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "uncertain");
        provider.read_error = true;
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "uncertain");
        assert_eq!(provider.writes.len(), 1);
        provider.read_error = false;
        provider.write_failure = None;
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "needs_attention");
        assert_eq!(provider.writes.len(), 1);
        run_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            true,
            &mut provider,
        )
        .unwrap();
        assert_eq!(fixture.job("item-a").state, "accepted");
        assert_eq!(provider.writes.len(), 2);
        fixture
            .db()
            .execute(
                "UPDATE publication_schedule_jobs SET state='applying' WHERE item_id='item-a'",
                [],
            )
            .unwrap();
        recover_interrupted(&fixture.db()).unwrap();
        assert_eq!(fixture.job("item-a").state, "uncertain");
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "accepted");
        assert_eq!(provider.writes.len(), 2);
    }

    #[test]
    fn accepted_write_without_confirmation_remains_uncertain_and_can_be_read_back() {
        let fixture = Fixture::new();
        fixture.seed("item-a");
        fixture.receipt("item-a");
        let mut provider = FakeProvider::new();
        provider.fail_confirmation = true;
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "uncertain");
        assert_eq!(
            completion_outcome(&fixture.db(), "item-a", "channel-a")
                .unwrap()
                .unwrap()
                .0,
            "unknown"
        );
        provider.fail_confirmation = false;
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "accepted");
        assert_eq!(provider.writes.len(), 1);
    }

    #[test]
    fn confirmed_publication_and_private_past_schedule_have_distinct_durable_results() {
        for published in [true, false] {
            let fixture = Fixture::new();
            let requested = fixture.seed("item-a");
            fixture.receipt("item-a");
            let mut provider = FakeProvider::new();
            run(&fixture, "item-a", &mut provider);
            provider.server_time = utc(&requested).unwrap() + ChronoDuration::seconds(60);
            if published {
                provider.video["status"]["privacyStatus"] = json!("public");
                provider.video["snippet"]["publishedAt"] = json!(requested);
            }
            run(&fixture, "item-a", &mut provider);
            assert_eq!(
                fixture.job("item-a").state,
                if published {
                    "published"
                } else {
                    "needs_attention"
                }
            );
            assert_eq!(provider.writes.len(), 1);
            assert!(next_wake(&fixture.db(), "channel-a").unwrap().is_none());
        }
    }

    #[test]
    fn reviewed_edits_are_atomic_preserve_receipts_and_prior_confirmed_schedule() {
        let fixture = Fixture::new();
        fixture.seed("item-a");
        fixture.seed("item-b");
        fixture.receipt("item-a");
        let mut provider = FakeProvider::new();
        run(&fixture, "item-a", &mut provider);
        let original = fixture.job("item-a");
        let retained = fixture.retained("item-a");
        let second = fixture.job("item-b");
        let new_time = (Utc::now() + ChronoDuration::days(2)).to_rfc3339();
        let first_change = change(&original, new_time.clone(), "Asia/Singapore");
        let mut invalid = change(&second, new_time.clone(), "UTC");
        invalid.expected_updated_at = "stale".into();
        assert!(edit_requests(
            &mut fixture.db(),
            "channel-a",
            &[first_change, invalid],
            true
        )
        .is_err());
        assert_eq!(fixture.job("item-a").publish_at, original.publish_at);
        let result = edit_requests(
            &mut fixture.db(),
            "channel-a",
            &[change(&original, new_time.clone(), "Asia/Singapore")],
            true,
        )
        .unwrap();
        assert_eq!(result[0].state, "pending");
        assert_eq!(result[0].time_zone, "Asia/Singapore");
        assert_eq!(
            result[0].provider_confirmed_publish_at.as_deref(),
            Some(original.publish_at.as_str())
        );
        assert_eq!(fixture.retained("item-a"), retained);
        let snapshot = completion_snapshot(&fixture.db(), "item-a", "channel-a")
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.job_id, "item-a");
        assert_eq!(snapshot.publish_at, new_time);
        assert!(snapshot.checked_at.is_some());
        assert!(repair_completion(
            &fixture.state,
            "item-a",
            "channel-a",
            "different-video",
            &snapshot.publish_at,
            &snapshot.updated_at
        )
        .is_err());
        assert!(repair_completion(
            &fixture.state,
            "item-a",
            "channel-a",
            "video-a",
            &original.publish_at,
            &snapshot.updated_at
        )
        .is_err());
        assert!(repair_completion(
            &fixture.state,
            "item-a",
            "channel-a",
            "video-a",
            &snapshot.publish_at,
            "stale-version"
        )
        .is_err());
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "accepted");
        assert_eq!(provider.writes.len(), 2);
        assert_eq!(fixture.job("item-a").publish_at, new_time);
        assert_eq!(page(&fixture.db(), "channel-a", None, 0).unwrap().total, 2);
        fixture
            .db()
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        assert!(run_with(
            &fixture.state,
            "item-a",
            "channel-a",
            true,
            false,
            &mut provider
        )
        .is_err());
        assert_eq!(provider.writes.len(), 2);
    }

    fn quota_fixture() -> (Fixture, String) {
        let fixture = Fixture::new();
        fixture.seed("item-a");
        fixture.receipt("item-a");
        fixture.db().execute("UPDATE connection_settings SET oauth_client_id='publication-client.apps.googleusercontent.com',oauth_project_id='publication-project'",[]).unwrap();
        fixture
            .db()
            .execute_batch(quota_planner::SCHEMA_SQL)
            .unwrap();
        let scope = oauth_replacement::configured_quota_scope(&fixture.db()).unwrap();
        (fixture, scope)
    }
    fn quota_body(reasons: &[&str]) -> String {
        json!({"error":{"errors":reasons.iter().map(|reason|json!({"reason":reason})).collect::<Vec<_>>()}}).to_string()
    }
    fn quota_count(connection: &Connection, table: &str) -> i64 {
        assert!(matches!(table, "api_quota_pauses" | "upload_quota_pauses"));
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    #[test]
    fn publication_quota_holds_preserve_uploaded_receipt_and_independent_channel_limits() {
        let (fixture, scope) = quota_fixture();
        let retained = fixture.retained("item-a");
        let requested = fixture.job("item-a").publish_at;
        let (detail, uncertain) = publication_write_error(
            &fixture.state,
            "channel-a",
            &scope,
            403,
            &quota_body(&["quotaExceeded"]),
        );
        assert!(!uncertain);
        assert!(detail.contains("Google Cloud API quota"));
        assert_eq!(quota_count(&fixture.db(), "api_quota_pauses"), 1);
        assert_eq!(quota_count(&fixture.db(), "upload_quota_pauses"), 0);
        assert!(record_response_quota(
            &fixture.state,
            "channel-a",
            &scope,
            429,
            &quota_body(&["dailyLimitExceeded", "uploadLimitExceeded"])
        )
        .unwrap()
        .is_some());
        assert_eq!(quota_count(&fixture.db(), "upload_quota_pauses"), 1);
        assert_eq!(fixture.retained("item-a"), retained);
        assert_eq!(fixture.job("item-a").publish_at, requested);
        let receipt: (String, String) = fixture
            .db()
            .query_row(
                "SELECT status,video_id FROM upload_items WHERE id='item-a'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(receipt, ("uploaded".into(), "video-a".into()));
        assert_eq!(fixture.db().query_row("SELECT COUNT(*) FROM audit_events WHERE kind='youtube_api_quota_paused' AND channel_id='channel-a' AND item_id IS NULL",[],|row|row.get::<_,i64>(0)).unwrap(),2);
    }

    #[test]
    fn publication_nonquota_rejections_and_backend_errors_never_claim_a_successful_write() {
        let (fixture, scope) = quota_fixture();
        for (status, body) in [
            (403, "quotaExceeded".into()),
            (403, quota_body(&["quotaExceededElsewhere"])),
            (403, quota_body(&["uploadLimitExceeded"])),
            (401, quota_body(&["quotaExceeded"])),
            (412, quota_body(&["quotaExceeded"])),
            (503, quota_body(&["quotaExceeded"])),
        ] {
            let (detail, uncertain) =
                publication_write_error(&fixture.state, "channel-a", &scope, status, &body);
            assert!(!detail.contains("Google Cloud API quota"));
            assert_eq!(uncertain, status == 503);
            assert_eq!(quota_count(&fixture.db(), "api_quota_pauses"), 0);
            assert_eq!(quota_count(&fixture.db(), "upload_quota_pauses"), 0);
        }
        let mut provider = FakeProvider::new();
        provider.write_failure = Some(publication_write_error(
            &fixture.state,
            "channel-a",
            &scope,
            503,
            "backend unavailable",
        ));
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "uncertain");
        assert_eq!(provider.writes.len(), 1);
        provider.write_failure = None;
        run(&fixture, "item-a", &mut provider);
        assert_eq!(fixture.job("item-a").state, "needs_attention");
        assert_eq!(provider.writes.len(), 1);
    }

    #[test]
    fn publication_late_quota_response_cannot_pause_a_new_profile_or_project() {
        let (fixture, scope) = quota_fixture();
        let retained = fixture.retained("item-a");
        fixture
            .db()
            .execute(
                "UPDATE connection_settings SET oauth_project_id='new-project'",
                [],
            )
            .unwrap();
        assert!(record_response_quota(
            &fixture.state,
            "channel-a",
            &scope,
            403,
            &quota_body(&["quotaExceeded", "uploadLimitExceeded"])
        )
        .is_err());
        fixture.db().execute("UPDATE connection_settings SET oauth_project_id='publication-project',active_channel_id='channel-b'",[]).unwrap();
        assert!(record_response_quota(
            &fixture.state,
            "channel-a",
            &scope,
            403,
            &quota_body(&["quotaExceeded"])
        )
        .is_err());
        assert_eq!(quota_count(&fixture.db(), "api_quota_pauses"), 0);
        assert_eq!(quota_count(&fixture.db(), "upload_quota_pauses"), 0);
        assert_eq!(fixture.retained("item-a"), retained);
    }

    #[test]
    fn publication_error_reader_is_bounded_and_discarded_if_unreadable() {
        use std::io::Cursor;
        let mut source = Cursor::new(vec![b'x'; MAX_ERROR_BYTES as usize * 2]);
        let result = bounded_error_body(&mut source);
        assert_eq!(result.len(), MAX_ERROR_BYTES as usize);
        assert_eq!(source.position(), MAX_ERROR_BYTES);
        assert_eq!(bounded_error_body(Cursor::new(vec![0xff, 0xfe])), "");
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fixture read failure"))
            }
        }
        assert_eq!(bounded_error_body(Broken), "");
    }

    #[test]
    fn publication_loopback_counts_physical_read_and_write_attempts_even_when_rejected() {
        use std::io::Write;
        let (fixture, scope) = quota_fixture();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = quota_body(&["quotaExceeded"]);
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                let mut byte = [0u8; 1];
                while request.len() < 4096 {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                assert!(request.starts_with(b"GET ") || request.starts_with(b"PUT "));
                let response=format!("HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        for (key, method) in [
            ("videos.list", reqwest::Method::GET),
            ("videos.update", reqwest::Method::PUT),
        ] {
            let request = client.request(method, format!("http://{address}/publication"));
            let response =
                observed_youtube_request(&fixture.state, key, || request.send()).unwrap();
            let status = response.status().as_u16();
            let body = bounded_error_body(response);
            assert!(
                record_response_quota(&fixture.state, "channel-a", &scope, status, &body)
                    .unwrap()
                    .is_some()
            );
        }
        server.join().unwrap();
        let observations:Vec<(String,String,String)>=fixture.db().prepare("SELECT quota_scope,channel_id,method_key FROM quota_observations ORDER BY method_key").unwrap().query_map([],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        assert_eq!(
            observations,
            vec![
                (scope.clone(), "channel-a".into(), "videos.list".into()),
                (scope, "channel-a".into(), "videos.update".into())
            ]
        );
        assert_eq!(
            fixture
                .db()
                .query_row(
                    "SELECT status FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "uploaded"
        );
    }
}
