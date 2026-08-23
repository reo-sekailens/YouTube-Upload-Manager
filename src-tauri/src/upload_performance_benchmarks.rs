//! Ignored, release-only upload transport performance evidence.
//!
//! The fixture is deliberately isolated from the application profile, secure
//! storage, and Google. It generates synthetic bytes in a disposable temporary
//! directory and sends them only to an IPv4 loopback HTTP server. Output is a
//! redacted numeric summary suitable for TASK103 baseline comparisons.

use crate::provider_transport::ProviderTransport;
use reqwest::blocking::{Body, Client};
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use std::{
    collections::HashMap,
    fs::{self, File},
    hint::black_box,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const SAMPLE_RUNS: usize = 7;
const MEDIA_BYTES: usize = 64 * 1024 * 1024;
const CHUNK_BYTES: usize = 8 * 1024 * 1024;
const SERVER_READ_BUFFER_BYTES: usize = 64 * 1024;

struct SyntheticMedia {
    root: PathBuf,
    path: PathBuf,
}

impl SyntheticMedia {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-upload-performance-{}-{}",
            std::process::id(),
            unique_fixture_nonce()
        ));
        fs::create_dir(&root).expect("create isolated performance fixture");
        let path = root.join("synthetic-media.bin");
        write_deterministic_media(&path, MEDIA_BYTES);
        Self { root, path }
    }
}

impl Drop for SyntheticMedia {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn unique_fixture_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos()
}

fn write_deterministic_media(path: &Path, bytes: usize) {
    let mut file = File::create(path).expect("create synthetic media");
    let mut absolute_offset = 0_usize;
    while absolute_offset < bytes {
        let block_bytes = (bytes - absolute_offset).min(SERVER_READ_BUFFER_BYTES);
        let block = (0..block_bytes)
            .map(|index| ((absolute_offset + index) % 251) as u8)
            .collect::<Vec<_>>();
        file.write_all(&block).expect("write synthetic media");
        absolute_offset += block.len();
    }
    file.sync_all().expect("flush synthetic media");
}

#[derive(Debug)]
struct SampleSummary {
    minimum_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
}

