//! Native-owned metadata validation, simple templates and local manifest mapping.
use super::*;
use serde::de::{MapAccess, SeqAccess, Visitor};

const MAX_FILES: usize = 512;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_PRESETS: i64 = 100;
const FIELDS: &[&str] = &[
    "title",
    "description",
    "tags",
    "categoryId",
    "language",
    "defaultAudioLanguage",
    "license",
    "notifySubscribers",
    "containsSyntheticMedia",
    "publishAt",
];

pub(super) const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS upload_metadata_presets (
 id TEXT PRIMARY KEY, channel_id TEXT NOT NULL, name TEXT NOT NULL,
 metadata_json TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS upload_metadata_presets_channel_idx ON upload_metadata_presets(channel_id, updated_at, id);";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UploadMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) category_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) default_audio_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) notify_subscribers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) contains_synthetic_media: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) publish_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UploadMetadataPreset {
    id: String,
    channel_id: String,
    name: String,
    metadata: UploadMetadata,
    updated_at: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(super) struct UploadMetadataMatch {
    index: usize,
    file_name: String,
    metadata: UploadMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    template_metadata: Option<UploadMetadata>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct UploadMetadataMapping {
    matches: Vec<UploadMetadataMatch>,
    issues: Vec<String>,
}

pub(super) fn require_channel(connection: &Connection, expected: &str) -> Result<(), String> {
    if expected.is_empty()
        || connection_settings(connection)?
            .active_channel_id
            .as_deref()
            != Some(expected)
    {
        return Err("The connected channel changed. Reopen the metadata editor.".into());
    }
    Ok(())
}

fn audit_preset(
    connection: &Connection,
    channel: &str,
    kind: &str,
    detail: &str,
) -> Result<(), String> {
    connection.execute("INSERT INTO audit_events(id,item_id,channel_id,channel_name,kind,detail,created_at) VALUES(?1,NULL,?2,(SELECT active_channel FROM connection_settings WHERE singleton=1 AND active_channel_id=?2),?3,?4,?5)",params![Uuid::new_v4().to_string(),channel,kind,detail,now()]).map_err(user_error)?;
    Ok(())
}

fn safe_text(value: &str, multiline: bool) -> bool {
    !value.chars().any(|c| {
        c == '<' || c == '>' || (c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))
    })
}

fn language_valid(value: &str) -> bool {
    let mut parts = value.split('-');
    let Some(first) = parts.next() else {
        return false;
    };
    value.len() <= 63
        && (2..=8).contains(&first.len())
        && first.bytes().all(|c| c.is_ascii_alphabetic())
        && parts.all(|part| {
            !part.is_empty() && part.len() <= 8 && part.bytes().all(|c| c.is_ascii_alphanumeric())
        })
}

/// Unicode scalar counts for titles/tags; UTF-8 bytes for descriptions, per API.
pub(super) fn validate(metadata: &UploadMetadata) -> Result<(), String> {
    if let Some(title) = &metadata.title {
        if title.trim().is_empty() || title.chars().count() > 100 || !safe_text(title, false) {
            return Err("Use a nonempty title of at most 100 characters, without angle brackets or control characters.".into());
        }
    }
    if let Some(description) = &metadata.description {
        if description.len() > 5000 || !safe_text(description, true) {
            return Err("Use a description of at most 5,000 UTF-8 bytes, without angle brackets or unsupported control characters.".into());
        }
    }
    if let Some(tags) = &metadata.tags {
        let cost = tags
            .iter()
            .map(|tag| tag.chars().count() + if tag.contains(' ') { 2 } else { 0 })
            .sum::<usize>()
            + tags.len().saturating_sub(1);
        if tags.len() > 100
            || tags
                .iter()
                .any(|tag| tag.trim().is_empty() || !safe_text(tag, false))
            || cost > 500
        {
            return Err("Tags must be nonempty and use at most 500 characters in total, including separators and quotes around tags with spaces.".into());
        }
    }
    if metadata.category_id.as_ref().is_some_and(|id| {
        id.is_empty() || id.len() > 6 || id == "0" || !id.bytes().all(|c| c.is_ascii_digit())
    }) {
        return Err("Choose a numeric YouTube category ID.".into());
    }
    if [&metadata.language, &metadata.default_audio_language]
        .into_iter()
        .any(|language| {
            language
                .as_ref()
                .is_some_and(|value| !language_valid(value))
        })
    {
        return Err("Use a language tag such as en, fil or en-US.".into());
    }
    if metadata
        .license
        .as_deref()
        .is_some_and(|value| !matches!(value, "youtube" | "creativeCommon"))
    {
        return Err("Choose the standard YouTube or Creative Commons license explicitly.".into());
    }
    if let Some(publish_at) = &metadata.publish_at {
        let time = DateTime::parse_from_rfc3339(publish_at)
            .map_err(|_| "The publication time must include an RFC 3339 timezone.".to_string())?;
        if time.with_timezone(&Utc) <= Utc::now() {
            return Err(
                "The publication time has passed. Review a future time before upload.".into(),
            );
        }
    }
    Ok(())
}

fn expand(value: &str, source: &Path, sequence: usize) -> Result<String, String> {
    let filename = source
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("The selected video filename is not valid Unicode.")?;
    let folder = source
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                result.push('{');
                continue;
            }
            let mut token = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(c) if c != '{' && token.len() < 32 => token.push(c),
                    _ => return Err("Templates support only {filename}, {folder} and {sequence}. Use doubled braces for literal braces.".into()),
                }
            }
            match token.as_str() {
                "filename" => result.push_str(filename),
                "folder" => result.push_str(folder),
                "sequence" => result.push_str(&sequence.to_string()),
                _ => {
                    return Err(
                        "Templates support only {filename}, {folder} and {sequence}.".into(),
                    )
                }
            }
        } else if character == '}' {
            if chars.next() != Some('}') {
                return Err("Use doubled braces for a literal closing brace.".into());
            }
            result.push('}');
        } else {
            result.push(character);
        }
        if result.len() > 20000 {
            return Err("The resolved template is too large.".into());
        }
    }
    Ok(result)
}

