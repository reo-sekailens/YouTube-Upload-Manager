//! Reviewed, filename-only discovery gates. No source bytes, provider calls or
//! captured upload choices are changed by this module.
use super::*;

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS folder_monitor_rules (
 monitor_id TEXT PRIMARY KEY NOT NULL,
 channel_id TEXT NOT NULL,
 rules_json TEXT NOT NULL,
 updated_at TEXT NOT NULL,
 FOREIGN KEY(monitor_id) REFERENCES folder_monitors(id)
);
CREATE INDEX IF NOT EXISTS folder_rules_channel ON folder_monitor_rules(channel_id,monitor_id);
CREATE TABLE IF NOT EXISTS folder_rule_exclusions (
 monitor_id TEXT NOT NULL,
 channel_id TEXT NOT NULL,
 file_name TEXT NOT NULL,
 size_bytes INTEGER NOT NULL,
 modified_key TEXT NOT NULL,
 PRIMARY KEY(monitor_id,channel_id,file_name),
 FOREIGN KEY(monitor_id) REFERENCES folder_monitors(id)
);
"#;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Rules {
    pub(super) include_patterns: Vec<String>,
    pub(super) exclude_patterns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) ready_marker_suffix: Option<String>,
    pub(super) admit_previously_excluded: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Settings {
    monitor_id: String,
    channel_id: String,
    rules: Rules,
    revision: String,
}

pub(super) struct ScanRules {
    settings: Settings,
    captured: HashSet<String>,
    exclusions: HashMap<String, (u64, String)>,
}

