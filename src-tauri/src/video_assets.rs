//! Reviewed image/caption attachments. Asset jobs never restart a video upload.
use super::*;
use rusqlite::Transaction;
use std::io::Cursor;

const THUMBNAIL_LIMIT: u64 = 50_000_000;
const CAPTION_LIMIT: u64 = 100_000_000;
const MAX_SELECTED: usize = 200;
const REVIEW_SECONDS: i64 = 900;
const LIMIT_DATE: &str = "2026-10-13";

pub(super) const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS video_asset_jobs (
 id TEXT PRIMARY KEY NOT NULL, item_id TEXT NOT NULL, channel_id TEXT NOT NULL,
 video_id TEXT, kind TEXT NOT NULL CHECK(kind IN ('thumbnail','caption')),
 source_path TEXT NOT NULL, file_name TEXT NOT NULL, source_size INTEGER NOT NULL,
 source_modified TEXT NOT NULL, source_identity TEXT NOT NULL, source_digest TEXT NOT NULL,
 language TEXT NOT NULL DEFAULT '', track_name TEXT NOT NULL DEFAULT '', is_draft INTEGER NOT NULL DEFAULT 0,
 declaration_key TEXT NOT NULL, status TEXT NOT NULL, detail TEXT NOT NULL,
 claim_id TEXT, write_started INTEGER NOT NULL DEFAULT 0, baseline_json TEXT,
 caption_id TEXT, receipt_json TEXT, next_check_at TEXT, check_count INTEGER NOT NULL DEFAULT 0,
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
 UNIQUE(channel_id,item_id,declaration_key), FOREIGN KEY(item_id) REFERENCES upload_items(id)
);
CREATE INDEX IF NOT EXISTS video_asset_jobs_pending ON video_asset_jobs(channel_id,status,next_check_at,item_id);
CREATE TABLE IF NOT EXISTS video_asset_attempts (
 id TEXT PRIMARY KEY NOT NULL, job_id TEXT NOT NULL, item_id TEXT NOT NULL,
 channel_id TEXT NOT NULL, video_id TEXT NOT NULL, operation TEXT NOT NULL,
 outcome TEXT NOT NULL, detail TEXT NOT NULL, created_at TEXT NOT NULL,
 FOREIGN KEY(job_id) REFERENCES video_asset_jobs(id)
);
"#;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    path: PathBuf,
    signature: (u64, String),
    identity: String,
    digest: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetItem {
    pub(super) item_id: String,
    pub(super) title: String,
    pub(super) file_name: String,
    pub(super) video_id: Option<String>,
    pub(super) status: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetCandidate {
    asset_id: String,
    file_name: String,
    kind: String,
    size_bytes: u64,
    valid: bool,
    dimensions: Option<String>,
    cue_count: Option<usize>,
    suggested_item_ids: Vec<String>,
    warnings: Vec<String>,
}
#[derive(Clone)]
struct Review {
    database_path: PathBuf,
    channel: String,
    items: Vec<AssetItem>,
    assets: Vec<(AssetCandidate, Option<Proof>)>,
    created: DateTime<Utc>,
    prospective: Option<ProspectiveScope>,
}
#[derive(Clone)]
struct ProspectiveScope {
    plan_id: String,
    targets: Vec<ProspectiveAssetTarget>,
    expires_at: DateTime<Utc>,
}
/// Native-only descriptors issued from an immutable, source-verified plan.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProspectiveAssetTarget {
    pub(super) ordinal: usize,
    pub(super) target_id: String,
    pub(super) title: String,
    pub(super) file_name: String,
    pub(super) source_path: PathBuf,
    pub(super) source_size: u64,
    pub(super) source_modified: String,
    pub(super) source_digest: String,
    pub(super) source_identity: String,
}
impl ProspectiveAssetTarget {
    fn item(&self) -> AssetItem {
        AssetItem {
            item_id: self.target_id.clone(),
            title: self.title.clone(),
            file_name: self.file_name.clone(),
            video_id: None,
            status: "prospective".into(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanAssetDeclaration {
    target_id: String,
    asset: AssetCandidate,
    proof: Proof,
    language: String,
    name: String,
    draft: bool,
}
/// Complete native proof, copied into the sealed plan and then into the durable
/// execution claim. It never enters the webview or redacted export.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreparedPlanAssets {
    database_path: PathBuf,
    channel: String,
    targets: Vec<ProspectiveAssetTarget>,
    declarations: Vec<PlanAssetDeclaration>,
    expires_at: DateTime<Utc>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PlanAssetSummary {
    kind: String,
    file_name: String,
    dimensions: Option<String>,
    cue_count: Option<usize>,
    language: Option<String>,
    name: Option<String>,
    draft: Option<bool>,
}
impl PreparedPlanAssets {
    pub(super) fn matches_targets(&self, targets: &[ProspectiveAssetTarget]) -> bool {
        self.targets == targets
    }
    pub(super) fn summaries(&self, ordinal: usize) -> Vec<PlanAssetSummary> {
        let Some(target) = self.targets.iter().find(|target| target.ordinal == ordinal) else {
            return Vec::new();
        };
        self.declarations
            .iter()
            .filter(|declaration| declaration.target_id == target.target_id)
            .map(|declaration| PlanAssetSummary {
                kind: declaration.asset.kind.clone(),
                file_name: declaration.asset.file_name.clone(),
                dimensions: declaration.asset.dimensions.clone(),
                cue_count: declaration.asset.cue_count,
                language: (declaration.asset.kind == "caption")
                    .then(|| declaration.language.clone()),
                name: (declaration.asset.kind == "caption").then(|| declaration.name.clone()),
                draft: (declaration.asset.kind == "caption").then_some(declaration.draft),
            })
            .collect()
    }
    pub(super) fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetReview {
    review_id: String,
    channel_id: String,
    items: Vec<AssetItem>,
    assets: Vec<AssetCandidate>,
    expires_at: String,
    limits_checked_at: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AssetBinding {
    asset_id: String,
    item_id: String,
    language: Option<String>,
    name: Option<String>,
    draft: Option<bool>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetJob {
    pub(super) job_id: String,
    pub(super) item_id: String,
    pub(super) channel_id: String,
    pub(super) video_id: Option<String>,
    pub(super) kind: String,
    pub(super) file_name: String,
    pub(super) language: Option<String>,
    pub(super) name: Option<String>,
    pub(super) draft: Option<bool>,
    pub(super) status: String,
    pub(super) detail: String,
    pub(super) caption_id: Option<String>,
    pub(super) updated_at: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetWorkspace {
    channel_id: String,
    items: Vec<AssetItem>,
    jobs: Vec<AssetJob>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetReceipt {
    channel_id: String,
    job_ids: Vec<String>,
}

fn reviews() -> &'static Mutex<HashMap<String, Review>> {
    static REVIEWS: OnceLock<Mutex<HashMap<String, Review>>> = OnceLock::new();
    REVIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
fn ensure_channel(connection: &Connection, channel: &str) -> Result<ConnectionSettings, String> {
    let settings = connection_settings(connection)?;
    if !valid_id(channel)
        || !settings.connected
        || settings.active_channel_id.as_deref() != Some(channel)
    {
        return Err("The connected channel changed. Review the selected videos again.".into());
    }
    Ok(settings)
}
fn selected_items(
    connection: &Connection,
    channel: &str,
    ids: &[String],
) -> Result<Vec<AssetItem>, String> {
    ensure_channel(connection, channel)?;
    if ids.is_empty()
        || ids.len() > MAX_SELECTED
        || ids.iter().any(|id| !valid_id(id))
        || ids.iter().collect::<HashSet<_>>().len() != ids.len()
    {
        return Err("Choose 1–200 distinct saved uploads for attachments.".into());
    }
    ids.iter().map(|id|connection.query_row("SELECT id,title,file_name,video_id,status FROM upload_items WHERE id=?1 AND channel_id=?2",params![id,channel],|row|Ok(AssetItem{item_id:row.get(0)?,title:row.get(1)?,file_name:row.get(2)?,video_id:row.get(3)?,status:row.get(4)?})).map_err(|_|"A selected upload is unavailable or belongs to another channel.".into())).collect()
}
pub(super) fn asset_identity(file: &File) -> Result<String, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata().map_err(user_error)?;
        return Ok(format!("{}:{}", meta.dev(), meta.ino()));
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        struct Info {
            attributes: u32,
            created: [u32; 2],
            accessed: [u32; 2],
            written: [u32; 2],
            volume: u32,
            high: u32,
            low: u32,
            links: u32,
            index_high: u32,
            index_low: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Info) -> i32;
        }
        let mut info = std::mem::MaybeUninit::<Info>::uninit();
        // SAFETY: the held File remains valid and Windows initializes the documented layout on success.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err("The original attachment identity is unavailable.".into());
        }
        let info = unsafe { info.assume_init() };
        Ok(format!(
            "{}:{}",
            info.volume,
            ((info.index_high as u64) << 32) | info.index_low as u64
        ))
    }
}
fn crc32(bytes: &[u8]) -> u32 {
    static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut table = [0; 256];
        for (index, value) in table.iter_mut().enumerate() {
            let mut crc = index as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    0xedb88320 ^ (crc >> 1)
                } else {
                    crc >> 1
                }
            }
            *value = crc
        }
        table
    });
    let mut crc = !0u32;
    for byte in bytes {
        crc = table[((crc ^ *byte as u32) & 255) as usize] ^ (crc >> 8)
    }
    !crc
}
fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Choose a complete PNG image.".into());
    }
    let mut at = 8usize;
    let mut size = None;
    let mut bit_depth = 0u8;
    let mut color = 0u8;
    let mut interlace = 0u8;
    let mut palette = false;
    let mut compressed = Vec::new();
    let mut ended = false;
    let mut idat_ended = false;
    while at + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let end = at
            .checked_add(12)
            .and_then(|value| value.checked_add(len))
            .ok_or("The PNG chunk length is invalid.")?;
        if end > bytes.len() {
            return Err("The PNG is incomplete.".into());
        }
        let kind = &bytes[at + 4..at + 8];
        if !kind.iter().all(u8::is_ascii_alphabetic) || !kind[2].is_ascii_uppercase() {
            return Err("The PNG chunk type is invalid.".into());
        }
        let data = &bytes[at + 8..at + 8 + len];
        if crc32(&bytes[at + 4..at + 8 + len])
            != u32::from_be_bytes(bytes[at + 8 + len..end].try_into().unwrap())
        {
            return Err("The PNG checksum is invalid.".into());
        }
        match kind {
            b"IHDR" => {
                if at != 8 || len != 13 || size.is_some() {
                    return Err("The PNG image header is invalid.".into());
                }
                let width = u32::from_be_bytes(data[..4].try_into().unwrap());
                let height = u32::from_be_bytes(data[4..8].try_into().unwrap());
                if width == 0
                    || height == 0
                    || width > 0x7fffffff
                    || height > 0x7fffffff
                    || data[10] != 0
                    || data[11] != 0
                    || data[12] > 1
                {
                    return Err("The PNG dimensions or encoding are invalid.".into());
                }
                bit_depth = data[8];
                color = data[9];
                interlace = data[12];
                if !match color {
                    0 => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
                    2 | 4 | 6 => matches!(bit_depth, 8 | 16),
                    3 => matches!(bit_depth, 1 | 2 | 4 | 8),
                    _ => false,
                } {
                    return Err("The PNG color encoding is invalid.".into());
                }
                size = Some((width, height));
            }
            b"PLTE" => {
                if size.is_none() || !compressed.is_empty() || len == 0 || len > 768 || len % 3 != 0
                {
                    return Err("The PNG palette is invalid.".into());
                }
                palette = true;
            }
            b"IDAT" => {
                if size.is_none() || idat_ended {
                    return Err("The PNG image chunks are invalid.".into());
                }
                compressed.extend_from_slice(data);
            }
            b"IEND" => {
                if len != 0 || end != bytes.len() {
                    return Err("The PNG end marker is invalid.".into());
                }
                ended = true;
                break;
            }
            _ => {
                if kind[0].is_ascii_uppercase() {
                    return Err("This PNG contains an unsupported critical image chunk.".into());
                }
                if !compressed.is_empty() {
                    idat_ended = true;
                }
            }
        }
        at = end;
    }
    let (width, height) = size.ok_or("The PNG header is missing.")?;
    if !ended || compressed.is_empty() || (color == 3 && !palette) {
        return Err("The PNG is incomplete.".into());
    }
    let channels = match color {
        0 | 3 => 1u64,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => unreachable!(),
    };
    let bits = channels * bit_depth as u64;
    let passes = if interlace == 0 {
        vec![(0, 0, 1, 1)]
    } else {
        vec![
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
    };
    let mut decoder = flate2::read::ZlibDecoder::new(compressed.as_slice());
    let mut total = 0u64;
    let mut buffer = [0u8; 8192];
    for (x, y, dx, dy) in passes {
        let w = if width > x {
            (width - x).div_ceil(dx)
        } else {
            0
        };
        let h = if height > y {
            (height - y).div_ceil(dy)
        } else {
            0
        };
        if w == 0 || h == 0 {
            continue;
        }
        let row = (w as u64 * bits).div_ceil(8);
        total = total
            .checked_add((row + 1) * h as u64)
            .ok_or("This PNG exceeds safe local validation capacity.")?;
        if total > 128_000_000 {
            return Err(
                "This PNG exceeds safe local validation capacity. Choose a smaller image.".into(),
            );
        }
        for _ in 0..h {
            let mut filter = [0u8];
            decoder
                .read_exact(&mut filter)
                .map_err(|_| "The PNG pixel stream is incomplete.")?;
            if filter[0] > 4 {
                return Err("The PNG pixel filter is invalid.".into());
            }
            let mut remaining = row;
            while remaining > 0 {
                let count = remaining.min(buffer.len() as u64) as usize;
                decoder
                    .read_exact(&mut buffer[..count])
                    .map_err(|_| "The PNG pixel stream is incomplete.")?;
                remaining -= count as u64
            }
        }
    }
    if decoder
        .read(&mut buffer[..1])
        .map_err(|_| "The PNG pixel stream is invalid.")?
        != 0
        || decoder.total_in() != compressed.len() as u64
    {
        return Err("The PNG pixel stream has unexpected data.".into());
    }
    Ok((width, height))
}
fn jpeg_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err("Choose a complete JPEG image.".into());
    }
    let mut at = 2;
    let mut size = None;
    let mut tables = false;
    let mut huffman = false;
    let mut scan = false;
    let mut entropy = false;
    while at < bytes.len() {
        if bytes[at] != 0xff {
            return Err("The JPEG marker stream is invalid.".into());
        }
        while at < bytes.len() && bytes[at] == 0xff {
            at += 1
        }
        let marker = *bytes.get(at).ok_or("The JPEG is incomplete.")?;
        at += 1;
        if marker == 0xd9 {
            if at != bytes.len() || !tables || !huffman || !scan || !entropy {
                return Err("The JPEG image is incomplete.".into());
            }
            return size.ok_or_else(|| "The JPEG frame is missing.".into());
        }
        if matches!(marker, 0x00 | 0xd8 | 0xd0..=0xd7) {
            return Err("The JPEG marker order is invalid.".into());
        }
        let length = bytes
            .get(at..at + 2)
            .map(|value| u16::from_be_bytes(value.try_into().unwrap()) as usize)
            .ok_or("The JPEG is incomplete.")?;
        if length < 2 || at + length > bytes.len() {
            return Err("The JPEG segment is incomplete.".into());
        }
        let data = &bytes[at + 2..at + length];
        match marker {
            0xc0 | 0xc1 | 0xc2 => {
                if size.is_some() || data.len() < 6 || !matches!(data[0], 8 | 12) {
                    return Err("The JPEG frame is invalid.".into());
                }
                let height = u16::from_be_bytes(data[1..3].try_into().unwrap()) as u32;
                let width = u16::from_be_bytes(data[3..5].try_into().unwrap()) as u32;
                if width == 0
                    || height == 0
                    || !matches!(data[5], 1 | 3 | 4)
                    || data.len() != 6 + 3 * data[5] as usize
                {
                    return Err("The JPEG dimensions are invalid.".into());
                }
                size = Some((width, height));
            }
            0xdb => {
                let mut cursor = 0;
                while cursor < data.len() {
                    let precision = data[cursor] >> 4;
                    if precision > 1 || data[cursor] & 15 > 3 {
                        return Err("The JPEG quantization table is invalid.".into());
                    }
                    cursor += 1 + 64 * (1 + precision as usize);
                    if cursor > data.len() {
                        return Err("The JPEG quantization table is incomplete.".into());
                    }
                }
                tables = !data.is_empty();
            }
            0xc4 => {
                let mut cursor = 0;
                while cursor < data.len() {
                    if cursor + 17 > data.len() || data[cursor] >> 4 > 1 || data[cursor] & 15 > 3 {
                        return Err("The JPEG Huffman table is invalid.".into());
                    }
                    let symbols = data[cursor + 1..cursor + 17]
                        .iter()
                        .map(|value| *value as usize)
                        .sum::<usize>();
                    if symbols == 0 || symbols > 256 {
                        return Err("The JPEG Huffman table is invalid.".into());
                    }
                    cursor += 17 + symbols;
                    if cursor > data.len() {
                        return Err("The JPEG Huffman table is incomplete.".into());
                    }
                }
                huffman = !data.is_empty();
            }
            0xda => {
                if size.is_none()
                    || data.len() < 6
                    || data[0] == 0
                    || data.len() != 4 + 2 * data[0] as usize
                {
                    return Err("The JPEG scan header is invalid.".into());
                }
                scan = true;
            }
            _ => {
                if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                    return Err("Choose a baseline or progressive JPEG image.".into());
                }
            }
        }
        at += length;
        if marker == 0xda {
            while at < bytes.len() {
                if bytes[at] != 0xff {
                    entropy = true;
                    at += 1;
                    continue;
                }
                let next = *bytes.get(at + 1).ok_or("The JPEG scan is incomplete.")?;
                if next == 0 || matches!(next, 0xd0..=0xd7) {
                    at += 2;
                    continue;
                }
                break;
            }
        }
    }
    Err("The JPEG end marker is missing.".into())
}
fn timestamp(value: &str, vtt: bool) -> Result<u64, String> {
    let separator = if vtt { '.' } else { ',' };
    let (clock, millis) = value
        .split_once(separator)
        .ok_or("Captions need complete timed cues.")?;
    if millis.len() != 3 || !millis.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Caption milliseconds must use three digits.".into());
    }
    let parts = clock.split(':').collect::<Vec<_>>();
    if !(parts.len() == 3 || (vtt && parts.len() == 2))
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("The caption timestamp is invalid.".into());
    }
    let numbers = parts
        .iter()
        .map(|part| {
            part.parse::<u64>()
                .map_err(|_| "The caption timestamp is too large.")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let offset = numbers.len() - 2;
    let minutes = numbers[offset];
    let seconds = numbers[offset + 1];
    if minutes > 59 || seconds > 59 {
        return Err("Caption minutes and seconds must be less than 60.".into());
    }
    let hours = if offset == 1 { numbers[0] } else { 0 };
    hours
        .checked_mul(3_600_000)
        .and_then(|value| {
            value.checked_add(minutes * 60_000 + seconds * 1000 + millis.parse::<u64>().unwrap())
        })
        .ok_or_else(|| "The caption timestamp is too large.".into())
}
fn timed_captions(bytes: &[u8], vtt: bool) -> Result<usize, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "Save the timed SRT or WebVTT file as UTF-8.")?
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n");
    if text.contains('\0') {
        return Err("The caption file contains invalid text.".into());
    }
    let mut blocks = text.split("\n\n");
    if vtt
        && !blocks.next().is_some_and(|block| {
            block
                .lines()
                .next()
                .is_some_and(|line| line == "WEBVTT" || line.starts_with("WEBVTT "))
        })
    {
        return Err("WebVTT captions must begin with WEBVTT and a blank line.".into());
    }
    let mut cues = 0usize;
    let mut previous = 0u64;
    for block in blocks {
        let lines = block.trim().lines().collect::<Vec<_>>();
        if lines.is_empty() {
            continue;
        }
        if vtt
            && (lines[0] == "NOTE"
                || lines[0].starts_with("NOTE ")
                || matches!(lines[0], "STYLE" | "REGION"))
        {
            continue;
        }
        let timing = if lines[0].contains(" --> ") { 0 } else { 1 };
        if timing >= lines.len()
            || (!vtt && timing == 1 && !lines[0].bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err("Each caption cue needs a valid timestamp line.".into());
        }
        let (start, end) = lines[timing]
            .split_once(" --> ")
            .ok_or("Each caption cue needs a start and end time.")?;
        let end = end
            .split_whitespace()
            .next()
            .ok_or("The caption end time is missing.")?;
        let start = timestamp(start.trim(), vtt)?;
        let end = timestamp(end, vtt)?;
        if end <= start
            || (cues > 0 && start < previous)
            || lines.len() <= timing + 1
            || lines[timing + 1..]
                .iter()
                .all(|line| line.trim().is_empty())
        {
            return Err(
                "Caption cues must have text, increasing start times and an end after the start."
                    .into(),
            );
        }
        previous = start;
        cues += 1;
        if cues > 1_000_000 {
            return Err("This caption file exceeds safe local validation capacity.".into());
        }
    }
    if cues == 0 {
        return Err("Choose a timed caption file containing at least one cue. Automatic timing is unavailable.".into());
    }
    Ok(cues)
}