pub(super) fn resolve(
    metadata: &UploadMetadata,
    source: &Path,
    sequence: usize,
) -> Result<UploadMetadata, String> {
    if sequence == 0 {
        return Err("File sequence starts at one.".into());
    }
    let mut resolved = metadata.clone();
    resolved.title = metadata
        .title
        .as_ref()
        .map(|value| expand(value, source, sequence))
        .transpose()?;
    resolved.description = metadata
        .description
        .as_ref()
        .map(|value| expand(value, source, sequence))
        .transpose()?;
    resolved.tags = metadata
        .tags
        .as_ref()
        .map(|tags| {
            tags.iter()
                .map(|value| expand(value, source, sequence))
                .collect()
        })
        .transpose()?;
    validate(&resolved)?;
    Ok(resolved)
}

pub(super) fn resolve_for_source(
    settings: &ManualUploadSettings,
    source: &Path,
    sequence: usize,
) -> Result<UploadMetadata, String> {
    let key = source.to_string_lossy();
    // An item override is the complete declaration, including intentionally absent fields.
    let metadata = settings
        .item_metadata
        .as_ref()
        .and_then(|items| items.get(key.as_ref()))
        .or(settings.metadata.as_ref())
        .cloned()
        .unwrap_or_default();
    let resolved = resolve(&metadata, source, sequence)?;
    if resolved.publish_at.is_some() && settings.visibility != "private" {
        return Err("Scheduled publication requires private upload visibility.".into());
    }
    Ok(resolved)
}

pub(super) fn payload(
    title: &str,
    visibility: &str,
    made_for_kids: bool,
    metadata: &UploadMetadata,
) -> Result<serde_json::Value, String> {
    validate(metadata)?;
    if !matches!(visibility, "private" | "unlisted" | "public") {
        return Err("Choose a valid upload visibility.".into());
    }
    if metadata.publish_at.is_some() && visibility != "private" {
        return Err("Scheduled publication requires private upload visibility.".into());
    }
    let title = metadata.title.as_deref().unwrap_or(title);
    validate(&UploadMetadata {
        title: Some(title.to_string()),
        ..Default::default()
    })?;
    let mut snippet = serde_json::json!({"title": title});
    let mut status =
        serde_json::json!({"privacyStatus": visibility, "selfDeclaredMadeForKids": made_for_kids});
    for (key, value) in [
        ("description", &metadata.description),
        ("categoryId", &metadata.category_id),
        ("defaultLanguage", &metadata.language),
        ("defaultAudioLanguage", &metadata.default_audio_language),
    ] {
        if let Some(value) = value {
            snippet[key] = value.clone().into();
        }
    }
    if let Some(tags) = &metadata.tags {
        snippet["tags"] = serde_json::json!(tags);
    }
    if let Some(license) = &metadata.license {
        status["license"] = license.clone().into();
    }
    if let Some(value) = metadata.contains_synthetic_media {
        status["containsSyntheticMedia"] = value.into();
    }
    if let Some(value) = &metadata.publish_at {
        status["publishAt"] = value.clone().into();
    }
    Ok(serde_json::json!({"snippet": snippet, "status": status}))
}

/// A videos.update part replaces omitted writable properties. Always merge an
/// owner-authorized fresh read, retaining only writable fields for included parts.
pub(super) fn merge_update_payload(
    current: &serde_json::Value,
    patch: &UploadMetadata,
) -> Result<serde_json::Value, String> {
    validate(patch)?;
    if patch.notify_subscribers.is_some() {
        return Err("Subscriber notifications can be chosen only for a new upload.".into());
    }
    let id = current
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or("Read the owned video before updating its metadata.")?;
    let mut result = serde_json::json!({"id": id});
    if patch.title.is_some()
        || patch.description.is_some()
        || patch.tags.is_some()
        || patch.category_id.is_some()
        || patch.language.is_some()
        || patch.default_audio_language.is_some()
    {
        let existing = current
            .get("snippet")
            .and_then(serde_json::Value::as_object)
            .ok_or("Read the complete video snippet before updating it.")?;
        let mut snippet = serde_json::Map::new();
        for key in [
            "title",
            "description",
            "tags",
            "categoryId",
            "defaultLanguage",
            "defaultAudioLanguage",
        ] {
            if let Some(value) = existing.get(key) {
                snippet.insert(key.into(), value.clone());
            }
        }
        for (key, value) in [
            ("title", &patch.title),
            ("description", &patch.description),
            ("categoryId", &patch.category_id),
            ("defaultLanguage", &patch.language),
            ("defaultAudioLanguage", &patch.default_audio_language),
        ] {
            if let Some(value) = value {
                snippet.insert(key.into(), value.clone().into());
            }
        }
        if let Some(tags) = &patch.tags {
            snippet.insert("tags".into(), serde_json::json!(tags));
        }
        if !snippet
            .get("title")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|title| !title.trim().is_empty())
            || !snippet
                .get("categoryId")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| !id.is_empty())
        {
            return Err("A snippet update requires the current title and category; refresh the owned video first.".into());
        }
        result["snippet"] = snippet.into();
    }
    if patch.license.is_some()
        || patch.contains_synthetic_media.is_some()
        || patch.publish_at.is_some()
    {
        let existing = current
            .get("status")
            .and_then(serde_json::Value::as_object)
            .ok_or("Read the complete video status before updating it.")?;
        let mut status = serde_json::Map::new();
        for key in [
            "privacyStatus",
            "license",
            "embeddable",
            "publicStatsViewable",
            "selfDeclaredMadeForKids",
            "containsSyntheticMedia",
            "publishAt",
        ] {
            if let Some(value) = existing.get(key) {
                status.insert(key.into(), value.clone());
            }
        }
        if !status.contains_key("privacyStatus") {
            return Err("Refresh video privacy before updating its status.".into());
        }
        if let Some(value) = &patch.license {
            status.insert("license".into(), value.clone().into());
        }
        if let Some(value) = patch.contains_synthetic_media {
            status.insert("containsSyntheticMedia".into(), value.into());
        }
        if let Some(value) = &patch.publish_at {
            if status
                .get("privacyStatus")
                .and_then(serde_json::Value::as_str)
                != Some("private")
            {
                return Err(
                    "Only a private, never-published video may receive a publication schedule."
                        .into(),
                );
            }
            status.insert("privacyStatus".into(), "private".into());
            status.insert("publishAt".into(), value.clone().into());
        }
        result["status"] = status.into();
    }
    Ok(result)
}

