//! Torrent engine boundary: the `Backend` trait wraps a concrete BitTorrent library,
//! `Engine` adds behaviour that must not depend on the library (the download queue).

mod demo;
mod rqbit;

use crate::settings::{Settings, app_dir};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// The piece map has one cell per piece up to this many (two rows); bigger torrents group pieces.
pub const PIECE_CELLS: usize = 128;
/// Cell value for pieces nobody asked for (they belong to deselected files).
pub const SKIPPED: f32 = -1.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Checking,
    Downloading,
    Seeding,
    Completed,
    Queued,
    Paused,
    Error,
}

#[derive(Clone, Debug)]
pub struct TorrentInfo {
    pub id: usize,
    /// Position in the queue, 0 is the highest priority.
    pub priority: usize,
    pub info_hash: String,
    pub name: String,
    pub folder: String,
    pub total: u64,
    pub progress: u64,
    pub uploaded: u64,
    pub state: State,
    pub finished: bool,
    pub error: Option<String>,
    pub down_bps: u64,
    pub up_bps: u64,
    pub eta: Option<u64>,
    pub peers: usize,
    pub pieces: u32,
}

#[derive(Clone, Debug)]
pub struct FileInfo {
    pub components: Vec<String>,
    pub length: u64,
    pub included: bool,
    pub progress: u64,
}

#[derive(Clone, Debug)]
pub struct PeerInfo {
    pub addr: String,
    pub client: String,
    pub state: String,
    pub downloaded: u64,
    pub uploaded: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Detail {
    pub id: usize,
    pub files: Vec<FileInfo>,
    pub peers: Vec<PeerInfo>,
    /// Share of wanted pieces downloaded for each cell of the piece map (0..=1, or `SKIPPED`).
    pub cells: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub torrents: Vec<TorrentInfo>,
    pub down_bps: u64,
    pub up_bps: u64,
    pub dht_nodes: Option<usize>,
    pub port: Option<u16>,
    pub detail: Option<Detail>,
}

/// File list of a .torrent before adding it.
#[derive(Clone, Debug)]
pub struct Preview {
    pub name: String,
    pub files: Vec<(Vec<String>, u64)>,
}

pub struct AddOpts {
    pub folder: String,
    pub start: bool,
    pub only_files: Option<Vec<usize>>,
    /// Extra trackers, e.g. the ones from a magnet link whose metadata was fetched separately.
    pub trackers: Vec<String>,
}

/// The result of resolving a magnet link: its file list and the .torrent to add it by.
#[derive(Clone, Debug)]
pub struct MagnetPreview {
    pub preview: Preview,
    pub bytes: Vec<u8>,
    pub trackers: Vec<String>,
}

#[async_trait]
pub trait Backend: Send + Sync {
    fn set_limits(&self, down_mb: u32, up_mb: u32);
    fn snapshot(&self, selected: Option<usize>, want_peers: bool) -> Snapshot;
    async fn pause(&self, id: usize) -> Result<(), String>;
    async fn resume(&self, id: usize) -> Result<(), String>;
    /// Hashes all the files again; with `resume_after` the torrent is started once the check is done.
    async fn recheck(&self, id: usize, resume_after: bool) -> Result<(), String>;
    async fn remove(&self, id: usize, delete_files: bool) -> Result<(), String>;
    async fn set_only_files(&self, id: usize, files: Vec<usize>) -> Result<(), String>;
    async fn preview(&self, bytes: Vec<u8>) -> Result<Preview, String>;
    /// Fetches the metadata of a magnet link from the network without adding the torrent.
    async fn preview_magnet(&self, link: String) -> Result<MagnetPreview, String>;
    /// Both add methods return the info hash of the (possibly already known) torrent.
    async fn add_bytes(&self, bytes: Vec<u8>, o: AddOpts) -> Result<String, String>;
    async fn add_magnet(&self, link: String, o: AddOpts) -> Result<String, String>;
    /// Moves the data to `base` (a download folder, not the torrent's own sub-folder).
    async fn move_torrent(&self, id: usize, base: PathBuf) -> Result<(), String>;
    /// Returns the .torrent bytes and the folder that holds the data.
    async fn create(
        &self,
        path: PathBuf,
        trackers: Vec<String>,
        piece_length: Option<u32>,
    ) -> Result<(Vec<u8>, PathBuf), String>;
    async fn shutdown(&self);
}

pub type Shared = Arc<Engine>;

pub struct Engine {
    backend: Box<dyn Backend>,
    queue: Mutex<Queue>,
    /// The folder the backend puts magnet downloads into until their metadata is known.
    default_dir: PathBuf,
    /// Magnet torrents that should be moved to this folder once their metadata is known.
    pending_moves: Mutex<HashMap<String, PathBuf>>,
    /// Smoothed download speed per torrent, for an ETA that doesn't jump with every spike.
    speeds: Mutex<HashMap<usize, (f64, Instant)>>,
}

/// How long (seconds) a speed sample keeps weighing on the average.
const ETA_WINDOW: f64 = 30.0;

/// Torrents are queued by pausing the lowest-priority ones above the limits; the ids paused by
/// the queue (as opposed to by the user) are remembered so they can be resumed automatically.
struct Queue {
    max_downloads: usize,
    max_seeds: usize,
    queued: HashSet<usize>,
    /// Started manually by the user: never paused by the queue.
    forced: HashSet<usize>,
    /// Torrent ids from the highest to the lowest priority.
    order: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PriorityMove {
    Top,
    Up,
    Down,
    Bottom,
}

#[derive(Serialize, Deserialize, Default)]
struct QueueFile {
    queued: Vec<usize>,
    order: Vec<usize>,
}

fn queue_file() -> PathBuf {
    app_dir().join("queue.json")
}

impl Queue {
    fn load() -> (HashSet<usize>, Vec<usize>) {
        let text = std::fs::read_to_string(queue_file()).unwrap_or_default();
        if let Ok(f) = serde_json::from_str::<QueueFile>(&text) {
            return (f.queued.into_iter().collect(), f.order);
        }
        // Older format: just the list of queued ids.
        let queued: Vec<usize> = serde_json::from_str(&text).unwrap_or_default();
        (queued.into_iter().collect(), Vec::new())
    }