fn inspect(path: &Path) -> Result<(Proof, String, Option<String>, Option<usize>), String> {
    if !path.is_absolute() {
        return Err("Choose an original attachment from an absolute local location.".into());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let kind = match extension.as_str() {
        "png" | "jpg" | "jpeg" => "thumbnail",
        "srt" | "vtt" => "caption",
        _ => return Err("Choose a JPEG/PNG thumbnail or timed SRT/WebVTT caption file.".into()),
    };
    let mut file = open_reference_source(path)?;
    let signature = monitored_file_signature(&file.metadata().map_err(user_error)?)?;
    let max = if kind == "thumbnail" {
        THUMBNAIL_LIMIT
    } else {
        CAPTION_LIMIT
    };
    if signature.0 == 0 || signature.0 > max {
        return Err(format!("The {kind} must contain bytes and be at most {} MB under the API limit checked {LIMIT_DATE}.",max/1_000_000));
    }
    let identity = asset_identity(&file)?;
    let canonical =
        fs::canonicalize(path).map_err(|_| "The attachment location is unavailable.")?;
    let permit = media_runtime::acquire_reader(path, &|| false)?;
    let mut bytes = Vec::with_capacity(signature.0 as usize);
    let mut buffer = [0u8; 65536];
    loop {
        permit.yield_to_upload(MediaIoPriority::Foreground, &|| false)?;
        let count = file
            .read(&mut buffer)
            .map_err(|_| "The attachment could not be read.")?;
        if count == 0 {
            break;
        }
        if bytes.len() + count > max as usize {
            return Err("The attachment changed or exceeds its size limit.".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    drop(permit);
    let (dimensions, cues) = if kind == "thumbnail" {
        let (width, height) = if extension == "png" {
            png_dimensions(&bytes)?
        } else {
            jpeg_dimensions(&bytes)?
        };
        (Some(format!("{width} × {height}")), None)
    } else {
        (None, Some(timed_captions(&bytes, extension == "vtt")?))
    };
    if monitored_file_signature(&file.metadata().map_err(user_error)?)? != signature
        || bytes.len() as u64 != signature.0
        || asset_identity(&open_reference_source(&canonical)?)? != identity
    {
        return Err("The attachment changed while it was checked. Choose it again.".into());
    }
    Ok((
        Proof {
            path: canonical,
            signature,
            identity,
            digest: blake3::hash(&bytes).to_hex().to_string(),
        },
        kind.into(),
        dimensions,
        cues,
    ))
}
fn verify(proof: &Proof) -> Result<File, String> {
    let mut file = open_reference_source(&proof.path)?;
    if asset_identity(&file)? != proof.identity
        || monitored_file_signature(&file.metadata().map_err(user_error)?)? != proof.signature
    {
        return Err(
            "The original attachment changed or moved. Review a fresh asset before retrying."
                .into(),
        );
    }
    let (size, digest) =
        digest_reader_for_path(&mut file, &proof.path, MediaIoPriority::Background, &|| {
            false
        })?;
    file.seek(SeekFrom::Start(0)).map_err(user_error)?;
    if size != proof.signature.0
        || digest != proof.digest
        || monitored_file_signature(&file.metadata().map_err(user_error)?)? != proof.signature
        || asset_identity(&open_reference_source(&proof.path)?)? != proof.identity
    {
        return Err(
            "The original attachment bytes changed. The video receipt is preserved.".into(),
        );
    }
    Ok(file)
}
fn prepare(
    state: &AppState,
    channel: &str,
    item_ids: Vec<String>,
    paths: Vec<String>,
    auto_match: bool,
) -> Result<AssetReview, String> {
    let items = selected_items(&database(state)?, channel, &item_ids)?;
    prepare_for_items(state, channel, items, paths, auto_match, None)
}
fn prepare_for_items(
    state: &AppState,
    channel: &str,
    items: Vec<AssetItem>,
    paths: Vec<String>,
    auto_match: bool,
    prospective: Option<ProspectiveScope>,
) -> Result<AssetReview, String> {
    if paths.is_empty()
        || paths.len() > MAX_SELECTED
        || paths.iter().collect::<HashSet<_>>().len() != paths.len()
    {
        return Err("Choose 1–200 distinct original attachment files.".into());
    }
    ensure_channel(&database(state)?, channel)?;
    let mut assets = Vec::new();
    for path in paths {
        let file_name = Path::new(&path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Selected attachment")
            .to_string();
        let suggested_item_ids = if auto_match {
            let stem = Path::new(&file_name)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_lowercase();
            items
                .iter()
                .filter(|item| {
                    Path::new(&item.file_name)
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("")
                        .to_lowercase()
                        == stem
                })
                .map(|item| item.item_id.clone())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let (proof, kind, size, dimensions, cues, mut warnings) = match inspect(Path::new(&path)) {
            Ok((proof, kind, dimensions, cues)) => {
                let size = proof.signature.0;
                (Some(proof), kind, size, dimensions, cues, Vec::new())
            }
            Err(error) => (None, "unknown".into(), 0, None, None, vec![error]),
        };
        if suggested_item_ids.len() > 1 {
            warnings.push(
                "More than one video has this basename. Choose the intended matches explicitly."
                    .into(),
            )
        }
        if kind == "thumbnail" {
            warnings.push("The API confirms final image decoding and channel eligibility. This job replaces the current thumbnail only after explicit review.".into())
        }
        assets.push((
            AssetCandidate {
                asset_id: Uuid::new_v4().to_string(),
                file_name,
                kind,
                size_bytes: size,
                valid: proof.is_some(),
                dimensions,
                cue_count: cues,
                suggested_item_ids,
                warnings,
            },
            proof,
        ));
    }
    ensure_channel(&database(state)?, channel)?;
    let id = Uuid::new_v4().to_string();
    let created = Utc::now();
    let result = AssetReview {
        review_id: id.clone(),
        channel_id: channel.into(),
        items: items.clone(),
        assets: assets.iter().map(|(asset, _)| asset.clone()).collect(),
        expires_at: prospective
            .as_ref()
            .map(|scope| {
                scope
                    .expires_at
                    .min(created + ChronoDuration::seconds(REVIEW_SECONDS))
            })
            .unwrap_or(created + ChronoDuration::seconds(REVIEW_SECONDS))
            .to_rfc3339(),
        limits_checked_at: LIMIT_DATE.into(),
    };
    let mut store = reviews().lock().map_err(user_error)?;
    store.retain(|_, review| Utc::now() - review.created < ChronoDuration::seconds(REVIEW_SECONDS));
    if store.len() >= 8 {
        if let Some(old) = store
            .iter()
            .min_by_key(|(_, review)| review.created)
            .map(|(id, _)| id.clone())
        {
            store.remove(&old);
        }
    }
    store.insert(
        id,
        Review {
            database_path: state.database_path.clone(),
            channel: channel.into(),
            items,
            assets,
            created,
            prospective,
        },
    );
    Ok(result)
}

fn prepare_prospective(
    state: &AppState,
    channel: &str,
    plan_id: &str,
    paths: Vec<String>,
    auto_match: bool,
) -> Result<AssetReview, String> {
    let (targets, expires_at) = upload_plan::prospective_targets(state, plan_id, channel)?;
    let items = targets.iter().map(ProspectiveAssetTarget::item).collect();
    prepare_for_items(
        state,
        channel,
        items,
        paths,
        auto_match,
        Some(ProspectiveScope {
            plan_id: plan_id.into(),
            targets,
            expires_at,
        }),
    )
}

pub(super) fn freeze_plan_assets(
    state: &AppState,
    channel: &str,
    plan_id: &str,
    review_id: &str,
    bindings: Vec<AssetBinding>,
) -> Result<PreparedPlanAssets, String> {
    let review = reviews()
        .lock()
        .map_err(user_error)?
        .get(review_id)
        .cloned()
        .ok_or("This prospective asset review expired. Check the original attachments again.")?;
    let scope = review
        .prospective
        .as_ref()
        .ok_or("Choose attachments for the prospective upload plan, rather than saved videos.")?;
    let (targets, expires_at) = upload_plan::prospective_targets(state, plan_id, channel)?;
    if review.database_path != state.database_path
        || review.channel != channel
        || scope.plan_id != plan_id
        || scope.targets != targets
        || Utc::now() - review.created >= ChronoDuration::seconds(REVIEW_SECONDS)
        || Utc::now() >= scope.expires_at
        || bindings.is_empty()
        || bindings.len() > 400
    {
        return Err(
            "The prospective attachment review changed or expired. Resolve a fresh plan.".into(),
        );
    }
    let mut declarations = Vec::new();
    let mut seen = HashSet::new();
    for binding in bindings {
        let (asset, proof) = review
            .assets
            .iter()
            .find(|(asset, _)| asset.asset_id == binding.asset_id)
            .ok_or("The attachment is outside this prospective review.")?;
        let proof = proof
            .as_ref()
            .ok_or("Leave invalid attachments unmatched before applying this plan.")?;
        if !targets
            .iter()
            .any(|target| target.target_id == binding.item_id)
        {
            return Err("The attachment target is outside this exact prospective plan.".into());
        }
        let (language, name, draft) = caption_declaration(&binding, &asset.kind)?;
        if !seen.insert((
            binding.item_id.clone(),
            asset.kind.clone(),
            language.clone(),
            name.clone(),
        )) {
            return Err("Choose one thumbnail per prospective video and unique caption language/name declarations.".into());
        }
        declarations.push(PlanAssetDeclaration {
            target_id: binding.item_id,
            asset: asset.clone(),
            proof: proof.clone(),
            language,
            name,
            draft,
        });
    }
    let prepared = PreparedPlanAssets {
        database_path: state.database_path.clone(),
        channel: channel.into(),
        targets,
        declarations,
        expires_at: expires_at
            .min(scope.expires_at)
            .min(review.created + ChronoDuration::seconds(REVIEW_SECONDS)),
    };
    verify_plan_assets(state, channel, &prepared)?;
    Ok(prepared)
}

fn validate_plan_asset_bundle(bundle: &PreparedPlanAssets) -> Result<(), String> {
    if bundle.targets.is_empty()
        || bundle.targets.len() > 512
        || bundle.declarations.is_empty()
        || bundle.declarations.len() > 400
        || bundle
            .targets
            .iter()
            .map(|target| target.ordinal)
            .collect::<HashSet<_>>()
            .len()
            != bundle.targets.len()
        || bundle
            .targets
            .iter()
            .map(|target| &target.target_id)
            .collect::<HashSet<_>>()
            .len()
            != bundle.targets.len()
        || bundle.targets.iter().any(|target| {
            !valid_id(&target.target_id)
                || !target.source_path.is_absolute()
                || target.source_digest.len() != 64
                || !target.source_digest.chars().all(|c| c.is_ascii_hexdigit())
        })
    {
        return Err("The approved attachment manifest is incomplete; retained upload identities cannot be repeated.".into());
    }
    let mut seen = HashSet::new();
    for declaration in &bundle.declarations {
        if !bundle
            .targets
            .iter()
            .any(|target| target.target_id == declaration.target_id)
            || !declaration.asset.valid
            || !matches!(declaration.asset.kind.as_str(), "thumbnail" | "caption")
            || !declaration.proof.path.is_absolute()
            || declaration.proof.digest.len() != 64
            || !declaration
                .proof
                .digest
                .chars()
                .all(|c| c.is_ascii_hexdigit())
            || declaration.proof.signature.0 != declaration.asset.size_bytes
            || declaration.proof.signature.0
                > if declaration.asset.kind == "thumbnail" {
                    THUMBNAIL_LIMIT
                } else {
                    CAPTION_LIMIT
                }
            || !seen.insert((
                &declaration.target_id,
                &declaration.asset.kind,
                &declaration.language,
                &declaration.name,
            ))
        {
            return Err("The approved attachment manifest changed. Preserve its existing video receipts and review repair.".into());
        }
        caption_declaration(
            &AssetBinding {
                asset_id: declaration.asset.asset_id.clone(),
                item_id: declaration.target_id.clone(),
                language: (declaration.asset.kind == "caption")
                    .then(|| declaration.language.clone()),
                name: (declaration.asset.kind == "caption").then(|| declaration.name.clone()),
                draft: (declaration.asset.kind == "caption").then_some(declaration.draft),
            },
            &declaration.asset.kind,
        )?;
    }
    Ok(())
}

/// Full original-video digest checks are performed by verified_intake before
/// this check. Attachment bytes are independently reverified immediately before
/// the durable plan claim, and again before their same-item transaction commits.
pub(super) fn verify_plan_assets(
    state: &AppState,
    channel: &str,
    bundle: &PreparedPlanAssets,
) -> Result<(), String> {
    ensure_channel(&database(state)?, channel)?;
    if bundle.database_path != state.database_path
        || bundle.channel != channel
        || Utc::now() >= bundle.expires_at
    {
        return Err("This approved attachment plan expired or changed channel. Resolve it again before intake.".into());
    }
    validate_plan_asset_bundle(bundle)?;
    for target in &bundle.targets {
        let file = open_reference_source(&target.source_path)?;
        if monitored_file_signature(&file.metadata().map_err(user_error)?)?
            != (target.source_size, target.source_modified.clone())
            || asset_identity(&file)? != target.source_identity
        {
            return Err("A prospective video changed after attachment review. Resolve its upload plan again.".into());
        }
    }
    let mut verified = HashSet::new();
    for declaration in &bundle.declarations {
        if verified.insert(&declaration.proof.path) {
            drop(verify(&declaration.proof)?);
        }
    }
    ensure_channel(&database(state)?, channel)?;
    Ok(())
}

/// Verified original handles are admitted before any SQLite write transaction.
/// Holding them prevents a locator replacement from inheriting the approval.
pub(super) struct PlanItemAssetGuard {
    plan_id: String,
    channel: String,
    ordinal: usize,
    source_path: PathBuf,
    bundle_fingerprint: String,
    held: Vec<(usize, File)>,
}
fn bundle_fingerprint(bundle: &PreparedPlanAssets) -> Result<String, String> {
    Ok(
        blake3::hash(&serde_json::to_vec(bundle).map_err(user_error)?)
            .to_hex()
            .to_string(),
    )
}
pub(super) fn hold_plan_item_assets(
    state: &AppState,
    plan_id: &str,
    channel: &str,
    ordinal: usize,
    source_path: &Path,
) -> Result<Option<PlanItemAssetGuard>, String> {
    let connection = database(state)?;
    ensure_channel(&connection, channel)?;
    let Some(bundle) = upload_plan_execution::asset_manifest(&connection, plan_id, channel)? else {
        if upload_plan::has_assets(plan_id, channel)? {
            return Err("The prospective attachments have no retained execution approval.".into());
        }
        return Ok(None);
    };
    validate_plan_asset_bundle(&bundle)?;
    if bundle.database_path != state.database_path || bundle.channel != channel {
        return Err("The approved attachment proof belongs to another workspace.".into());
    }
    let target = bundle
        .targets
        .iter()
        .find(|target| target.ordinal == ordinal)
        .ok_or("The reviewed attachment ordinal is outside this plan.")?;
    if target.source_path != source_path {
        return Err("The original does not match its reviewed attachment ordinal.".into());
    }
    drop(connection);
    let mut held = Vec::new();
    for (index, declaration) in bundle
        .declarations
        .iter()
        .enumerate()
        .filter(|(_, declaration)| declaration.target_id == target.target_id)
    {
        held.push((index, verify(&declaration.proof)?));
    }
    Ok(Some(PlanItemAssetGuard {
        plan_id: plan_id.into(),
        channel: channel.into(),
        ordinal,
        source_path: source_path.into(),
        bundle_fingerprint: bundle_fingerprint(&bundle)?,
        held,
    }))
}
fn verify_held_asset(proof: &Proof, file: &File) -> Result<(), String> {
    let current = open_reference_source(&proof.path)?;
    if monitored_file_signature(&file.metadata().map_err(user_error)?)? != proof.signature
        || asset_identity(file)? != proof.identity
        || monitored_file_signature(&current.metadata().map_err(user_error)?)? != proof.signature
        || asset_identity(&current)? != proof.identity
    {
        return Err(
            "The verified attachment handle or original location changed before intake committed."
                .into(),
        );
    }
    Ok(())
}

/// Called only inside the video INSERT transaction. The execution claim stores
/// the full native proof so crash recovery never asks for an existing asset again.
pub(super) fn materialize_plan_item_assets(
    transaction: &Transaction<'_>,
    plan_id: &str,
    channel: &str,
    ordinal: usize,
    source_path: &Path,
    item_id: &str,
    guard: Option<&PlanItemAssetGuard>,
) -> Result<Vec<String>, String> {
    ensure_channel(transaction, channel)?;
    let Some(bundle) = upload_plan_execution::asset_manifest(transaction, plan_id, channel)? else {
        if upload_plan::has_assets(plan_id, channel)? {
            return Err("The approved attachment manifest was not claimed with this intake. No new upload identity was committed.".into());
        }
        return Ok(Vec::new());
    };
    validate_plan_asset_bundle(&bundle)?;
    let guard = guard.ok_or(
        "The prospective attachment originals were not verified before the intake transaction.",
    )?;
    if guard.plan_id != plan_id
        || guard.channel != channel
        || guard.ordinal != ordinal
        || guard.source_path != source_path
        || guard.bundle_fingerprint != bundle_fingerprint(&bundle)?
    {
        return Err(
            "The held attachment approval does not match this exact intake transaction.".into(),
        );
    }
    if bundle.channel != channel
        || transaction.path().map(Path::new) != Some(bundle.database_path.as_path())
    {
        return Err("This approved attachment manifest belongs to another workspace.".into());
    }
    let target = bundle
        .targets
        .iter()
        .find(|target| target.ordinal == ordinal)
        .ok_or("This intake ordinal is outside the approved asset plan.")?;
    if target.source_path != source_path {
        return Err("This original does not match the reviewed attachment ordinal.".into());
    }
    let (item, path, digest, size, modified, owner_plan): (AssetItem,String,Option<String>,u64,String,Option<String>) = transaction.query_row(
        "SELECT id,title,file_name,video_id,status,source_path,digest,size_bytes,source_modified_key,upload_plan_id FROM upload_items WHERE id=?1 AND channel_id=?2",params![item_id,channel],|row|Ok((AssetItem {item_id:row.get(0)?,title:row.get(1)?,file_name:row.get(2)?,video_id:row.get(3)?,status:row.get(4)?},row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?)))
        .map_err(|_|"The prospective attachment target has no matching saved upload identity.".to_string())?;
    if Path::new(&path) != source_path
        || digest.as_deref() != Some(target.source_digest.as_str())
        || size != target.source_size
        || modified != target.source_modified
        || owner_plan.as_deref() != Some(plan_id)
    {
        return Err(
            "The saved upload no longer matches its immutable prospective attachment proof.".into(),
        );
    }
    if let Some((owner, ids)) =
        upload_plan_execution::asset_materialization(transaction, plan_id, channel, ordinal)?
    {
        if owner != item_id {
            return Err(
                "This prospective ordinal already belongs to a different retained upload identity."
                    .into(),
            );
        }
        for id in &ids {
            if load_job(transaction, id, channel)?.view.item_id != item_id {
                return Err("An attachment receipt no longer matches this upload identity.".into());
            }
        }
        return Ok(ids);
    }
    let mut job_ids = Vec::new();
    for (index, declaration) in bundle
        .declarations
        .iter()
        .enumerate()
        .filter(|(_, declaration)| declaration.target_id == target.target_id)
    {
        let file = &guard
            .held
            .iter()
            .find(|(held_index, _)| *held_index == index)
            .ok_or(
                "A verified attachment handle is unavailable; no upload identity was committed.",
            )?
            .1;
        verify_held_asset(&declaration.proof, file)?;
        job_ids.push(persist_declaration(
            transaction,
            channel,
            &item,
            &declaration.asset,
            &declaration.proof,
            &declaration.language,
            &declaration.name,
            declaration.draft,
        )?);
    }
    upload_plan_execution::record_asset_materialization(
        transaction,
        plan_id,
        channel,
        ordinal,
        item_id,
        &job_ids,
    )?;
    Ok(job_ids)
}

fn caption_declaration(
    binding: &AssetBinding,
    kind: &str,
) -> Result<(String, String, bool), String> {
    if kind == "thumbnail" {
        if binding.language.is_some() || binding.name.is_some() || binding.draft.is_some() {
            return Err("Caption choices cannot be applied to a thumbnail.".into());
        }
        return Ok((String::new(), String::new(), false));
    }
    let language = binding.language.as_deref().unwrap_or("").trim();
    let name = binding.name.as_deref().unwrap_or("").trim();
    let parts = language.split('-').collect::<Vec<_>>();
    if language.len() > 35
        || parts.is_empty()
        || !(2..=3).contains(&parts[0].len())
        || !parts[0].bytes().all(|byte| byte.is_ascii_alphabetic())
        || parts.iter().skip(1).any(|part| {
            part.is_empty()
                || part.len() > 8
                || !part.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
    {
        return Err("Declare a caption language tag such as en, fil or zh-Hant.".into());
    }
    if name.is_empty() || name.chars().count() > 150 || name.chars().any(char::is_control) {
        return Err("Declare a caption name of 1–150 characters.".into());
    }
    Ok((
        language.to_ascii_lowercase(),
        name.into(),
        binding
            .draft
            .ok_or("Choose whether this caption track is draft or visible.")?,
    ))
}
fn asset_audit(
    connection: &Connection,
    item: &str,
    channel: &str,
    kind: &str,
    detail: &str,
) -> Result<(), String> {
    connection.execute("INSERT INTO audit_events(id,item_id,channel_id,kind,detail,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![Uuid::new_v4().to_string(),item,channel,kind,detail,now()]).map_err(user_error)?;
    Ok(())
}
fn persist_declaration(
    connection: &Connection,
    channel: &str,
    item: &AssetItem,
    asset: &AssetCandidate,
    proof: &Proof,
    language: &str,
    name: &str,
    draft: bool,
) -> Result<String, String> {
    let declaration = blake3::hash(
        serde_json::to_string(&(
            &asset.kind,
            &proof.digest,
            &proof.path,
            &proof.identity,
            &proof.signature,
            language,
            name,
            draft,
        ))
        .map_err(user_error)?
        .as_bytes(),
    )
    .to_hex()
    .to_string();
    if let Some(id) = connection.query_row("SELECT id FROM video_asset_jobs WHERE channel_id=?1 AND item_id=?2 AND declaration_key=?3",params![channel,item.item_id,declaration],|row|row.get::<_,String>(0)).optional().map_err(user_error)? { return Ok(id); }
    let active:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM video_asset_jobs WHERE channel_id=?1 AND item_id=?2 AND kind=?3 AND (?3='thumbnail' OR (language=?4 AND track_name=?5)) AND status IN ('waiting_upload','queued','waiting_permission','running','needs_reconciliation','waiting_processing'))",params![channel,item.item_id,asset.kind,language,name],|row|row.get(0)).map_err(user_error)?;
    if active {
        return Err("A job for this thumbnail or caption declaration is still pending or uncertain. Reconcile it before creating another.".into());
    }
    if asset.kind == "thumbnail" {
        connection.execute("UPDATE video_asset_jobs SET status='superseded',detail='A later explicitly reviewed thumbnail superseded this completed or failed declaration; its receipt is retained',updated_at=?1 WHERE channel_id=?2 AND item_id=?3 AND kind='thumbnail' AND status IN ('succeeded','failed','conflict')",params![now(),channel,item.item_id]).map_err(user_error)?;
    }
    let id = Uuid::new_v4().to_string();
    let video = item.video_id.clone().filter(|_| item.status == "uploaded");
    let status = if video.is_some() {
        "queued"
    } else {
        "waiting_upload"
    };
    connection.execute("INSERT INTO video_asset_jobs(id,item_id,channel_id,video_id,kind,source_path,file_name,source_size,source_modified,source_identity,source_digest,language,track_name,is_draft,declaration_key,status,detail,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,'Reviewed attachment retained separately from the video upload',?17,?17)",params![id,item.item_id,channel,video,asset.kind,proof.path.to_string_lossy(),asset.file_name,proof.signature.0,proof.signature.1,proof.identity,proof.digest,language,name,draft,declaration,status,now()]).map_err(user_error)?;
    asset_audit(connection,&item.item_id,channel,"video_asset_job_requested","Operator approved a verified original attachment; video upload bytes, receipt and checkpoints retained")?;
    Ok(id)
}
fn save(
    state: &AppState,
    channel: &str,
    review_id: &str,
    bindings: Vec<AssetBinding>,
) -> Result<AssetReceipt, String> {
    let review = reviews()
        .lock()
        .map_err(user_error)?
        .get(review_id)
        .cloned()
        .ok_or("This asset review expired. Match the original files again.")?;
    if review.database_path != state.database_path
        || review.channel != channel
        || Utc::now() - review.created >= ChronoDuration::seconds(REVIEW_SECONDS)
        || bindings.is_empty()
        || bindings.len() > 400
    {
        return Err("Review 1–400 channel-bound attachment matches before saving.".into());
    }
    if review.prospective.is_some() {
        return Err(
            "Prospective attachments must be applied to their immutable upload plan before intake."
                .into(),
        );
    }
    let mut declarations = Vec::new();
    let mut seen = HashSet::new();
    for binding in bindings {
        let (asset, proof) = review
            .assets
            .iter()
            .find(|(asset, _)| asset.asset_id == binding.asset_id)
            .ok_or("An attachment does not belong to this reviewed selection.")?;
        let proof = proof
            .as_ref()
            .ok_or("Remove or replace invalid attachments before saving.")?;
        if !review
            .items
            .iter()
            .any(|item| item.item_id == binding.item_id)
        {
            return Err("A target video is outside the reviewed selection.".into());
        }
        let (language, name, draft) = caption_declaration(&binding, &asset.kind)?;
        let target = format!("{}:{}:{}:{}", binding.item_id, asset.kind, language, name);
        if !seen.insert(target) {
            return Err("Each video can have one reviewed thumbnail and one caption per language/name declaration.".into());
        }
        declarations.push((
            binding.item_id.clone(),
            asset.clone(),
            proof.clone(),
            language,
            name,
            draft,
        ));
    }
    for (_, _, proof, _, _, _) in &declarations {
        drop(verify(proof)?)
    }
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let ids = review
        .items
        .iter()
        .map(|item| item.item_id.clone())
        .collect::<Vec<_>>();
    let current = selected_items(&transaction, channel, &ids)?;
    let mut job_ids = Vec::new();
    for (item_id, asset, proof, language, name, draft) in declarations {
        let item = current.iter().find(|item| item.item_id == item_id).unwrap();
        let reviewed = review
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .unwrap();
        if reviewed.video_id.is_some() && reviewed.video_id != item.video_id {
            return Err("A confirmed video identity changed. Review its attachments again.".into());
        }
        let declaration = blake3::hash(
            serde_json::to_string(&(
                &asset.kind,
                &proof.digest,
                &proof.path,
                &proof.identity,
                &proof.signature,
                &language,
                &name,
                draft,
            ))
            .map_err(user_error)?
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        if let Some(id)=transaction.query_row("SELECT id FROM video_asset_jobs WHERE channel_id=?1 AND item_id=?2 AND declaration_key=?3",params![channel,item_id,declaration],|row|row.get::<_,String>(0)).optional().map_err(user_error)?{job_ids.push(id);continue}
        let active:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM video_asset_jobs WHERE channel_id=?1 AND item_id=?2 AND kind=?3 AND (?3='thumbnail' OR (language=?4 AND track_name=?5)) AND status IN ('waiting_upload','queued','waiting_permission','running','needs_reconciliation','waiting_processing'))",params![channel,item_id,asset.kind,language,name],|row|row.get(0)).map_err(user_error)?;
        if active {
            return Err("A job for this thumbnail or caption declaration is still pending or uncertain. Reconcile it before creating another.".into());
        }
        if asset.kind == "thumbnail" {
            transaction.execute("UPDATE video_asset_jobs SET status='superseded',detail='A later explicitly reviewed thumbnail superseded this completed or failed declaration; its receipt is retained',updated_at=?1 WHERE channel_id=?2 AND item_id=?3 AND kind='thumbnail' AND status IN ('succeeded','failed','conflict')",params![now(),channel,item_id]).map_err(user_error)?;
        }
        let id = Uuid::new_v4().to_string();
        let video = item.video_id.clone().filter(|_| item.status == "uploaded");
        let status = if video.is_some() {
            "queued"
        } else {
            "waiting_upload"
        };
        transaction.execute("INSERT INTO video_asset_jobs(id,item_id,channel_id,video_id,kind,source_path,file_name,source_size,source_modified,source_identity,source_digest,language,track_name,is_draft,declaration_key,status,detail,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,'Reviewed attachment retained separately from the video upload',?17,?17)",params![id,item_id,channel,video,asset.kind,proof.path.to_string_lossy(),asset.file_name,proof.signature.0,proof.signature.1,proof.identity,proof.digest,language,name,draft,declaration,status,now()]).map_err(user_error)?;
        asset_audit(&transaction,&item_id,channel,"video_asset_job_requested","Operator approved a verified original attachment; video upload bytes, receipt and checkpoints retained")?;
        job_ids.push(id)
    }
    transaction.commit().map_err(user_error)?;
    reviews().lock().map_err(user_error)?.remove(review_id);
    Ok(AssetReceipt {
        channel_id: channel.into(),
        job_ids,
    })
}

const JOB_SELECT:&str="SELECT id,item_id,channel_id,video_id,kind,file_name,language,track_name,is_draft,status,detail,caption_id,updated_at,source_path,source_size,source_modified,source_identity,source_digest,claim_id,baseline_json,check_count FROM video_asset_jobs";
#[derive(Clone)]
struct Job {
    view: AssetJob,
    proof: Proof,
    claim: Option<String>,
    baseline: Option<Vec<CaptionTrack>>,
    checks: u32,
}
fn job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let kind: String = row.get(4)?;
    let baseline: Option<String> = row.get(19)?;
    Ok(Job {
        view: AssetJob {
            job_id: row.get(0)?,
            item_id: row.get(1)?,
            channel_id: row.get(2)?,
            video_id: row.get(3)?,
            kind: kind.clone(),
            file_name: row.get(5)?,
            language: if kind == "caption" {
                Some(row.get(6)?)
            } else {
                None
            },
            name: if kind == "caption" {
                Some(row.get(7)?)
            } else {
                None
            },
            draft: if kind == "caption" {
                Some(row.get::<_, bool>(8)?)
            } else {
                None
            },
            status: row.get(9)?,
            detail: row.get(10)?,
            caption_id: row.get(11)?,
            updated_at: row.get(12)?,
        },
        proof: Proof {
            path: PathBuf::from(row.get::<_, String>(13)?),
            signature: (row.get(14)?, row.get(15)?),
            identity: row.get(16)?,
            digest: row.get(17)?,
        },
        claim: row.get(18)?,
        baseline: baseline.and_then(|json| serde_json::from_str(&json).ok()),
        checks: row.get(20)?,
    })
}
fn load_job(connection: &Connection, id: &str, channel: &str) -> Result<Job, String> {
    connection
        .query_row(
            &format!("{JOB_SELECT} WHERE id=?1 AND channel_id=?2"),
            params![id, channel],
            job_row,
        )
        .map_err(|_| "This attachment job is unavailable for the reviewed channel.".into())
}
fn load_jobs(
    connection: &Connection,
    channel: &str,
    ids: &[String],
) -> Result<Vec<AssetJob>, String> {
    let placeholders = (0..ids.len())
        .map(|index| format!("?{}", index + 2))
        .collect::<Vec<_>>()
        .join(",");
    let mut values = vec![channel.to_string()];
    values.extend(ids.iter().cloned());
    connection.prepare(&format!("{JOB_SELECT} WHERE channel_id=?1 AND item_id IN ({placeholders}) ORDER BY created_at DESC,id LIMIT 1000")).map_err(user_error)?.query_map(rusqlite::params_from_iter(values),job_row).map_err(user_error)?.map(|row|row.map(|job|job.view)).collect::<Result<Vec<_>,_>>().map_err(user_error)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptionTrack {
    id: String,
    video_id: String,
    language: String,
    name: String,
    draft: bool,
    status: String,
}
fn caption_track(value: &serde_json::Value) -> Result<CaptionTrack, String> {
    let text = |path: &str| {
        value
            .pointer(path)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "YouTube returned incomplete caption evidence.".to_string())
    };
    let track = CaptionTrack {
        id: text("/id")?,
        video_id: text("/snippet/videoId")?,
        language: text("/snippet/language")?,
        name: text("/snippet/name")?,
        draft: value
            .pointer("/snippet/isDraft")
            .and_then(serde_json::Value::as_bool)
            .ok_or("YouTube did not confirm the caption draft choice.")?,
        status: text("/snippet/status")?,
    };
    if track.id.is_empty()
        || track.id.len() > 512
        || track.id.chars().any(char::is_control)
        || !valid_id(&track.video_id)
    {
        return Err("YouTube returned an invalid caption identity.".into());
    }
    Ok(track)
}
fn same_track(job: &Job, track: &CaptionTrack) -> bool {
    job.view.video_id.as_deref() == Some(track.video_id.as_str())
        && job
            .view
            .language
            .as_ref()
            .is_some_and(|language| language.eq_ignore_ascii_case(&track.language))
        && job.view.name.as_deref() == Some(track.name.as_str())
}
struct WriteFailure {
    detail: String,
    uncertain: bool,
}
trait AssetProvider {
    fn ensure(&mut self, channel: &str, kind: &str) -> Result<(), String>;
    fn owner(&mut self, video: &str) -> Result<String, String>;
    fn tracks(&mut self, video: &str, refresh: bool) -> Result<Vec<CaptionTrack>, String>;
    fn thumbnail(&mut self, job: &Job, file: File) -> Result<(), WriteFailure>;
    fn insert_caption(&mut self, job: &Job, file: File) -> Result<CaptionTrack, WriteFailure>;
}

/// Evidence is shared only by jobs for one item and permission kind while the
/// existing execution lock fences the run. No proof survives this run.
struct RunProvider<P> {
    inner: P,
    owners: HashMap<String, String>,
    tracks: HashMap<String, Vec<CaptionTrack>>,
}
impl<P> RunProvider<P> {
    fn new(inner: P) -> Self {
        Self {
            inner,
            owners: HashMap::new(),
            tracks: HashMap::new(),
        }
    }
}
impl<P: AssetProvider> AssetProvider for RunProvider<P> {
    fn ensure(&mut self, channel: &str, kind: &str) -> Result<(), String> {
        // Keep fresh local channel, permission expiry and quota checks even
        // when the provider's remote proof has already succeeded in this run.
        self.inner.ensure(channel, kind)
    }
    fn owner(&mut self, video: &str) -> Result<String, String> {
        if let Some(owner) = self.owners.get(video) {
            return Ok(owner.clone());
        }
        let owner = self.inner.owner(video)?;
        self.owners.insert(video.into(), owner.clone());
        Ok(owner)
    }
    fn tracks(&mut self, video: &str, refresh: bool) -> Result<Vec<CaptionTrack>, String> {
        if refresh {
            self.tracks.remove(video);
        } else if let Some(tracks) = self.tracks.get(video) {
            return Ok(tracks.clone());
        }
        let tracks = self.inner.tracks(video, refresh)?;
        self.tracks.insert(video.into(), tracks.clone());
        Ok(tracks)
    }
    fn thumbnail(&mut self, job: &Job, file: File) -> Result<(), WriteFailure> {
        self.inner.thumbnail(job, file)
    }
    fn insert_caption(&mut self, job: &Job, file: File) -> Result<CaptionTrack, WriteFailure> {
        let video = job.view.video_id.as_deref().unwrap_or_default();
        let result = self.inner.insert_caption(job, file);
        match &result {
            Ok(track)
                if same_track(job, track)
                    && job.view.draft == Some(track.draft)
                    && self.tracks.get(video).is_some_and(|tracks| {
                        !tracks.iter().any(|existing| existing.id == track.id)
                    }) =>
            {
                self.tracks.get_mut(video).unwrap().push(track.clone());
            }
            // Unknown outcomes, duplicate conflicts and incomplete receipts
            // must never leave a pre-write membership snapshot reusable.
            _ => {
                self.tracks.remove(video);
            }
        }
        result
    }
}

fn process_run_jobs<P: AssetProvider>(
    state: &AppState,
    channel: &str,
    jobs: impl IntoIterator<Item = (String, String, String)>,
    repair: bool,
    mut create: impl FnMut(&str, &str) -> Result<P, String>,
) -> Result<u64, String> {
    let mut providers = HashMap::new();
    let mut written = 0;
    for (id, kind, item_id) in jobs {
        let key = (item_id, kind);
        if !providers.contains_key(&key) {
            providers.insert(key.clone(), RunProvider::new(create(&key.0, &key.1)?));
        }
        if process_with(
            state,
            &id,
            channel,
            repair,
            providers.get_mut(&key).unwrap(),
        )? {
            written += 1;
        }
    }
    Ok(written)
}

fn outcome(
    state: &AppState,
    job: &Job,
    status: &str,
    detail: &str,
    caption: Option<&CaptionTrack>,
    baseline: Option<&[CaptionTrack]>,
) -> Result<(), String> {
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    let owned:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM upload_items WHERE id=?1 AND channel_id=?2 AND video_id=?3 AND status='uploaded')",params![job.view.item_id,job.view.channel_id,job.view.video_id],|row|row.get(0)).map_err(user_error)?;
    if !owned {
        return Err(
            "The confirmed video identity changed; attachment receipt is retained for review."
                .into(),
        );
    }
    let check_at = if status == "waiting_processing" {
        Some(
            (Utc::now() + ChronoDuration::seconds((60_i64 << job.checks.min(5)).min(1800)))
                .to_rfc3339(),
        )
    } else {
        None
    };
    let receipt = if matches!(status, "succeeded" | "waiting_processing") {
        Some(serde_json::to_string(&serde_json::json!({"kind":job.view.kind,"videoId":job.view.video_id,"assetDigest":job.proof.digest,"captionId":caption.map(|track|track.id.as_str()),"providerReadConfirmed":true})).map_err(user_error)?)
    } else {
        None
    };
    let updated=transaction.execute("UPDATE video_asset_jobs SET status=?1,detail=?2,caption_id=COALESCE(?3,caption_id),receipt_json=COALESCE(?4,receipt_json),baseline_json=COALESCE(?5,baseline_json),next_check_at=?6,check_count=CASE WHEN ?1='waiting_processing' THEN check_count+1 ELSE check_count END,claim_id=NULL,updated_at=?7 WHERE id=?8 AND item_id=?9 AND channel_id=?10 AND video_id=?11 AND claim_id IS ?12",params![status,detail,caption.map(|track|track.id.as_str()),receipt,baseline.map(serde_json::to_string).transpose().map_err(user_error)?,check_at,now(),job.view.job_id,job.view.item_id,job.view.channel_id,job.view.video_id,job.claim]).map_err(user_error)?;
    if updated != 1 {
        return Err("The attachment claim changed. Its existing receipt was retained.".into());
    }
    transaction.execute("INSERT INTO video_asset_attempts(id,job_id,item_id,channel_id,video_id,operation,outcome,detail,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![Uuid::new_v4().to_string(),job.view.job_id,job.view.item_id,job.view.channel_id,job.view.video_id,job.view.kind,status,detail,now()]).map_err(user_error)?;
    asset_audit(
        &transaction,
        &job.view.item_id,
        &job.view.channel_id,
        "video_asset_job_result",
        detail,
    )?;
    transaction.commit().map_err(user_error)
}
fn caption_outcome(state: &AppState, job: &Job, track: &CaptionTrack) -> Result<(), String> {
    if !same_track(job, track) || job.view.draft != Some(track.draft) {
        return outcome(state,job,"needs_reconciliation","YouTube returned caption choices different from the review; no repeat write is allowed",None,None);
    }
    match track.status.as_str(){"serving"=>outcome(state,job,"succeeded","YouTube confirmed the requested caption track and its reviewed choices",Some(track),None),"failed"=>outcome(state,job,"conflict","The inserted caption track failed processing; no automatic replacement or deletion is allowed",Some(track),None),"syncing" if job.checks<8=>outcome(state,job,"waiting_processing","Caption insertion has a retained provider ID; processing confirmation is pending",Some(track),None),_=>outcome(state,job,"needs_reconciliation","Caption processing is not confirmed; reread the retained track before any action",Some(track),None)}
}
fn claim(state: &AppState, channel: &str, id: &str, repair: bool) -> Result<Option<Job>, String> {
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    ensure_channel(&transaction, channel)?;
    let job = load_job(&transaction, id, channel)?;
    let eligible = matches!(
        job.view.status.as_str(),
        "waiting_upload" | "queued" | "waiting_permission" | "waiting_processing"
    ) || (repair
        && matches!(job.view.status.as_str(), "failed" | "needs_reconciliation"));
    if !eligible {
        return Ok(None);
    }
    let video: Option<String> = transaction
        .query_row(
            "SELECT video_id FROM upload_items WHERE id=?1 AND channel_id=?2 AND status='uploaded'",
            params![job.view.item_id, channel],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?
        .flatten();
    let Some(video) = video else { return Ok(None) };
    if job.view.video_id.as_ref().is_some_and(|old| old != &video) || !valid_id(&video) {
        return Err(
            "The attachment's confirmed YouTube video changed. Its saved evidence is retained."
                .into(),
        );
    }
    let claim = Uuid::new_v4().to_string();
    transaction.execute("UPDATE video_asset_jobs SET video_id=?1,claim_id=?2,status='running',write_started=CASE WHEN status IN ('needs_reconciliation','waiting_processing') THEN write_started ELSE 0 END,updated_at=?3 WHERE id=?4 AND channel_id=?5 AND status=?6",params![video,claim,now(),id,channel,job.view.status]).map_err(user_error)?;
    asset_audit(
        &transaction,
        &job.view.item_id,
        channel,
        "video_asset_job_claimed",
        "A separate attachment job claimed the confirmed video; video upload receipt retained",
    )?;
    transaction.commit().map_err(user_error)?;
    let mut claimed = load_job(&connection, id, channel)?;
    claimed.view.status = job.view.status;
    Ok(Some(claimed))
}
fn write_claim(
    state: &AppState,
    job: &Job,
    baseline: Option<&[CaptionTrack]>,
) -> Result<(), String> {
    let mut connection = database(state)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(user_error)?;
    ensure_channel(&transaction, &job.view.channel_id)?;
    if transaction.execute("UPDATE video_asset_jobs SET write_started=1,baseline_json=COALESCE(?1,baseline_json),updated_at=?2 WHERE id=?3 AND channel_id=?4 AND item_id=?5 AND video_id=?6 AND claim_id=?7 AND status='running'",params![baseline.map(serde_json::to_string).transpose().map_err(user_error)?,now(),job.view.job_id,job.view.channel_id,job.view.item_id,job.view.video_id,job.claim]).map_err(user_error)?!=1{return Err("The attachment claim changed before its write. No additional write was approved.".into())}
    transaction.execute("INSERT INTO video_asset_attempts(id,job_id,item_id,channel_id,video_id,operation,outcome,detail,created_at) VALUES(?1,?2,?3,?4,?5,?6,'write_started','A provider write was durably claimed after source, owner and caption baseline checks',?7)",params![Uuid::new_v4().to_string(),job.view.job_id,job.view.item_id,job.view.channel_id,job.view.video_id,job.view.kind,now()]).map_err(user_error)?;
    transaction.commit().map_err(user_error)
}
fn process_with(
    state: &AppState,
    id: &str,
    channel: &str,
    repair: bool,
    provider: &mut impl AssetProvider,
) -> Result<bool, String> {
    let Some(job) = claim(state, channel, id, repair)? else {
        return Ok(false);
    };
    let reconcile =
        job.view.status == "needs_reconciliation" || job.view.status == "waiting_processing";
    if let Err(error) = provider.ensure(channel, &job.view.kind) {
        outcome(state, &job, "waiting_permission", &error, None, None)?;
        return Ok(false);
    }
    let video = job.view.video_id.as_deref().unwrap();
    if provider.owner(video).as_deref() != Ok(channel) {
        outcome(
            state,
            &job,
            "failed",
            "YouTube did not confirm this video's immutable channel; no attachment was sent",
            None,
            None,
        )?;
        return Ok(false);
    }
    if job.view.kind == "caption" {
        let tracks = match provider.tracks(video, reconcile) {
            Ok(tracks) => tracks,
            Err(_) => {
                outcome(
                    state,
                    &job,
                    if reconcile {
                        "needs_reconciliation"
                    } else {
                        "failed"
                    },
                    "Current caption tracks could not be read; no insertion was sent",
                    None,
                    None,
                )?;
                return Ok(false);
            }
        };
        let matching = tracks
            .iter()
            .filter(|track| same_track(&job, track))
            .collect::<Vec<_>>();
        if let Some(id) = job.view.caption_id.as_deref() {
            if let Some(track) = tracks.iter().find(|track| track.id == id) {
                caption_outcome(state, &job, track)?;
                return Ok(false);
            }
            outcome(state,&job,"needs_reconciliation","The retained caption ID is missing from current tracks; no insertion or replacement was sent",None,None)?;
            return Ok(false);
        }
        if reconcile {
            let Some(baseline) = job.baseline.as_ref() else {
                outcome(
                    state,
                    &job,
                    "needs_reconciliation",
                    "The interrupted caption has no complete baseline; review remains blocked",
                    None,
                    None,
                )?;
                return Ok(false);
            };
            let new = matching
                .iter()
                .filter(|track| !baseline.iter().any(|old| old.id == track.id))
                .collect::<Vec<_>>();
            if matching.len() == 1 && new.len() == 1 && job.view.draft == Some(new[0].draft) {
                caption_outcome(state, &job, new[0])?
            } else {
                outcome(state,&job,"needs_reconciliation","Caption outcome remains absent or ambiguous; duplicate insertion, replacement and deletion are blocked",None,None)?
            }
            return Ok(false);
        }
        if !matching.is_empty() {
            outcome(state,&job,"conflict","A caption with this language/name already exists. Choose another explicit name; no existing track was replaced",None,Some(&tracks))?;
            return Ok(false);
        }
        let source = match verify(&job.proof) {
            Ok(source) => source,
            Err(error) => {
                outcome(state, &job, "failed", &error, None, None)?;
                return Ok(false);
            }
        };
        if let Err(error) = provider.ensure(channel, "caption") {
            outcome(state, &job, "waiting_permission", &error, None, None)?;
            return Ok(false);
        }
        write_claim(state, &job, Some(&tracks))?;
        match provider.insert_caption(&job, source) {
            Ok(track) => caption_outcome(state, &job, &track)?,
            Err(error) => outcome(
                state,
                &job,
                if error.uncertain {
                    "needs_reconciliation"
                } else {
                    "failed"
                },
                &error.detail,
                None,
                Some(&tracks),
            )?,
        };
        return Ok(true);
    }
    let source = match verify(&job.proof) {
        Ok(source) => source,
        Err(error) => {
            outcome(state, &job, "failed", &error, None, None)?;
            return Ok(false);
        }
    };
    if let Err(error) = provider.ensure(channel, "thumbnail") {
        outcome(state, &job, "waiting_permission", &error, None, None)?;
        return Ok(false);
    }
    write_claim(state, &job, None)?;
    match provider.thumbnail(&job, source) {
        Ok(()) => outcome(
            state,
            &job,
            "succeeded",
            "YouTube accepted the reviewed original thumbnail; video bytes and receipt retained",
            None,
            None,
        )?,
        Err(error) => outcome(
            state,
            &job,
            if error.uncertain {
                "needs_reconciliation"
            } else {
                "failed"
            },
            &error.detail,
            None,
            None,
        )?,
    };
    Ok(true)
}

struct StreamProof {
    file: File,
    path: PathBuf,
    expected: Proof,
    remaining: u64,
    hasher: blake3::Hasher,
    verified: Arc<AtomicBool>,
    permit: media_runtime::MediaReadGuard<'static>,
}
impl Read for StreamProof {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        self.permit
            .yield_to_upload(MediaIoPriority::Background, &|| false)
            .map_err(std::io::Error::other)?;
        let limit = buffer.len().min(self.remaining as usize);
        if limit == 0 {
            return Ok(0);
        }
        let count = self.file.read(&mut buffer[..limit])?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Attachment ended before its reviewed byte length",
            ));
        }
        self.hasher.update(&buffer[..count]);
        self.remaining -= count as u64;
        if self.remaining == 0 {
            let current = open_reference_source(&self.path).map_err(std::io::Error::other)?;
            if self.hasher.finalize().to_hex().as_str() != self.expected.digest
                || monitored_file_signature(&self.file.metadata()?)
                    .map_err(std::io::Error::other)?
                    != self.expected.signature
                || asset_identity(&current).map_err(std::io::Error::other)?
                    != self.expected.identity
            {
                return Err(std::io::Error::other(
                    "Attachment changed during its provider write",
                ));
            }
            self.verified.store(true, Ordering::Release)
        }
        Ok(count)
    }
}
struct NativeProvider<'a> {
    state: &'a AppState,
    channel: String,
    kind: String,
    token: Option<String>,
    channel_verified: bool,
    quota_scope: String,
    item_id: String,
}
fn permission_kind(
    settings: &ConnectionSettings,
    kind: &str,
) -> Result<provider_transport::AccessTokenKind, String> {
    match kind{"thumbnail"=>Ok(provider_transport::AccessTokenKind::Upload),"caption" if settings.deletion_authorized&&settings.deletion_sudo_active=>Ok(provider_transport::AccessTokenKind::Deletion),"caption"=>Err("Grant separate caption-management permission and enable its 15-minute mode before caption writes.".into()),_=>Err("Choose a supported thumbnail or caption job.".into())}
}
impl<'a> NativeProvider<'a> {
    fn new(state: &'a AppState, channel: &str, kind: &str, item_id: &str) -> Result<Self, String> {
        let connection = database(state)?;
        let scope = oauth_replacement::configured_quota_scope(&connection)?;
        Ok(Self {
            state,
            channel: channel.into(),
            kind: kind.into(),
            token: None,
            channel_verified: false,
            quota_scope: scope,
            item_id: item_id.into(),
        })
    }
    fn read(&self, resource: &str, query: &[(&str, &str)]) -> Result<serde_json::Value, String> {
        let request = provider_transport(self.state)
            .control_client()?
            .get(format!("https://www.googleapis.com/youtube/v3/{resource}"))
            .bearer_auth(
                self.token
                    .as_deref()
                    .ok_or("The attachment permission is unavailable.")?,
            )
            .query(query);
        let response =
            observed_youtube_request(self.state, &format!("{resource}.list"), || request.send())
                .map_err(|_| {
                    "Current attachment ownership or track evidence could not be reached."
                })?;
        let status = response.status();
        let mut body = String::new();
        response
            .take(1_048_577)
            .read_to_string(&mut body)
            .map_err(|_| "Current attachment evidence could not be read.")?;
        if body.len() > 1_048_576 {
            return Err(
                "Current attachment evidence exceeds safe read bounds; no write was sent.".into(),
            );
        }
        if !status.is_success() {
            let _ = self.record_limit(status.as_u16(), &body);
            return Err("YouTube rejected the attachment evidence read; permission or quota may need attention. No asset was sent.".into());
        }
        serde_json::from_str(&body).map_err(|_| {
            "YouTube returned incomplete attachment evidence; no write was sent.".into()
        })
    }
    fn record_limit(&self, status: u16, body: &str) -> Result<(), String> {
        if youtube_limits::classify(status, body) != Some(youtube_limits::UploadLimitKind::ApiQuota)
        {
            return Ok(());
        }
        oauth_replacement::record_scoped_api_pause(
            self.state,
            &self.channel,
            &self.quota_scope,
            Some(&self.item_id),
            youtube_limits::includes_channel_limit(status, body),
        )?;
        Ok(())
    }
    fn send(
        &self,
        job: &Job,
        file: File,
        caption: bool,
    ) -> Result<serde_json::Value, WriteFailure> {
        let failure = |detail: &str, uncertain| WriteFailure {
            detail: detail.into(),
            uncertain,
        };
        let verified = Arc::new(AtomicBool::new(false));
        let permit = media_runtime::acquire_reader(&job.proof.path, &|| false).map_err(|_| {
            failure(
                "Attachment read admission is unavailable; no provider write was sent",
                false,
            )
        })?;
        let reader = StreamProof {
            file,
            path: job.proof.path.clone(),
            expected: job.proof.clone(),
            remaining: job.proof.signature.0,
            hasher: blake3::Hasher::new(),
            verified: verified.clone(),
            permit,
        };
        let client = provider_transport(self.state)
            .upload_client()
            .map_err(|_| {
                failure(
                    "Attachment transfer is unavailable; no provider write was sent",
                    false,
                )
            })?;
        let request = if caption {
            let boundary = format!("youtube-assets-{}", Uuid::new_v4());
            let metadata = serde_json::json!({"snippet":{"videoId":job.view.video_id,"language":job.view.language,"name":job.view.name,"isDraft":job.view.draft}});
            let header=format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n");
            let footer = format!("\r\n--{boundary}--\r\n");
            let length = header.len() as u64 + job.proof.signature.0 + footer.len() as u64;
            client
                .post("https://www.googleapis.com/upload/youtube/v3/captions")
                .query(&[("part", "snippet"), ("uploadType", "multipart")])
                .header(
                    reqwest::header::CONTENT_TYPE,
                    format!("multipart/related; boundary={boundary}"),
                )
                .body(reqwest::blocking::Body::sized(
                    Cursor::new(header.into_bytes())
                        .chain(reader)
                        .chain(Cursor::new(footer.into_bytes())),
                    length,
                ))
        } else {
            let mime = if job
                .proof
                .path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
            {
                "image/png"
            } else {
                "image/jpeg"
            };
            client
                .post("https://www.googleapis.com/upload/youtube/v3/thumbnails/set")
                .query(&[
                    ("videoId", job.view.video_id.as_deref().unwrap()),
                    ("uploadType", "media"),
                ])
                .header(reqwest::header::CONTENT_TYPE, mime)
                .body(reqwest::blocking::Body::sized(
                    reader,
                    job.proof.signature.0,
                ))
        };
        let response = observed_youtube_request(
            self.state,
            if caption {
                "captions.insert"
            } else {
                "thumbnails.set"
            },
            || {
                request
                    .bearer_auth(self.token.as_deref().unwrap_or(""))
                    .send()
            },
        )
        .map_err(|_| {
            failure(
                "Attachment request outcome is unknown; its durable claim requires reconciliation",
                true,
            )
        })?;
        let status = response.status();
        let mut body = String::new();
        response
            .take(1_048_577)
            .read_to_string(&mut body)
            .map_err(|_| {
                failure(
                    "YouTube's attachment response could not be read; reconciliation is required",
                    true,
                )
            })?;
        if body.len() > 1_048_576 {
            return Err(failure(
                "YouTube's attachment response exceeded safe bounds; reconciliation is required",
                true,
            ));
        }
        if !status.is_success() {
            let _ = self.record_limit(status.as_u16(), &body);
            let uncertain = status.is_server_error() || status.is_redirection();
            return Err(failure(
                if status.as_u16() == 409 {
                    "YouTube reported an existing caption declaration; read its tracks before any retry"
                } else if status.as_u16() == 403 {
                    "YouTube rejected this attachment's permission, eligibility or quota; the video receipt is retained"
                } else {
                    "YouTube rejected the attachment; its video receipt and original file are retained"
                },
                uncertain,
            ));
        }
        if !verified.load(Ordering::Acquire) {
            return Err(failure("The sent attachment bytes could not be confirmed against the review; reconciliation is required",true));
        }
        serde_json::from_str(&body).map_err(|_|failure("YouTube accepted the write but returned no usable receipt; reconciliation is required",true))
    }
}
impl AssetProvider for NativeProvider<'_> {
    fn ensure(&mut self, channel: &str, kind: &str) -> Result<(), String> {
        if channel != self.channel || kind != self.kind {
            return Err("The attachment scope changed.".into());
        }
        let connection = database(self.state)?;
        let settings = ensure_channel(&connection, channel)?;
        if oauth_replacement::active_api_pause(&connection)?.is_some() {
            return Err(
                "The saved Cloud API pause retains this attachment for later review.".into(),
            );
        }
        let token_kind = permission_kind(&settings, kind)?;
        if self.token.is_none() {
            self.token = Some(
                if matches!(token_kind, provider_transport::AccessTokenKind::Deletion) {
                    refreshed_access_token_for(
                        self.state,
                        provider_transport::AccessTokenKind::Deletion,
                        deletion_refresh_token_entry(),
                        "Caption-management permission is unavailable; grant it again.",
                    )?
                } else {
                    refreshed_access_token(self.state)?
                },
            );
        }
        if !self.channel_verified {
            let own = self.read("channels", &[("part", "snippet"), ("mine", "true")])?;
            if !own
                .get("items")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|items| {
                    items.iter().any(|item| {
                        item.get("id").and_then(serde_json::Value::as_str) == Some(channel)
                    })
                })
            {
                return Err(
                    "Google did not confirm the reviewed caption/thumbnail channel.".into(),
                );
            }
            self.channel_verified = true;
        }
        Ok(())
    }
    fn owner(&mut self, video: &str) -> Result<String, String> {
        let value = self.read("videos", &[("part", "snippet"), ("id", video)])?;
        value
            .get("items")
            .and_then(serde_json::Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("id").and_then(serde_json::Value::as_str) == Some(video))
            })
            .and_then(|item| item.pointer("/snippet/channelId"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "YouTube did not confirm the target video's owner.".into())
    }
    fn tracks(&mut self, video: &str, _refresh: bool) -> Result<Vec<CaptionTrack>, String> {
        let value = self.read("captions", &[("part", "snippet"), ("videoId", video)])?;
        let items = value
            .get("items")
            .and_then(serde_json::Value::as_array)
            .ok_or("YouTube did not return complete caption membership.")?;
        if items.len() > 5000
            || value
                .get("nextPageToken")
                .is_some_and(|value| !value.is_null())
        {
            return Err(
                "Caption membership exceeds safe complete-read bounds; no insertion was sent."
                    .into(),
            );
        }
        let tracks = items
            .iter()
            .map(caption_track)
            .collect::<Result<Vec<_>, _>>()?;
        if tracks.iter().any(|track| track.video_id != video)
            || tracks
                .iter()
                .map(|track| &track.id)
                .collect::<HashSet<_>>()
                .len()
                != tracks.len()
        {
            return Err(
                "YouTube returned inconsistent caption membership; no insertion was sent.".into(),
            );
        }
        Ok(tracks)
    }
    fn thumbnail(&mut self, job: &Job, file: File) -> Result<(), WriteFailure> {
        let value = self.send(job, file, false)?;
        if !value
            .get("items")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| {
                !items.is_empty()
                    && items.iter().all(|item| {
                        item.as_object().is_some_and(|sizes| {
                            sizes.values().any(|size| {
                                size.get("url")
                                    .and_then(serde_json::Value::as_str)
                                    .is_some_and(|url| url.starts_with("https://"))
                            })
                        })
                    })
            })
        {
            return Err(WriteFailure{detail:"YouTube accepted the thumbnail request but its receipt is incomplete; review before reapplying".into(),uncertain:true});
        }
        Ok(())
    }
    fn insert_caption(&mut self, job: &Job, file: File) -> Result<CaptionTrack, WriteFailure> {
        let value = self.send(job, file, true)?;
        caption_track(&value).map_err(|_|WriteFailure{detail:"Caption insertion was accepted without a usable identity; reconcile membership before any retry".into(),uncertain:true})
    }
}