fn validate_preset(metadata: &UploadMetadata) -> Result<(), String> {
    // Validating a representative expansion catches malformed/unsupported tokens.
    // Real filenames and sequences are revalidated per video before approval.
    resolve(metadata, Path::new("x/a.mp4"), 1)?;
    Ok(())
}

fn list_presets(
    connection: &Connection,
    channel: &str,
) -> Result<Vec<UploadMetadataPreset>, String> {
    require_channel(connection, channel)?;
    let rows = connection.prepare("SELECT id, name, metadata_json, updated_at FROM upload_metadata_presets WHERE channel_id = ?1 ORDER BY name COLLATE NOCASE, id LIMIT 100").map_err(user_error)?.query_map([channel], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))).map_err(user_error)?.collect::<Result<Vec<_>, _>>().map_err(user_error)?;
    rows.into_iter()
        .map(|(id, name, json, updated_at)| {
            Ok(UploadMetadataPreset {
                id,
                channel_id: channel.to_string(),
                name,
                metadata: serde_json::from_str(&json).map_err(|_| {
                    "A saved preset is invalid. Remove it and create a reviewed replacement."
                        .to_string()
                })?,
                updated_at,
            })
        })
        .collect()
}

fn save_preset(
    connection: &mut Connection,
    channel: &str,
    id: Option<String>,
    name: String,
    metadata: UploadMetadata,
) -> Result<UploadMetadataPreset, String> {
    validate_preset(&metadata)?;
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("Use a preset name of 1–80 characters.".into());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_channel(&transaction, channel)?;
    let id = if let Some(id) = id {
        if !transaction.query_row("SELECT EXISTS(SELECT 1 FROM upload_metadata_presets WHERE id=?1 AND channel_id=?2)", params![id,channel], |row| row.get::<_, bool>(0)).map_err(user_error)? { return Err("This preset is unavailable for the connected channel.".into()); }
        id
    } else {
        if transaction
            .query_row(
                "SELECT COUNT(*) FROM upload_metadata_presets WHERE channel_id=?1",
                [channel],
                |row| row.get::<_, i64>(0),
            )
            .map_err(user_error)?
            >= MAX_PRESETS
        {
            return Err(
                "This channel already has 100 presets. Remove one before creating another.".into(),
            );
        }
        Uuid::new_v4().to_string()
    };
    let updated_at = now();
    transaction.execute("INSERT INTO upload_metadata_presets(id,channel_id,name,metadata_json,updated_at) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,metadata_json=excluded.metadata_json,updated_at=excluded.updated_at WHERE upload_metadata_presets.channel_id=excluded.channel_id", params![id,channel,name,serde_json::to_string(&metadata).map_err(user_error)?,updated_at]).map_err(user_error)?;
    audit_preset(
        &transaction,
        channel,
        "upload_metadata_preset_saved",
        "Operator saved a reviewed metadata preset; existing queue choices were retained",
    )?;
    transaction.commit().map_err(user_error)?;
    Ok(UploadMetadataPreset {
        id,
        channel_id: channel.into(),
        name: name.into(),
        metadata,
        updated_at,
    })
}

#[tauri::command]
pub(super) fn load_upload_presets(
    state: State<'_, AppState>,
    expected_channel_id: String,
) -> Result<Vec<UploadMetadataPreset>, String> {
    list_presets(&database(&state)?, &expected_channel_id)
}

#[tauri::command]
pub(super) fn save_upload_preset(
    state: State<'_, AppState>,
    expected_channel_id: String,
    name: String,
    metadata: UploadMetadata,
    id: Option<String>,
) -> Result<UploadMetadataPreset, String> {
    save_preset(
        &mut database(&state)?,
        &expected_channel_id,
        id,
        name,
        metadata,
    )
}

