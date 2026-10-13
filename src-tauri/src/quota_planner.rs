//! Dated operation estimates and device-local observations. No quota admission,
//! provider calls, credential changes, queue mutations or pause clearing.
use super::*;
use chrono::{Datelike, FixedOffset, NaiveDate, TimeZone};

const COST_VERSION: &str = "youtube-data-api-2026-10-13-v1";
const COST_DATE: &str = "2026-10-13";
const COST_SOURCE: &str = "https://developers.google.com/youtube/v3/determine_quota_cost";
const MAX_COUNT: u64 = 1_000_000;
const MAX_LIMIT: u64 = 1_000_000_000_000;

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS quota_cost_tables (
 version TEXT PRIMARY KEY NOT NULL, accessed_at TEXT NOT NULL,
 source_url TEXT NOT NULL, table_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS quota_project_limits (
 quota_scope TEXT PRIMARY KEY NOT NULL, upload_calls INTEGER,
 search_calls INTEGER, other_units INTEGER, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS quota_observations (
 id TEXT PRIMARY KEY NOT NULL, quota_scope TEXT NOT NULL,
 channel_id TEXT NOT NULL, pacific_day TEXT NOT NULL,
 method_key TEXT NOT NULL, bucket TEXT NOT NULL,
 estimated_cost INTEGER, cost_version TEXT NOT NULL,
 observed_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS quota_observations_scope_day_channel_idx
 ON quota_observations(quota_scope, pacific_day, channel_id, bucket);
"#;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MethodCost {
    method: String,
    bucket: String,
    cost: u64,
}

fn cost(method: &str) -> Option<(&'static str, u64)> {
    // Method references and the table body take precedence over the table's
    // generated summary, which still displayed an obsolete upload cost.
    match method {
        "videos.insert" => Some(("video_uploads", 1)),
        "search.list" => Some(("search", 1)),
        "channels.list"
        | "videos.list"
        | "playlists.list"
        | "playlistItems.list"
        | "videoCategories.list"
        | "i18nLanguages.list" => Some(("other", 1)),
        "videos.update"
        | "videos.delete"
        | "playlists.insert"
        | "playlists.update"
        | "playlists.delete"
        | "playlistItems.insert"
        | "playlistItems.update"
        | "playlistItems.delete"
        | "thumbnails.set"
        | "captions.list"
        | "captions.delete" => Some(("other", 50)),
        "captions.insert" => Some(("other", 400)),
        "captions.update" => Some(("other", 450)),
        _ => None,
    }
}

fn costs() -> Vec<MethodCost> {
    [
        "videos.insert",
        "search.list",
        "channels.list",
        "videos.list",
        "playlists.list",
        "playlistItems.list",
        "videoCategories.list",
        "i18nLanguages.list",
        "videos.update",
        "videos.delete",
        "playlists.insert",
        "playlists.update",
        "playlists.delete",
        "playlistItems.insert",
        "playlistItems.update",
        "playlistItems.delete",
        "thumbnails.set",
        "captions.list",
        "captions.delete",
        "captions.insert",
        "captions.update",
    ]
    .into_iter()
    .map(|method| {
        let (bucket, units) = cost(method).expect("listed method has dated cost");
        MethodCost {
            method: method.into(),
            bucket: bucket.into(),
            cost: units,
        }
    })
    .collect()
}

pub(super) fn install_cost_table(connection: &Connection) -> Result<(), String> {
    connection.execute("INSERT OR IGNORE INTO quota_cost_tables(version,accessed_at,source_url,table_json) VALUES(?1,?2,?3,?4)", params![COST_VERSION, COST_DATE, COST_SOURCE, serde_json::to_string(&costs()).map_err(user_error)?]).map_err(user_error)?;
    Ok(())
}

fn ensure_channel(connection: &Connection, expected: &str) -> Result<ConnectionSettings, String> {
    let settings = connection_settings(connection)?;
    if expected.is_empty() || settings.active_channel_id.as_deref() != Some(expected) {
        return Err("The connected channel changed. Open its quota planner again.".into());
    }
    Ok(settings)
}

fn scope(connection: &Connection) -> Result<(String, Option<String>), String> {
    let project = oauth_replacement::configured_project_id(connection)?
        .filter(|project| !project.trim().is_empty());
    let client = configured_oauth_client_id(connection)?;
    Ok((
        match project
            .as_deref()
            .filter(|project| !project.trim().is_empty())
        {
            Some(project) => format!("project:{project}"),
            None => format!("client:{client}"),
        },
        project,
    ))
}

fn scope_id(value: &str) -> String {
    blake3::hash(value.as_bytes()).to_hex().to_string()
}

/// Capture before admitting a physical provider request. A late response is
/// recorded against these credentials' scope, even if the connection changes.
pub(super) struct QuotaObservationContext {
    scope: String,
    channel: String,
}

pub(super) fn capture_observation_context(
    connection: &Connection,
    expected_channel_id: &str,
) -> Result<QuotaObservationContext, String> {
    ensure_channel(connection, expected_channel_id)?;
    Ok(QuotaObservationContext {
        scope: scope(connection)?.0,
        channel: expected_channel_id.into(),
    })
}

fn nth_sunday(year: i32, month: u32, occurrence: u32) -> Option<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let day = 1 + (7 - first.weekday().num_days_from_sunday()) % 7 + (occurrence - 1) * 7;
    NaiveDate::from_ymd_opt(year, month, day)
}

fn pacific_offset(at: DateTime<Utc>) -> Result<FixedOffset, String> {
    // Current US Pacific rules, checked against NIST on COST_DATE: second
    // Sunday March at 02:00 PST to first Sunday November at 02:00 PDT.
    // Keep this dated rule explicit rather than silently using a fixed UTC-8.
    if !(2007..=2099).contains(&at.year()) {
        return Err("Pacific reset rules need an updated cost-table release for this date.".into());
    }
    let start = nth_sunday(at.year(), 3, 2)
        .and_then(|day| day.and_hms_opt(10, 0, 0))
        .map(|value| Utc.from_utc_datetime(&value))
        .ok_or("Pacific reset could not be calculated.")?;
    let end = nth_sunday(at.year(), 11, 1)
        .and_then(|day| day.and_hms_opt(9, 0, 0))
        .map(|value| Utc.from_utc_datetime(&value))
        .ok_or("Pacific reset could not be calculated.")?;
    FixedOffset::west_opt(if at >= start && at < end {
        7 * 3600
    } else {
        8 * 3600
    })
    .ok_or_else(|| "Pacific reset could not be calculated.".into())
}

fn pacific_window(at: DateTime<Utc>) -> Result<(String, String), String> {
    let local = at.with_timezone(&pacific_offset(at)?);
    let next = local
        .date_naive()
        .succ_opt()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .ok_or("Pacific reset could not be calculated.")?;
    // Start with standard time, then resolve the offset at that midnight. DST
    // transition days use the offset in effect before their 02:00 transition.
    let mut reset = Utc.from_utc_datetime(&next) + ChronoDuration::hours(8);
    for _ in 0..2 {
        let seconds = pacific_offset(reset)?.local_minus_utc();
        reset = Utc.from_utc_datetime(&next) - ChronoDuration::seconds(seconds as i64);
    }
    Ok((local.date_naive().to_string(), reset.to_rfc3339()))
}

/// One immutable receipt per physical request (including failures). Unknown
/// methods remain unclassified; no invented bucket or unit charge is assigned.
pub(super) fn observe_request(
    connection: &Connection,
    context: &QuotaObservationContext,
    request_id: &str,
    method_key: &str,
    observed_at: DateTime<Utc>,
) -> Result<(), String> {
    if Uuid::parse_str(request_id).is_err()
        || method_key.is_empty()
        || method_key.len() > 80
        || !method_key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '.')
    {
        return Err("The local quota observation identity is invalid.".into());
    }
    let (day, _) = pacific_window(observed_at)?;
    let (bucket, estimated_cost) = cost(method_key)
        .map(|(bucket, cost)| (bucket, Some(cost)))
        .unwrap_or(("unknown", None));
    connection.execute("INSERT OR IGNORE INTO quota_observations(id,quota_scope,channel_id,pacific_day,method_key,bucket,estimated_cost,cost_version,observed_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![request_id, context.scope, context.channel, day, method_key, bucket, estimated_cost, COST_VERSION, observed_at.to_rfc3339()]).map_err(user_error)?;
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProjectLimits {
    upload_calls: Option<u64>,
    search_calls: Option<u64>,
    other_units: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BucketUsage {
    bucket: String,
    requests: u64,
    estimated_units: u64,
    configured_daily_limit: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SavedHold {
    deadline: String,
    active: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuotaSnapshot {
    channel_id: String,
    channel_name: String,
    scope_id: String,
    configured_project_id: Option<String>,
    limits: ProjectLimits,
    usage: Vec<BucketUsage>,
    unknown_requests: u64,
    pacific_day: String,
    pacific_reset_at: String,
    observed_at: String,
    cloud_hold: Option<SavedHold>,
    channel_hold: Option<SavedHold>,
    cost_version: String,
    cost_accessed_at: String,
    cost_source_url: String,
    costs: Vec<MethodCost>,
    observation_label: String,
    warnings: Vec<String>,
}

fn limits(connection: &Connection, scope: &str) -> Result<ProjectLimits, String> {
    connection.query_row("SELECT upload_calls,search_calls,other_units FROM quota_project_limits WHERE quota_scope=?1", [scope], |row| Ok(ProjectLimits { upload_calls: row.get(0)?, search_calls: row.get(1)?, other_units: row.get(2)? })).optional().map_err(user_error).map(|value| value.unwrap_or_default())
}

fn hold(deadline: Option<String>, at: DateTime<Utc>) -> Option<SavedHold> {
    deadline.map(|deadline| SavedHold {
        active: DateTime::parse_from_rfc3339(&deadline)
            .map(|value| value.with_timezone(&Utc) > at)
            .unwrap_or(true),
        deadline,
    })
}

fn snapshot(
    connection: &Connection,
    expected: &str,
    at: DateTime<Utc>,
) -> Result<QuotaSnapshot, String> {
    let settings = ensure_channel(connection, expected)?;
    let (scope, project) = scope(connection)?;
    let observation_label = if project.is_some() {
        "This app/device observed · all local channels in this Cloud project"
    } else {
        "This app/device observed · all local channels using this OAuth client"
    };
    let configured = limits(connection, &scope)?;
    let (day, reset) = pacific_window(at)?;
    let mut usage = Vec::new();
    for (bucket, limit) in [
        ("video_uploads", configured.upload_calls),
        ("search", configured.search_calls),
        ("other", configured.other_units),
    ] {
        // Cloud quota is shared by project, so changing the selected channel
        // must not hide requests already recorded by another local profile.
        // Unknown-project records stay within the exact captured client scope.
        let (requests, units) = connection.query_row("SELECT COUNT(*), COALESCE(SUM(estimated_cost),0) FROM quota_observations WHERE quota_scope=?1 AND pacific_day=?2 AND bucket=?3", params![scope, day, bucket], |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?))).map_err(user_error)?;
        usage.push(BucketUsage {
            bucket: bucket.into(),
            requests,
            estimated_units: units,
            configured_daily_limit: limit,
        });
    }
    let unknown = connection.query_row("SELECT COUNT(*) FROM quota_observations WHERE quota_scope=?1 AND pacific_day=?2 AND bucket='unknown'", params![scope, day], |row| row.get::<_, u64>(0)).map_err(user_error)?;
    let cloud = connection
        .query_row(
            "SELECT pause_until FROM api_quota_pauses WHERE quota_scope=?1",
            [&scope],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(user_error)?;
    let channel = connection
        .query_row(
            "SELECT pause_until FROM upload_quota_pauses WHERE channel_id=?1",
            [expected],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(user_error)?;
    Ok(QuotaSnapshot { channel_id: expected.into(), channel_name: settings.active_channel.unwrap_or_else(|| "Connected channel".into()), scope_id: scope_id(&scope), configured_project_id: project,
        limits: configured, usage, unknown_requests: unknown, pacific_day: day, pacific_reset_at: reset, observed_at: at.to_rfc3339(), cloud_hold: hold(cloud, at), channel_hold: hold(channel, at),
        cost_version: COST_VERSION.into(), cost_accessed_at: COST_DATE.into(), cost_source_url: COST_SOURCE.into(), costs: costs(),
        observation_label: observation_label.into(),
        warnings: vec!["Recorded requests and dated unit estimates include all local channels in the captured project or client scope. They remain incomplete local evidence: unrecorded requests and usage from other devices or apps are invisible; actual project remaining quota is unknown.".into(), "When the OAuth JSON has no project identity, counts include only this OAuth client; other clients in its unknown project cannot be identified.".into(), "Daily Cloud reset is expected at midnight Pacific under the dated timezone rules. It does not guarantee request acceptance or clear a saved channel pause.".into(), "Previewing makes no provider request and does not alter queues, credentials or saved pauses.".into()] })
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PlaylistEstimate {
    existing_items: u64,
    additions: u64,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct QuotaChoices {
    uploads: u64,
    refresh_library: bool,
    library_videos: u64,
    playlists: Vec<PlaylistEstimate>,
    thumbnail_writes: u64,
    caption_inserts: u64,
    caption_updates: u64,
    metadata_updates: u64,
    search_pages: u64,
    processing_checks_per_video: u64,
    retry_reserve: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct OperationEstimate {
    label: String,
    method: String,
    bucket: String,
    requests: u64,
    retry_requests: u64,
    cost_per_request: u64,
    estimated_units: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BucketEstimate {
    bucket: String,
    estimated_requests: u64,
    estimated_units: u64,
    observed_units: u64,
    configured_daily_limit: Option<u64>,
    exceeds_configured_limit: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QuotaEstimate {
    snapshot: QuotaSnapshot,
    operations: Vec<OperationEstimate>,
    buckets: Vec<BucketEstimate>,
    warnings: Vec<String>,
}

fn pages(items: u64) -> u64 {
    items.div_ceil(50).max(1)
}

fn estimate(snapshot: QuotaSnapshot, choices: &QuotaChoices) -> Result<QuotaEstimate, String> {
    if [
        choices.uploads,
        choices.library_videos,
        choices.thumbnail_writes,
        choices.caption_inserts,
        choices.caption_updates,
        choices.metadata_updates,
        choices.search_pages,
    ]
    .into_iter()
    .any(|count| count > MAX_COUNT)
        || choices.playlists.len() > 64
        || choices
            .playlists
            .iter()
            .any(|playlist| playlist.existing_items > MAX_COUNT || playlist.additions > MAX_COUNT)
        || choices.processing_checks_per_video > 100
        || choices.retry_reserve > 5
    {
        return Err("Use counts up to 1,000,000, at most 64 playlists, 100 processing checks and five retry reserves.".into());
    }
    let mut operations = Vec::new();
    let mut add = |label: &str, method: &str, requests: u64| {
        if requests == 0 {
            return;
        }
        let (bucket, units) = cost(method).expect("estimate method has dated cost");
        let retries = requests * choices.retry_reserve;
        operations.push(OperationEstimate {
            label: label.into(),
            method: method.into(),
            bucket: bucket.into(),
            requests,
            retry_requests: retries,
            cost_per_request: units,
            estimated_units: (requests + retries) * units,
        });
    };
    add(
        "New video upload sessions",
        "videos.insert",
        choices.uploads,
    );
    add(
        "Owner channel check",
        "channels.list",
        u64::from(
            choices.uploads > 0
                || choices.refresh_library
                || !choices.playlists.is_empty()
                || choices.thumbnail_writes
                    + choices.caption_inserts
                    + choices.caption_updates
                    + choices.metadata_updates
                    > 0,
        ),
    );
    if choices.refresh_library {
        add(
            "Upload-library pages (50 videos per page)",
            "playlistItems.list",
            pages(choices.library_videos),
        );
        add(
            "Library metadata pages (50 IDs per request)",
            "videos.list",
            choices.library_videos.div_ceil(50),
        );
    }
    add(
        "Upload ownership and processing checks",
        "videos.list",
        choices.uploads * (1 + choices.processing_checks_per_video),
    );
    add(
        "Playlist owner reads",
        "playlists.list",
        choices.playlists.len() as u64,
    );
    add(
        "Playlist membership pages (50 per page)",
        "playlistItems.list",
        choices
            .playlists
            .iter()
            .map(|playlist| pages(playlist.existing_items))
            .sum(),
    );
    add(
        "New memberships after skip-existing review",
        "playlistItems.insert",
        choices
            .playlists
            .iter()
            .map(|playlist| playlist.additions)
            .sum(),
    );
    add(
        "New membership video ownership (50 IDs per request)",
        "videos.list",
        choices
            .playlists
            .iter()
            .map(|playlist| playlist.additions.div_ceil(50))
            .sum(),
    );
    add(
        "Custom thumbnails",
        "thumbnails.set",
        choices.thumbnail_writes,
    );
    add(
        "Asset video ownership and confirmation reads",
        "videos.list",
        2 * (choices.thumbnail_writes + choices.caption_inserts + choices.caption_updates),
    );
    add(
        "Caption ownership and confirmation reads",
        "captions.list",
        2 * (choices.caption_inserts + choices.caption_updates),
    );
    add(
        "New caption tracks",
        "captions.insert",
        choices.caption_inserts,
    );
    add(
        "Caption track replacements",
        "captions.update",
        choices.caption_updates,
    );
    add(
        "Metadata read/confirmation",
        "videos.list",
        2 * choices.metadata_updates,
    );
    add(
        "Metadata or publication updates",
        "videos.update",
        choices.metadata_updates,
    );
    add("Search pages", "search.list", choices.search_pages);
    let mut buckets = Vec::new();
    for usage in &snapshot.usage {
        let units = operations
            .iter()
            .filter(|operation| operation.bucket == usage.bucket)
            .map(|operation| operation.estimated_units)
            .sum::<u64>();
        let requests = operations
            .iter()
            .filter(|operation| operation.bucket == usage.bucket)
            .map(|operation| operation.requests + operation.retry_requests)
            .sum();
        buckets.push(BucketEstimate {
            bucket: usage.bucket.clone(),
            estimated_requests: requests,
            estimated_units: units,
            observed_units: usage.estimated_units,
            configured_daily_limit: usage.configured_daily_limit,
            exceeds_configured_limit: usage
                .configured_daily_limit
                .map(|limit| units.saturating_add(usage.estimated_units) > limit),
        });
    }
    Ok(QuotaEstimate { snapshot, operations, buckets, warnings: vec!["Counts are reviewed planning assumptions. Pagination, ownership checks, confirmations and possible retries can vary with actual provider responses.".into(), "Retry reserve is an upper estimate, not permission to repeat an uncertain write. Resumable chunks/probes are excluded from documented method costs; their consumption is not established by this planner.".into(), "Entered limits are local planning references. Even below them, remaining project quota and channel upload allowance are unknown.".into()] })
}

#[tauri::command]
pub(super) fn load_quota_planner(
    state: State<'_, AppState>,
    expected_channel_id: String,
) -> Result<QuotaSnapshot, String> {
    snapshot(&database(&state)?, &expected_channel_id, Utc::now())
}

#[tauri::command]
pub(super) fn estimate_quota_operations(
    state: State<'_, AppState>,
    expected_channel_id: String,
    choices: QuotaChoices,
) -> Result<QuotaEstimate, String> {
    estimate(
        snapshot(&database(&state)?, &expected_channel_id, Utc::now())?,
        &choices,
    )
}

fn save_limits(
    connection: &mut Connection,
    expected: &str,
    expected_scope_id: &str,
    limits: ProjectLimits,
) -> Result<QuotaSnapshot, String> {
    if [limits.upload_calls, limits.search_calls, limits.other_units]
        .into_iter()
        .flatten()
        .any(|limit| limit > MAX_LIMIT)
    {
        return Err(
            "Planning limits must be whole numbers no greater than 1,000,000,000,000.".into(),
        );
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    ensure_channel(&transaction, expected)?;
    let (scope, project) = scope(&transaction)?;
    if project
        .as_ref()
        .is_none_or(|project| project.trim().is_empty())
    {
        return Err("The connected OAuth JSON has no Cloud project identity. Keep allowance unknown and check the official Console.".into());
    }
    if expected_scope_id != scope_id(&scope) {
        return Err("The connected Cloud project changed. Review its limits again.".into());
    }
    transaction.execute("INSERT INTO quota_project_limits(quota_scope,upload_calls,search_calls,other_units,updated_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(quota_scope) DO UPDATE SET upload_calls=excluded.upload_calls,search_calls=excluded.search_calls,other_units=excluded.other_units,updated_at=excluded.updated_at", params![scope, limits.upload_calls, limits.search_calls, limits.other_units, now()]).map_err(user_error)?;
    transaction.execute("INSERT INTO audit_events(id,item_id,channel_id,kind,detail,created_at) VALUES(?1,NULL,?2,'quota_planning_limits_saved','Operator saved local project planning limits; provider allowance, queues and pause records were not changed',?3)", params![Uuid::new_v4().to_string(),expected,now()]).map_err(user_error)?;
    transaction.commit().map_err(user_error)?;
    snapshot(connection, expected, Utc::now())
}

#[tauri::command]
pub(super) fn save_quota_project_limits(
    state: State<'_, AppState>,
    expected_channel_id: String,
    expected_scope_id: String,
    limits: ProjectLimits,
) -> Result<QuotaSnapshot, String> {
    save_limits(
        &mut database(&state)?,
        &expected_channel_id,
        &expected_scope_id,
        limits,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }
    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-quota-planner-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute_batch(SCHEMA_SQL).unwrap();
        install_cost_table(&connection).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,oauth_client_id,oauth_project_id,active_channel,active_channel_id,updated_at) VALUES(1,'fixture-client.apps.googleusercontent.com','fixture-project','Fixture channel','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id='fixture-client.apps.googleusercontent.com',oauth_project_id='fixture-project',active_channel='Fixture channel',active_channel_id='channel-a'", [now()]).unwrap();
        (root, state, connection)
    }
    fn finish(root: PathBuf, state: AppState, connection: Connection) {
        drop(connection);
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quota_planner_stores_dated_bucket_costs_without_obsolete_upload_charge() {
        let (root, state, connection) = fixture();
        let (accessed, source, json): (String, String, String) = connection
            .query_row(
                "SELECT accessed_at,source_url,table_json FROM quota_cost_tables WHERE version=?1",
                [COST_VERSION],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(accessed, "2026-10-13");
        assert_eq!(source, COST_SOURCE);
        let table: serde_json::Value = serde_json::from_str(&json).unwrap();
        let upload = table
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["method"] == "videos.insert")
            .unwrap();
        assert_eq!(upload["bucket"], "video_uploads");
        assert_eq!(upload["cost"], 1);
        assert_eq!(cost("search.list"), Some(("search", 1)));
        assert_eq!(cost("captions.insert"), Some(("other", 400)));
        assert_eq!(cost("captions.update"), Some(("other", 450)));
        assert_eq!(cost("playlistItems.list"), Some(("other", 1)));
        assert_eq!(cost("thumbnails.set"), Some(("other", 50)));
        install_cost_table(&connection).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM quota_cost_tables", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        finish(root, state, connection);
    }

    #[test]
    fn quota_planner_pacific_windows_handle_midnight_and_both_dst_transitions() {
        for (time, day, reset) in [
            (
                "2026-01-13T07:59:59Z",
                "2026-01-12",
                "2026-01-13T08:00:00+00:00",
            ),
            (
                "2026-01-13T08:00:00Z",
                "2026-01-13",
                "2026-01-14T08:00:00+00:00",
            ),
            (
                "2026-03-08T07:59:59Z",
                "2026-03-07",
                "2026-03-08T08:00:00+00:00",
            ),
            (
                "2026-03-08T10:00:00Z",
                "2026-03-08",
                "2026-03-09T07:00:00+00:00",
            ),
            (
                "2026-11-01T06:59:59Z",
                "2026-10-31",
                "2026-11-01T07:00:00+00:00",
            ),
            (
                "2026-11-01T09:00:00Z",
                "2026-11-01",
                "2026-11-02T08:00:00+00:00",
            ),
            (
                "2026-10-13T07:00:00Z",
                "2026-10-13",
                "2026-10-14T07:00:00+00:00",
            ),
        ] {
            assert_eq!(
                pacific_window(at(time)).unwrap(),
                (day.into(), reset.into()),
                "{time}"
            );
        }
        assert!(pacific_window(at("2006-01-01T00:00:00Z")).is_err());
    }

    #[test]
    fn quota_planner_aggregates_local_channels_in_one_project_and_excludes_other_projects() {
        let (root, state, connection) = fixture();
        let context = capture_observation_context(&connection, "channel-a").unwrap();
        let time = at("2026-10-13T08:00:00Z");
        let request = Uuid::new_v4().to_string();
        observe_request(&connection, &context, &request, "videos.insert", time).unwrap();
        observe_request(&connection, &context, &request, "videos.insert", time).unwrap();
        observe_request(
            &connection,
            &context,
            &Uuid::new_v4().to_string(),
            "captions.insert",
            time,
        )
        .unwrap();
        observe_request(
            &connection,
            &context,
            &Uuid::new_v4().to_string(),
            "resumable.probe",
            time,
        )
        .unwrap();
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        let other = capture_observation_context(&connection, "channel-b").unwrap();
        observe_request(
            &connection,
            &other,
            &Uuid::new_v4().to_string(),
            "videos.insert",
            time,
        )
        .unwrap();
        for method in ["captions.insert", "resumable.probe"] {
            observe_request(
                &connection,
                &other,
                &Uuid::new_v4().to_string(),
                method,
                time,
            )
            .unwrap();
        }
        let same_project = snapshot(&connection, "channel-b", time).unwrap();
        assert_eq!(same_project.channel_id, "channel-b");
        assert_eq!(same_project.usage[0].requests, 2);
        assert_eq!(same_project.usage[2].estimated_units, 800);
        assert_eq!(same_project.unknown_requests, 2);
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-c',oauth_project_id='different-project'",
                [],
            )
            .unwrap();
        let foreign_project = capture_observation_context(&connection, "channel-c").unwrap();
        for method in [
            "videos.insert",
            "search.list",
            "captions.insert",
            "resumable.probe",
        ] {
            observe_request(
                &connection,
                &foreign_project,
                &Uuid::new_v4().to_string(),
                method,
                time,
            )
            .unwrap();
        }
        let foreign_seen = snapshot(&connection, "channel-c", time).unwrap();
        assert_eq!(foreign_seen.usage[0].requests, 1);
        assert_eq!(foreign_seen.usage[1].requests, 1);
        assert_eq!(foreign_seen.usage[2].estimated_units, 400);
        assert_eq!(foreign_seen.unknown_requests, 1);
        // An old in-flight request remains attributed to its captured channel.
        observe_request(
            &connection,
            &context,
            &Uuid::new_v4().to_string(),
            "search.list",
            time,
        )
        .unwrap();
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-a',oauth_project_id='fixture-project'",
                [],
            )
            .unwrap();
        let seen = snapshot(&connection, "channel-a", time).unwrap();
        assert_eq!(seen.usage[0].requests, 2);
        assert_eq!(seen.usage[0].estimated_units, 2);
        assert_eq!(seen.usage[1].requests, 1);
        assert_eq!(seen.usage[2].estimated_units, 800);
        assert_eq!(seen.unknown_requests, 2);
        assert_eq!(seen.scope_id, same_project.scope_id);
        assert_ne!(seen.scope_id, foreign_seen.scope_id);
        assert_eq!(
            seen.observation_label,
            "This app/device observed · all local channels in this Cloud project"
        );
        assert!(seen
            .usage
            .iter()
            .all(|usage| usage.configured_daily_limit.is_none()));
        assert!(observe_request(
            &connection,
            &context,
            "not-a-native-request",
            "videos.insert",
            time
        )
        .is_err());
        assert!(observe_request(
            &connection,
            &context,
            &Uuid::new_v4().to_string(),
            "https://private-payload",
            time
        )
        .is_err());
        finish(root, state, connection);
    }

    #[test]
    fn quota_planner_unknown_projects_aggregate_only_the_exact_oauth_client() {
        let (root, state, connection) = fixture();
        let time = at("2026-10-13T08:00:00Z");
        connection
            .execute("UPDATE connection_settings SET oauth_project_id=NULL", [])
            .unwrap();
        let first = capture_observation_context(&connection, "channel-a").unwrap();
        observe_request(
            &connection,
            &first,
            &Uuid::new_v4().to_string(),
            "videos.insert",
            time,
        )
        .unwrap();
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        let second = capture_observation_context(&connection, "channel-b").unwrap();
        observe_request(
            &connection,
            &second,
            &Uuid::new_v4().to_string(),
            "videos.insert",
            time,
        )
        .unwrap();
        connection.execute("UPDATE connection_settings SET oauth_client_id='different-client.apps.googleusercontent.com'", []).unwrap();
        let foreign = capture_observation_context(&connection, "channel-b").unwrap();
        observe_request(
            &connection,
            &foreign,
            &Uuid::new_v4().to_string(),
            "videos.insert",
            time,
        )
        .unwrap();
        assert_eq!(
            snapshot(&connection, "channel-b", time).unwrap().usage[0].requests,
            1
        );
        connection.execute("UPDATE connection_settings SET active_channel_id='channel-a',oauth_client_id='fixture-client.apps.googleusercontent.com'", []).unwrap();
        let seen = snapshot(&connection, "channel-a", time).unwrap();
        assert_eq!(seen.usage[0].requests, 2);
        assert!(seen.configured_project_id.is_none());
        assert_eq!(
            seen.observation_label,
            "This app/device observed · all local channels using this OAuth client"
        );
        assert!(seen
            .warnings
            .iter()
            .any(|warning| warning
                .contains("other clients in its unknown project cannot be identified")));
        finish(root, state, connection);
    }

    #[test]
    fn quota_planner_captured_project_survives_replacement_and_usage_rolls_at_pacific_midnight() {
        let (root, state, connection) = fixture();
        let old = capture_observation_context(&connection, "channel-a").unwrap();
        observe_request(
            &connection,
            &old,
            &Uuid::new_v4().to_string(),
            "videos.insert",
            at("2026-10-13T06:59:59Z"),
        )
        .unwrap();
        observe_request(
            &connection,
            &old,
            &Uuid::new_v4().to_string(),
            "videos.insert",
            at("2026-10-13T07:00:00Z"),
        )
        .unwrap();
        assert_eq!(
            snapshot(&connection, "channel-a", at("2026-10-13T07:00:01Z"))
                .unwrap()
                .usage[0]
                .requests,
            1
        );
        connection
            .execute(
                "UPDATE connection_settings SET oauth_project_id='new-project'",
                [],
            )
            .unwrap();
        observe_request(
            &connection,
            &old,
            &Uuid::new_v4().to_string(),
            "videos.insert",
            at("2026-10-13T08:00:00Z"),
        )
        .unwrap();
        assert_eq!(
            snapshot(&connection, "channel-a", at("2026-10-13T08:00:01Z"))
                .unwrap()
                .usage[0]
                .requests,
            0
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM quota_observations WHERE quota_scope='project:fixture-project'",[],|row|row.get::<_,i64>(0)).unwrap(),3);
        finish(root, state, connection);
    }

    #[test]
    fn quota_planner_estimates_paginated_reads_assets_membership_and_retry_buckets() {
        let (root, state, connection) = fixture();
        let snapshot = snapshot(&connection, "channel-a", at("2026-10-13T08:00:00Z")).unwrap();
        let choices = QuotaChoices {
            uploads: 51,
            refresh_library: true,
            library_videos: 101,
            playlists: vec![PlaylistEstimate {
                existing_items: 51,
                additions: 5,
            }],
            thumbnail_writes: 2,
            caption_inserts: 1,
            caption_updates: 1,
            metadata_updates: 2,
            search_pages: 3,
            processing_checks_per_video: 1,
            retry_reserve: 1,
        };
        let estimate = estimate(snapshot, &choices).unwrap();
        assert_eq!(estimate.buckets[0].estimated_units, 102);
        assert_eq!(estimate.buckets[1].estimated_units, 6);
        let rows = &estimate.operations;
        assert_eq!(
            rows.iter()
                .find(|row| row.label.starts_with("Upload-library pages"))
                .unwrap()
                .requests,
            3
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.label.starts_with("Library metadata pages"))
                .unwrap()
                .requests,
            3
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.label.starts_with("Playlist membership pages"))
                .unwrap()
                .requests,
            2
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.method == "playlistItems.insert")
                .unwrap()
                .estimated_units,
            500
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.method == "thumbnails.set")
                .unwrap()
                .estimated_units,
            200
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.method == "captions.insert")
                .unwrap()
                .estimated_units,
            800
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.method == "captions.update")
                .unwrap()
                .estimated_units,
            900
        );
        assert!(estimate
            .buckets
            .iter()
            .all(|bucket| bucket.exceeds_configured_limit.is_none()));
        assert!(estimate
            .warnings
            .iter()
            .any(|warning| warning.contains("not permission")));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM quota_observations", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        finish(root, state, connection);
    }

    #[test]
    fn quota_planner_saves_explicit_limits_atomically_and_retains_all_independent_holds() {
        let (root, state, mut connection) = fixture();
        connection.execute("INSERT INTO api_quota_pauses(quota_scope,channel_id,pause_until,updated_at) VALUES('project:fixture-project','channel-a','2099-01-02T00:00:00Z',?1),('project:other-project','channel-b','2099-01-04T00:00:00Z',?1)",[now()]).unwrap();
        connection.execute("INSERT INTO upload_quota_pauses(channel_id,pause_until,updated_at) VALUES('channel-a','2099-01-03T00:00:00Z',?1),('channel-b','2099-01-05T00:00:00Z',?1)",[now()]).unwrap();
        let before = snapshot(&connection, "channel-a", Utc::now()).unwrap();
        assert!(before.cloud_hold.as_ref().unwrap().active);
        assert!(before.channel_hold.as_ref().unwrap().active);
        assert_eq!(
            before.cloud_hold.as_ref().unwrap().deadline,
            "2099-01-02T00:00:00Z"
        );
        assert_eq!(
            before.channel_hold.as_ref().unwrap().deadline,
            "2099-01-03T00:00:00Z"
        );
        let saved = save_limits(
            &mut connection,
            "channel-a",
            &before.scope_id,
            ProjectLimits {
                upload_calls: Some(0),
                search_calls: None,
                other_units: Some(12000),
            },
        )
        .unwrap();
        assert_eq!(saved.limits.upload_calls, Some(0));
        assert_eq!(saved.limits.search_calls, None);
        assert_eq!(saved.limits.other_units, Some(12000));
        let planned = estimate(
            saved,
            &QuotaChoices {
                uploads: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(planned.buckets[0].exceeds_configured_limit, Some(true));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM api_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT pause_until FROM upload_quota_pauses WHERE channel_id='channel-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "2099-01-03T00:00:00Z"
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM audit_events WHERE channel_id='channel-a' AND kind='quota_planning_limits_saved'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
        drop(connection);
        let connection = initialize_database(&state).unwrap();
        assert_eq!(
            snapshot(&connection, "channel-a", Utc::now())
                .unwrap()
                .limits
                .other_units,
            Some(12000)
        );
        finish(root, state, connection);
    }

    #[test]
    fn quota_planner_rejects_stale_scopes_unknown_projects_and_unbounded_inputs() {
        let (root, state, mut connection) = fixture();
        let before = snapshot(&connection, "channel-a", Utc::now()).unwrap();
        assert!(save_limits(
            &mut connection,
            "channel-b",
            &before.scope_id,
            ProjectLimits::default()
        )
        .is_err());
        connection
            .execute(
                "UPDATE connection_settings SET oauth_project_id='other-project'",
                [],
            )
            .unwrap();
        assert!(save_limits(
            &mut connection,
            "channel-a",
            &before.scope_id,
            ProjectLimits::default()
        )
        .is_err());
        connection
            .execute("UPDATE connection_settings SET oauth_project_id=NULL", [])
            .unwrap();
        let unknown = snapshot(&connection, "channel-a", Utc::now()).unwrap();
        assert!(unknown.configured_project_id.is_none());
        assert!(unknown
            .usage
            .iter()
            .all(|usage| usage.configured_daily_limit.is_none()));
        assert!(save_limits(
            &mut connection,
            "channel-a",
            &unknown.scope_id,
            ProjectLimits {
                upload_calls: Some(100),
                ..Default::default()
            }
        )
        .is_err());
        for choices in [
            QuotaChoices {
                uploads: MAX_COUNT + 1,
                ..Default::default()
            },
            QuotaChoices {
                retry_reserve: 6,
                ..Default::default()
            },
            QuotaChoices {
                processing_checks_per_video: 101,
                ..Default::default()
            },
            QuotaChoices {
                playlists: vec![PlaylistEstimate::default(); 65],
                ..Default::default()
            },
        ] {
            assert!(estimate(unknown.clone(), &choices).is_err());
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM quota_project_limits", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        finish(root, state, connection);
    }

    #[test]
    fn quota_planner_read_preserves_elapsed_and_malformed_pause_records() {
        let (root, state, connection) = fixture();
        connection.execute("INSERT INTO api_quota_pauses(quota_scope,channel_id,pause_until,updated_at) VALUES('project:fixture-project','channel-a','2007-01-01T00:00:00Z',?1)",[now()]).unwrap();
        connection.execute("INSERT INTO upload_quota_pauses(channel_id,pause_until,updated_at) VALUES('channel-a','malformed retained deadline',?1)",[now()]).unwrap();
        let before = connection.total_changes();
        let view = snapshot(&connection, "channel-a", at("2026-10-13T08:00:00Z")).unwrap();
        assert!(!view.cloud_hold.unwrap().active);
        assert!(view.channel_hold.unwrap().active);
        assert_eq!(connection.total_changes(), before);
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
        finish(root, state, connection);
    }
}