    fn save(&self) {
        let file = QueueFile {
            queued: self.queued.iter().copied().collect(),
            order: self.order.clone(),
        };
        if let Ok(json) = serde_json::to_string(&file) {
            let _ = std::fs::create_dir_all(app_dir());
            let _ = std::fs::write(queue_file(), json);
        }
    }

    /// Keeps `order` in sync with the existing torrents: drops removed ones, appends new ones.
    fn sync_order(&mut self, ids: &[usize]) {
        self.order.retain(|id| ids.contains(id));
        let mut fresh: Vec<usize> = ids.iter().copied().filter(|id| !self.order.contains(id)).collect();
        fresh.sort_unstable();
        self.order.extend(fresh);
    }

    fn rank(&self, id: usize) -> usize {
        self.order.iter().position(|&x| x == id).unwrap_or(usize::MAX)
    }

    fn apply_move(&mut self, ids: &[usize], mv: PriorityMove) {
        let selected = |id: &usize| ids.contains(id);
        match mv {
            PriorityMove::Top => {
                let (mut first, rest): (Vec<usize>, Vec<usize>) = self.order.iter().partition(|id| selected(id));
                first.extend(rest);
                self.order = first;
            }
            PriorityMove::Bottom => {
                let (picked, mut rest): (Vec<usize>, Vec<usize>) = self.order.iter().partition(|id| selected(id));
                rest.extend(picked);
                self.order = rest;
            }
            PriorityMove::Up => {
                for i in 1..self.order.len() {
                    if selected(&self.order[i]) && !selected(&self.order[i - 1]) {
                        self.order.swap(i - 1, i);
                    }
                }
            }
            PriorityMove::Down => {
                for i in (0..self.order.len().saturating_sub(1)).rev() {
                    if selected(&self.order[i]) && !selected(&self.order[i + 1]) {
                        self.order.swap(i, i + 1);
                    }
                }
            }
        }
    }
}

fn is_active(t: &TorrentInfo) -> bool {
    match t.state {
        State::Downloading | State::Checking => !t.finished,
        State::Seeding => true,
        _ => false,
    }
}

impl Engine {
    pub async fn start(s: &Settings) -> Result<Shared, String> {
        let backend: Box<dyn Backend> = if std::env::var_os("PITORRENT_DEMO").is_some() {
            Box::new(demo::DemoBackend::new())
        } else {
            Box::new(rqbit::RqbitBackend::start(s).await?)
        };
        let (queued, order) = Queue::load();
        Ok(Arc::new(Engine {
            backend,
            default_dir: PathBuf::from(&s.download_dir),
            pending_moves: Mutex::new(HashMap::new()),
            speeds: Mutex::new(HashMap::new()),
            queue: Mutex::new(Queue {
                max_downloads: s.max_downloads as usize,
                max_seeds: s.max_seeds as usize,
                queued,
                forced: HashSet::new(),
                order,
            }),
        }))
    }