#[tauri::command]
pub(super) fn delete_upload_preset(
    state: State<'_, AppState>,
    expected_channel_id: String,
    id: String,
) -> Result<(), String> {
    let mut connection = database(&state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    require_channel(&transaction, &expected_channel_id)?;
    if transaction
        .execute(
            "DELETE FROM upload_metadata_presets WHERE id=?1 AND channel_id=?2",
            params![id, expected_channel_id],
        )
        .map_err(user_error)?
        != 1
    {
        return Err("This preset is unavailable for the connected channel.".into());
    }
    audit_preset(
        &transaction,
        &expected_channel_id,
        "upload_metadata_preset_removed",
        "Operator removed a metadata preset; existing queue choices were retained",
    )?;
    transaction.commit().map_err(user_error)?;
    Ok(())
}

fn validate_paths(paths: &[String]) -> Result<(), String> {
    if paths.is_empty()
        || paths.len() > MAX_FILES
        || paths.iter().any(|path| {
            path.is_empty() || path.len() > 32768 || path.contains('\0') || path.contains("://")
        })
        || paths.iter().collect::<HashSet<_>>().len() != paths.len()
    {
        return Err("Choose 1–512 distinct local original videos for metadata review.".into());
    }
    Ok(())
}

fn resolve_mapping(
    paths: &[String],
    metadata: &UploadMetadata,
    overrides: Option<&HashMap<String, UploadMetadata>>,
) -> Result<UploadMetadataMapping, String> {
    validate_paths(paths)?;
    if overrides.is_some_and(|items| {
        items.len() > MAX_FILES || items.keys().any(|path| !paths.contains(path))
    }) {
        return Err("Item metadata includes a file outside the selected originals.".into());
    }
    let mut result = UploadMetadataMapping::default();
    for (index, path) in paths.iter().enumerate() {
        let chosen = overrides
            .and_then(|items| items.get(path))
            .unwrap_or(metadata);
        match resolve(chosen, Path::new(path), index + 1) {
            Ok(metadata) => result.matches.push(UploadMetadataMatch {
                index,
                file_name: Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Selected video")
                    .into(),
                metadata,
                template_metadata: None,
            }),
            Err(error) => result.issues.push(format!("Video {}: {error}", index + 1)),
        }
    }
    Ok(result)
}

#[tauri::command]
pub(super) async fn resolve_upload_metadata(
    state: State<'_, AppState>,
    expected_channel_id: String,
    paths: Vec<String>,
    metadata: UploadMetadata,
    item_metadata: Option<HashMap<String, UploadMetadata>>,
) -> Result<UploadMetadataMapping, String> {
    require_channel(&database(&state)?, &expected_channel_id)?;
    let result = resolve_mapping(&paths, &metadata, item_metadata.as_ref())?;
    require_channel(&database(&state)?, &expected_channel_id)?;
    Ok(result)
}

/// serde_json::Value normally accepts duplicate keys. A manifest must not.
struct StrictValue(serde_json::Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(value.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|n| StrictValue(n.into()))
                    .ok_or_else(|| E::custom("Invalid JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(value.into()))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictValue(value.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(serde_json::Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(StrictValue(value)) = sequence.next_element()? {
                    if values.len() > MAX_FILES + 100 {
                        return Err(serde::de::Error::custom(
                            "JSON array exceeds the manifest bound",
                        ));
                    }
                    values.push(value);
                }
                Ok(StrictValue(values.into()))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.len() >= 64 {
                        return Err(serde::de::Error::custom("JSON object exceeds 64 fields"));
                    }
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("Duplicate JSON object key"));
                    }
                    let StrictValue(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(StrictValue(values.into()))
            }
        }
        deserializer.deserialize_any(StrictVisitor)
    }
}

fn csv_rows(input: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    let mut closed_quote = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    closed_quote = true;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' if field.is_empty() && !closed_quote => quoted = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                    closed_quote = false;
                }
                '\n' | '\r' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    if row.iter().any(|value| !value.is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                    closed_quote = false;
                }
                _ if closed_quote => {
                    return Err("CSV contains characters after a closing quote.".into())
                }
                '"' => return Err("CSV quotes must wrap the entire field.".into()),
                _ => field.push(c),
            }
        }
        if rows.len() > MAX_FILES {
            return Err("A manifest supports at most 512 video rows.".into());
        }
    }
    if quoted {
        return Err("CSV contains an unclosed quoted field.".into());
    }
    if !field.is_empty() || !row.is_empty() || closed_quote {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

fn manifest_rows(input: &str, extension: &str) -> Result<Vec<serde_json::Value>, String> {
    if extension.eq_ignore_ascii_case("json") {
        let StrictValue(value) = serde_json::from_str(input).map_err(|_| {
            "The JSON manifest is malformed or contains duplicate fields.".to_string()
        })?;
        let object = value
            .as_object()
            .ok_or("Use a JSON object with schemaVersion: 1 and a videos array.")?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "schemaVersion" | "videos"))
            || object
                .get("schemaVersion")
                .and_then(serde_json::Value::as_u64)
                != Some(1)
        {
            return Err(
                "The JSON manifest schema is unsupported. Use only schemaVersion: 1 and videos."
                    .into(),
            );
        }
        let rows = object
            .get("videos")
            .and_then(serde_json::Value::as_array)
            .ok_or("The JSON manifest requires a videos array.")?;
        if rows.len() > MAX_FILES {
            return Err("A manifest supports at most 512 video rows.".into());
        }
        Ok(rows.clone())
    } else if extension.eq_ignore_ascii_case("csv") {
        let mut rows = csv_rows(input)?.into_iter();
        let header = rows.next().ok_or("The CSV manifest is empty.")?;
        if header.len() > FIELDS.len() + 1
            || header.iter().collect::<HashSet<_>>().len() != header.len()
            || !header.iter().any(|field| field == "file")
        {
            return Err("CSV requires distinct headers including file; at most 11 metadata fields are supported.".into());
        }
        rows.map(|row| {
            if row.len() != header.len() {
                return Err("A CSV row has a different number of fields than its header.".into());
            }
            let mut object = serde_json::Map::new();
            for (key, value) in header.iter().zip(row) {
                if value.is_empty() && key != "description" && key != "file" {
                    continue;
                }
                let converted =
                    if matches!(key.as_str(), "notifySubscribers" | "containsSyntheticMedia") {
                        match value.as_str() {
                            "true" => true.into(),
                            "false" => false.into(),
                            _ => value.into(),
                        }
                    } else if key == "tags" {
                        if value.trim_start().starts_with('[') {
                            serde_json::from_str::<StrictValue>(&value)
                                .map(|StrictValue(v)| v)
                                .map_err(|_| "CSV tags JSON is malformed.".to_string())?
                        } else {
                            serde_json::json!(value
                                .split(';')
                                .map(str::trim)
                                .filter(|tag| !tag.is_empty())
                                .collect::<Vec<_>>())
                        }
                    } else {
                        value.into()
                    };
                object.insert(key.clone(), converted);
            }
            Ok(object.into())
        })
        .collect()
    } else {
        Err("Choose a local CSV or JSON manifest.".into())
    }
}

