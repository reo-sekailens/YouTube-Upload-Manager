use reqwest::blocking::{Client, Response};
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

const CONTROL_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CONTROL_REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const UPLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const UPLOAD_REQUEST_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const IDLE_POOL_TIMEOUT: Duration = Duration::from_secs(90);
const ACCESS_TOKEN_REFRESH_SKEW: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
pub(crate) enum AccessTokenKind {
    Upload,
    Deletion,
}

struct CachedAccessToken {
    value: String,
    expires_at: Instant,
}

struct AccessTokenCache {
    value: Mutex<Option<CachedAccessToken>>,
    refresh_skew: Duration,
}

impl AccessTokenCache {
    fn new(refresh_skew: Duration) -> Self {
        Self {
            value: Mutex::new(None),
            refresh_skew,
        }
    }

    fn get_or_refresh(
        &self,
        refresh: impl FnOnce() -> Result<(String, Duration), String>,
    ) -> Result<String, String> {
        // The lock intentionally spans refresh. Concurrent workers either use a
        // still-valid token or wait for exactly one secure-store/provider refresh.
        let mut cached = self
            .value
            .lock()
            .map_err(|_| "The in-memory authorization cache is unavailable.".to_string())?;
        let valid = cached.as_ref().is_some_and(|token| {
            Instant::now()
                .checked_add(self.refresh_skew)
                .is_some_and(|minimum_expiry| minimum_expiry < token.expires_at)
        });
        if valid {
            return Ok(cached.as_ref().expect("valid token exists").value.clone());
        }
        let (value, lifetime) = refresh()?;
        if value.is_empty() {
            return Err("Google did not return a refreshed access token.".into());
        }
        let expires_at = Instant::now()
            .checked_add(lifetime)
            .unwrap_or_else(Instant::now);
        *cached = Some(CachedAccessToken {
            value: value.clone(),
            expires_at,
        });
        Ok(value)
    }

    fn invalidate(&self) {
        if let Ok(mut value) = self.value.lock() {
            *value = None;
        }
    }

    fn store(&self, value: &str, lifetime: Duration) {
        if value.is_empty() {
            return;
        }
        if let Ok(mut cached) = self.value.lock() {
            *cached = Some(CachedAccessToken {
                value: value.to_string(),
                expires_at: Instant::now()
                    .checked_add(lifetime)
                    .unwrap_or_else(Instant::now),
            });
        }
    }
}

pub(crate) struct ProviderTransport {
    control: LazyClient,
    upload: LazyClient,
    upload_tokens: AccessTokenCache,
    deletion_tokens: AccessTokenCache,
    client_builds: AtomicUsize,
}

#[derive(Clone, Copy)]
enum ClientPolicy {
    Control,
    Upload,
}

struct LazyClient {
    value: Mutex<Option<Client>>,
    policy: ClientPolicy,
}

impl LazyClient {
    fn new(policy: ClientPolicy) -> Self {
        Self {
            value: Mutex::new(None),
            policy,
        }
    }

    fn get(&self, client_builds: &AtomicUsize) -> Result<Client, String> {
        let mut value = self
            .value
            .lock()
            .map_err(|_| "Google transport initialization is unavailable.".to_string())?;
        if let Some(client) = value.as_ref() {
            return Ok(client.clone());
        }
        let mut builder = Client::builder()
            .pool_idle_timeout(IDLE_POOL_TIMEOUT)
            .pool_max_idle_per_host(8);
        builder = match self.policy {
            ClientPolicy::Control => builder
                .connect_timeout(CONTROL_CONNECT_TIMEOUT)
                .timeout(CONTROL_REQUEST_TIMEOUT),
            ClientPolicy::Upload => builder
                .connect_timeout(UPLOAD_CONNECT_TIMEOUT)
                .timeout(UPLOAD_REQUEST_TIMEOUT),
        };
        let client = builder.build().map_err(|_| match self.policy {
            ClientPolicy::Control => "Google transport could not be prepared.".to_string(),
            ClientPolicy::Upload => "YouTube upload transport could not be prepared.".to_string(),
        })?;
        client_builds.fetch_add(1, Ordering::Relaxed);
        *value = Some(client.clone());
        Ok(client)
    }
}

impl ProviderTransport {
    pub(crate) fn new() -> Self {
        Self::with_refresh_skew(ACCESS_TOKEN_REFRESH_SKEW)
    }