pub(super) fn prepare_scan(
    connection: &Connection,
    id: &str,
    channel: &str,
) -> Result<ScanRules, String> {
    let settings = load(connection, id, channel)?;
    let captured=connection.prepare("SELECT file_path FROM folder_monitor_observations WHERE monitor_id=?1 AND channel_id=?2 AND upload_item_id IS NOT NULL").map_err(user_error)?.query_map(params![id,channel],|row|row.get::<_,String>(0)).map_err(user_error)?.collect::<Result<HashSet<_>,_>>().map_err(user_error)?;
    let exclusions=connection.prepare("SELECT file_name,size_bytes,modified_key FROM folder_rule_exclusions WHERE monitor_id=?1 AND channel_id=?2").map_err(user_error)?.query_map(params![id,channel],|row|Ok((row.get::<_,String>(0)?,(row.get::<_,i64>(1)?.max(0) as u64,row.get::<_,String>(2)?)))).map_err(user_error)?.collect::<Result<HashMap<_,_>,_>>().map_err(user_error)?;
    Ok(ScanRules {
        settings,
        captured,
        exclusions,
    })
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Example {
    file_name: String,
    disposition: String,
    detail: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Preview {
    monitor_id: String,
    channel_id: String,
    folder_name: String,
    rules: Rules,
    fingerprint: String,
    total: usize,
    included: usize,
    excluded: usize,
    waiting_ready: usize,
    changing: usize,
    retained_queue: usize,
    examples: Vec<Example>,
}

fn validate(mut rules: Rules) -> Result<Rules, String> {
    if rules.include_patterns.len() + rules.exclude_patterns.len() > 32 {
        return Err("Use at most 32 filename patterns in total.".into());
    }
    for patterns in [&mut rules.include_patterns, &mut rules.exclude_patterns] {
        let mut unique = HashSet::new();
        for pattern in patterns.iter_mut() {
            *pattern = pattern.trim().to_string();
            if pattern.is_empty()
                || pattern.chars().count() > 256
                || pattern.chars().any(|c| {
                    c.is_control() || matches!(c, '/' | '\\' | ':' | '[' | ']' | '{' | '}')
                })
            {
                return Err("Use filename patterns up to 256 characters with * or ? only. Folder paths and bracket patterns are not supported.".into());
            }
            if !unique.insert(pattern.to_lowercase()) {
                return Err("Remove repeated filename patterns before reviewing.".into());
            }
        }
    }
    if let Some(suffix) = rules.ready_marker_suffix.as_mut() {
        *suffix = suffix.trim().to_string();
        if suffix.len() < 2
            || suffix.len() > 32
            || !suffix.starts_with('.')
            || suffix[1..]
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.')
            || suffix.contains("..")
            || suffix.rsplit('.').next().is_some_and(|part| {
                SUPPORTED_VIDEO_EXTENSIONS.contains(&part.to_ascii_lowercase().as_str())
            })
        {
            return Err("Use a marker suffix such as .ready: 2–32 simple letters, numbers, dots, dashes or underscores, starting with a dot and not a video extension.".into());
        }
    }
    Ok(rules)
}

/// Case-insensitive filename globbing; * spans characters and ? spans one.
fn matches(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_lowercase().chars().collect::<Vec<_>>();
    let name = name.to_lowercase().chars().collect::<Vec<_>>();
    let (mut p, mut n, mut star, mut after_star) = (0, 0, None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            after_star = n;
        } else if let Some(saved) = star {
            after_star += 1;
            n = after_star;
            p = saved + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}
fn allowed(rules: &Rules, name: &str) -> bool {
    (rules.include_patterns.is_empty() || rules.include_patterns.iter().any(|p| matches(p, name)))
        && !rules.exclude_patterns.iter().any(|p| matches(p, name))
}
fn scoped(
    connection: &Connection,
    id: &str,
    channel: &str,
    allow_removed: bool,
) -> Result<FolderMonitorSettings, String> {
    folder_monitors::resolve(connection, Some(id), Some(channel), allow_removed)?;
    let settings = folder_monitors::settings_for(connection, id)?;
    if settings.channel_id.as_deref() != Some(channel) {
        return Err("This folder belongs to a different channel.".into());
    }
    Ok(settings)
}
pub(super) fn load(connection: &Connection, id: &str, channel: &str) -> Result<Settings, String> {
    scoped(connection, id, channel, false)?;
    let row: Option<(String, String, String)> = connection
        .query_row(
            "SELECT channel_id,rules_json,updated_at FROM folder_monitor_rules WHERE monitor_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(user_error)?;
    if row.as_ref().is_some_and(|(owner, _, _)| owner != channel) {
        return Err(
            "Saved folder rules have inconsistent channel ownership; discovery remains stopped."
                .into(),
        );
    }
    let (rules, revision) = match row {
        Some((_, json, revision)) => (
            validate(serde_json::from_str(&json).map_err(|_| {
                "Saved folder rules are unreadable; discovery must stop.".to_string()
            })?)?,
            revision,
        ),
        None => (Rules::default(), "initial".into()),
    };
    Ok(Settings {
        monitor_id: id.into(),
        channel_id: channel.into(),
        rules,
        revision,
    })
}
fn retained(
    connection: &Connection,
    id: &str,
    channel: &str,
    file: &MonitoredFile,
) -> Result<bool, String> {
    connection.query_row("SELECT EXISTS(SELECT 1 FROM folder_rule_exclusions WHERE monitor_id=?1 AND channel_id=?2 AND file_name=?3 AND size_bytes=?4 AND modified_key=?5)",params![id,channel,file.file_name,file.size_bytes as i64,file.modified_key],|r|r.get(0)).map_err(user_error)
}
fn marker_status(file: &MonitoredFile, suffix: &str) -> (&'static str, &'static str) {
    let Some(parent) = file.path.parent() else {
        return (
            "invalid_ready_marker",
            "The producer-ready marker location is invalid.",
        );
    };
    let marker = parent.join(format!("{}{suffix}", file.file_name));
    let exists = fs::symlink_metadata(&marker);
    if exists
        .as_ref()
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return (
            "waiting_ready_marker",
            "Waiting for an empty producer-ready marker next to this video.",
        );
    }
    if has_link_or_reparse_component(&marker).unwrap_or(true) {
        return (
            "invalid_ready_marker",
            "The ready marker is linked or unavailable; it cannot authorize this source.",
        );
    }
    let Ok(handle) = open_reference_source(&marker) else {
        return (
            "invalid_ready_marker",
            "The ready marker must be a regular, readable empty file.",
        );
    };
    let Ok(metadata) = handle.metadata() else {
        return ("invalid_ready_marker", "The ready marker is unavailable.");
    };
    if !metadata.is_file() || metadata.len() != 0 {
        return (
            "invalid_ready_marker",
            "The ready marker must be empty. Its contents are never read or executed.",
        );
    }
    let current_source =
        open_reference_source(&file.path).and_then(|handle| handle.metadata().map_err(user_error));
    let Ok(source) = current_source else {
        return (
            "invalid_ready_marker",
            "The source is unavailable; the marker cannot replace source-integrity checks.",
        );
    };
    if monitored_file_signature(&source).ok() != Some((file.size_bytes, file.modified_key.clone()))
    {
        return (
            "changing",
            "The source changed during this scan; wait for two unchanged scans.",
        );
    }
    match (metadata.modified(),source.modified()) {
        (Ok(marker_time),Ok(source_time)) if marker_time>=source_time=>("included","The empty ready marker was created or updated after this source."),
        _=>("waiting_ready_marker","The marker predates the current source. Have the producer refresh it after completing the export."),
    }
}
fn classify(
    connection: &Connection,
    id: &str,
    channel: &str,
    file: &MonitoredFile,
    rules: &Rules,
) -> Result<Example, String> {
    let captured:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM folder_monitor_observations WHERE monitor_id=?1 AND channel_id=?2 AND file_path=?3 AND upload_item_id IS NOT NULL)",params![id,channel,file.path.to_string_lossy()],|r|r.get(0)).map_err(user_error)?;
    let (disposition, detail) = if captured {
        ("retained_queue","This video already has a captured queue/history identity. Rules do not change its approved choices.")
    } else if !allowed(rules, &file.file_name) {
        (
            "excluded",
            "This filename is excluded by the proposed include/exclude rules.",
        )
    } else if !rules.admit_previously_excluded && retained(connection, id, channel, file)? {
        ("retained_excluded","This previously excluded, unchanged file stays withheld until explicitly admitted in final review.")
    } else if let Some(suffix) = rules.ready_marker_suffix.as_deref() {
        marker_status(file, suffix)
    } else {
        ("included","Included by these rules. Existing stability, integrity, duplicate and channel checks still apply.")
    };
    Ok(Example {
        file_name: file.file_name.clone(),
        disposition: disposition.into(),
        detail: detail.into(),
    })
}
pub(super) fn preview(
    state: &AppState,
    id: &str,
    channel: &str,
    rules: Rules,
) -> Result<Preview, String> {
    let rules = validate(rules)?;
    let connection = database(state)?;
    let settings = scoped(&connection, id, channel, false)?;
    let saved = load(&connection, id, channel)?;
    let folder = Path::new(
        settings
            .folder_path
            .as_deref()
            .ok_or("The saved folder is unavailable.")?,
    );
    // Preview is read-only, including legacy roots whose normal scan may
    // refresh a lexical migration key. Never call that mutating scan helper.
    let canonical = folder
        .canonicalize()
        .map_err(|_| "The watched folder is unavailable for preview.".to_string())?;
    let key = folder_monitors::root_key(&canonical.to_string_lossy());
    let saved_key: String = connection
        .query_row(
            "SELECT folder_key FROM folder_monitors WHERE id=?1 AND channel_id=?2",
            params![id, channel],
            |row| row.get(0),
        )
        .map_err(user_error)?;
    if saved_key != key && id != "legacy-singleton" {
        return Err("This folder now resolves to a different location. Restore its approved root before previewing rules.".into());
    }
    let others = connection
        .prepare("SELECT folder_path FROM folder_monitors WHERE id!=?1 AND removed=0")
        .map_err(user_error)?
        .query_map([id], |row| row.get::<_, Option<String>>(0))
        .map_err(user_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(user_error)?;
    if others
        .iter()
        .filter_map(|path| path.as_deref())
        .filter_map(|path| Path::new(path).canonicalize().ok())
        .any(|path| folder_monitors::root_key(&path.to_string_lossy()) == key)
    {
        return Err("This folder overlaps another approved root. Resolve that configuration before reviewing rules.".into());
    }
    let files = monitored_files(folder)?;
    if files.len() > 10_000 {
        return Err("This folder has more than 10,000 supported direct-child videos. Narrow it before reviewing rules.".into());
    }
    let mut result = Preview {
        monitor_id: id.into(),
        channel_id: channel.into(),
        folder_name: folder
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Watched folder")
            .into(),
        rules: rules.clone(),
        fingerprint: String::new(),
        total: files.len(),
        included: 0,
        excluded: 0,
        waiting_ready: 0,
        changing: 0,
        retained_queue: 0,
        examples: Vec::new(),
    };
    let mut digest = blake3::Hasher::new();
    digest.update(
        serde_json::to_string(&(
            id,
            channel,
            &settings.folder_path,
            &saved.revision,
            &saved.rules,
            &rules,
        ))
        .map_err(user_error)?
        .as_bytes(),
    );
    let mut sampled = HashMap::<String, usize>::new();
    for file in files {
        let mut example = classify(&connection, id, channel, &file, &rules)?;
        if example.disposition == "included" {
            let stable:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM folder_monitor_observations WHERE monitor_id=?1 AND channel_id=?2 AND file_path=?3 AND size_bytes=?4 AND modified_key=?5)",params![id,channel,file.path.to_string_lossy(),file.size_bytes as i64,file.modified_key],|r|r.get(0)).map_err(user_error)?;
            if !stable {
                example.disposition = "changing".into();
                example.detail =
                    "Included, but an unchanged second scan is still required before intake."
                        .into();
            }
        }
        match example.disposition.as_str() {
            "included" => result.included += 1,
            "excluded" | "retained_excluded" => result.excluded += 1,
            "retained_queue" => result.retained_queue += 1,
            "changing" => result.changing += 1,
            _ => result.waiting_ready += 1,
        }
        digest.update(
            serde_json::to_string(&(
                &file.file_name,
                file.size_bytes,
                &file.modified_key,
                &example.disposition,
            ))
            .map_err(user_error)?
            .as_bytes(),
        );
        let category = sampled.entry(example.disposition.clone()).or_default();
        if result.examples.len() < 100 && *category < 15 {
            *category += 1;
            result.examples.push(example);
        }
    }
    scoped(&connection, id, channel, false)?;
    result.fingerprint = digest.finalize().to_hex().to_string();
    Ok(result)
}

pub(super) fn save(
    state: &AppState,
    id: &str,
    channel: &str,
    rules: Rules,
    fingerprint: &str,
    approved: bool,
) -> Result<Settings, String> {
    if !approved {
        return Err("Review and approve these folder rules before saving.".into());
    }
    let _guard = state
        .folder_monitor_lock
        .lock()
        .map_err(|_| "Folder discovery is unavailable.".to_string())?;
    let preview = preview(state, id, channel, rules)?;
    if preview.fingerprint != fingerprint {
        return Err("The folder or its saved rules changed during review. Refresh the examples and approve the current result.".into());
    }
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    scoped(&transaction, id, channel, false)?;
    if preview.rules.admit_previously_excluded {
        transaction
            .execute(
                "DELETE FROM folder_rule_exclusions WHERE monitor_id=?1 AND channel_id=?2",
                params![id, channel],
            )
            .map_err(user_error)?;
    }
    let mut persisted = preview.rules.clone();
    persisted.admit_previously_excluded = false;
    transaction.execute("INSERT INTO folder_monitor_rules(monitor_id,channel_id,rules_json,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(monitor_id) DO UPDATE SET rules_json=excluded.rules_json,updated_at=excluded.updated_at WHERE folder_monitor_rules.channel_id=excluded.channel_id",params![id,channel,serde_json::to_string(&persisted).map_err(user_error)?,now()]).map_err(user_error)?;
    folder_monitors::audit_monitor(
        &transaction,
        id,
        "folder_rules_reviewed",
        if preview.rules.admit_previously_excluded {
            "Operator approved filename rules and readiness gates, including reconsideration of previously excluded originals. Existing queue choices remain unchanged."
        } else {
            "Operator approved filename rules and readiness gates; previously excluded unchanged originals remain withheld. Existing queue choices remain unchanged."
        },
    )?;
    transaction.commit().map_err(user_error)?;
    load(&connection, id, channel)
}

/// Return a displayed withholding state, or None to preserve normal discovery.
/// Retained queue identities bypass these new discovery-only gates.
pub(super) fn gate(
    connection: &Connection,
    id: &str,
    channel: &str,
    file: &MonitoredFile,
) -> Result<Option<&'static str>, String> {
    gate_for_scan(connection, &prepare_scan(connection, id, channel)?, file)
}

pub(super) fn gate_for_scan(
    connection: &Connection,
    scan: &ScanRules,
    file: &MonitoredFile,
) -> Result<Option<&'static str>, String> {
    let Settings {
        monitor_id: id,
        channel_id: channel,
        rules,
        ..
    } = &scan.settings;
    if scan.captured.contains(file.path.to_string_lossy().as_ref()) {
        return Ok(None);
    }
    let retained = scan
        .exclusions
        .get(&file.file_name)
        .is_some_and(|(size, key)| *size == file.size_bytes && key == &file.modified_key);
    let disposition = if !allowed(rules, &file.file_name) {
        "excluded"
    } else if !rules.admit_previously_excluded && retained {
        "retained_excluded"
    } else if let Some(suffix) = rules.ready_marker_suffix.as_deref() {
        marker_status(file, suffix).0
    } else {
        "included"
    };
    let path = file.path.to_string_lossy();
    let blocked = match disposition {
        "excluded" => Some("excluded"),
        "retained_excluded" => Some("retained_excluded"),
        "waiting_ready_marker" => Some("waiting_ready_marker"),
        "invalid_ready_marker" => Some("invalid_ready_marker"),
        "changing" => Some("observed"),
        _ => None,
    };
    if let Some(blocked) = blocked {
        let transaction = connection.unchecked_transaction().map_err(user_error)?;
        scoped(&transaction, id, channel, false)?;
        transaction.execute("INSERT INTO folder_monitor_observations(monitor_id,channel_id,channel_name,file_path,size_bytes,modified_key,state,first_seen_at,updated_at) SELECT ?1,?2,channel_name,?3,?4,?5,?6,?7,?7 FROM folder_monitors WHERE id=?1 AND channel_id=?2 ON CONFLICT(monitor_id,file_path) DO UPDATE SET size_bytes=excluded.size_bytes,modified_key=excluded.modified_key,state=excluded.state,updated_at=excluded.updated_at WHERE folder_monitor_observations.channel_id=excluded.channel_id AND folder_monitor_observations.upload_item_id IS NULL AND (folder_monitor_observations.state!=excluded.state OR folder_monitor_observations.size_bytes!=excluded.size_bytes OR folder_monitor_observations.modified_key!=excluded.modified_key)",params![id,channel,path,file.size_bytes as i64,file.modified_key,blocked,now()]).map_err(user_error)?;
        if blocked == "excluded" {
            transaction.execute("INSERT INTO folder_rule_exclusions(monitor_id,channel_id,file_name,size_bytes,modified_key) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(monitor_id,channel_id,file_name) DO UPDATE SET size_bytes=excluded.size_bytes,modified_key=excluded.modified_key",params![id,channel,file.file_name,file.size_bytes as i64,file.modified_key]).map_err(user_error)?;
        }
        transaction.commit().map_err(user_error)?;
        return Ok(Some(blocked));
    }
    let reopened=connection.execute("UPDATE folder_monitor_observations SET state='observed',size_bytes=?1,modified_key=?2,updated_at=?3 WHERE monitor_id=?4 AND channel_id=?5 AND file_path=?6 AND upload_item_id IS NULL AND state IN ('excluded','retained_excluded','waiting_ready_marker','invalid_ready_marker')",params![file.size_bytes as i64,file.modified_key,now(),id,channel,path]).map_err(user_error)?;
    Ok(if reopened > 0 { Some("observed") } else { None })
}