/// Startup classification is database-only. An interrupted claim that had not
/// begun its write can resume; an admitted write is retained as uncertain.
pub(super) fn recover_interrupted(connection: &Connection) -> Result<(), String> {
    connection.execute("UPDATE video_asset_jobs SET status=CASE WHEN write_started=1 THEN 'needs_reconciliation' ELSE 'queued' END,detail=CASE WHEN write_started=1 THEN 'Attachment write was interrupted; reconcile before any repeat insertion' ELSE 'Interrupted pre-write attachment claim returned to its reviewed queue' END,claim_id=NULL,next_check_at=NULL,updated_at=?1 WHERE status='running'",[now()]).map_err(user_error)?;
    Ok(())
}
pub(super) fn pending_item_ids(state: &AppState) -> Result<Vec<String>, String> {
    let connection = database(state)?;
    let settings = connection_settings(&connection)?;
    let Some(channel) = settings.active_channel_id.filter(|_| settings.connected) else {
        return Ok(Vec::new());
    };
    if oauth_replacement::active_api_pause(&connection)?.is_some() {
        return Ok(Vec::new());
    }
    let captions = settings.deletion_authorized && settings.deletion_sudo_active;
    let pending = connection.prepare("SELECT DISTINCT a.item_id FROM video_asset_jobs a JOIN upload_items u ON u.id=a.item_id AND u.channel_id=a.channel_id WHERE a.channel_id=?1 AND u.status='uploaded' AND u.video_id IS NOT NULL AND (a.video_id IS NULL OR a.video_id=u.video_id) AND (a.kind='thumbnail' OR ?2) AND (a.status IN ('queued','waiting_upload','waiting_permission') OR (a.status='waiting_processing' AND a.next_check_at<=?3)) ORDER BY a.item_id LIMIT 50").map_err(user_error)?.query_map(params![channel,captions,now()],|row|row.get(0)).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    Ok(pending)
}
pub(super) fn next_deadline(
    connection: &Connection,
    channel: &str,
) -> Result<Option<String>, String> {
    connection.query_row("SELECT MIN(next_check_at) FROM video_asset_jobs WHERE channel_id=?1 AND status='waiting_processing'",[channel],|row|row.get(0)).map_err(user_error)
}
fn run_for_item(
    state: &AppState,
    channel: &str,
    item_id: &str,
    repair: bool,
    kind_filter: Option<&str>,
) -> Result<u64, String> {
    let connection = database(state)?;
    ensure_channel(&connection, channel)?;
    let ids=connection.prepare("SELECT id,kind FROM video_asset_jobs WHERE channel_id=?1 AND item_id=?2 AND (status IN ('waiting_upload','queued','waiting_permission') OR (status='waiting_processing' AND (next_check_at<=?3 OR ?4)) OR (?4 AND status IN ('failed','needs_reconciliation'))) ORDER BY created_at,id LIMIT 100").map_err(user_error)?.query_map(params![channel,item_id,now(),repair],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
    drop(connection);
    process_run_jobs(
        state,
        channel,
        ids.into_iter()
            .filter(|(_, kind)| !kind_filter.is_some_and(|selected| selected != kind))
            .map(|(id, kind)| (id, kind, item_id.to_string())),
        repair,
        |item, kind| NativeProvider::new(state, channel, kind, item),
    )
}
/// The lower-priority coordinator may call this before taking its existing
/// execution lock. Caption management and credential promotion share that lock.
pub(super) fn process_pending_for_item(state: &AppState, item_id: &str) -> Result<(), String> {
    let _guard = deletion_execution_lock(state)
        .lock()
        .map_err(|_| "Attachment management is temporarily unavailable.")?;
    {
        let _credentials = oauth_replacement::lock_credentials()?;
        oauth_replacement::ensure_no_pending(state)?;
    }
    let settings = connection_settings(&database(state)?)?;
    let Some(channel) = settings.active_channel_id.filter(|_| settings.connected) else {
        return Ok(());
    };
    run_for_item(state, &channel, item_id, false, None).map(|_| ())
}
fn reviewed_repair_jobs(
    connection: &Connection,
    channel: &str,
    item_id: &str,
    expected_video_id: &str,
    kind: &str,
    reviewed_job_ids: &[String],
) -> Result<Vec<Job>, String> {
    if !matches!(kind, "thumbnail" | "caption")
        || reviewed_job_ids.is_empty()
        || reviewed_job_ids.len() > 400
        || reviewed_job_ids.iter().any(|id| !valid_id(id))
        || reviewed_job_ids.iter().collect::<HashSet<_>>().len() != reviewed_job_ids.len()
    {
        return Err("Review exact saved thumbnail or caption jobs before repair.".into());
    }
    ensure_channel(connection, channel)?;
    let video_id: Option<String> = connection
        .query_row(
            "SELECT video_id FROM upload_items WHERE id=?1 AND channel_id=?2 AND status='uploaded'",
            params![item_id, channel],
            |row| row.get(0),
        )
        .optional()
        .map_err(user_error)?
        .flatten();
    if video_id.as_deref() != Some(expected_video_id) {
        return Err(
            "The confirmed video changed after repair review. Refresh its checklist.".into(),
        );
    }
    reviewed_job_ids
        .iter()
        .map(|id| {
            let job = load_job(connection, id, channel)?;
            if job.view.item_id != item_id
                || job.view.kind != kind
                || job.view.video_id.as_deref() != Some(expected_video_id)
                || job.view.status == "superseded"
            {
                return Err(
                    "The reviewed attachment jobs changed. Refresh their checklist before repair."
                        .into(),
                );
            }
            Ok(job)
        })
        .collect()
}
/// Completion approval carries these immutable job IDs and its confirmed video
/// receipt. Later attachments can never join an already-reviewed family repair.
pub(super) fn repair_job_ids(
    state: &AppState,
    channel: &str,
    item_id: &str,
    expected_video_id: &str,
    kind: &str,
    reviewed_job_ids: &[String],
) -> Result<u64, String> {
    let _guard = deletion_execution_lock(state)
        .lock()
        .map_err(|_| "Attachment management is temporarily unavailable.")?;
    repair_job_ids_locked(
        state,
        channel,
        item_id,
        expected_video_id,
        kind,
        reviewed_job_ids,
    )
}
/// The completion coordinator already owns the execution lock while validating
/// its immutable review. Callers must not hold the credential lock here.
pub(super) fn repair_job_ids_locked(
    state: &AppState,
    channel: &str,
    item_id: &str,
    expected_video_id: &str,
    kind: &str,
    reviewed_job_ids: &[String],
) -> Result<u64, String> {
    let jobs = reviewed_repair_jobs(
        &database(state)?,
        channel,
        item_id,
        expected_video_id,
        kind,
        reviewed_job_ids,
    )?;
    {
        let _credentials = oauth_replacement::lock_credentials()?;
        oauth_replacement::ensure_no_pending(state)?;
    }
    process_run_jobs(
        state,
        channel,
        jobs.into_iter()
            .map(|job| (job.view.job_id, job.view.kind, job.view.item_id)),
        true,
        |item, kind| NativeProvider::new(state, channel, kind, item),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetCompletionStep {
    pub(super) key: String,
    pub(super) outcome: String,
    pub(super) detail: String,
    pub(super) repairable: bool,
    pub(super) repair_job_ids: Vec<String>,
    pub(super) checked_at: Option<String>,
}
pub(super) fn completion_steps(
    connection: &Connection,
    channel: &str,
    item_id: &str,
) -> Result<Vec<AssetCompletionStep>, String> {
    let mut steps = Vec::new();
    for kind in ["thumbnail", "caption"] {
        let rows=connection.prepare("SELECT status,detail,updated_at,id FROM video_asset_jobs WHERE channel_id=?1 AND item_id=?2 AND kind=?3 AND status!='superseded' ORDER BY created_at,id").map_err(user_error)?.query_map(params![channel,item_id,kind],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?))).map_err(user_error)?.collect::<Result<Vec<_>,_>>().map_err(user_error)?;
        let outcome = if rows.is_empty() {
            "not_requested"
        } else if rows.iter().all(|row| row.0 == "succeeded") {
            "succeeded"
        } else if rows.iter().any(|row| row.0 == "needs_reconciliation") {
            "unknown"
        } else if rows
            .iter()
            .any(|row| matches!(row.0.as_str(), "failed" | "conflict"))
        {
            "failed"
        } else {
            "waiting"
        };
        let repair_job_ids: Vec<String> = rows
            .iter()
            .filter(|row| {
                matches!(
                    row.0.as_str(),
                    "failed" | "needs_reconciliation" | "waiting_permission" | "waiting_processing"
                )
            })
            .map(|row| row.3.clone())
            .collect();
        let repairable = !repair_job_ids.is_empty();
        let detail = if rows.is_empty() {
            format!("No {kind} attachment was requested.")
        } else {
            let succeeded = rows.iter().filter(|row| row.0 == "succeeded").count();
            format!(
                "{succeeded}/{} {kind} jobs confirmed. {}",
                rows.len(),
                rows.iter()
                    .find(|row| row.0 != "succeeded")
                    .map(|row| row.1.as_str())
                    .unwrap_or("Separate asset receipts retained; video upload remains confirmed.")
            )
        };
        steps.push(AssetCompletionStep {
            key: kind.into(),
            outcome: outcome.into(),
            detail,
            repairable,
            repair_job_ids,
            checked_at: rows.iter().map(|row| row.2.clone()).max(),
        });
    }
    Ok(steps)
}

#[tauri::command]
pub(super) fn load_upload_plan_asset_workspace(
    state: State<'_, AppState>,
    expected_channel_id: String,
    plan_id: String,
) -> Result<AssetWorkspace, String> {
    let (targets, _) = upload_plan::prospective_targets(&state, &plan_id, &expected_channel_id)?;
    Ok(AssetWorkspace {
        channel_id: expected_channel_id,
        items: targets.iter().map(ProspectiveAssetTarget::item).collect(),
        jobs: Vec::new(),
    })
}
#[tauri::command]
pub(super) async fn prepare_upload_plan_assets(
    state: State<'_, AppState>,
    expected_channel_id: String,
    plan_id: String,
    paths: Vec<String>,
    auto_match: Option<bool>,
) -> Result<AssetReview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        prepare_prospective(
            &state,
            &expected_channel_id,
            &plan_id,
            paths,
            auto_match.unwrap_or(false),
        )
    })
    .await
    .map_err(|_| "Prospective asset review stopped; no jobs were created.")?
}
#[tauri::command]
pub(super) async fn apply_upload_plan_assets(
    state: State<'_, AppState>,
    expected_channel_id: String,
    plan_id: String,
    review_id: String,
    bindings: Vec<AssetBinding>,
) -> Result<upload_plan::UploadPlanPage, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let assets =
            freeze_plan_assets(&state, &expected_channel_id, &plan_id, &review_id, bindings)?;
        let page = upload_plan::seal_assets(&state, &plan_id, &expected_channel_id, assets)?;
        reviews().lock().map_err(user_error)?.remove(&review_id);
        Ok(page)
    })
    .await
    .map_err(|_| "Prospective attachments were not applied; no jobs were created.")?
}
#[tauri::command]
pub(super) fn load_video_asset_workspace(
    state: State<'_, AppState>,
    expected_channel_id: String,
    item_ids: Vec<String>,
) -> Result<AssetWorkspace, String> {
    let connection = database(&state)?;
    let items = selected_items(&connection, &expected_channel_id, &item_ids)?;
    let jobs = load_jobs(&connection, &expected_channel_id, &item_ids)?;
    Ok(AssetWorkspace {
        channel_id: expected_channel_id,
        items,
        jobs,
    })
}
#[tauri::command]
pub(super) async fn prepare_video_assets(
    state: State<'_, AppState>,
    expected_channel_id: String,
    item_ids: Vec<String>,
    paths: Vec<String>,
    auto_match: Option<bool>,
) -> Result<AssetReview, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        prepare(
            &state,
            &expected_channel_id,
            item_ids,
            paths,
            auto_match.unwrap_or(false),
        )
    })
    .await
    .map_err(|_| "Attachment review stopped without creating jobs.")?
}
#[tauri::command]
pub(super) async fn save_video_asset_jobs(
    state: State<'_, AppState>,
    expected_channel_id: String,
    review_id: String,
    bindings: Vec<AssetBinding>,
) -> Result<AssetReceipt, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let receipt = save(&state, &expected_channel_id, &review_id, bindings)?;
        let _ = ensure_upload_postprocess_worker(state.clone());
        Ok(receipt)
    })
    .await
    .map_err(|_| {
        "Attachment jobs were not confirmed; refresh their retained receipts before retry."
    })?
}
#[tauri::command]
pub(super) fn discard_video_asset_review(
    state: State<'_, AppState>,
    expected_channel_id: String,
    review_id: String,
) -> Result<(), String> {
    let mut store = reviews().lock().map_err(user_error)?;
    if store.get(&review_id).is_some_and(|review| {
        review.database_path != state.database_path || review.channel != expected_channel_id
    }) {
        return Err("This asset review does not belong to the requested channel workspace.".into());
    }
    store.remove(&review_id);
    Ok(())
}
#[tauri::command]
pub(super) async fn repair_video_asset_jobs(
    state: State<'_, AppState>,
    expected_channel_id: String,
    job_ids: Vec<String>,
) -> Result<AssetReceipt, String> {
    if job_ids.is_empty()
        || job_ids.len() > 400
        || job_ids.iter().any(|id| !valid_id(id))
        || job_ids.iter().collect::<HashSet<_>>().len() != job_ids.len()
    {
        return Err("Choose 1–400 saved asset jobs to review for repair.".into());
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = database(&state)?;
        ensure_channel(&connection, &expected_channel_id)?;
        for id in &job_ids {
            load_job(&connection, id, &expected_channel_id)?;
        }
        drop(connection);
        let _guard = deletion_execution_lock(&state)
            .lock()
            .map_err(|_| "Attachment management is temporarily unavailable.")?;
        {
            let _credentials = oauth_replacement::lock_credentials()?;
            oauth_replacement::ensure_no_pending(&state)?;
        }
        let connection = database(&state)?;
        let jobs = job_ids
            .iter()
            .map(|id| {
                let job = load_job(&connection, id, &expected_channel_id)?;
                Ok((job.view.job_id, job.view.kind, job.view.item_id))
            })
            .collect::<Result<Vec<_>, String>>()?;
        drop(connection);
        process_run_jobs(&state, &expected_channel_id, jobs, true, |item, kind| {
            NativeProvider::new(&state, &expected_channel_id, kind, item)
        })?;
        Ok(AssetReceipt {
            channel_id: expected_channel_id,
            job_ids,
        })
    })
    .await
    .map_err(|_| "Asset repair stopped; the video and asset receipts were retained.")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    fn png() -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut chunk = |kind: &[u8], data: &[u8]| {
            bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
            bytes.extend_from_slice(kind);
            bytes.extend_from_slice(data);
            let mut check = kind.to_vec();
            check.extend_from_slice(data);
            bytes.extend_from_slice(&crc32(&check).to_be_bytes());
        };
        chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]);
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&[0, 255, 0, 0]).unwrap();
        chunk(b"IDAT", &encoder.finish().unwrap());
        chunk(b"IEND", &[]);
        bytes
    }
    fn jpeg() -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8];
        let mut segment = |marker: u8, data: Vec<u8>| {
            bytes.extend_from_slice(&[0xff, marker]);
            bytes.extend_from_slice(&((data.len() + 2) as u16).to_be_bytes());
            bytes.extend(data);
        };
        let mut quant = vec![0];
        quant.extend([1; 64]);
        segment(0xdb, quant);
        segment(0xc0, vec![8, 0, 1, 0, 1, 1, 1, 0x11, 0]);
        let mut table = vec![0, 1];
        table.extend([0; 15]);
        table.push(0);
        table.extend([0x10, 1]);
        table.extend([0; 15]);
        table.push(0);
        segment(0xc4, table);
        segment(0xda, vec![1, 1, 0, 0, 63, 0]);
        bytes.extend([0x3f, 0xff, 0xd9]);
        bytes
    }
    fn fixture() -> (PathBuf, AppState, Connection) {
        let root = std::env::temp_dir().join(format!("youtube-assets-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("media")).unwrap();
        let state = AppState {
            database_path: root.join("queue.sqlite3"),
            media_directory: root.join("media"),
            folder_monitor_lock: Arc::new(Mutex::new(())),
            oauth_attempts: Arc::new(Mutex::new(HashMap::new())),
        };
        let connection = initialize_database(&state).unwrap();
        connection.execute_batch(SCHEMA_SQL).unwrap();
        connection.execute("INSERT INTO connection_settings(singleton,oauth_client_id,oauth_project_id,active_channel,active_channel_id,updated_at) VALUES(1,'fixture.apps.googleusercontent.com','fixture-project','Fixture','channel-a',?1) ON CONFLICT(singleton) DO UPDATE SET oauth_client_id='fixture.apps.googleusercontent.com',oauth_project_id='fixture-project',active_channel='Fixture',active_channel_id='channel-a'",[now()]).unwrap();
        connection.execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,status,total_bytes,confirmed_bytes,video_id,created_at,updated_at) VALUES('item-a','Uploaded A','camera.mp4','channel-a','video-original','video-original',11,'uploaded',11,11,'video-a',?1,?1),('item-b','Queued B','camera.mp4','channel-a','queued-original','queued-original',11,'queued',11,0,NULL,?1,?1),('item-hidden','Other channel','hidden.mp4','channel-b','hidden','hidden',11,'uploaded',11,11,'hidden-video',?1,?1)",[now()]).unwrap();
        (root, state, connection)
    }
    fn source(root: &Path, name: &str, bytes: &[u8]) -> String {
        let path = root.join(name);
        fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }
    fn binding(asset: &str, item: &str, kind: &str) -> AssetBinding {
        AssetBinding {
            asset_id: asset.into(),
            item_id: item.into(),
            language: if kind == "caption" {
                Some("en".into())
            } else {
                None
            },
            name: if kind == "caption" {
                Some("English".into())
            } else {
                None
            },
            draft: if kind == "caption" { Some(false) } else { None },
        }
    }
    fn create(state: &AppState, path: String, item: &str) -> String {
        let review = prepare(state, "channel-a", vec![item.into()], vec![path], false).unwrap();
        let asset = &review.assets[0];
        save(
            state,
            "channel-a",
            &review.review_id,
            vec![binding(&asset.asset_id, item, &asset.kind)],
        )
        .unwrap()
        .job_ids[0]
            .clone()
    }
    fn caption_jobs(state: &AppState, root: &Path, names: &[&str]) -> Vec<String> {
        let review = prepare(
            state,
            "channel-a",
            vec!["item-a".into()],
            vec![source(
                root,
                "run-captions.vtt",
                b"WEBVTT\n\n00:00.000 --> 00:01.000\nCaption\n",
            )],
            false,
        )
        .unwrap();
        let bindings = names
            .iter()
            .map(|name| {
                let mut declaration = binding(&review.assets[0].asset_id, "item-a", "caption");
                declaration.name = Some((*name).into());
                declaration
            })
            .collect();
        save(state, "channel-a", &review.review_id, bindings)
            .unwrap()
            .job_ids
    }
    fn finish(root: PathBuf, state: AppState, connection: Connection) {
        upload_plan::test_discard_workspace(&state);
        reviews()
            .lock()
            .unwrap()
            .retain(|_, review| review.database_path != state.database_path);
        drop(connection);
        drop(state);
        fs::remove_dir_all(root).unwrap();
    }
    #[derive(Default)]
    struct ProviderCounts {
        builds: usize,
        ensures: usize,
        owners: usize,
        tracks: usize,
        writes: usize,
    }
    struct Mock<'a> {
        state: &'a AppState,
        allowed: bool,
        owner: String,
        tracks: Vec<CaptionTrack>,
        writes: usize,
        uncertain: bool,
        status: String,
        caption_insertions: usize,
        counts: Rc<RefCell<ProviderCounts>>,
    }
    impl Mock<'_> {
        fn check_write(&self, job: &Job, mut file: File) {
            let connection = database(self.state).unwrap();
            let (status, started, baseline): (String, bool, Option<String>) = connection
                .query_row(
                    "SELECT status,write_started,baseline_json FROM video_asset_jobs WHERE id=?1",
                    [&job.view.job_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(status, "running");
            assert!(started);
            if job.view.kind == "caption" {
                assert!(baseline.is_some())
            }
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            assert_eq!(blake3::hash(&bytes).to_hex().as_str(), job.proof.digest);
            assert_eq!(
                connection
                    .query_row(
                        "SELECT status FROM upload_items WHERE id=?1",
                        [&job.view.item_id],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                "uploaded"
            );
        }
    }
    impl AssetProvider for Mock<'_> {
        fn ensure(&mut self, _: &str, _: &str) -> Result<(), String> {
            self.counts.borrow_mut().ensures += 1;
            if self.allowed {
                Ok(())
            } else {
                Err("Separate caption permission is required".into())
            }
        }
        fn owner(&mut self, _: &str) -> Result<String, String> {
            self.counts.borrow_mut().owners += 1;
            Ok(self.owner.clone())
        }
        fn tracks(&mut self, _: &str, _: bool) -> Result<Vec<CaptionTrack>, String> {
            self.counts.borrow_mut().tracks += 1;
            Ok(self.tracks.clone())
        }
        fn thumbnail(&mut self, job: &Job, file: File) -> Result<(), WriteFailure> {
            self.check_write(job, file);
            self.writes += 1;
            self.counts.borrow_mut().writes += 1;
            if self.uncertain {
                Err(WriteFailure {
                    detail: "Unknown response retained for review".into(),
                    uncertain: true,
                })
            } else {
                Ok(())
            }
        }
        fn insert_caption(&mut self, job: &Job, file: File) -> Result<CaptionTrack, WriteFailure> {
            self.check_write(job, file);
            self.writes += 1;
            self.counts.borrow_mut().writes += 1;
            if self.uncertain {
                Err(WriteFailure {
                    detail: "Unknown response retained for reconciliation".into(),
                    uncertain: true,
                })
            } else {
                self.caption_insertions += 1;
                let track = CaptionTrack {
                    id: if self.caption_insertions == 1 {
                        "track-new".into()
                    } else {
                        format!("track-new-{}", self.caption_insertions)
                    },
                    video_id: job.view.video_id.clone().unwrap(),
                    language: job.view.language.clone().unwrap(),
                    name: job.view.name.clone().unwrap(),
                    draft: job.view.draft.unwrap(),
                    status: self.status.clone(),
                };
                self.tracks.push(track.clone());
                Ok(track)
            }
        }
    }
    fn provider(state: &AppState) -> Mock<'_> {
        Mock {
            state,
            allowed: true,
            owner: "channel-a".into(),
            tracks: Vec::new(),
            writes: 0,
            uncertain: false,
            status: "serving".into(),
            caption_insertions: 0,
            counts: Rc::new(RefCell::new(ProviderCounts::default())),
        }
    }
    fn track(id: &str, status: &str) -> CaptionTrack {
        CaptionTrack {
            id: id.into(),
            video_id: "video-a".into(),
            language: "en".into(),
            name: "English".into(),
            draft: false,
            status: status.into(),
        }
    }

    #[test]
    fn video_assets_item_run_reuses_providers_and_reads_with_updated_caption_baselines() {
        let (root, state, connection) = fixture();
        let thumbnail = create(&state, source(&root, "run-image.png", &png()), "item-a");
        let captions = caption_jobs(
            &state,
            &root,
            &["English", "English SDH", "English commentary"],
        );
        let mut jobs = vec![(thumbnail, "thumbnail".into(), "item-a".into())];
        jobs.extend(
            captions
                .iter()
                .map(|id| (id.clone(), "caption".into(), "item-a".into())),
        );
        let counts = Rc::new(RefCell::new(ProviderCounts::default()));
        assert_eq!(
            process_run_jobs(&state, "channel-a", jobs, false, |_, _| {
                counts.borrow_mut().builds += 1;
                let mut mock = provider(&state);
                mock.counts = counts.clone();
                Ok(mock)
            })
            .unwrap(),
            4
        );
        let observed = counts.borrow();
        assert_eq!(
            (
                observed.builds,
                observed.owners,
                observed.tracks,
                observed.writes
            ),
            (2, 2, 1, 4)
        );
        // All four jobs still run both preflight and immediate pre-write guards.
        assert_eq!(observed.ensures, 8);
        drop(observed);
        let baseline: String = connection
            .query_row(
                "SELECT baseline_json FROM video_asset_jobs WHERE id=?1",
                [&captions[2]],
                |row| row.get(0),
            )
            .unwrap();
        let baseline: Vec<CaptionTrack> = serde_json::from_str(&baseline).unwrap();
        assert_eq!(
            baseline
                .iter()
                .map(|track| track.id.as_str())
                .collect::<Vec<_>>(),
            vec!["track-new", "track-new-2"]
        );
        for id in captions {
            assert_eq!(
                load_job(&connection, &id, "channel-a").unwrap().view.status,
                "succeeded"
            );
        }
        finish(root, state, connection);
    }

    #[test]
    fn video_assets_item_run_discards_caption_snapshot_after_uncertain_writes() {
        let (root, state, connection) = fixture();
        let ids = caption_jobs(&state, &root, &["English", "English SDH"]);
        let counts = Rc::new(RefCell::new(ProviderCounts::default()));
        let jobs = ids
            .iter()
            .map(|id| (id.clone(), "caption".into(), "item-a".into()));
        assert_eq!(
            process_run_jobs(&state, "channel-a", jobs, false, |_, _| {
                counts.borrow_mut().builds += 1;
                let mut mock = provider(&state);
                mock.uncertain = true;
                mock.counts = counts.clone();
                Ok(mock)
            })
            .unwrap(),
            2
        );
        let observed = counts.borrow();
        assert_eq!(
            (
                observed.builds,
                observed.owners,
                observed.tracks,
                observed.writes
            ),
            (1, 1, 2, 2)
        );
        drop(observed);
        for id in ids {
            assert_eq!(
                load_job(&connection, &id, "channel-a").unwrap().view.status,
                "needs_reconciliation"
            );
        }
        finish(root, state, connection);
    }

    #[test]
    fn video_assets_item_run_reconciliation_forces_fresh_membership_without_repeat_write() {
        let (root, state, connection) = fixture();
        let ids = caption_jobs(&state, &root, &["English"]);
        let mut mock = provider(&state);
        mock.status = "syncing".into();
        let counts = mock.counts.clone();
        let mut run = RunProvider::new(mock);
        assert!(process_with(&state, &ids[0], "channel-a", false, &mut run).unwrap());
        assert_eq!(counts.borrow().tracks, 1);
        run.inner.tracks[0].status = "failed".into();
        assert!(!process_with(&state, &ids[0], "channel-a", true, &mut run).unwrap());
        assert_eq!(
            (
                counts.borrow().owners,
                counts.borrow().tracks,
                counts.borrow().writes
            ),
            (1, 2, 1)
        );
        assert_eq!(
            load_job(&connection, &ids[0], "channel-a")
                .unwrap()
                .view
                .status,
            "conflict"
        );
        finish(root, state, connection);
    }

    #[test]
    fn video_assets_item_run_keeps_permission_guards_and_drops_proof_between_runs() {
        let (root, state, connection) = fixture();
        let ids = caption_jobs(&state, &root, &["English", "English SDH"]);
        let mock = provider(&state);
        let counts = mock.counts.clone();
        let mut run = RunProvider::new(mock);
        assert!(process_with(&state, &ids[0], "channel-a", false, &mut run).unwrap());
        run.inner.allowed = false;
        assert!(!process_with(&state, &ids[1], "channel-a", false, &mut run).unwrap());
        assert_eq!(
            (
                counts.borrow().ensures,
                counts.borrow().owners,
                counts.borrow().tracks,
                counts.borrow().writes
            ),
            (3, 1, 1, 1)
        );
        assert_eq!(
            load_job(&connection, &ids[1], "channel-a")
                .unwrap()
                .view
                .status,
            "waiting_permission"
        );
        let mut next = provider(&state);
        next.counts = counts.clone();
        assert!(process_with(
            &state,
            &ids[1],
            "channel-a",
            false,
            &mut RunProvider::new(next)
        )
        .unwrap());
        assert_eq!(
            (
                counts.borrow().owners,
                counts.borrow().tracks,
                counts.borrow().writes
            ),
            (2, 2, 2)
        );
        finish(root, state, connection);
    }

    #[test]
    fn video_assets_validate_complete_png_jpeg_and_timed_caption_formats() {
        assert_eq!(png_dimensions(&png()).unwrap(), (1, 1));
        assert_eq!(jpeg_dimensions(&jpeg()).unwrap(), (1, 1));
        let mut bad = png();
        bad[20] ^= 1;
        assert!(png_dimensions(&bad).is_err());
        assert!(png_dimensions(b"\x89PNG\r\n\x1a\n").is_err());
        assert!(jpeg_dimensions(&[0xff, 0xd8, 0xff, 0xd9]).is_err());
        assert_eq!(timed_captions(b"1\n00:00:00,000 --> 00:00:01,500\nFirst cue\n\n2\n00:00:01,500 --> 00:00:02,000\nSecond cue\n",false).unwrap(),2);
        assert_eq!(
            timed_captions(b"WEBVTT\n\n00:00.000 --> 00:01.000\nTimed text\n", true).unwrap(),
            1
        );
        for bytes in [
            b"1\n00:00:02,000 --> 00:00:01,000\nBad".as_slice(),
            b"Untimed transcript".as_slice(),
            b"1\n00:70:00,000 --> 00:71:00,000\nInvalid".as_slice(),
        ] {
            assert!(timed_captions(bytes, false).is_err())
        }
        assert!(timed_captions(b"WEBVTT\nAn untimed transcript", true).is_err());
    }
    #[test]
    fn video_assets_prepare_holds_ambiguous_basename_and_hidden_channel_without_jobs() {
        let (root, state, connection) = fixture();
        let path = source(&root, "camera.png", &png());
        let review = prepare(
            &state,
            "channel-a",
            vec!["item-a".into(), "item-b".into()],
            vec![path],
            true,
        )
        .unwrap();
        assert_eq!(review.assets[0].suggested_item_ids.len(), 2);
        assert!(review.assets[0]
            .warnings
            .iter()
            .any(|warning| warning.contains("More than one")));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(prepare(
            &state,
            "channel-a",
            vec!["item-hidden".into()],
            vec![source(
                &root,
                "hidden.srt",
                b"1\n00:00:00,000 --> 00:00:01,000\nHidden"
            )],
            true
        )
        .is_err());
        let invalid = prepare(
            &state,
            "channel-a",
            vec!["item-a".into()],
            vec![source(&root, "invalid.png", b"not an image")],
            false,
        )
        .unwrap();
        assert!(!invalid.assets[0].valid);
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_save_before_and_after_upload_with_explicit_declarations_and_restart() {
        let (root, state, connection) = fixture();
        let image = source(&root, "camera.png", &png());
        let captions = source(
            &root,
            "camera.srt",
            b"1\n00:00:00,000 --> 00:00:01,000\nCaption",
        );
        let review = prepare(
            &state,
            "channel-a",
            vec!["item-a".into(), "item-b".into()],
            vec![image.clone(), captions],
            false,
        )
        .unwrap();
        let mut missing = binding(&review.assets[1].asset_id, "item-a", "caption");
        missing.draft = None;
        assert!(save(&state, "channel-a", &review.review_id, vec![missing]).is_err());
        let receipt = save(
            &state,
            "channel-a",
            &review.review_id,
            vec![
                binding(&review.assets[0].asset_id, "item-a", "thumbnail"),
                binding(&review.assets[0].asset_id, "item-b", "thumbnail"),
                binding(&review.assets[1].asset_id, "item-a", "caption"),
            ],
        )
        .unwrap();
        assert_eq!(receipt.job_ids.len(), 3);
        assert_eq!(
            load_job(&connection, &receipt.job_ids[0], "channel-a")
                .unwrap()
                .view
                .status,
            "queued"
        );
        assert_eq!(
            load_job(&connection, &receipt.job_ids[1], "channel-a")
                .unwrap()
                .view
                .status,
            "waiting_upload"
        );
        assert_eq!(
            load_job(&connection, &receipt.job_ids[2], "channel-a")
                .unwrap()
                .view
                .draft,
            Some(false)
        );
        let repeated = create(&state, image, "item-a");
        assert_eq!(repeated, receipt.job_ids[0]);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        drop(connection);
        let connection = initialize_database(&state).unwrap();
        assert_eq!(
            load_job(&connection, &receipt.job_ids[2], "channel-a")
                .unwrap()
                .proof
                .digest
                .len(),
            64
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_permitted_thumbnail_and_caption_writes_are_durably_claimed_separately() {
        let (root, state, connection) = fixture();
        let thumb = create(&state, source(&root, "image.png", &png()), "item-a");
        let caption = create(
            &state,
            source(
                &root,
                "captions.vtt",
                b"WEBVTT\n\n00:00.000 --> 00:01.000\nCaption\n",
            ),
            "item-a",
        );
        let mut provider = provider(&state);
        assert!(process_with(&state, &thumb, "channel-a", false, &mut provider).unwrap());
        assert!(process_with(&state, &caption, "channel-a", false, &mut provider).unwrap());
        assert_eq!(provider.writes, 2);
        assert_eq!(
            load_job(&connection, &thumb, "channel-a")
                .unwrap()
                .view
                .status,
            "succeeded"
        );
        assert_eq!(
            load_job(&connection, &caption, "channel-a")
                .unwrap()
                .view
                .caption_id
                .as_deref(),
            Some("track-new")
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT video_id FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "video-a"
        );
        assert_eq!(
            completion_steps(&connection, "channel-a", "item-a")
                .unwrap()
                .iter()
                .filter(|step| step.outcome == "succeeded")
                .count(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM video_asset_attempts WHERE outcome='write_started'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_completion_repair_keeps_exact_reviewed_jobs_when_new_jobs_arrive() {
        let (root, state, connection) = fixture();
        let path = source(
            &root,
            "captions.srt",
            b"1\n00:00:00,000 --> 00:00:01,000\nCaption",
        );
        let reviewed = create(&state, path.clone(), "item-a");
        connection
            .execute(
                "UPDATE video_asset_jobs SET status='failed' WHERE id=?1",
                [&reviewed],
            )
            .unwrap();
        let snapshot = completion_steps(&connection, "channel-a", "item-a")
            .unwrap()
            .into_iter()
            .find(|step| step.key == "caption")
            .unwrap();
        assert_eq!(snapshot.repair_job_ids, vec![reviewed.clone()]);
        let next = prepare(
            &state,
            "channel-a",
            vec!["item-a".into()],
            vec![path],
            false,
        )
        .unwrap();
        let mut declaration = binding(&next.assets[0].asset_id, "item-a", "caption");
        declaration.name = Some("English alternate".into());
        let later = save(&state, "channel-a", &next.review_id, vec![declaration])
            .unwrap()
            .job_ids[0]
            .clone();
        let jobs = reviewed_repair_jobs(
            &connection,
            "channel-a",
            "item-a",
            "video-a",
            "caption",
            &snapshot.repair_job_ids,
        )
        .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].view.job_id, reviewed);
        let mut provider = provider(&state);
        for job in jobs {
            process_with(&state, &job.view.job_id, "channel-a", true, &mut provider).unwrap();
        }
        assert_eq!(provider.writes, 1);
        assert_eq!(
            load_job(&connection, &later, "channel-a")
                .unwrap()
                .view
                .status,
            "queued"
        );
        assert!(reviewed_repair_jobs(
            &connection,
            "channel-a",
            "item-a",
            "changed-video",
            "caption",
            &snapshot.repair_job_ids
        )
        .is_err());
        assert!(reviewed_repair_jobs(
            &connection,
            "channel-a",
            "item-b",
            "video-a",
            "caption",
            &snapshot.repair_job_ids
        )
        .is_err());
        assert!(reviewed_repair_jobs(
            &connection,
            "channel-a",
            "item-a",
            "video-a",
            "thumbnail",
            &snapshot.repair_job_ids
        )
        .is_err());
        assert!(reviewed_repair_jobs(
            &connection,
            "channel-b",
            "item-a",
            "video-a",
            "caption",
            &snapshot.repair_job_ids
        )
        .is_err());
        assert!(reviewed_repair_jobs(
            &connection,
            "channel-a",
            "item-a",
            "video-a",
            "caption",
            &[reviewed.clone(), reviewed]
        )
        .is_err());
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_completion_repair_rejects_superseded_thumbnail_snapshot() {
        let (root, state, connection) = fixture();
        let reviewed = create(&state, source(&root, "old.png", &png()), "item-a");
        connection
            .execute(
                "UPDATE video_asset_jobs SET status='failed' WHERE id=?1",
                [&reviewed],
            )
            .unwrap();
        let ids = completion_steps(&connection, "channel-a", "item-a")
            .unwrap()
            .into_iter()
            .find(|step| step.key == "thumbnail")
            .unwrap()
            .repair_job_ids;
        let later = create(&state, source(&root, "new.png", &png()), "item-a");
        assert_eq!(
            load_job(&connection, &reviewed, "channel-a")
                .unwrap()
                .view
                .status,
            "superseded"
        );
        assert!(reviewed_repair_jobs(
            &connection,
            "channel-a",
            "item-a",
            "video-a",
            "thumbnail",
            &ids
        )
        .is_err());
        assert_eq!(
            load_job(&connection, &later, "channel-a")
                .unwrap()
                .view
                .status,
            "queued"
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_uncertain_caption_absence_and_ambiguity_never_reinsert() {
        let (root, state, connection) = fixture();
        let id = create(
            &state,
            source(
                &root,
                "captions.srt",
                b"1\n00:00:00,000 --> 00:00:01,000\nCaption",
            ),
            "item-a",
        );
        let mut provider = provider(&state);
        provider.uncertain = true;
        process_with(&state, &id, "channel-a", false, &mut provider).unwrap();
        assert_eq!(provider.writes, 1);
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "needs_reconciliation"
        );
        provider.uncertain = false;
        process_with(&state, &id, "channel-a", true, &mut provider).unwrap();
        assert_eq!(provider.writes, 1);
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "needs_reconciliation"
        );
        provider.tracks = vec![track("new-a", "serving"), track("new-b", "serving")];
        process_with(&state, &id, "channel-a", true, &mut provider).unwrap();
        assert_eq!(provider.writes, 1);
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "needs_reconciliation"
        );
        provider.tracks = vec![track("new-a", "serving")];
        process_with(&state, &id, "channel-a", true, &mut provider).unwrap();
        assert_eq!(provider.writes, 1);
        assert_eq!(
            load_job(&connection, &id, "channel-a")
                .unwrap()
                .view
                .caption_id
                .as_deref(),
            Some("new-a")
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_existing_caption_and_failed_processing_never_replace_or_delete() {
        let (root, state, connection) = fixture();
        let id = create(
            &state,
            source(
                &root,
                "captions.srt",
                b"1\n00:00:00,000 --> 00:00:01,000\nCaption",
            ),
            "item-a",
        );
        let mut provider = provider(&state);
        provider.tracks = vec![track("existing", "serving")];
        process_with(&state, &id, "channel-a", false, &mut provider).unwrap();
        assert_eq!(provider.writes, 0);
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "conflict"
        );
        assert_eq!(provider.tracks[0].id, "existing");
        connection
            .execute(
                "UPDATE video_asset_jobs SET status='queued' WHERE id=?1",
                [&id],
            )
            .unwrap();
        provider.tracks.clear();
        provider.status = "syncing".into();
        process_with(&state, &id, "channel-a", false, &mut provider).unwrap();
        assert_eq!(provider.writes, 1);
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "waiting_processing"
        );
        provider.tracks = vec![track("track-new", "failed")];
        process_with(&state, &id, "channel-a", true, &mut provider).unwrap();
        assert_eq!(provider.writes, 1);
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "conflict"
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_changed_original_wrong_channel_and_endpoint_permissions_fail_closed() {
        let (root, state, connection) = fixture();
        let path = source(&root, "image.png", &png());
        let review = prepare(
            &state,
            "channel-a",
            vec!["item-a".into()],
            vec![path.clone()],
            false,
        )
        .unwrap();
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, b"changed attachment").unwrap();
        assert!(save(
            &state,
            "channel-a",
            &review.review_id,
            vec![binding(&review.assets[0].asset_id, "item-a", "thumbnail")]
        )
        .is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        fs::write(&path, bytes).unwrap();
        let id = create(&state, path, "item-a");
        let mut provider = provider(&state);
        provider.owner = "other-channel".into();
        process_with(&state, &id, "channel-a", false, &mut provider).unwrap();
        assert_eq!(provider.writes, 0);
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "failed"
        );
        let settings = connection_settings(&connection).unwrap();
        assert!(matches!(
            permission_kind(&settings, "thumbnail").unwrap(),
            provider_transport::AccessTokenKind::Upload
        ));
        assert!(permission_kind(&settings, "caption").is_err());
        let mut native = NativeProvider::new(&state, "channel-a", "caption", "item-a").unwrap();
        assert!(native
            .ensure("channel-a", "caption")
            .unwrap_err()
            .contains("15-minute"));
        assert!(native.token.is_none());
        connection
            .execute(
                "UPDATE connection_settings SET active_channel_id='channel-b'",
                [],
            )
            .unwrap();
        assert!(process_with(&state, &id, "channel-a", true, &mut provider).is_err());
        assert_eq!(provider.writes, 0);
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_startup_classifies_interrupted_claims_without_reading_originals() {
        let (root, state, connection) = fixture();
        let path = source(&root, "image.png", &png());
        let id = create(&state, path.clone(), "item-a");
        connection.execute("UPDATE video_asset_jobs SET status='running',claim_id='before-write',write_started=0 WHERE id=?1",[&id]).unwrap();
        fs::remove_file(path).unwrap();
        recover_interrupted(&connection).unwrap();
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "queued"
        );
        connection.execute("UPDATE video_asset_jobs SET status='running',claim_id='after-write',write_started=1 WHERE id=?1",[&id]).unwrap();
        recover_interrupted(&connection).unwrap();
        assert_eq!(
            load_job(&connection, &id, "channel-a").unwrap().view.status,
            "needs_reconciliation"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "uploaded"
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_cloud_quota_pause_preserves_video_receipt_and_independent_channel_pause() {
        let (root, state, connection) = fixture();
        connection.execute("INSERT INTO upload_quota_pauses(channel_id,pause_until,updated_at) VALUES('channel-a','2099-01-01T00:00:00Z',?1)",[now()]).unwrap();
        let native = NativeProvider::new(&state, "channel-a", "thumbnail", "item-a").unwrap();
        native
            .record_limit(403, r#"{"error":{"errors":[{"reason":"quotaExceeded"}]}}"#)
            .unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT status FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "uploaded"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT video_id FROM upload_items WHERE id='item-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "video-a"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT pause_until FROM upload_quota_pauses WHERE channel_id='channel-a'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "2099-01-01T00:00:00Z"
        );
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM api_quota_pauses WHERE quota_scope='project:fixture-project'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
        finish(root, state, connection);
    }
    fn prospective_choices() -> ManualUploadSettings {
        serde_json::from_value(serde_json::json!({"madeForKids":false,"visibility":"private","deleteSourceAfterUpload":false,"expectedChannelId":"channel-a"})).unwrap()
    }
    fn prospective_fixture(
        state: &AppState,
        root: &Path,
        two: bool,
    ) -> (
        upload_plan::UploadPlanPage,
        upload_plan::UploadPlanPage,
        AssetReview,
        Vec<AssetBinding>,
    ) {
        let mut paths = vec![source(
            root,
            "prospective-a.mp4",
            b"Verified prospective original A",
        )];
        if two {
            paths.push(source(
                root,
                "prospective-b.mp4",
                b"Verified prospective original B",
            ));
        }
        let base = upload_plan::test_retained_plan(state, paths, prospective_choices());
        let review = prepare_prospective(
            state,
            "channel-a",
            &base.id,
            vec![
                source(root, "cover.png", &png()),
                source(
                    root,
                    "timed.srt",
                    b"1\n00:00:00,000 --> 00:00:01,000\nCaption",
                ),
            ],
            false,
        )
        .unwrap();
        let mut bindings: Vec<_> = review
            .items
            .iter()
            .map(|item| binding(&review.assets[0].asset_id, &item.item_id, "thumbnail"))
            .collect();
        bindings.push(binding(
            &review.assets[1].asset_id,
            &review.items[0].item_id,
            "caption",
        ));
        let prepared = freeze_plan_assets(
            state,
            "channel-a",
            &base.id,
            &review.review_id,
            bindings.clone(),
        )
        .unwrap();
        let sealed = upload_plan::seal_assets(state, &base.id, "channel-a", prepared).unwrap();
        (base, sealed, review, bindings)
    }
    #[test]
    fn video_assets_prospective_plan_stays_readonly_then_intake_commits_exact_jobs_atomically() {
        let (root, state, mut connection) = fixture();
        let before: (i64, i64) = connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM upload_items),(SELECT COUNT(*) FROM audit_events)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let (base, sealed, review, bindings) = prospective_fixture(&state, &root, true);
        assert_ne!(base.id, sealed.id);
        let page = serde_json::to_value(&sealed).unwrap();
        assert_eq!(
            page["createdAt"],
            serde_json::to_value(&base).unwrap()["createdAt"]
        );
        assert_eq!(
            page["expiresAt"],
            serde_json::to_value(&base).unwrap()["expiresAt"]
        );
        assert_eq!(page["files"][0]["assetJobs"].as_array().unwrap().len(), 2);
        assert_eq!(page["files"][0]["assetJobs"][1]["draft"], false);
        assert!(!page
            .to_string()
            .contains(&root.to_string_lossy().to_string()));
        assert!(save(&state, "channel-a", &review.review_id, bindings).is_err());
        assert_eq!(connection.query_row("SELECT (SELECT COUNT(*) FROM upload_items),(SELECT COUNT(*) FROM audit_events)",[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?))).unwrap(),before);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        upload_plan::test_discard_plan(&state, &base.id).unwrap();
        reviews().lock().unwrap().remove(&review.review_id);
        let (paths, choices) = upload_plan::test_verified_intake(&state, &sealed.id).unwrap();
        upload_plan_execution::claim(&mut connection, &sealed.id, "channel-a").unwrap();
        assert!(upload_plan_execution::claim(&mut connection, &sealed.id, "channel-a").is_err());
        drop(connection);
        let mut connection = initialize_database(&state).unwrap();
        let first = import_asset_with_context(
            paths[0].clone(),
            choices.clone(),
            &state,
            Some((&sealed.id, "channel-a")),
            1,
            Some("plan-fixture-batch"),
        )
        .unwrap();
        let second = import_asset_with_context(
            paths[1].clone(),
            choices,
            &state,
            Some((&sealed.id, "channel-a")),
            2,
            Some("plan-fixture-batch"),
        )
        .unwrap();
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM video_asset_jobs WHERE item_id=?1 AND channel_id='channel-a'",[&first.id],|row|row.get::<_,i64>(0)).unwrap(),2);
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM video_asset_jobs WHERE item_id=?1 AND channel_id='channel-a'",[&second.id],|row|row.get::<_,i64>(0)).unwrap(),1);
        let guard = hold_plan_item_assets(&state, &sealed.id, "channel-a", 0, Path::new(&paths[0]))
            .unwrap();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let repeated = materialize_plan_item_assets(
            &transaction,
            &sealed.id,
            "channel-a",
            0,
            Path::new(&paths[0]),
            &first.id,
            guard.as_ref(),
        )
        .unwrap();
        assert_eq!(repeated.len(), 2);
        transaction.commit().unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT status,video_id FROM upload_items WHERE id='item-a'",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                )
                .unwrap(),
            ("uploaded".into(), "video-a".into())
        );
        let result = Ok(BatchImportReceipt {
            requested_count: 2,
            imported_count: 2,
            queued_count: 0,
            duplicate_count: 0,
            failed_count: 0,
            items: vec![
                BatchImportItemReceipt {
                    ordinal: 0,
                    file_name: first.file_name.clone(),
                    status: "imported".into(),
                    item: Some(first),
                    detail: None,
                },
                BatchImportItemReceipt {
                    ordinal: 1,
                    file_name: second.file_name.clone(),
                    status: "imported".into(),
                    item: Some(second),
                    detail: None,
                },
            ],
            detail: None,
        });
        upload_plan_execution::finish(&connection, &sealed.id, "channel-a", &result).unwrap();
        assert_eq!(
            upload_plan_execution::receipt(&connection, &sealed.id, "channel-a")
                .unwrap()
                .unwrap()
                .items
                .len(),
            2
        );
        assert!(upload_plan_execution::claim(&mut connection, &sealed.id, "channel-a").is_err());
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_prospective_changed_asset_rolls_back_entire_item_and_keeps_claim_evidence() {
        let (root, state, mut connection) = fixture();
        let (_, sealed, _, _) = prospective_fixture(&state, &root, false);
        let (paths, choices) = upload_plan::test_verified_intake(&state, &sealed.id).unwrap();
        upload_plan_execution::claim(&mut connection, &sealed.id, "channel-a").unwrap();
        let count = connection
            .query_row("SELECT COUNT(*) FROM upload_items", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        fs::write(root.join("cover.png"), b"Changed approved attachment").unwrap();
        assert!(import_asset_with_context(
            paths[0].clone(),
            choices,
            &state,
            Some((&sealed.id, "channel-a")),
            1,
            Some("failed-plan-fixture")
        )
        .is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            count
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(upload_plan_execution::asset_materialization(
            &connection,
            &sealed.id,
            "channel-a",
            0
        )
        .unwrap()
        .is_none());
        upload_plan_execution::finish(
            &connection,
            &sealed.id,
            "channel-a",
            &Err("Controlled changed-asset failure".into()),
        )
        .unwrap();
        drop(connection);
        let mut connection = initialize_database(&state).unwrap();
        assert!(
            upload_plan_execution::asset_manifest(&connection, &sealed.id, "channel-a")
                .unwrap()
                .is_some()
        );
        assert!(upload_plan_execution::claim(&mut connection, &sealed.id, "channel-a").is_err());
        assert!(upload_plan_execution::receipt(&connection, &sealed.id, "channel-a").is_err());
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_prospective_rejects_foreign_plan_targets_channel_and_stale_approval() {
        let (root, state, connection) = fixture();
        let (base, _, review, bindings) = prospective_fixture(&state, &root, false);
        let other = upload_plan::test_retained_plan(
            &state,
            vec![root
                .join("prospective-a.mp4")
                .to_string_lossy()
                .into_owned()],
            prospective_choices(),
        );
        assert!(freeze_plan_assets(
            &state,
            "channel-a",
            &other.id,
            &review.review_id,
            bindings.clone()
        )
        .is_err());
        let mut foreign = bindings.clone();
        foreign[0].item_id = "item-a".into();
        assert!(
            freeze_plan_assets(&state, "channel-a", &base.id, &review.review_id, foreign).is_err()
        );
        let mut undeclared = bindings.clone();
        undeclared.last_mut().unwrap().draft = None;
        assert!(
            freeze_plan_assets(&state, "channel-a", &base.id, &review.review_id, undeclared)
                .is_err()
        );
        assert!(freeze_plan_assets(
            &state,
            "channel-b",
            &base.id,
            &review.review_id,
            bindings.clone()
        )
        .is_err());
        reviews()
            .lock()
            .unwrap()
            .get_mut(&review.review_id)
            .unwrap()
            .created = Utc::now() - ChronoDuration::seconds(REVIEW_SECONDS + 1);
        assert!(
            freeze_plan_assets(&state, "channel-a", &base.id, &review.review_id, bindings).is_err()
        );
        fs::write(
            root.join("prospective-a.mp4"),
            b"Changed prospective original",
        )
        .unwrap();
        assert!(prepare_prospective(
            &state,
            "channel-a",
            &base.id,
            vec![root.join("cover.png").to_string_lossy().into_owned()],
            false
        )
        .is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_prospective_materialization_fences_ordinal_path_identity_and_corrupt_manifest()
    {
        let (root, state, mut connection) = fixture();
        let (_, sealed, _, _) = prospective_fixture(&state, &root, true);
        let (paths, choices) = upload_plan::test_verified_intake(&state, &sealed.id).unwrap();
        upload_plan_execution::claim(&mut connection, &sealed.id, "channel-a").unwrap();
        let first = import_asset_with_context(
            paths[0].clone(),
            choices,
            &state,
            Some((&sealed.id, "channel-a")),
            1,
            Some("scope-fixture"),
        )
        .unwrap();
        let guard = hold_plan_item_assets(&state, &sealed.id, "channel-a", 0, Path::new(&paths[0]))
            .unwrap();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(materialize_plan_item_assets(
            &transaction,
            &sealed.id,
            "channel-a",
            1,
            Path::new(&paths[0]),
            &first.id,
            guard.as_ref()
        )
        .is_err());
        assert!(materialize_plan_item_assets(
            &transaction,
            &sealed.id,
            "channel-a",
            0,
            Path::new(&paths[1]),
            &first.id,
            guard.as_ref()
        )
        .is_err());
        assert!(materialize_plan_item_assets(
            &transaction,
            &sealed.id,
            "channel-a",
            0,
            Path::new(&paths[0]),
            "item-hidden",
            guard.as_ref()
        )
        .is_err());
        assert!(materialize_plan_item_assets(
            &transaction,
            &sealed.id,
            "channel-b",
            0,
            Path::new(&paths[0]),
            &first.id,
            guard.as_ref()
        )
        .is_err());
        drop(transaction);
        connection
            .execute(
                "UPDATE upload_plan_executions SET receipt_json='broken' WHERE plan_id=?1",
                [&sealed.id],
            )
            .unwrap();
        assert!(
            upload_plan_execution::asset_manifest(&connection, &sealed.id, "channel-a").is_err()
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_quota_recorder_keeps_exact_reason_and_captured_project_fences() {
        let (root, state, connection) = fixture();
        let native = NativeProvider::new(&state, "channel-a", "thumbnail", "item-a").unwrap();
        native
            .record_limit(403, r#"{"error":{"message":"quotaExceeded"}}"#)
            .unwrap();
        native
            .record_limit(503, r#"{"error":{"errors":[{"reason":"quotaExceeded"}]}}"#)
            .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM api_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        connection
            .execute(
                "UPDATE connection_settings SET oauth_project_id='different-project'",
                [],
            )
            .unwrap();
        assert!(native
            .record_limit(403, r#"{"error":{"errors":[{"reason":"quotaExceeded"}]}}"#)
            .is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM api_quota_pauses", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        connection
            .execute(
                "UPDATE connection_settings SET oauth_project_id='fixture-project'",
                [],
            )
            .unwrap();
        native.record_limit(403,r#"{"error":{"errors":[{"reason":"quotaExceeded"},{"reason":"uploadLimitExceeded"}]}}"#).unwrap();
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
        assert_eq!(
            connection
                .query_row(
                    "SELECT status,video_id FROM upload_items WHERE id='item-a'",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                )
                .unwrap(),
            ("uploaded".into(), "video-a".into())
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_prospective_no_assets_plan_preserves_ordinary_intake() {
        let (root, state, mut connection) = fixture();
        let plan = upload_plan::test_retained_plan(
            &state,
            vec![source(
                &root,
                "without-assets.mp4",
                b"Verified ordinary original",
            )],
            prospective_choices(),
        );
        let (paths, choices) = upload_plan::test_verified_intake(&state, &plan.id).unwrap();
        upload_plan_execution::claim(&mut connection, &plan.id, "channel-a").unwrap();
        let item = import_asset_with_context(
            paths[0].clone(),
            choices,
            &state,
            Some((&plan.id, "channel-a")),
            1,
            Some("ordinary-plan"),
        )
        .unwrap();
        assert_eq!(item.status, "draft");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(
            upload_plan_execution::asset_manifest(&connection, &plan.id, "channel-a")
                .unwrap()
                .is_none()
        );
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_prospective_audit_failure_rolls_back_video_jobs_and_manifest_ordinal() {
        let (root, state, mut connection) = fixture();
        let (_, sealed, _, _) = prospective_fixture(&state, &root, false);
        let (paths, choices) = upload_plan::test_verified_intake(&state, &sealed.id).unwrap();
        upload_plan_execution::claim(&mut connection, &sealed.id, "channel-a").unwrap();
        connection.execute_batch("CREATE TRIGGER fixture_asset_audit_fault BEFORE INSERT ON audit_events WHEN NEW.kind='video_asset_job_requested' BEGIN SELECT RAISE(ABORT,'controlled asset audit failure'); END;").unwrap();
        let count = connection
            .query_row("SELECT COUNT(*) FROM upload_items", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        assert!(import_asset_with_context(
            paths[0].clone(),
            choices,
            &state,
            Some((&sealed.id, "channel-a")),
            1,
            Some("audit-fault")
        )
        .is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM upload_items", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            count
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(upload_plan_execution::asset_materialization(
            &connection,
            &sealed.id,
            "channel-a",
            0
        )
        .unwrap()
        .is_none());
        finish(root, state, connection);
    }
    #[test]
    fn video_assets_prospective_missing_or_moved_held_guard_cannot_commit_attachment_intake() {
        let (root, state, mut connection) = fixture();
        let (_, sealed, _, _) = prospective_fixture(&state, &root, false);
        upload_plan::test_verified_intake(&state, &sealed.id).unwrap();
        upload_plan_execution::claim(&mut connection, &sealed.id, "channel-a").unwrap();
        let bundle = upload_plan_execution::asset_manifest(&connection, &sealed.id, "channel-a")
            .unwrap()
            .unwrap();
        let target = &bundle.targets[0];
        let guard =
            hold_plan_item_assets(&state, &sealed.id, "channel-a", 0, &target.source_path).unwrap();
        let insert = |tx: &Transaction<'_>| {
            tx.execute("INSERT INTO upload_items(id,title,file_name,channel_id,source_path,workspace_path,size_bytes,source_modified_key,digest,status,total_bytes,upload_plan_id,created_at,updated_at) VALUES('guard-fixture','Fixture',?1,'channel-a',?2,?2,?3,?4,?5,'draft',?3,?6,?7,?7)",params![target.file_name,target.source_path.to_string_lossy(),target.source_size,target.source_modified,target.source_digest,sealed.id,now()]).unwrap()
        };
        {
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            insert(&tx);
            assert!(materialize_plan_item_assets(
                &tx,
                &sealed.id,
                "channel-a",
                0,
                &target.source_path,
                "guard-fixture",
                None
            )
            .is_err());
        }
        fs::rename(root.join("cover.png"), root.join("moved-cover.png")).unwrap();
        {
            let tx = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .unwrap();
            insert(&tx);
            assert!(materialize_plan_item_assets(
                &tx,
                &sealed.id,
                "channel-a",
                0,
                &target.source_path,
                "guard-fixture",
                guard.as_ref()
            )
            .is_err());
        }
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM upload_items WHERE id='guard-fixture'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM video_asset_jobs", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(upload_plan_execution::asset_materialization(
            &connection,
            &sealed.id,
            "channel-a",
            0
        )
        .unwrap()
        .is_none());
        drop(guard);
        finish(root, state, connection);
    }
}