    pub fn set_limits(&self, down_mb: u32, up_mb: u32) {
        self.backend.set_limits(down_mb, up_mb);
    }

    /// 0 means unlimited.
    pub fn set_queue_limits(&self, downloads: u32, seeds: u32) {
        let mut q = self.queue.lock().unwrap();
        q.max_downloads = downloads as usize;
        q.max_seeds = seeds as usize;
    }

    pub async fn snapshot(self: &Arc<Self>, selected: Option<usize>, want_peers: bool) -> Snapshot {
        let mut snap = self.backend.snapshot(selected, want_peers);
        self.start_pending_moves(&snap);
        self.rebalance(&mut snap).await;
        self.smooth_eta(&mut snap);
        snap
    }

    /// Replaces the instantaneous ETA with one based on an exponential moving average of the speed.
    fn smooth_eta(&self, snap: &mut Snapshot) {
        let now = Instant::now();
        let mut speeds = self.speeds.lock().unwrap();
        speeds.retain(|id, _| snap.torrents.iter().any(|t| t.id == *id && t.state == State::Downloading));
        for t in snap.torrents.iter_mut().filter(|t| t.state == State::Downloading) {
            let cur = t.down_bps as f64;
            let avg = match speeds.get(&t.id) {
                Some(&(avg, at)) => {
                    let k = 1.0 - (-now.duration_since(at).as_secs_f64() / ETA_WINDOW).exp();
                    avg + (cur - avg) * k
                }
                // Don't start from zero: the first sample is all there is.
                None if cur > 0.0 => cur,
                None => continue,
            };
            speeds.insert(t.id, (avg, now));
            let left = t.total.saturating_sub(t.progress);
            t.eta = (avg >= 1.0 && left > 0).then(|| (left as f64 / avg) as u64);
        }
    }

    fn start_pending_moves(self: &Arc<Self>, snap: &Snapshot) {
        let ready: Vec<(usize, PathBuf)> = {
            let mut pending = self.pending_moves.lock().unwrap();
            if pending.is_empty() {
                return;
            }
            snap.torrents
                .iter()
                .filter(|t| t.pieces > 0)
                .filter_map(|t| pending.remove(&t.info_hash).map(|dest| (t.id, dest)))
                .collect()
        };
        for (id, dest) in ready {
            let engine = self.clone();
            tokio::spawn(async move {
                let _ = engine.backend.move_torrent(id, dest).await;
            });
        }
    }

    pub async fn move_torrent(&self, id: usize, base: PathBuf) -> Result<(), String> {
        self.backend.move_torrent(id, base).await
    }