#[tauri::command]
pub(super) async fn load_folder_rules(
    state: State<'_, AppState>,
    monitor_id: String,
    expected_channel_id: String,
) -> Result<Settings, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        load(&database(&state)?, &monitor_id, &expected_channel_id)
    })
    .await
    .map_err(user_error)?
}
#[tauri::command]
pub(super) async fn preview_folder_rules(
    state: State<'_, AppState>,
    monitor_id: String,
    expected_channel_id: String,
    rules: Rules,
) -> Result<Preview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        preview(&state, &monitor_id, &expected_channel_id, rules)
    })
    .await
    .map_err(user_error)?
}
#[tauri::command]
pub(super) async fn save_folder_rules(
    state: State<'_, AppState>,
    monitor_id: String,
    expected_channel_id: String,
    rules: Rules,
    preview_fingerprint: String,
    approved: bool,
) -> Result<Settings, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        save(
            &state,
            &monitor_id,
            &expected_channel_id,
            rules,
            &preview_fingerprint,
            approved,
        )
    })
    .await
    .map_err(user_error)?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, AppState, Connection, String) {
        let root = std::env::temp_dir().join(format!("youtube-folder-rules-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        fs::create_dir_all(root.join("watched")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute_batch(SCHEMA_SQL).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET active_channel_id='channel-a'",[now()]).unwrap();
        let settings = folder_monitors::configure(
            &state,
            root.join("watched").to_string_lossy().into_owned(),
            "unlisted".into(),
            false,
            false,
            None,
            None,
            None,
            Some("channel-a".into()),
        )
        .unwrap();
        (root, state, connection, settings.id.unwrap())
    }
    fn video(root: &Path, name: &str) -> MonitoredFile {
        let path = root.join("watched").join(name);
        fs::write(&path, b"video-original").unwrap();
        let (size_bytes, modified_key) =
            monitored_file_signature(&fs::metadata(&path).unwrap()).unwrap();
        MonitoredFile {
            path,
            file_name: name.into(),
            size_bytes,
            modified_key,
        }
    }
    fn approve(state: &AppState, id: &str, rules: Rules) -> Settings {
        let result = preview(state, id, "channel-a", rules.clone()).unwrap();
        save(state, id, "channel-a", rules, &result.fingerprint, true).unwrap()
    }
    #[test]
    fn simple_globs_are_case_insensitive_filename_only_and_exclusions_win() {
        let rules = validate(Rules {
            include_patterns: vec!["Camera_*.MP4".into()],
            exclude_patterns: vec!["*_preview.*".into()],
            ..Rules::default()
        })
        .unwrap();
        assert!(allowed(&rules, "camera_A.mp4"));
        assert!(!allowed(&rules, "Camera_preview.mp4"));
        assert!(!allowed(&rules, "Session_A.mp4"));
        assert!(matches("session_??.mov", "SESSION_01.MOV"));
        assert!(!matches("session_??.mov", "session_1.mov"));
        assert!(matches("*hé?lo*", "HÉLLO.mp4"));
        for pattern in ["../*.mp4", "C:\\*.mp4", "[a-z]*", "{one,two}*"] {
            assert!(validate(Rules {
                include_patterns: vec![pattern.into()],
                ..Rules::default()
            })
            .is_err());
        }
        for suffix in ["../ready", ".mp4", ".3g2", ".", ".ready/other"] {
            assert!(validate(Rules {
                ready_marker_suffix: Some(suffix.into()),
                ..Rules::default()
            })
            .is_err());
        }
    }
    #[test]
    fn preview_reads_examples_without_creating_observations_or_changing_rules() {
        let (root, state, connection, id) = fixture();
        video(&root, "Camera_A.mp4");
        video(&root, "test_preview.mp4");
        let rules = Rules {
            include_patterns: vec!["Camera_*.mp4".into()],
            ..Rules::default()
        };
        let result = preview(&state, &id, "channel-a", rules).unwrap();
        assert_eq!((result.total, result.excluded, result.changing), (2, 1, 1));
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM folder_monitor_observations",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM folder_monitor_rules", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(root.join("watched/Camera_A.mp4").exists());
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn excluded_originals_need_explicit_admission_and_restart_stability_after_approval() {
        let (root, state, connection, id) = fixture();
        let file = video(&root, "test_clip.mp4");
        approve(
            &state,
            &id,
            Rules {
                exclude_patterns: vec!["test_*".into()],
                ..Rules::default()
            },
        );
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("excluded")
        );
        approve(&state, &id, Rules::default());
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("retained_excluded")
        );
        let saved = approve(
            &state,
            &id,
            Rules {
                admit_previously_excluded: true,
                ..Rules::default()
            },
        );
        assert!(!saved.rules.admit_previously_excluded);
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("observed")
        );
        assert_eq!(gate(&connection, &id, "channel-a", &file).unwrap(), None);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ready_markers_add_a_gate_and_never_parse_payload_or_override_stability() {
        let (root, state, connection, id) = fixture();
        let file = video(&root, "clip.mp4");
        let marker = root.join("watched/clip.mp4.ready");
        approve(
            &state,
            &id,
            Rules {
                ready_marker_suffix: Some(".ready".into()),
                ..Rules::default()
            },
        );
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("waiting_ready_marker")
        );
        fs::write(&marker, b"{\"source\":\"other.mp4\",\"public\":true}").unwrap();
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("invalid_ready_marker")
        );
        fs::write(&marker, b"").unwrap();
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("observed")
        );
        assert_eq!(gate(&connection, &id, "channel-a", &file).unwrap(), None);
        assert_eq!(fs::read(&file.path).unwrap(), b"video-original");
        fs::remove_file(&marker).unwrap();
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("waiting_ready_marker")
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_marker_does_not_authorize_a_new_export() {
        let (root, state, connection, id) = fixture();
        let file = video(&root, "clip.mp4");
        let marker = root.join("watched/clip.mp4.ready");
        fs::File::create(&marker)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH))
            .unwrap();
        approve(
            &state,
            &id,
            Rules {
                ready_marker_suffix: Some(".ready".into()),
                ..Rules::default()
            },
        );
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("waiting_ready_marker")
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn queued_identity_and_approved_choices_survive_rules_pause_remove_and_readd() {
        let (root, state, connection, id) = fixture();
        let file = video(&root, "clip.mp4");
        let timestamp = now();
        connection.execute("INSERT INTO upload_items(id,title,file_name,channel_id,folder_monitor_id,workspace_path,size_bytes,status,total_bytes,visibility,made_for_kids,created_at,updated_at) VALUES('retained','Retained','clip.mp4','channel-a',?1,?2,14,'queued',14,'private',1,?3,?3)",params![id,file.path.to_string_lossy(),timestamp]).unwrap();
        connection.execute("INSERT INTO folder_monitor_observations(monitor_id,channel_id,channel_name,file_path,size_bytes,modified_key,state,upload_item_id,first_seen_at,updated_at) VALUES(?1,'channel-a','Fixture',?2,?3,?4,'queued','retained',?5,?5)",params![id,file.path.to_string_lossy(),file.size_bytes as i64,file.modified_key,timestamp]).unwrap();
        approve(
            &state,
            &id,
            Rules {
                exclude_patterns: vec!["*".into()],
                ready_marker_suffix: Some(".ready".into()),
                ..Rules::default()
            },
        );
        assert_eq!(gate(&connection, &id, "channel-a", &file).unwrap(), None);
        folder_monitors::pause(&state, &id, Some("channel-a"), true).unwrap();
        let restored = folder_monitors::configure(
            &state,
            root.join("watched").to_string_lossy().into_owned(),
            "unlisted".into(),
            false,
            false,
            None,
            None,
            None,
            Some("channel-a".into()),
        )
        .unwrap();
        assert_eq!(restored.id.as_deref(), Some(id.as_str()));
        assert_eq!(
            load(&connection, &id, "channel-a")
                .unwrap()
                .rules
                .exclude_patterns,
            vec!["*"]
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT status,visibility,made_for_kids FROM upload_items WHERE id='retained'",
                    [],
                    |r| Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, bool>(2)?
                    ))
                )
                .unwrap(),
            ("queued".into(), "private".into(), true)
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stale_folder_preview_wrong_channel_and_missing_approval_fail_before_save() {
        let (root, state, connection, id) = fixture();
        video(&root, "clip.mp4");
        let rules = Rules {
            exclude_patterns: vec!["test_*".into()],
            ..Rules::default()
        };
        let result = preview(&state, &id, "channel-a", rules.clone()).unwrap();
        assert!(save(
            &state,
            &id,
            "channel-a",
            rules.clone(),
            &result.fingerprint,
            false
        )
        .is_err());
        video(&root, "another.mp4");
        assert!(save(&state, &id, "channel-a", rules, &result.fingerprint, true).is_err());
        assert!(load(&connection, &id, "channel-b").is_err());
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        assert!(preview(&state, &id, "channel-a", Rules::default()).is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM folder_monitor_rules", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn a_linked_ready_marker_cannot_admit_a_source() {
        let (root, state, connection, id) = fixture();
        let file = video(&root, "clip.mp4");
        let target = root.join("outside.ready");
        fs::write(&target, b"").unwrap();
        std::os::unix::fs::symlink(&target, root.join("watched/clip.mp4.ready")).unwrap();
        approve(
            &state,
            &id,
            Rules {
                ready_marker_suffix: Some(".ready".into()),
                ..Rules::default()
            },
        );
        assert_eq!(
            gate(&connection, &id, "channel-a", &file).unwrap(),
            Some("invalid_ready_marker")
        );
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn large_preview_keeps_representative_excluded_examples_beyond_the_first_hundred_files() {
        let (root, state, connection, id) = fixture();
        for index in 0..110 {
            video(&root, &format!("Camera_{index:03}.mp4"));
        }
        video(&root, "test_preview.mp4");
        let result = preview(
            &state,
            &id,
            "channel-a",
            Rules {
                exclude_patterns: vec!["test_*".into()],
                ..Rules::default()
            },
        )
        .unwrap();
        assert_eq!(
            (result.total, result.excluded, result.changing),
            (111, 1, 110)
        );
        assert!(result
            .examples
            .iter()
            .any(|example| example.file_name == "test_preview.mp4"
                && example.disposition == "excluded"));
        assert!(result.examples.len() <= 100);
        drop(connection);
        fs::remove_dir_all(root).unwrap();
    }
}