fn path_key(path: &Path) -> String {
    let key = path.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    {
        key.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        key
    }
}

fn relative_reference(value: &str) -> Result<PathBuf, String> {
    if value.is_empty()
        || value.len() > 32768
        || value.contains('\0')
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.contains(':')
    {
        return Err(
            "File references must be relative local paths within the selected root.".into(),
        );
    }
    let normalized = value.replace('\\', "/");
    if normalized
        .split('/')
        .any(|part| part == ".." || part.is_empty())
    {
        return Err(
            "File references cannot escape the selected root or contain empty path segments."
                .into(),
        );
    }
    Ok(PathBuf::from(normalized))
}

fn map_rows(
    rows: Vec<serde_json::Value>,
    root: &Path,
    paths: &[String],
) -> Result<UploadMetadataMapping, String> {
    validate_paths(paths)?;
    if rows.len() > MAX_FILES {
        return Err("A manifest supports at most 512 video rows.".into());
    }
    let root = fs::canonicalize(root)
        .map_err(|_| "The selected matching root is unavailable.".to_string())?;
    if !root.is_dir() {
        return Err("Choose a folder as the matching root.".into());
    }
    let mut selected = HashMap::new();
    for (index, path) in paths.iter().enumerate() {
        let canonical = fs::canonicalize(path).map_err(|_| {
            format!(
                "Selected video {} is unavailable. Restore its original before mapping.",
                index + 1
            )
        })?;
        if !canonical.starts_with(&root) {
            return Err(format!(
                "Selected video {} is outside the explicitly selected matching root.",
                index + 1
            ));
        }
        if selected.insert(path_key(&canonical), index).is_some() {
            return Err(
                "Two selected source locators refer to the same original. Select it once.".into(),
            );
        }
    }
    let mut result = UploadMetadataMapping::default();
    let mut matched = HashSet::new();
    let mut duplicates = HashSet::new();
    for (row_index, row) in rows.into_iter().enumerate() {
        let prefix = format!("Row {}", row_index + 1);
        let Some(mut object) = row.as_object().cloned() else {
            result
                .issues
                .push(format!("{prefix}: use a metadata object."));
            continue;
        };
        let unknown = object
            .keys()
            .filter(|key| key.as_str() != "file" && !FIELDS.contains(&key.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !unknown.is_empty() {
            result.issues.push(format!(
                "{prefix}: unknown fields {}; this row was not applied.",
                unknown.join(", ")
            ));
            continue;
        }
        let Some(reference) = object
            .remove("file")
            .and_then(|value| value.as_str().map(str::to_string))
        else {
            result
                .issues
                .push(format!("{prefix}: a relative file string is required."));
            continue;
        };
        let relative = match relative_reference(&reference) {
            Ok(path) => path,
            Err(error) => {
                result.issues.push(format!("{prefix}: {error}"));
                continue;
            }
        };
        let canonical = match fs::canonicalize(root.join(relative)) {
            Ok(path) if path.starts_with(&root) => path,
            _ => {
                result.issues.push(format!(
                    "{prefix}: the reference is unavailable or escapes the matching root."
                ));
                continue;
            }
        };
        let Some(&index) = selected.get(&path_key(&canonical)) else {
            result.issues.push(format!(
                "{prefix}: the file is not among the selected originals."
            ));
            continue;
        };
        if !matched.insert(index) {
            duplicates.insert(index);
            result.issues.push(format!("{prefix}: another row refers to the same selected video; all its manifest rows were withheld."));
            continue;
        }
        let metadata: UploadMetadata = match serde_json::from_value(object.into()) {
            Ok(value) => value,
            Err(_) => {
                result.issues.push(format!(
                    "{prefix}: metadata values do not match the supported schema."
                ));
                continue;
            }
        };
        match resolve(&metadata, Path::new(&paths[index]), index + 1) {
            Ok(resolved) => result.matches.push(UploadMetadataMatch {
                index,
                file_name: Path::new(&paths[index])
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Selected video")
                    .into(),
                metadata: resolved,
                template_metadata: Some(metadata),
            }),
            Err(error) => result.issues.push(format!("{prefix}: {error}")),
        }
    }
    result
        .matches
        .retain(|entry| !duplicates.contains(&entry.index));
    result.matches.sort_by_key(|entry| entry.index);
    for index in 0..paths.len() {
        if !result.matches.iter().any(|entry| entry.index == index) {
            result.issues.push(format!(
                "Video {} has no usable manifest row; its current metadata remains unchanged.",
                index + 1
            ));
        }
    }
    Ok(result)
}

fn map_manifest(
    path: &Path,
    root: &Path,
    selected_paths: &[String],
) -> Result<UploadMetadataMapping, String> {
    let mut file = open_reference_source(path)?;
    let before = monitored_file_signature(&file.metadata().map_err(user_error)?)?;
    if before.0 > MAX_MANIFEST_BYTES {
        return Err("Choose a manifest no larger than 1 MiB.".into());
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(user_error)?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES
        || monitored_file_signature(&file.metadata().map_err(user_error)?)? != before
    {
        return Err("The manifest changed while being read. Choose it again.".into());
    }
    let input = std::str::from_utf8(&bytes)
        .map_err(|_| "Save the manifest as UTF-8 text.".to_string())?
        .trim_start_matches('\u{feff}');
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    map_rows(manifest_rows(input, extension)?, root, selected_paths)
}

#[tauri::command]
pub(super) async fn map_upload_manifest(
    state: State<'_, AppState>,
    expected_channel_id: String,
    path: String,
    selected_paths: Vec<String>,
    root_path: Option<String>,
) -> Result<UploadMetadataMapping, String> {
    require_channel(&database(&state)?, &expected_channel_id)?;
    let root = root_path
        .filter(|root| !root.is_empty())
        .ok_or("Explicitly select the matching root before importing a manifest.")?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        require_channel(&database(&state)?, &expected_channel_id)?;
        let result = map_manifest(Path::new(&path), Path::new(&root), &selected_paths)?;
        require_channel(&database(&state)?, &expected_channel_id)?;
        Ok(result)
    })
    .await
    .map_err(|_| "Manifest mapping stopped. The current metadata was retained.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-metadata-db-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,active_channel,active_channel_id,updated_at) VALUES(1,'Fixture channel','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET active_channel='Fixture channel',active_channel_id='channel-a'", [now()]).unwrap();
        (root, state, connection)
    }

    #[test]
    fn presets_are_channel_scoped_persistent_and_never_rewrite_queue_choices() {
        let (root, state, mut connection) = fixture();
        connection.execute("INSERT INTO upload_items(id,title,normalized_title,file_name,workspace_path,size_bytes,total_bytes,status,visibility,made_for_kids,channel_id,upload_metadata_json,created_at,updated_at) VALUES('queued','Approved','approved','clip.mp4','original',1,1,'queued','private',0,'channel-a',?1,?2,?2)", params![r#"{"description":"Approved description"}"#,now()]).unwrap();
        let first = save_preset(
            &mut connection,
            "channel-a",
            None,
            "Sessions".into(),
            UploadMetadata {
                title: Some("{filename}".into()),
                notify_subscribers: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        let id = first.id.clone();
        save_preset(
            &mut connection,
            "channel-a",
            Some(id.clone()),
            "Updated".into(),
            UploadMetadata {
                description: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            list_presets(&connection, "channel-a").unwrap()[0].name,
            "Updated"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT upload_metadata_json FROM upload_items WHERE id='queued'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            r#"{"description":"Approved description"}"#
        );
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        assert!(list_presets(&connection, "channel-a").is_err());
        assert!(list_presets(&connection, "channel-b").unwrap().is_empty());
        assert!(save_preset(
            &mut connection,
            "channel-b",
            Some(id.clone()),
            "Foreign".into(),
            UploadMetadata::default()
        )
        .is_err());
        drop(connection);
        let restarted = database(&state).unwrap();
        restarted
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-a'",
                [],
            )
            .unwrap();
        let saved = list_presets(&restarted, "channel-a").unwrap();
        assert_eq!(saved[0].id, id);
        assert_eq!(saved[0].metadata.description.as_deref(), Some(""));
        assert_eq!(restarted.query_row("SELECT COUNT(*) FROM audit_events WHERE kind='upload_metadata_preset_saved' AND channel_id='channel-a'",[],|row|row.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(restarted.query_row("SELECT COUNT(*) FROM audit_events WHERE kind='upload_metadata_preset_saved' AND channel_id='channel-b'",[],|row|row.get::<_,i64>(0)).unwrap(),0);
        drop(restarted);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_overrides_replace_common_metadata_without_a_second_token_expansion() {
        let common = UploadMetadata {
            title: Some("Common".into()),
            description: Some("Common description".into()),
            ..Default::default()
        };
        let source = Path::new("Sessions/{folder}.mp4");
        let settings = ManualUploadSettings {
            made_for_kids: false,
            visibility: "private".into(),
            playlist_id: None,
            playlist_title: None,
            delete_source_after_upload: false,
            metadata: Some(common),
            item_metadata: Some(HashMap::from([(
                source.to_string_lossy().to_string(),
                UploadMetadata {
                    title: Some("{filename}".into()),
                    ..Default::default()
                },
            )])),
            expected_channel_id: Some("channel-a".into()),
            publication_contexts: None,
        };
        let resolved = resolve_for_source(&settings, source, 2).unwrap();
        assert_eq!(resolved.title.as_deref(), Some("{folder}"));
        assert_eq!(resolved.description, None);
        assert_eq!(
            payload("fallback", "private", false, &resolved).unwrap()["snippet"]["title"],
            "{folder}"
        );
    }

    #[test]
    fn manifests_bound_bytes_and_rows_without_changing_originals() {
        let root = std::env::temp_dir().join(format!("youtube-metadata-bounds-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("clip.mp4");
        fs::write(&source, b"original bytes").unwrap();
        let paths = vec![source.to_string_lossy().to_string()];
        let manifest = root.join("batch.json");
        fs::write(
            &manifest,
            r#"{"schemaVersion":1,"videos":[{"file":"clip.mp4","title":"{filename}"}]}"#,
        )
        .unwrap();
        let result = map_manifest(&manifest, &root, &paths).unwrap();
        assert_eq!(result.matches[0].metadata.title.as_deref(), Some("clip"));
        assert_eq!(
            result.matches[0]
                .template_metadata
                .as_ref()
                .unwrap()
                .title
                .as_deref(),
            Some("{filename}")
        );
        assert_eq!(fs::read(&source).unwrap(), b"original bytes");
        fs::write(&manifest, vec![b' '; MAX_MANIFEST_BYTES as usize + 1]).unwrap();
        assert!(map_manifest(&manifest, &root, &paths)
            .unwrap_err()
            .contains("1 MiB"));
        let rows = (0..513)
            .map(|_| serde_json::json!({"file":"clip.mp4"}))
            .collect();
        assert!(map_rows(rows, &root, &paths).is_err());
        assert!(resolve_mapping(
            &vec!["clip.mp4".into(); 513],
            &UploadMetadata::default(),
            None
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unicode_limits_use_characters_for_title_and_utf8_bytes_for_description() {
        assert!(validate(&UploadMetadata {
            title: Some("🦊".repeat(100)),
            description: Some("é".repeat(2500)),
            ..Default::default()
        })
        .is_ok());
        assert!(validate(&UploadMetadata {
            title: Some("🦊".repeat(101)),
            ..Default::default()
        })
        .is_err());
        assert!(validate(&UploadMetadata {
            description: Some("é".repeat(2501)),
            ..Default::default()
        })
        .is_err());
        for invalid in ["bad<", "bad>", "bad\0"] {
            assert!(validate(&UploadMetadata {
                title: Some(invalid.into()),
                ..Default::default()
            })
            .is_err());
        }
        assert!(validate(&UploadMetadata {
            description: Some("first\nsecond\tline".into()),
            ..Default::default()
        })
        .is_ok());
    }

    #[test]
    fn preset_tokens_do_not_assume_long_sample_names_and_json_objects_are_bounded() {
        assert!(validate_preset(&UploadMetadata {
            title: Some(format!("{}{{filename}}", "x".repeat(99))),
            ..Default::default()
        })
        .is_ok());
        assert!(resolve(
            &UploadMetadata {
                title: Some(format!("{}{{filename}}", "x".repeat(99))),
                ..Default::default()
            },
            Path::new("actual-long-name.mp4"),
            1
        )
        .is_err());
        let oversized = (0..65)
            .map(|index| format!("\"key{index}\":0"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(serde_json::from_str::<StrictValue>(&format!("{{{oversized}}}")).is_err());
    }

    #[test]
    fn tags_count_separators_space_quotes_and_unicode() {
        assert!(validate(&UploadMetadata {
            tags: Some(vec!["a".repeat(498)]),
            ..Default::default()
        })
        .is_ok());
        assert!(validate(&UploadMetadata {
            tags: Some(vec![format!("{} b", "a".repeat(497))]),
            ..Default::default()
        })
        .is_err());
        assert!(validate(&UploadMetadata {
            tags: Some(vec!["🦊".repeat(250), "b".repeat(249)]),
            ..Default::default()
        })
        .is_ok());
        assert!(validate(&UploadMetadata {
            tags: Some(vec!["".into()]),
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn simple_tokens_expand_without_expressions_and_preserve_literal_braces() {
        let resolved = resolve(
            &UploadMetadata {
                title: Some("{sequence}: {filename}".into()),
                description: Some("From {folder} {{literal}}".into()),
                ..Default::default()
            },
            Path::new("Sessions/演奏.mp4"),
            2,
        )
        .unwrap();
        assert_eq!(resolved.title.as_deref(), Some("2: 演奏"));
        assert_eq!(
            resolved.description.as_deref(),
            Some("From Sessions {literal}")
        );
        for invalid in [
            "{filename.toUpperCase()}",
            "${shell}",
            "{unknown}",
            "{filename",
            "bad}",
        ] {
            assert!(resolve(
                &UploadMetadata {
                    title: Some(invalid.into()),
                    ..Default::default()
                },
                Path::new("clip.mp4"),
                1
            )
            .is_err());
        }
        assert!(resolve(&UploadMetadata::default(), Path::new("clip.mp4"), 0).is_err());
    }

    #[test]
    fn optional_declarations_and_schema_types_are_strict() {
        for json in [
            r#"{"unknown":1}"#,
            r#"{"notifySubscribers":"false"}"#,
            r#"{"tags":"tag"}"#,
        ] {
            assert!(serde_json::from_str::<UploadMetadata>(json).is_err());
        }
        for metadata in [
            UploadMetadata {
                license: Some("anything".into()),
                ..Default::default()
            },
            UploadMetadata {
                language: Some("en_US".into()),
                ..Default::default()
            },
            UploadMetadata {
                category_id: Some("x".into()),
                ..Default::default()
            },
            UploadMetadata {
                publish_at: Some("2000-01-01T00:00:00Z".into()),
                ..Default::default()
            },
        ] {
            assert!(validate(&metadata).is_err());
        }
        let payload = payload(
            "Filename",
            "private",
            false,
            &UploadMetadata {
                description: Some(String::new()),
                notify_subscribers: Some(false),
                contains_synthetic_media: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(payload["snippet"]["description"], "");
        assert_eq!(payload["status"]["containsSyntheticMedia"], false);
        assert!(payload.get("notifySubscribers").is_none());
        assert!(payload["status"].get("license").is_none());
    }

    #[test]
    fn mapping_keeps_full_item_override_and_rejects_unselected_keys() {
        let paths: Vec<String> = vec!["Sessions/one.mp4".into(), "Sessions/two.mp4".into()];
        let common = UploadMetadata {
            title: Some("Common {sequence}".into()),
            description: Some("Common description".into()),
            ..Default::default()
        };
        let overrides = HashMap::from([(
            paths[1].clone(),
            UploadMetadata {
                description: Some(String::new()),
                ..Default::default()
            },
        )]);
        let result = resolve_mapping(&paths, &common, Some(&overrides)).unwrap();
        assert_eq!(
            result.matches[0].metadata.title.as_deref(),
            Some("Common 1")
        );
        assert_eq!(result.matches[1].metadata.title, None);
        assert_eq!(result.matches[1].metadata.description.as_deref(), Some(""));
        assert!(resolve_mapping(
            &paths,
            &common,
            Some(&HashMap::from([(
                "elsewhere.mp4".into(),
                UploadMetadata::default()
            )]))
        )
        .is_err());
    }

    #[test]
    fn csv_quoted_unicode_newlines_and_json_schema_are_validated() {
        let rows = manifest_rows("file,title,description,tags,notifySubscribers\r\nclip.mp4,演奏,\"First, line\nSecond \"\"line\"\"\",music;live,false\r\n", "csv").unwrap();
        assert_eq!(rows[0]["description"], "First, line\nSecond \"line\"");
        assert_eq!(rows[0]["notifySubscribers"], false);
        assert_eq!(rows[0]["tags"], serde_json::json!(["music", "live"]));
        assert!(manifest_rows("file,file\na,b", "csv").is_err());
        assert!(manifest_rows("file,title\na,\"unclosed", "csv").is_err());
        assert!(manifest_rows(r#"{"schemaVersion":2,"videos":[]}"#, "json").is_err());
        assert!(manifest_rows(
            r#"{"schemaVersion":1,"videos":[{"file":"a","title":"x","title":"y"}]}"#,
            "json"
        )
        .is_err());
        assert!(
            manifest_rows(r#"{"schemaVersion":1,"videos":[],"script":"bad"}"#, "json").is_err()
        );
    }

    #[test]
    fn manifest_containment_discloses_duplicate_unknown_unmatched_and_bad_values() {
        let root = std::env::temp_dir().join(format!("youtube-metadata-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("selected")).unwrap();
        fs::write(root.join("selected/one.mp4"), b"one").unwrap();
        fs::write(root.join("selected/two.mp4"), b"two").unwrap();
        fs::write(root.join("selected/extra.mp4"), b"extra").unwrap();
        let paths = vec![
            root.join("selected/one.mp4").to_string_lossy().to_string(),
            root.join("selected/two.mp4").to_string_lossy().to_string(),
        ];
        let normal = map_rows(
            vec![
                serde_json::json!({"file":"one.mp4","title":"{sequence} {filename}"}),
                serde_json::json!({"file":"two.mp4","description":""}),
            ],
            &root.join("selected"),
            &paths,
        )
        .unwrap();
        assert_eq!(normal.matches.len(), 2);
        assert!(normal.issues.is_empty());
        let rejected = map_rows(
            vec![
                serde_json::json!({"file":"one.mp4","title":"first"}),
                serde_json::json!({"file":"one.mp4","title":"duplicate"}),
                serde_json::json!({"file":"two.mp4","unknown":"x"}),
                serde_json::json!({"file":"extra.mp4","title":"Unselected"}),
                serde_json::json!({"file":"../outside.mp4"}),
                serde_json::json!({"file":"https://example.test/a.mp4"}),
                serde_json::json!({"file":"two.mp4","notifySubscribers":"false"}),
            ],
            &root.join("selected"),
            &paths,
        )
        .unwrap();
        assert!(rejected.matches.is_empty());
        assert!(rejected
            .issues
            .iter()
            .any(|issue| issue.contains("same selected video")));
        assert!(rejected
            .issues
            .iter()
            .any(|issue| issue.contains("unknown fields")));
        assert!(rejected
            .issues
            .iter()
            .any(|issue| issue.contains("not among")));
        assert!(rejected.issues.iter().any(|issue| issue.contains("schema")));
        assert!(map_rows(vec![], &root.join("elsewhere"), &paths).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_merge_keeps_unrelated_writable_metadata_and_explicit_clears() {
        let current = serde_json::json!({"id":"video-a","snippet":{"title":"Original","description":"Keep","tags":["keep"],"categoryId":"22","defaultLanguage":"en","channelId":"channel-a"},"status":{"privacyStatus":"private","license":"youtube","embeddable":false,"publicStatsViewable":true,"selfDeclaredMadeForKids":false,"uploadStatus":"processed"}});
        let updated = merge_update_payload(
            &current,
            &UploadMetadata {
                title: Some("New".into()),
                description: Some(String::new()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated["snippet"]["tags"], serde_json::json!(["keep"]));
        assert_eq!(updated["snippet"]["categoryId"], "22");
        assert_eq!(updated["snippet"]["description"], "");
        assert!(updated["snippet"].get("channelId").is_none());
        assert!(updated.get("status").is_none());
        let updated = merge_update_payload(
            &current,
            &UploadMetadata {
                license: Some("creativeCommon".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated["status"]["embeddable"], false);
        assert_eq!(updated["status"]["privacyStatus"], "private");
        assert!(updated["status"].get("uploadStatus").is_none());
        assert!(merge_update_payload(
            &serde_json::json!({"id":"video-a","snippet":{"title":"Missing category"}}),
            &UploadMetadata {
                title: Some("New".into()),
                ..Default::default()
            }
        )
        .is_err());
    }
}