    async fn rebalance(&self, snap: &mut Snapshot) {
        let (to_pause, to_resume) = {
            let mut q = self.queue.lock().unwrap();
            let (queued_before, order_before) = (q.queued.clone(), q.order.clone());

            let ids: Vec<usize> = snap.torrents.iter().map(|t| t.id).collect();
            q.sync_order(&ids);
            let find = |id: usize| snap.torrents.iter().find(|t| t.id == id);
            q.queued
                .retain(|&id| find(id).is_some_and(|t| matches!(t.state, State::Paused | State::Completed)));
            q.forced.retain(|&id| find(id).is_some());

            let (mut to_pause, mut to_resume) = (Vec::new(), Vec::new());
            for (finished, max) in [(false, q.max_downloads), (true, q.max_seeds)] {
                let in_group = |t: &&TorrentInfo| t.finished == finished;
                let active: Vec<usize> = snap.torrents.iter().filter(in_group).filter(|t| is_active(t)).map(|t| t.id).collect();
                let waiting: Vec<usize> = snap
                    .torrents
                    .iter()
                    .filter(in_group)
                    .map(|t| t.id)
                    .filter(|id| q.queued.contains(id))
                    .collect();

                // Manually started torrents always run and use up slots; the remaining slots
                // go to the highest-priority candidates, running or waiting.
                let forced_running = active.iter().filter(|id| q.forced.contains(id)).count();
                let slots = if max == 0 { usize::MAX } else { max.saturating_sub(forced_running) };
                let mut candidates: Vec<usize> = active
                    .iter()
                    .chain(&waiting)
                    .copied()
                    .filter(|id| !q.forced.contains(id))
                    .collect();
                candidates.sort_by_key(|&id| q.rank(id));
                candidates.truncate(slots);

                to_pause.extend(active.iter().copied().filter(|id| !q.forced.contains(id) && !candidates.contains(id)));
                to_resume.extend(waiting.iter().copied().filter(|id| candidates.contains(id)));
            }

            for id in &to_pause {
                q.queued.insert(*id);
            }
            for id in &to_resume {
                q.queued.remove(id);
            }
            if q.queued != queued_before || q.order != order_before {
                q.save();
            }
            (to_pause, to_resume)
        };

        for id in &to_pause {
            let _ = self.backend.pause(*id).await;
        }
        for id in &to_resume {
            let _ = self.backend.resume(*id).await;
        }

        let q = self.queue.lock().unwrap();
        for t in &mut snap.torrents {
            t.priority = q.rank(t.id);
            if to_resume.contains(&t.id) {
                t.state = if t.finished { State::Seeding } else { State::Downloading };
            } else if q.queued.contains(&t.id) {
                t.state = State::Queued;
                t.down_bps = 0;
                t.up_bps = 0;
                t.eta = None;
            }
        }
    }

    /// Changes the priority of the given torrents; the queue applies it on the next snapshot.
    pub fn move_priority(&self, ids: &[usize], mv: PriorityMove) {
        let mut q = self.queue.lock().unwrap();
        q.apply_move(ids, mv);
        q.save();
    }

    pub async fn pause(&self, id: usize) -> Result<(), String> {
        {
            let mut q = self.queue.lock().unwrap();
            q.queued.remove(&id);
            q.forced.remove(&id);
        }
        self.backend.pause(id).await
    }

    /// A manual resume ignores the queue limits for this torrent.
    pub async fn resume(&self, id: usize) -> Result<(), String> {
        {
            let mut q = self.queue.lock().unwrap();
            q.queued.remove(&id);
            q.forced.insert(id);
        }
        self.backend.resume(id).await
    }

    pub async fn recheck(&self, id: usize, resume_after: bool) -> Result<(), String> {
        {
            let mut q = self.queue.lock().unwrap();
            q.queued.remove(&id);
        }
        self.backend.recheck(id, resume_after).await
    }

    pub async fn remove(&self, id: usize, delete_files: bool) -> Result<(), String> {
        {
            let mut q = self.queue.lock().unwrap();
            q.queued.remove(&id);
            q.forced.remove(&id);
        }
        self.backend.remove(id, delete_files).await
    }

    pub async fn set_only_files(&self, id: usize, files: Vec<usize>) -> Result<(), String> {
        self.backend.set_only_files(id, files).await
    }

    pub async fn preview(&self, bytes: Vec<u8>) -> Result<Preview, String> {
        self.backend.preview(bytes).await
    }

    pub async fn preview_magnet(&self, link: String) -> Result<MagnetPreview, String> {
        self.backend.preview_magnet(link).await
    }

    pub async fn add_bytes(&self, bytes: Vec<u8>, o: AddOpts) -> Result<String, String> {
        self.backend.add_bytes(bytes, o).await
    }

    pub async fn add_magnet(&self, link: String, o: AddOpts) -> Result<String, String> {
        let base = PathBuf::from(&o.folder);
        let hash = self.backend.add_magnet(link, o).await?;
        if base != self.default_dir {
            self.pending_moves.lock().unwrap().insert(hash.clone(), base);
        }
        Ok(hash)
    }

    pub async fn create(
        &self,
        path: PathBuf,
        trackers: Vec<String>,
        piece_length: Option<u32>,
    ) -> Result<(Vec<u8>, PathBuf), String> {
        self.backend.create(path, trackers, piece_length).await
    }