fn sample(mut operation: impl FnMut(usize)) -> SampleSummary {
    let mut elapsed = Vec::with_capacity(SAMPLE_RUNS);
    for sample_index in 0..SAMPLE_RUNS {
        let started = Instant::now();
        operation(sample_index);
        elapsed.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    elapsed.sort_by(f64::total_cmp);
    let p95_index = ((elapsed.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(elapsed.len() - 1);
    SampleSummary {
        minimum_ms: elapsed[0],
        p50_ms: elapsed[elapsed.len() / 2],
        p95_ms: elapsed[p95_index],
    }
}

fn mib_per_second(bytes: usize, elapsed_ms: f64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0) / (elapsed_ms / 1_000.0)
}

fn emit_result(
    surface: &str,
    summary: &SampleSummary,
    application_chunk_buffer_bytes: usize,
    full_chunk_copies_per_request: usize,
    reference_ratio: f64,
    implementation_status: &str,
) {
    eprintln!(
        concat!(
            "PERF_RESULT {{",
            "\"schemaVersion\":1,",
            "\"localOnly\":true,",
            "\"redacted\":true,",
            "\"surface\":\"{}\",",
            "\"implementationStatus\":\"{}\",",
            "\"fixtureBytes\":{},",
            "\"chunkBytes\":{},",
            "\"chunksPerSample\":{},",
            "\"samples\":{},",
            "\"minimumMs\":{:.3},",
            "\"p50Ms\":{:.3},",
            "\"p95Ms\":{:.3},",
            "\"p50MiBPerSecond\":{:.3},",
            "\"p95LatencyMiBPerSecond\":{:.3},",
            "\"applicationChunkBufferBytes\":{},",
            "\"mockServerReadBufferBytes\":{},",
            "\"fullChunkCopiesPerRequest\":{},",
            "\"p50ThroughputToFileReadRatio\":{:.4}",
            "}}"
        ),
        surface,
        implementation_status,
        MEDIA_BYTES,
        CHUNK_BYTES,
        MEDIA_BYTES / CHUNK_BYTES,
        SAMPLE_RUNS,
        summary.minimum_ms,
        summary.p50_ms,
        summary.p95_ms,
        mib_per_second(MEDIA_BYTES, summary.p50_ms),
        mib_per_second(MEDIA_BYTES, summary.p95_ms),
        application_chunk_buffer_bytes,
        SERVER_READ_BUFFER_BYTES,
        full_chunk_copies_per_request,
        reference_ratio,
    );
}

fn require_release() {
    assert!(
        !cfg!(debug_assertions),
        "upload performance benchmarks must run with `cargo test --release performance_benchmark_local_mock_resumable_upload -- --ignored --nocapture`"
    );
}

fn file_read_baseline(path: &Path) -> SampleSummary {
    sample(|_| {
        let mut file = File::open(path).expect("open synthetic media");
        let mut buffer = vec![0_u8; CHUNK_BYTES];
        let mut total = 0_usize;
        loop {
            let read = file.read(&mut buffer).expect("read synthetic media");
            if read == 0 {
                break;
            }
            total += read;
            black_box(&buffer[..read]);
        }
        assert_eq!(total, MEDIA_BYTES);
    })
}

#[derive(Debug, Default)]
struct MockState {
    next_offset_by_session: HashMap<String, usize>,
    acknowledged_ends_by_session: HashMap<String, Vec<usize>>,
    requests: usize,
}

struct MockServer {
    address: SocketAddr,
    state: Arc<Mutex<MockState>>,
    thread: Option<thread::JoinHandle<Result<(), String>>>,
}

impl MockServer {
    fn start(expected_requests: usize) -> Self {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind loopback-only upload mock");
        let address = listener.local_addr().expect("read mock address");
        assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        listener
            .set_nonblocking(true)
            .expect("configure mock listener");
        let state = Arc::new(Mutex::new(MockState::default()));
        let server_state = Arc::clone(&state);
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(180);
            while server_state
                .lock()
                .map_err(|_| "mock state poisoned")?
                .requests
                < expected_requests
            {
                if Instant::now() >= deadline {
                    return Err("loopback upload mock timed out".into());
                }
                match listener.accept() {
                    Ok((stream, peer)) => {
                        if peer.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
                            return Err("upload mock rejected a non-loopback peer".into());
                        }
                        handle_connection(stream, &server_state, expected_requests, deadline)?;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(error) => return Err(format!("upload mock accept failed: {error}")),
                }
            }
            Ok(())
        });
        Self {
            address,
            state,
            thread: Some(handle),
        }
    }

    fn session_url(&self, session: &str) -> String {
        format!("http://{}/upload/{session}", self.address)
    }

    fn finish(mut self, expected_sessions: usize) {
        self.thread
            .take()
            .expect("mock server thread")
            .join()
            .expect("join mock server")
            .expect("mock server result");
        let state = self.state.lock().expect("read mock state");
        assert_eq!(state.next_offset_by_session.len(), expected_sessions);
        for (session, acknowledgements) in &state.acknowledged_ends_by_session {
            assert!(
                session.starts_with("current-")
                    || session.starts_with("reference-")
                    || session.starts_with("optimized-"),
                "unexpected synthetic session label"
            );
            assert_eq!(acknowledgements.len(), MEDIA_BYTES / CHUNK_BYTES);
            for (index, acknowledged_end) in acknowledgements.iter().enumerate() {
                assert_eq!(*acknowledged_end, (index + 1) * CHUNK_BYTES - 1);
            }
        }
    }
}

