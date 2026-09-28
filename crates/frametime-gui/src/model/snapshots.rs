//! A presentation-only worker: requests and results contain no window or mutation handles.
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
};

use super::catalog_filter::catalog_row_matches_filter;

pub type Rows = Vec<(String, String, String)>;
#[derive(Debug, Clone, Default)]
pub struct ReadSnapshot {
    pub rows: Rows,
    /// Optional selectors, kept separate from display text. Execution revalidates current records.
    pub recovery_keys: BTreeMap<usize, String>,
}
impl From<Rows> for ReadSnapshot {
    fn from(rows: Rows) -> Self {
        Self {
            rows,
            recovery_keys: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Resource {
    Overview,
    History,
    Recovery,
    Video,
    Drivers,
}

#[derive(Debug, Clone)]
pub struct ReadRequest {
    pub resource: Resource,
    pub video: Option<(String, frametime_domain::video::VideoGoal)>,
}
impl ReadRequest {
    pub fn new(resource: Resource) -> Self {
        Self {
            resource,
            video: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotState {
    Loading,
    Ready,
    Empty,
    Stale(String),
    Failed(String),
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub state: SnapshotState,
    rows: Option<ReadSnapshot>,
    generation: u64,
}
impl Snapshot {
    /// Metadata and non-ready rows cannot be used as a recovery selection.
    pub fn selected_data_key(&self, row: usize, filter: &str) -> Option<String> {
        if self.state != SnapshotState::Ready {
            return None;
        }
        let index = row.checked_sub(1)?;
        let snapshot = self.rows.as_ref()?;
        let (source_index, _) = snapshot
            .rows
            .iter()
            .enumerate()
            .filter(|(_, (a, b, _))| catalog_row_matches_filter(a, b, filter))
            .nth(index)?;
        snapshot.recovery_keys.get(&source_index).cloned()
    }
    pub fn filtered(&self, filter: &str) -> Rows {
        let (label, detail) = match &self.state {
            SnapshotState::Loading => ("Loading", "Reading current state"),
            SnapshotState::Ready => ("Ready", "Last successful read"),
            SnapshotState::Empty => ("Empty", "No saved data"),
            SnapshotState::Stale(error) => ("Stale", error.as_str()),
            SnapshotState::Failed(error) => ("Failed", error.as_str()),
        };
        let mut rows = vec![("Snapshot".into(), label.into(), detail.into())];
        rows.extend(
            self.rows
                .iter()
                .flat_map(|snapshot| &snapshot.rows)
                .filter(|(a, b, _)| catalog_row_matches_filter(a, b, filter))
                .cloned(),
        );
        rows
    }
}
struct Pending {
    requests: BTreeMap<Resource, (u64, ReadRequest)>,
    closed: bool,
}
struct ReadResult {
    resource: Resource,
    generation: u64,
    rows: Result<ReadSnapshot, String>,
}
type Queue = Arc<(Mutex<Pending>, Condvar)>;

pub struct SnapshotWorker {
    queue: Queue,
    results: mpsc::Receiver<ReadResult>,
    snapshots: BTreeMap<Resource, Snapshot>,
    generation: u64,
}
impl SnapshotWorker {
    pub fn new(
        mut reader: impl FnMut(ReadRequest) -> Result<ReadSnapshot, String> + Send + 'static,
    ) -> Self {
        let queue = Arc::new((
            Mutex::new(Pending {
                requests: BTreeMap::new(),
                closed: false,
            }),
            Condvar::new(),
        ));
        let shared = Arc::clone(&queue);
        let (sender, results) = mpsc::channel();
        thread::spawn(move || {
            while let Some((generation, request)) = next_request(&shared) {
                let resource = request.resource;
                let rows =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reader(request)))
                        .unwrap_or_else(|_| Err("Read worker failed unexpectedly".into()));
                if sender
                    .send(ReadResult {
                        resource,
                        generation,
                        rows,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            queue,
            results,
            snapshots: BTreeMap::new(),
            generation: 0,
        }
    }
    pub fn request(&mut self, request: ReadRequest) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("presentation generation exhausted");
        let snapshot = self.snapshots.entry(request.resource).or_insert(Snapshot {
            state: SnapshotState::Loading,
            rows: None,
            generation: self.generation,
        });
        snapshot.generation = self.generation;
        snapshot.state = SnapshotState::Loading;
        let (lock, wake) = &*self.queue;
        lock.lock()
            .expect("read queue")
            .requests
            .insert(request.resource, (self.generation, request));
        wake.notify_one();
    }
    /// Invalidates even reads already finished but not yet consumed by the UI.
    pub fn invalidate(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("presentation generation exhausted");
        self.queue.0.lock().expect("read queue").requests.clear();
        for snapshot in self.snapshots.values_mut() {
            snapshot.generation = self.generation;
            snapshot.state = if snapshot.rows.is_some() {
                SnapshotState::Stale("Operation completed; refresh required".into())
            } else {
                SnapshotState::Loading
            };
        }
    }
    pub fn poll(&mut self) -> Vec<Resource> {
        let mut changed = Vec::new();
        while let Ok(result) = self.results.try_recv() {
            let Some(snapshot) = self.snapshots.get_mut(&result.resource) else {
                continue;
            };
            if snapshot.generation != result.generation {
                continue;
            }
            snapshot.state = match result.rows {
                Ok(rows) => {
                    let state = if rows.rows.is_empty() {
                        SnapshotState::Empty
                    } else {
                        SnapshotState::Ready
                    };
                    snapshot.rows = Some(rows);
                    state
                }
                Err(error) if snapshot.rows.is_some() => SnapshotState::Stale(error),
                Err(error) => SnapshotState::Failed(error),
            };
            changed.push(result.resource);
        }
        changed
    }
    pub fn snapshot(&self, resource: Resource) -> Option<&Snapshot> {
        self.snapshots.get(&resource)
    }
}
impl Drop for SnapshotWorker {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        let mut pending = lock.lock().expect("read queue");
        pending.closed = true;
        pending.requests.clear();
        wake.notify_one();
        // Never join a potentially blocked read on the UI thread. Results have no HWND.
    }
}
fn next_request(queue: &Queue) -> Option<(u64, ReadRequest)> {
    let (lock, wake) = &**queue;
    let mut pending = lock.lock().expect("read queue");
    loop {
        if pending.closed {
            return None;
        }
        if let Some((_, request)) = pending.requests.pop_first() {
            return Some(request);
        }
        pending = wake.wait(pending).expect("read queue");
    }
}