    fn with_refresh_skew(refresh_skew: Duration) -> Self {
        Self {
            control: LazyClient::new(ClientPolicy::Control),
            upload: LazyClient::new(ClientPolicy::Upload),
            upload_tokens: AccessTokenCache::new(refresh_skew),
            deletion_tokens: AccessTokenCache::new(refresh_skew),
            client_builds: AtomicUsize::new(0),
        }
    }

    pub(crate) fn control_client(&self) -> Result<Client, String> {
        self.control.get(&self.client_builds)
    }

    pub(crate) fn upload_client(&self) -> Result<Client, String> {
        self.upload.get(&self.client_builds)
    }

    pub(crate) fn access_token(
        &self,
        kind: AccessTokenKind,
        refresh: impl FnOnce() -> Result<(String, Duration), String>,
    ) -> Result<String, String> {
        match kind {
            AccessTokenKind::Upload => self.upload_tokens.get_or_refresh(refresh),
            AccessTokenKind::Deletion => self.deletion_tokens.get_or_refresh(refresh),
        }
    }

    pub(crate) fn invalidate_access_token(&self, kind: AccessTokenKind) {
        match kind {
            AccessTokenKind::Upload => self.upload_tokens.invalidate(),
            AccessTokenKind::Deletion => self.deletion_tokens.invalidate(),
        }
    }

    pub(crate) fn cache_access_token(
        &self,
        kind: AccessTokenKind,
        value: &str,
        lifetime: Duration,
    ) {
        match kind {
            AccessTokenKind::Upload => self.upload_tokens.store(value, lifetime),
            AccessTokenKind::Deletion => self.deletion_tokens.store(value, lifetime),
        }
    }

    /// Reads exactly one acknowledged-range candidate into the request-owned
    /// buffer. Moving that Vec into reqwest avoids the old persistent-buffer plus
    /// full-chunk clone, while memory remains bounded to one chunk per worker.
    pub(crate) fn put_upload_chunk(
        &self,
        session_uri: &str,
        path: &Path,
        offset: u64,
        bytes: u64,
        total_bytes: u64,
    ) -> Result<Response, String> {
        let client = self.upload_client()?;
        self.put_upload_chunk_with_client(&client, session_uri, path, offset, bytes, total_bytes)
    }