fn handle_connection(
    stream: TcpStream,
    state: &Arc<Mutex<MockState>>,
    expected_requests: usize,
    deadline: Instant,
) -> Result<(), String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(stream);
    loop {
        if Instant::now() >= deadline {
            return Err("loopback upload mock connection timed out".into());
        }
        let mut request_line = String::new();
        let read = reader
            .read_line(&mut request_line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(());
        }
        let mut request_parts = request_line.split_whitespace();
        if request_parts.next() != Some("PUT") {
            return Err("upload mock accepts PUT only".into());
        }
        let path = request_parts
            .next()
            .ok_or_else(|| "upload mock request omitted its path".to_string())?;
        let session = path
            .strip_prefix("/upload/")
            .filter(|value| !value.is_empty() && !value.contains('/'))
            .ok_or_else(|| "upload mock request used an invalid session path".to_string())?
            .to_string();

        let mut headers = HashMap::new();
        loop {
            let mut header = String::new();
            reader
                .read_line(&mut header)
                .map_err(|error| error.to_string())?;
            if header == "\r\n" {
                break;
            }
            let (name, value) = header
                .split_once(':')
                .ok_or_else(|| "upload mock received a malformed header".to_string())?;
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
        let content_length = headers
            .get("content-length")
            .ok_or_else(|| "upload chunk omitted Content-Length".to_string())?
            .parse::<usize>()
            .map_err(|_| "upload chunk used invalid Content-Length".to_string())?;
        let (start, end, total) = parse_content_range(
            headers
                .get("content-range")
                .ok_or_else(|| "upload chunk omitted Content-Range".to_string())?,
        )?;
        if total != MEDIA_BYTES || end - start + 1 != content_length {
            return Err("upload chunk length did not match its declared range".into());
        }

        let expected_start = state
            .lock()
            .map_err(|_| "mock state poisoned")?
            .next_offset_by_session
            .get(&session)
            .copied()
            .unwrap_or(0);
        if start != expected_start {
            return Err("upload chunk did not follow the acknowledged range".into());
        }

        let mut remaining = content_length;
        let mut received = 0_usize;
        let mut body_buffer = vec![0_u8; SERVER_READ_BUFFER_BYTES];
        while remaining > 0 {
            let read = reader
                .read(&mut body_buffer[..remaining.min(SERVER_READ_BUFFER_BYTES)])
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("upload chunk body ended early".into());
            }
            validate_deterministic_bytes(&body_buffer[..read], start + received)?;
            received += read;
            remaining -= read;
        }

        let final_chunk = end + 1 == total;
        {
            let mut state = state.lock().map_err(|_| "mock state poisoned")?;
            state
                .next_offset_by_session
                .insert(session.clone(), end + 1);
            state
                .acknowledged_ends_by_session
                .entry(session)
                .or_default()
                .push(end);
            state.requests += 1;
        }

        if final_chunk {
            let body = br#"{"id":"mock-success"}"#;
            write!(
                reader.get_mut(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
                body.len()
            )
            .map_err(|error| error.to_string())?;
            reader
                .get_mut()
                .write_all(body)
                .map_err(|error| error.to_string())?;
        } else {
            write!(
                reader.get_mut(),
                "HTTP/1.1 308 Permanent Redirect\r\nRange: bytes=0-{end}\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n"
            )
            .map_err(|error| error.to_string())?;
        }
        reader
            .get_mut()
            .flush()
            .map_err(|error| error.to_string())?;

        if state.lock().map_err(|_| "mock state poisoned")?.requests >= expected_requests {
            return Ok(());
        }
    }
}

fn validate_deterministic_bytes(bytes: &[u8], absolute_offset: usize) -> Result<(), String> {
    for (index, value) in bytes.iter().enumerate() {
        if *value != ((absolute_offset + index) % 251) as u8 {
            return Err("upload chunk did not contain the synthetic fixture bytes".into());
        }
    }
    Ok(())
}

