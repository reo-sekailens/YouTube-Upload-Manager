use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UploadCandidate {
    pub(crate) item_id: String,
    pub(crate) volume_id: String,
    pub(crate) volume_limit: usize,
    /// A saved YouTube resumable session has provider-confirmed work already
    /// in flight, so it must consume available capacity before a new upload.
    pub(crate) resume_from_youtube_session: bool,
}

#[derive(Default)]
struct ActiveUploads {
    volume_by_item: HashMap<String, String>,
    last_selected_volume: Option<String>,
}

pub(crate) struct UploadScheduler {
    global_limit: usize,
    dispatch_lock: Mutex<()>,
    active: Mutex<ActiveUploads>,
    postprocess_worker_running: AtomicBool,
}

impl UploadScheduler {
    pub(crate) fn new(global_limit: usize) -> Self {
        assert!(global_limit > 0, "upload scheduler limit must be positive");
        Self {
            global_limit,
            dispatch_lock: Mutex::new(()),
            active: Mutex::new(ActiveUploads::default()),
            postprocess_worker_running: AtomicBool::new(false),
        }
    }

    pub(crate) fn lock_dispatch(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.dispatch_lock
            .lock()
            .map_err(|_| "The upload scheduler is unavailable.".to_string())
    }

    /// Rebuild the in-memory permit ledger from the same durable claim states
    /// used for crash recovery. This makes relaunch and worker-panics fail
    /// closed without relying on a stale process-local counter.
    pub(crate) fn synchronize_active(
        &self,
        active: impl IntoIterator<Item = (String, String)>,
    ) -> Result<(), String> {
        let mut state = self
            .active
            .lock()
            .map_err(|_| "The upload scheduler permit ledger is unavailable.".to_string())?;
        state.volume_by_item = active.into_iter().collect();
        Ok(())
    }

    pub(crate) fn select_fair(
        &self,
        candidates: impl IntoIterator<Item = UploadCandidate>,
    ) -> Result<Vec<UploadCandidate>, String> {
        let mut state = self
            .active
            .lock()
            .map_err(|_| "The upload scheduler permit ledger is unavailable.".to_string())?;
        let mut active_by_volume = HashMap::<String, usize>::new();
        for volume in state.volume_by_item.values() {
            *active_by_volume.entry(volume.clone()).or_default() += 1;
        }
        let slots = self.global_limit.saturating_sub(state.volume_by_item.len());
        if slots == 0 {
            return Ok(Vec::new());
        }
        let selected = select_fair_candidates(
            candidates,
            &active_by_volume,
            slots,
            state.last_selected_volume.as_deref(),
        );
        if let Some(last) = selected.last() {
            state.last_selected_volume = Some(last.volume_id.clone());
        }
        Ok(selected)
    }

    pub(crate) fn register_claimed(
        &self,
        claimed: impl IntoIterator<Item = (String, String)>,
    ) -> Result<(), String> {
        let mut state = self
            .active
            .lock()
            .map_err(|_| "The upload scheduler permit ledger is unavailable.".to_string())?;
        let claimed = claimed.into_iter().collect::<HashMap<_, _>>();
        let new_claims = claimed
            .keys()
            .filter(|item_id| !state.volume_by_item.contains_key(*item_id))
            .count();
        if state.volume_by_item.len().saturating_add(new_claims) > self.global_limit {
            return Err("The upload scheduler refused an over-capacity claim.".into());
        }
        state.volume_by_item.extend(claimed);
        Ok(())
    }

    pub(crate) fn release(&self, item_id: &str) {
        if let Ok(mut state) = self.active.lock() {
            state.volume_by_item.remove(item_id);
        }
    }

    pub(crate) fn begin_postprocess_worker(&self) -> bool {
        self.postprocess_worker_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn end_postprocess_worker(&self) {
        self.postprocess_worker_running
            .store(false, Ordering::Release);
    }
}

fn select_fair_candidates(
    candidates: impl IntoIterator<Item = UploadCandidate>,
    active_by_volume: &HashMap<String, usize>,
    slots: usize,
    last_selected_volume: Option<&str>,
) -> Vec<UploadCandidate> {
    // Keep recovered sessions at the front of every volume queue. Stable sort
    // preserves FIFO order among recoveries and among brand-new uploads.
    let mut candidates = candidates.into_iter().collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| !candidate.resume_from_youtube_session);
    let mut queues = Vec::<(String, VecDeque<UploadCandidate>)>::new();
    let mut queue_index = HashMap::<String, usize>::new();
    for candidate in candidates {
        let index = *queue_index
            .entry(candidate.volume_id.clone())
            .or_insert_with(|| {
                queues.push((candidate.volume_id.clone(), VecDeque::new()));
                queues.len() - 1
            });
        queues[index].1.push_back(candidate);
    }
    if let Some(last_selected_volume) = last_selected_volume {
        if let Some(last_index) = queues
            .iter()
            .position(|(volume_id, _)| volume_id == last_selected_volume)
        {
            let next_index = (last_index + 1) % queues.len();
            queues.rotate_left(next_index);
        }
    }

