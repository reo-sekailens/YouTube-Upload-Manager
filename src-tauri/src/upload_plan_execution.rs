//! Durable, channel-scoped intake claims prevent replaying one reviewed plan.
use super::*;

pub(super) const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS upload_plan_executions (plan_id TEXT PRIMARY KEY NOT NULL, channel_id TEXT NOT NULL, state TEXT NOT NULL, receipt_json TEXT, updated_at TEXT NOT NULL);";
const MAX_ASSET_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetExecutionManifest {
    kind: String,
    assets: video_assets::PreparedPlanAssets,
    materializations: HashMap<usize, AssetMaterialization>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetMaterialization {
    item_id: String,
    job_ids: Vec<String>,
}
fn manifest_text(manifest: &AssetExecutionManifest) -> Result<String, String> {
    let json = serde_json::to_string(manifest).map_err(user_error)?;
    if json.len() > MAX_ASSET_MANIFEST_BYTES {
        return Err("The approved attachment manifest exceeds native recovery bounds.".into());
    }
    Ok(json)
}
fn read_manifest(
    connection: &Connection,
    plan: &str,
    channel: &str,
) -> Result<Option<AssetExecutionManifest>, String> {
    upload_metadata::require_channel(connection, channel)?;
    let row: Option<(String, String, Option<String>)> = connection
        .query_row(
            "SELECT channel_id,state,receipt_json FROM upload_plan_executions WHERE plan_id=?1",
            [plan],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(user_error)?;
    let (owner, state, json) =
        row.ok_or("The prospective attachment intake has no durable execution claim.")?;
    if owner != channel {
        return Err("The approved attachment manifest belongs to another channel.".into());
    }
    if state == "complete" {
        return Ok(None);
    }
    let Some(json) = json else {
        return Ok(None);
    };
    if json.len() > MAX_ASSET_MANIFEST_BYTES {
        return Err("The retained attachment manifest exceeds recovery bounds; upload identities remain saved.".into());
    }
    let manifest:AssetExecutionManifest=serde_json::from_str(&json).map_err(|_|"The retained attachment proof could not be read. Existing upload identities must not be imported again.".to_string())?;
    if manifest.kind != "asset_manifest" || manifest.materializations.len() > 512 {
        return Err(
            "The retained attachment manifest is invalid. Preserve the existing upload receipts."
                .into(),
        );
    }
    Ok(Some(manifest))
}
pub(super) fn asset_manifest(
    connection: &Connection,
    plan: &str,
    channel: &str,
) -> Result<Option<video_assets::PreparedPlanAssets>, String> {
    Ok(read_manifest(connection, plan, channel)?.map(|manifest| manifest.assets))
}
pub(super) fn asset_materialization(
    connection: &Connection,
    plan: &str,
    channel: &str,
    ordinal: usize,
) -> Result<Option<(String, Vec<String>)>, String> {
    Ok(read_manifest(connection, plan, channel)?
        .and_then(|manifest| manifest.materializations.get(&ordinal).cloned())
        .map(|saved| (saved.item_id, saved.job_ids)))
}
pub(super) fn record_asset_materialization(
    connection: &Connection,
    plan: &str,
    channel: &str,
    ordinal: usize,
    item_id: &str,
    job_ids: &[String],
) -> Result<(), String> {
    let mut manifest = read_manifest(connection, plan, channel)?
        .ok_or("This prospective attachment intake has no retained approval manifest.")?;
    if manifest.materializations.contains_key(&ordinal) || ordinal >= 512 || job_ids.len() > 400 {
        return Err("This attachment ordinal was already bound to a saved upload identity.".into());
    }
    manifest.materializations.insert(
        ordinal,
        AssetMaterialization {
            item_id: item_id.into(),
            job_ids: job_ids.to_vec(),
        },
    );
    let changed=connection.execute("UPDATE upload_plan_executions SET receipt_json=?3,updated_at=?4 WHERE plan_id=?1 AND channel_id=?2 AND state IN ('started','needs_attention')",params![plan,channel,manifest_text(&manifest)?,now()]).map_err(user_error)?;
    if changed != 1 {
        return Err(
            "The attachment execution claim changed before saving its video identity.".into(),
        );
    }
    Ok(())
}

pub(super) fn receipt(
    connection: &Connection,
    plan: &str,
    channel: &str,
) -> Result<Option<BatchImportReceipt>, String> {
    upload_metadata::require_channel(connection, channel)?;
    let saved: Option<(String, String, Option<String>)> = connection
        .query_row(
            "SELECT channel_id,state,receipt_json FROM upload_plan_executions WHERE plan_id=?1",
            [plan],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(user_error)?;
    match saved {
        None => Ok(None),
        Some((owner, state, Some(json))) if owner == channel && state == "complete" => serde_json::from_str(&json).map(Some).map_err(|_| "The saved intake receipt could not be read. Existing queued originals remain saved; this plan cannot be repeated.".into()),
        Some((owner,_,_)) if owner != channel => Err("This batch plan belongs to another channel.".into()),
        _ => Err("This plan already started intake. Review its existing queue items and receipts; repeating it would create another upload identity.".into()),
    }
}

/// Call only after the retained plan has passed native source/channel review.
pub(super) fn claim(connection: &mut Connection, plan: &str, channel: &str) -> Result<(), String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    upload_metadata::require_channel(&transaction, channel)?;
    let assets = upload_plan::execution_assets(&transaction, plan, channel)?
        .map(|assets| {
            manifest_text(&AssetExecutionManifest {
                kind: "asset_manifest".into(),
                assets,
                materializations: HashMap::new(),
            })
        })
        .transpose()?;
    let changed = transaction.execute("INSERT OR IGNORE INTO upload_plan_executions(plan_id,channel_id,state,receipt_json,updated_at) VALUES(?1,?2,'started',?3,?4)", params![plan,channel,assets,now()]).map_err(user_error)?;
    if changed != 1 {
        return Err("This reviewed plan already started intake. Its saved upload identities cannot be created again.".into());
    }
    audit_global(
        &transaction,
        "upload_plan_intake_claimed",
        "Operator submitted a reviewed plan; its intake identity was claimed once",
    )?;
    transaction.commit().map_err(user_error)
}

pub(super) fn finish(
    connection: &Connection,
    plan: &str,
    channel: &str,
    result: &Result<BatchImportReceipt, String>,
) -> Result<(), String> {
    let (state, receipt) = match result {
        Ok(receipt) => (
            "complete",
            Some(serde_json::to_string(receipt).map_err(user_error)?),
        ),
        Err(_) => ("needs_attention", None),
    };
    let changed = connection.execute("UPDATE upload_plan_executions SET state=?3,receipt_json=COALESCE(?4,receipt_json),updated_at=?5 WHERE plan_id=?1 AND channel_id=?2 AND state='started'", params![plan,channel,state,receipt,now()]).map_err(user_error)?;
    if changed != 1 {
        return Err("The intake claim changed. Existing queue receipts remain saved; do not repeat the reviewed plan.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn retained_plan(root: &Path, path: &Path) -> (AppState, String) {
        let state = AppState {
            database_path: path.into(),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let source = root.join("prospective-original.mp4");
        fs::write(&source, b"Verified fixture original").unwrap();
        let settings=serde_json::from_value(serde_json::json!({"madeForKids":false,"visibility":"private","deleteSourceAfterUpload":false,"expectedChannelId":"channel-a"})).unwrap();
        let plan = upload_plan::test_retained_plan(
            &state,
            vec![source.to_string_lossy().into_owned()],
            settings,
        );
        (state, plan.id)
    }
    #[test]
    fn intake_claims_survive_reopen_and_reject_replay_or_foreign_channels() {
        let root = std::env::temp_dir().join(format!("youtube-plan-claim-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("queue.sqlite3");
        let mut connection = Connection::open(&path).unwrap();
        persistence::migrate_database(&mut connection).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Channel A','channel-a',?1)",[now()]).unwrap();
        assert!(claim(&mut connection, "missing-unreviewed-plan", "channel-a").is_err());
        let (state, plan) = retained_plan(&root, &path);
        claim(&mut connection, &plan, "channel-a").unwrap();
        assert!(claim(&mut connection, &plan, "channel-a").is_err());
        drop(connection);
        let connection = Connection::open(&path).unwrap();
        assert!(receipt(&connection, &plan, "channel-a").is_err());
        let result = Ok(BatchImportReceipt {
            requested_count: 2,
            imported_count: 2,
            queued_count: 1,
            duplicate_count: 1,
            failed_count: 0,
            items: vec![],
            detail: None,
        });
        finish(&connection, &plan, "channel-a", &result).unwrap();
        let replay = receipt(&connection, &plan, "channel-a").unwrap().unwrap();
        assert_eq!(replay.imported_count, 2);
        assert_eq!(replay.queued_count, 1);
        connection.execute("UPDATE connection_settings SET active_channel='Channel B',active_channel_id='channel-b' WHERE singleton=1",[]).unwrap();
        assert!(receipt(&connection, &plan, "channel-b").is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_plan_executions", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        upload_plan::test_discard_plan(&state, &plan).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn partial_intake_failure_retains_claim_without_a_second_job_creation() {
        let root = std::env::temp_dir().join(format!("youtube-partial-plan-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("queue.sqlite3");
        let mut connection = Connection::open(&path).unwrap();
        persistence::migrate_database(&mut connection).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Channel A','channel-a',?1)",[now()]).unwrap();
        let (state, plan) = retained_plan(&root, &path);
        claim(&mut connection, &plan, "channel-a").unwrap();
        finish(
            &connection,
            &plan,
            "channel-a",
            &Err("Controlled failure".into()),
        )
        .unwrap();
        assert!(receipt(&connection, &plan, "channel-a").is_err());
        assert!(claim(&mut connection, &plan, "channel-a").is_err());
        assert_eq!(
            connection
                .query_row(
                    "SELECT state FROM upload_plan_executions WHERE plan_id=?1",
                    [&plan],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "needs_attention"
        );
        drop(connection);
        upload_plan::test_discard_plan(&state, &plan).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