fn parse_content_range(value: &str) -> Result<(usize, usize, usize), String> {
    let value = value
        .strip_prefix("bytes ")
        .ok_or_else(|| "upload chunk used an invalid Content-Range unit".to_string())?;
    let (range, total) = value
        .split_once('/')
        .ok_or_else(|| "upload chunk used an invalid Content-Range total".to_string())?;
    let (start, end) = range
        .split_once('-')
        .ok_or_else(|| "upload chunk used an invalid Content-Range bounds".to_string())?;
    let start = start
        .parse::<usize>()
        .map_err(|_| "upload chunk used an invalid Content-Range start".to_string())?;
    let end = end
        .parse::<usize>()
        .map_err(|_| "upload chunk used an invalid Content-Range end".to_string())?;
    let total = total
        .parse::<usize>()
        .map_err(|_| "upload chunk used an invalid Content-Range length".to_string())?;
    if start > end || end >= total {
        return Err("upload chunk used an out-of-bounds Content-Range".into());
    }
    Ok((start, end, total))
}

fn confirmed_offset(response: &reqwest::blocking::Response, expected_end: usize) -> usize {
    assert_eq!(response.status().as_u16(), 308);
    let range = response
        .headers()
        .get(RANGE)
        .expect("mock 308 response includes Range")
        .to_str()
        .expect("mock Range is ASCII");
    let acknowledged_end = range
        .strip_prefix("bytes=0-")
        .expect("mock Range has expected prefix")
        .parse::<usize>()
        .expect("mock Range has numeric end");
    assert_eq!(acknowledged_end, expected_end);
    acknowledged_end + 1
}