    /// The upload hot loop obtains the pooled client once per resumable
    /// transfer, keeping client initialization and the lazy-client mutex out of
    /// every acknowledged chunk.
    pub(crate) fn put_upload_chunk_with_client(
        &self,
        client: &Client,
        session_uri: &str,
        path: &Path,
        offset: u64,
        bytes: u64,
        total_bytes: u64,
    ) -> Result<Response, String> {
        if bytes == 0 || offset.saturating_add(bytes) > total_bytes {
            return Err("The resumable upload chunk range is invalid.".into());
        }
        let mut source = File::open(path).map_err(|error| error.to_string())?;
        source
            .seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        let mut body = vec![0_u8; bytes as usize];
        source.read_exact(&mut body).map_err(|_| {
            "Managed local media ended before the expected upload range.".to_string()
        })?;
        let end = offset + bytes - 1;
        client
            .put(session_uri)
            .header(CONTENT_LENGTH, bytes)
            .header(CONTENT_TYPE, "application/octet-stream")
            .header(CONTENT_RANGE, format!("bytes {offset}-{end}/{total_bytes}"))
            .body(body)
            .send()
            .map_err(|_| {
                "Upload connection interrupted; the saved session will be reconciled on retry."
                    .to_string()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Barrier,
        },
        thread,
    };

    #[test]
    fn access_token_cache_is_expiry_aware() {
        let cache = AccessTokenCache::new(Duration::ZERO);
        let refreshes = AtomicUsize::new(0);
        let first = cache
            .get_or_refresh(|| {
                refreshes.fetch_add(1, Ordering::SeqCst);
                Ok(("first".into(), Duration::from_millis(15)))
            })
            .unwrap();
        let reused = cache
            .get_or_refresh(|| {
                refreshes.fetch_add(1, Ordering::SeqCst);
                Ok(("unexpected".into(), Duration::from_secs(1)))
            })
            .unwrap();
        assert_eq!(
            (
                first.as_str(),
                reused.as_str(),
                refreshes.load(Ordering::SeqCst)
            ),
            ("first", "first", 1)
        );
        thread::sleep(Duration::from_millis(25));
        let refreshed = cache
            .get_or_refresh(|| {
                refreshes.fetch_add(1, Ordering::SeqCst);
                Ok(("second".into(), Duration::from_secs(1)))
            })
            .unwrap();
        assert_eq!(refreshed, "second");
        assert_eq!(refreshes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn concurrent_access_token_requests_use_one_refresh() {
        let cache = Arc::new(AccessTokenCache::new(Duration::ZERO));
        let barrier = Arc::new(Barrier::new(8));
        let refreshes = Arc::new(AtomicUsize::new(0));
        let workers = (0..8)
            .map(|_| {
                let cache = cache.clone();
                let barrier = barrier.clone();
                let refreshes = refreshes.clone();
                thread::spawn(move || {
                    barrier.wait();
                    cache
                        .get_or_refresh(|| {
                            refreshes.fetch_add(1, Ordering::SeqCst);
                            thread::sleep(Duration::from_millis(20));
                            Ok(("shared".into(), Duration::from_secs(30)))
                        })
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            assert_eq!(worker.join().unwrap(), "shared");
        }
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn upload_and_deletion_tokens_are_independently_invalidated() {
        let transport = ProviderTransport::with_refresh_skew(Duration::ZERO);
        let upload = transport
            .access_token(AccessTokenKind::Upload, || {
                Ok(("upload".into(), Duration::from_secs(30)))
            })
            .unwrap();
        let deletion = transport
            .access_token(AccessTokenKind::Deletion, || {
                Ok(("delete".into(), Duration::from_secs(30)))
            })
            .unwrap();
        transport.invalidate_access_token(AccessTokenKind::Upload);
        let refreshed = transport
            .access_token(AccessTokenKind::Upload, || {
                Ok(("upload-2".into(), Duration::from_secs(30)))
            })
            .unwrap();
        let retained = transport
            .access_token(AccessTokenKind::Deletion, || {
                panic!("deletion cache should remain valid")
            })
            .unwrap();
        assert_eq!(
            (upload, deletion, refreshed, retained),
            (
                "upload".into(),
                "delete".into(),
                "upload-2".into(),
                "delete".into()
            )
        );
    }

    #[test]
    fn constructing_native_provider_state_builds_no_http_client() {
        let transport = ProviderTransport::new();
        assert_eq!(transport.client_builds.load(Ordering::Relaxed), 0);
        let _ = transport.control_client().unwrap();
        assert_eq!(transport.client_builds.load(Ordering::Relaxed), 1);
        let _ = transport.control_client().unwrap();
        assert_eq!(transport.client_builds.load(Ordering::Relaxed), 1);
        let _ = transport.upload_client().unwrap();
        assert_eq!(transport.client_builds.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn single_owner_chunk_preserves_mock_308_without_an_extra_full_chunk_copy() {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-task108-308-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let media = root.join("media.bin");
        fs::write(&media, vec![23_u8; 256 * 1024]).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 8192];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
            assert!(headers.contains("content-range: bytes 0-262143/524288"));
            let content_length = 256 * 1024;
            while request.len() - header_end < content_length {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
            }
            assert_eq!(request.len() - header_end, content_length);
            stream
                .write_all(b"HTTP/1.1 308 Permanent Redirect\r\nRange: bytes=0-262143\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .unwrap();
        });
        let transport = ProviderTransport::new();
        let response = transport
            .put_upload_chunk(
                &format!("http://{address}/session"),
                &media,
                0,
                256 * 1024,
                512 * 1024,
            )
            .unwrap();
        assert_eq!(response.status().as_u16(), 308);
        assert_eq!(
            response.headers().get("range").unwrap().to_str().unwrap(),
            "bytes=0-262143"
        );
        assert_eq!(transport.client_builds.load(Ordering::Relaxed), 1);
        server.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_mock_chunk_returns_a_reconciliation_safe_error() {
        let root = std::env::temp_dir().join(format!(
            "youtube-uploader-task108-interruption-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let media = root.join("media.bin");
        fs::write(&media, vec![31_u8; 64 * 1024]).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            drop(stream);
        });
        let error = ProviderTransport::new()
            .put_upload_chunk(
                &format!("http://{address}/interrupted-session"),
                &media,
                0,
                64 * 1024,
                64 * 1024,
            )
            .unwrap_err();
        assert!(error.contains("saved session will be reconciled"));
        server.join().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