    let mut selected = Vec::with_capacity(slots);
    let mut selected_by_volume = HashMap::<String, usize>::new();
    while selected.len() < slots {
        // Do not start a new upload while a resumable session can still be
        // dispatched. A recovery blocked by its per-volume limit does not
        // waste unrelated capacity on other volumes.
        let resumable_recovery_available = queues.iter().any(|(volume_id, queue)| {
            queue.front().is_some_and(|candidate| {
                candidate.resume_from_youtube_session
                    && active_by_volume.get(volume_id).copied().unwrap_or(0)
                        + selected_by_volume.get(volume_id).copied().unwrap_or(0)
                        < candidate.volume_limit
            })
        });
        let mut made_progress = false;
        for (volume_id, queue) in &mut queues {
            if selected.len() == slots {
                break;
            }
            let Some(next) = queue.front() else { continue };
            if resumable_recovery_available && !next.resume_from_youtube_session {
                continue;
            }
            let active = active_by_volume.get(volume_id).copied().unwrap_or(0)
                + selected_by_volume.get(volume_id).copied().unwrap_or(0);
            if active >= next.volume_limit {
                continue;
            }
            let selected_candidate = queue.pop_front().expect("front candidate exists");
            *selected_by_volume.entry(volume_id.clone()).or_default() += 1;
            selected.push(selected_candidate);
            made_progress = true;
        }
        if !made_progress {
            break;
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_transport::ProviderTransport;
    use std::{
        fs::{self, File},
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Barrier, Mutex,
        },
        thread,
    };

    fn candidate(id: &str, volume: &str, limit: usize) -> UploadCandidate {
        UploadCandidate {
            item_id: id.into(),
            volume_id: volume.into(),
            volume_limit: limit,
            resume_from_youtube_session: false,
        }
    }

    fn resumable_candidate(id: &str, volume: &str, limit: usize) -> UploadCandidate {
        UploadCandidate {
            resume_from_youtube_session: true,
            ..candidate(id, volume, limit)
        }
    }

    #[test]
    fn fair_selection_rotates_volumes_and_honors_limits() {
        let scheduler = UploadScheduler::new(4);
        scheduler
            .synchronize_active([("a0".into(), "a".into())])
            .unwrap();
        let selected = scheduler
            .select_fair([
                candidate("a1", "a", 2),
                candidate("a2", "a", 2),
                candidate("b1", "b", 2),
                candidate("b2", "b", 2),
                candidate("c1", "c", 1),
            ])
            .unwrap();
        assert_eq!(
            selected
                .iter()
                .map(|candidate| candidate.item_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a1", "b1", "c1"]
        );
    }

    struct TransferFixture {
        root: PathBuf,
        media: PathBuf,
    }

    impl TransferFixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "youtube-uploader-task108-overlap-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&root).unwrap();
            let media = root.join("media.bin");
            let mut file = File::create(&media).unwrap();
            file.write_all(&vec![17_u8; 64 * 1024]).unwrap();
            Self { root, media }
        }
    }