    pub async fn shutdown(&self) {
        self.backend.shutdown().await;
    }
}

pub fn dir_size(p: &Path) -> u64 {
    if p.is_file() {
        return std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    }
    std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| dir_size(&e.path())).sum())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        torrents: Mutex<Vec<TorrentInfo>>,
    }

    impl Fake {
        fn set_state(&self, id: usize, state: State) {
            let mut v = self.torrents.lock().unwrap();
            v.iter_mut().find(|t| t.id == id).unwrap().state = state;
        }
    }

    #[async_trait]
    impl Backend for Arc<Fake> {
        fn set_limits(&self, _: u32, _: u32) {}
        fn snapshot(&self, _: Option<usize>, _: bool) -> Snapshot {
            Snapshot { torrents: self.torrents.lock().unwrap().clone(), ..Default::default() }
        }
        async fn pause(&self, id: usize) -> Result<(), String> {
            let finished = self.torrents.lock().unwrap().iter().find(|t| t.id == id).unwrap().finished;
            self.set_state(id, if finished { State::Completed } else { State::Paused });
            Ok(())
        }
        async fn resume(&self, id: usize) -> Result<(), String> {
            let finished = self.torrents.lock().unwrap().iter().find(|t| t.id == id).unwrap().finished;
            self.set_state(id, if finished { State::Seeding } else { State::Downloading });
            Ok(())
        }
        async fn recheck(&self, _: usize, _: bool) -> Result<(), String> { Ok(()) }
        async fn remove(&self, id: usize, _: bool) -> Result<(), String> {
            self.torrents.lock().unwrap().retain(|t| t.id != id);
            Ok(())
        }
        async fn set_only_files(&self, _: usize, _: Vec<usize>) -> Result<(), String> { Ok(()) }
        async fn preview(&self, _: Vec<u8>) -> Result<Preview, String> { unimplemented!() }
        async fn preview_magnet(&self, _: String) -> Result<MagnetPreview, String> { unimplemented!() }
        async fn add_bytes(&self, _: Vec<u8>, _: AddOpts) -> Result<String, String> { Ok(String::new()) }
        async fn add_magnet(&self, _: String, _: AddOpts) -> Result<String, String> { Ok(String::new()) }
        async fn move_torrent(&self, _: usize, _: PathBuf) -> Result<(), String> { Ok(()) }
        async fn create(&self, _: PathBuf, _: Vec<String>, _: Option<u32>) -> Result<(Vec<u8>, PathBuf), String> {
            unimplemented!()
        }
        async fn shutdown(&self) {}
    }

    fn torrent(id: usize, state: State, finished: bool) -> TorrentInfo {
        TorrentInfo {
            id, priority: 0, info_hash: String::new(), name: format!("t{id}"), folder: String::new(), total: 100, progress: 0, uploaded: 0,
            state, finished, error: None, down_bps: 1, up_bps: 1, eta: None, peers: 0, pieces: 1,
        }
    }

    fn engine(fake: &Arc<Fake>, downloads: usize, seeds: usize) -> Arc<Engine> {
        Arc::new(Engine {
            backend: Box::new(fake.clone()),
            default_dir: PathBuf::new(),
            pending_moves: Mutex::new(HashMap::new()),
            speeds: Mutex::new(HashMap::new()),
            queue: Mutex::new(Queue {
                max_downloads: downloads,
                max_seeds: seeds,
                queued: HashSet::new(),
                forced: HashSet::new(),
                order: Vec::new(),
            }),
        })
    }

    fn states(s: &Snapshot) -> Vec<State> {
        s.torrents.iter().map(|t| t.state).collect()
    }

    #[tokio::test]
    async fn pauses_newest_over_limit_and_resumes_when_slot_frees() {
        let fake = Arc::new(Fake::default());
        *fake.torrents.lock().unwrap() = (1..=5).map(|i| torrent(i, State::Downloading, false)).collect();
        let e = engine(&fake, 3, 0);

        let s = e.snapshot(None, false).await;
        assert_eq!(
            states(&s),
            [State::Downloading, State::Downloading, State::Downloading, State::Queued, State::Queued]
        );

        // Stays stable on the next tick.
        let s = e.snapshot(None, false).await;
        assert_eq!(states(&s)[3..], [State::Queued, State::Queued]);

        // Torrent 1 finishes (becomes a seed): the oldest queued one takes the slot.
        {
            let mut v = fake.torrents.lock().unwrap();
            v[0].state = State::Seeding;
            v[0].finished = true;
        }
        let s = e.snapshot(None, false).await;
        assert_eq!(s.torrents[3].state, State::Downloading);
        assert_eq!(s.torrents[4].state, State::Queued);
    }

    #[tokio::test]
    async fn user_paused_torrents_are_not_resumed_by_queue() {
        let fake = Arc::new(Fake::default());
        *fake.torrents.lock().unwrap() =
            vec![torrent(1, State::Downloading, false), torrent(2, State::Paused, false)];
        let e = engine(&fake, 3, 0);
        let s = e.snapshot(None, false).await;
        assert_eq!(states(&s), [State::Downloading, State::Paused]);
    }

    #[tokio::test]
    async fn manual_resume_is_never_queued() {
        let fake = Arc::new(Fake::default());
        *fake.torrents.lock().unwrap() = (1..=3).map(|i| torrent(i, State::Downloading, false)).collect();
        let e = engine(&fake, 2, 0);
        let s = e.snapshot(None, false).await;
        assert_eq!(s.torrents[2].state, State::Queued);

        e.resume(3).await.unwrap();
        let s = e.snapshot(None, false).await;
        assert_eq!(s.torrents[2].state, State::Downloading);
        // A forced torrent counts as active, so another (newer-than-limit) one is queued instead.
        assert_eq!(states(&s).iter().filter(|&&x| x == State::Queued).count(), 1);
    }

    #[tokio::test]
    async fn seeds_have_their_own_limit() {
        let fake = Arc::new(Fake::default());
        *fake.torrents.lock().unwrap() = vec![
            torrent(1, State::Seeding, true),
            torrent(2, State::Seeding, true),
            torrent(3, State::Downloading, false),
        ];
        let e = engine(&fake, 3, 1);
        let s = e.snapshot(None, false).await;
        assert_eq!(states(&s), [State::Seeding, State::Queued, State::Downloading]);
    }

    #[tokio::test]
    async fn zero_means_unlimited() {
        let fake = Arc::new(Fake::default());
        *fake.torrents.lock().unwrap() = (1..=6).map(|i| torrent(i, State::Downloading, false)).collect();
        let e = engine(&fake, 0, 0);
        let s = e.snapshot(None, false).await;
        assert!(states(&s).iter().all(|&x| x == State::Downloading));
    }

    #[tokio::test]
    async fn moving_to_top_pushes_out_the_lowest_priority() {
        let fake = Arc::new(Fake::default());
        *fake.torrents.lock().unwrap() = (1..=4).map(|i| torrent(i, State::Downloading, false)).collect();
        let e = engine(&fake, 2, 0);
        let s = e.snapshot(None, false).await;
        assert_eq!(states(&s), [State::Downloading, State::Downloading, State::Queued, State::Queued]);

        e.move_priority(&[4], PriorityMove::Top);
        let s = e.snapshot(None, false).await;
        assert_eq!(states(&s), [State::Downloading, State::Queued, State::Queued, State::Downloading]);
        assert_eq!(s.torrents[3].priority, 0);
    }

    #[test]
    fn priority_moves_keep_relative_order() {
        let mut q = Queue {
            max_downloads: 0,
            max_seeds: 0,
            queued: HashSet::new(),
            forced: HashSet::new(),
            order: vec![1, 2, 3, 4, 5],
        };
        q.apply_move(&[3, 5], PriorityMove::Top);
        assert_eq!(q.order, [3, 5, 1, 2, 4]);
        q.apply_move(&[3, 5], PriorityMove::Bottom);
        assert_eq!(q.order, [1, 2, 4, 3, 5]);
        q.apply_move(&[4, 5], PriorityMove::Up);
        assert_eq!(q.order, [1, 4, 2, 5, 3]);
        q.apply_move(&[1, 4], PriorityMove::Up);
        assert_eq!(q.order, [1, 4, 2, 5, 3]);
        q.apply_move(&[1], PriorityMove::Down);
        assert_eq!(q.order, [4, 1, 2, 5, 3]);
    }
}