fn transfer_current_shaped(path: &Path, session_url: &str) {
    let mut source = File::open(path).expect("open synthetic media");
    let mut buffer = vec![0_u8; CHUNK_BYTES];
    let mut offset = 0_usize;
    while offset < MEDIA_BYTES {
        let bytes = (MEDIA_BYTES - offset).min(CHUNK_BYTES);
        source
            .read_exact(&mut buffer[..bytes])
            .expect("read current-shaped upload chunk");
        let end = offset + bytes - 1;
        // This intentionally mirrors the current uploader's per-chunk client
        // and full body copy so TASK103 freezes the cost instead of hiding it.
        let response = Client::new()
            .put(session_url)
            .header(CONTENT_LENGTH, bytes)
            .header(CONTENT_RANGE, format!("bytes {offset}-{end}/{MEDIA_BYTES}"))
            .body(buffer[..bytes].to_vec())
            .send()
            .expect("send current-shaped upload chunk");
        if end + 1 == MEDIA_BYTES {
            assert!(response.status().is_success());
            let body = response.text().expect("read final mock response");
            assert_eq!(body, r#"{"id":"mock-success"}"#);
            offset = MEDIA_BYTES;
        } else {
            offset = confirmed_offset(&response, end);
        }
    }
}

fn transfer_reference_streaming(path: &Path, session_url: &str, client: &Client) {
    let mut offset = 0_usize;
    while offset < MEDIA_BYTES {
        let bytes = (MEDIA_BYTES - offset).min(CHUNK_BYTES);
        let end = offset + bytes - 1;
        let mut source = File::open(path).expect("open synthetic media");
        source
            .seek(SeekFrom::Start(offset as u64))
            .expect("seek synthetic media");
        let response = client
            .put(session_url)
            .header(CONTENT_LENGTH, bytes)
            .header(CONTENT_RANGE, format!("bytes {offset}-{end}/{MEDIA_BYTES}"))
            .body(Body::new(source.take(bytes as u64)))
            .send()
            .expect("send reference streaming upload chunk");
        if end + 1 == MEDIA_BYTES {
            assert!(response.status().is_success());
            let body = response.text().expect("read final mock response");
            assert_eq!(body, r#"{"id":"mock-success"}"#);
            offset = MEDIA_BYTES;
        } else {
            offset = confirmed_offset(&response, end);
        }
    }
}

fn transfer_optimized_single_owner(path: &Path, session_url: &str, transport: &ProviderTransport) {
    let client = transport
        .upload_client()
        .expect("obtain pooled optimized upload client");
    let mut offset = 0_usize;
    while offset < MEDIA_BYTES {
        let bytes = (MEDIA_BYTES - offset).min(CHUNK_BYTES);
        let end = offset + bytes - 1;
        let response = transport
            .put_upload_chunk_with_client(
                &client,
                session_url,
                path,
                offset as u64,
                bytes as u64,
                MEDIA_BYTES as u64,
            )
            .expect("send optimized single-owner upload chunk");
        if end + 1 == MEDIA_BYTES {
            assert!(response.status().is_success());
            let body = response.text().expect("read final mock response");
            assert_eq!(body, r#"{"id":"mock-success"}"#);
            offset = MEDIA_BYTES;
        } else {
            offset = confirmed_offset(&response, end);
        }
    }
}

#[test]
#[ignore = "release-only local performance evidence"]
fn performance_benchmark_local_mock_resumable_upload() {
    require_release();
    assert_eq!(CHUNK_BYTES % (256 * 1024), 0);
    let media = SyntheticMedia::new();
    let file_read = file_read_baseline(&media.path);
    let file_read_rate = mib_per_second(MEDIA_BYTES, file_read.p50_ms);

    let requests_per_sample = MEDIA_BYTES / CHUNK_BYTES;
    let expected_requests = SAMPLE_RUNS * requests_per_sample * 3;
    let server = MockServer::start(expected_requests);

    let current = sample(|sample_index| {
        let url = server.session_url(&format!("current-{sample_index}"));
        transfer_current_shaped(&media.path, &url);
    });

    let reference_client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("build pooled loopback reference client");
    let reference = sample(|sample_index| {
        let url = server.session_url(&format!("reference-{sample_index}"));
        transfer_reference_streaming(&media.path, &url, &reference_client);
    });
    drop(reference_client);

    let optimized_transport = ProviderTransport::new();
    let optimized = sample(|sample_index| {
        let url = server.session_url(&format!("optimized-{sample_index}"));
        transfer_optimized_single_owner(&media.path, &url, &optimized_transport);
    });
    let optimized_reference_ratio = reference.p50_ms / optimized.p50_ms;
    assert!(
        optimized_reference_ratio >= 0.90,
        "TASK108 optimized throughput ratio {optimized_reference_ratio:.4} must be at least 0.90 of the pooled streaming reference"
    );

    server.finish(SAMPLE_RUNS * 3);

    emit_result(
        "upload-file-read",
        &file_read,
        CHUNK_BYTES,
        0,
        1.0,
        "reference-file-read",
    );
    emit_result(
        "upload-resumable-current-shaped",
        &current,
        CHUNK_BYTES * 2,
        1,
        mib_per_second(MEDIA_BYTES, current.p50_ms) / file_read_rate,
        "current-shaped-baseline",
    );
    emit_result(
        "upload-resumable-reference-streaming",
        &reference,
        0,
        0,
        mib_per_second(MEDIA_BYTES, reference.p50_ms) / file_read_rate,
        "reference-for-task108",
    );
    emit_result(
        "upload-resumable-optimized-single-owner-buffer",
        &optimized,
        CHUNK_BYTES,
        0,
        optimized_reference_ratio,
        "task108-optimized-vs-streaming-reference",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_range_parser_rejects_invalid_bounds() {
        assert_eq!(parse_content_range("bytes 0-7/8").unwrap(), (0, 7, 8));
        assert!(parse_content_range("items 0-7/8").is_err());
        assert!(parse_content_range("bytes 7-0/8").is_err());
        assert!(parse_content_range("bytes 0-8/8").is_err());
    }
}