    impl Drop for TransferFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        let header_end = loop {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0);
            request.extend_from_slice(&buffer[..read]);
            if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        while request.len() - header_end < content_length {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0);
            request.extend_from_slice(&buffer[..read]);
        }
        headers.lines().next().unwrap().to_string()
    }

    fn assert_barrier_overlap(global_limit: usize, per_volume_limit: usize) {
        let scheduler = Arc::new(UploadScheduler::new(global_limit));
        let candidates = (0..global_limit * 2)
            .map(|index| {
                candidate(
                    &format!("item-{index}"),
                    if index % 2 == 0 { "a" } else { "b" },
                    per_volume_limit,
                )
            })
            .collect::<Vec<_>>();
        let selected = scheduler.select_fair(candidates).unwrap();
        assert_eq!(selected.len(), global_limit.min(per_volume_limit * 2));
        scheduler
            .register_claimed(
                selected
                    .iter()
                    .map(|job| (job.item_id.clone(), job.volume_id.clone())),
            )
            .unwrap();
        let fixture = TransferFixture::new();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let barrier = Arc::new(Barrier::new(selected.len()));
        let active_total = Arc::new(AtomicUsize::new(0));
        let maximum_total = Arc::new(AtomicUsize::new(0));
        let active_by_volume = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
        let maximum_by_volume = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
        let server = {
            let barrier = barrier.clone();
            let active_total = active_total.clone();
            let maximum_total = maximum_total.clone();
            let active_by_volume = active_by_volume.clone();
            let maximum_by_volume = maximum_by_volume.clone();
            let expected = selected.len();
            thread::spawn(move || {
                let handlers = (0..expected)
                    .map(|_| {
                        let (mut stream, _) = listener.accept().unwrap();
                        let barrier = barrier.clone();
                        let active_total = active_total.clone();
                        let maximum_total = maximum_total.clone();
                        let active_by_volume = active_by_volume.clone();
                        let maximum_by_volume = maximum_by_volume.clone();
                        thread::spawn(move || {
                            let request_line = read_http_request(&mut stream);
                            let volume = request_line
                                .split_whitespace()
                                .nth(1)
                                .and_then(|path| path.trim_start_matches('/').split('/').next())
                                .unwrap()
                                .to_string();
                            let total = active_total.fetch_add(1, Ordering::SeqCst) + 1;
                            maximum_total.fetch_max(total, Ordering::SeqCst);
                            {
                                let mut active = active_by_volume.lock().unwrap();
                                let current = active.entry(volume.clone()).or_default();
                                *current += 1;
                                let mut maximum = maximum_by_volume.lock().unwrap();
                                maximum
                                    .entry(volume.clone())
                                    .and_modify(|value| *value = (*value).max(*current))
                                    .or_insert(*current);
                            }
                            // The provider-side barrier proves that every selected
                            // HTTP body arrived concurrently; a serial uploader
                            // cannot release this barrier.
                            barrier.wait();
                            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{\"id\":\"ok\"}").unwrap();
                            active_total.fetch_sub(1, Ordering::SeqCst);
                            *active_by_volume.lock().unwrap().get_mut(&volume).unwrap() -= 1;
                        })
                    })
                    .collect::<Vec<_>>();
                for handler in handlers {
                    handler.join().unwrap();
                }
            })
        };
        let transport = Arc::new(ProviderTransport::new());
        let workers = selected
            .into_iter()
            .map(|job| {
                let scheduler = scheduler.clone();
                let transport = transport.clone();
                let media = fixture.media.clone();
                thread::spawn(move || {
                    let url = format!("http://{address}/{}/{}", job.volume_id, job.item_id);
                    let response = transport
                        .put_upload_chunk(&url, &media, 0, 64 * 1024, 64 * 1024)
                        .unwrap();
                    assert!(response.status().is_success());
                    scheduler.release(&job.item_id);
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap();
        }
        server.join().unwrap();
        assert_eq!(maximum_total.load(Ordering::SeqCst), global_limit);
        assert!(maximum_by_volume
            .lock()
            .unwrap()
            .values()
            .all(|maximum| *maximum <= per_volume_limit));
        assert!(scheduler.active.lock().unwrap().volume_by_item.is_empty());
    }

    #[test]
    fn capacity_two_and_four_create_true_bounded_overlap() {
        assert_barrier_overlap(2, 1);
        assert_barrier_overlap(4, 2);
    }

    #[test]
    fn registering_duplicate_item_does_not_consume_an_extra_permit() {
        let scheduler = UploadScheduler::new(2);
        scheduler
            .register_claimed([("one".into(), "a".into()), ("one".into(), "a".into())])
            .unwrap();
        assert_eq!(scheduler.active.lock().unwrap().volume_by_item.len(), 1);
    }

    #[test]
    fn successive_handoffs_rotate_the_first_eligible_volume() {
        let scheduler = UploadScheduler::new(1);
        let candidates = || [candidate("a", "volume-a", 1), candidate("b", "volume-b", 1)];
        let first = scheduler.select_fair(candidates()).unwrap();
        assert_eq!(first[0].item_id, "a");
        let second = scheduler.select_fair(candidates()).unwrap();
        assert_eq!(second[0].item_id, "b");
        let third = scheduler.select_fair(candidates()).unwrap();
        assert_eq!(third[0].item_id, "a");
    }

    #[test]
    fn resumable_sessions_are_selected_before_new_uploads() {
        let scheduler = UploadScheduler::new(2);
        let selected = scheduler
            .select_fair([
                candidate("new-a", "volume-a", 2),
                candidate("new-b", "volume-b", 2),
                resumable_candidate("resume-a", "volume-a", 2),
            ])
            .unwrap();
        assert_eq!(selected[0].item_id, "resume-a");
        assert!(selected[0].resume_from_youtube_session);
        assert_eq!(selected[1].item_id, "new-a");
    }

    #[test]
    fn rejected_over_capacity_registration_does_not_mutate_the_ledger() {
        let scheduler = UploadScheduler::new(1);
        scheduler
            .register_claimed([("one".into(), "a".into())])
            .unwrap();
        assert!(scheduler
            .register_claimed([("two".into(), "b".into())])
            .is_err());
        let state = scheduler.active.lock().unwrap();
        assert_eq!(state.volume_by_item.len(), 1);
        assert_eq!(
            state.volume_by_item.get("one").map(String::as_str),
            Some("a")
        );
    }
}
