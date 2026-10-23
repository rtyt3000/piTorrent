//! A fake backend with sample torrents (`PITORRENT_DEMO=1`), for developing and screenshotting
//! the interface without a network.

use super::*;

struct Sample {
    name: &'static str,
    files: &'static [(&'static [&'static str], u64)],
    percent: u64,
    state: State,
    down: u64,
    up: u64,
    peers: usize,
    error: Option<&'static str>,
}

const MB: u64 = 1 << 20;
const GB: u64 = 1 << 30;

const SAMPLES: &[Sample] = &[
    Sample { name: "debian-13.1.0-amd64-DVD-1.iso", files: &[(&["debian-13.1.0-amd64-DVD-1.iso"], 4 * GB)], percent: 64, state: State::Downloading, down: 8 * MB, up: 200 << 10, peers: 38, error: None },
    Sample { name: "fedora-workstation-live-43-x86_64.iso", files: &[(&["fedora-workstation-live-43-x86_64.iso"], 2300 * MB)], percent: 31, state: State::Downloading, down: 3 * MB, up: 100 << 10, peers: 17, error: None },
    Sample {
        name: "sintel-4k-blender-open-movie",
        files: &[(&["Sintel", "sintel_4k.mkv"], 2700 * MB), (&["Sintel", "sintel_en.srt"], 212 << 10), (&["Sintel", "extras", "poster.jpg"], 412 << 10)],
        percent: 88, state: State::Downloading, down: 5 * MB, up: 400 << 10, peers: 24, error: None,
    },
    Sample { name: "archlinux-2026.10.01-x86_64.iso", files: &[(&["archlinux-2026.10.01-x86_64.iso"], 1400 * MB)], percent: 100, state: State::Seeding, down: 0, up: MB + 100 << 10, peers: 9, error: None },
    Sample { name: "big-buck-bunny-1080p", files: &[(&["bbb", "bbb_1080p.mp4"], 900 * MB)], percent: 100, state: State::Completed, down: 0, up: 0, peers: 0, error: None },
    Sample { name: "ubuntu-24.04.3-desktop-amd64.iso", files: &[(&["ubuntu-24.04.3-desktop-amd64.iso"], 6200 * MB)], percent: 42, state: State::Paused, down: 0, up: 0, peers: 0, error: None },
    Sample { name: "tears-of-steel-1080p", files: &[(&["tears_of_steel_1080p.mkv"], 4800 * MB)], percent: 71, state: State::Error, down: 0, up: 0, peers: 0, error: Some("files not found on disk") },
    Sample { name: "A-very-long-torrent-name-that-keeps-going-and-going-and-going-until-it-has-to-be-cut-off-somewhere.iso", files: &[(&["long.iso"], 3 * GB)], percent: 12, state: State::Downloading, down: 1 * MB, up: 0, peers: 3, error: None },
];

pub struct DemoBackend {
    states: Mutex<Vec<State>>,
}

impl DemoBackend {
    pub fn new() -> Self {
        Self { states: Mutex::new(SAMPLES.iter().map(|s| s.state).collect()) }
    }
}

fn info(id: usize, s: &Sample, state: State) -> TorrentInfo {
    let total: u64 = s.files.iter().map(|f| f.1).sum();
    let live = state == State::Downloading || state == State::Seeding;
    let (down, up) = if live { (s.down, s.up) } else { (0, 0) };
    let progress = total * s.percent / 100;
    TorrentInfo {
        id,
        priority: 0,
        info_hash: format!("{:040x}", id * 7919),
        name: s.name.to_string(),
        folder: "C:\\Users\\demo\\Downloads\\Torrents".into(),
        total,
        progress,
        uploaded: progress / 3,
        state,
        finished: s.percent == 100,
        error: s.error.map(String::from),
        down_bps: down,
        up_bps: up,
        eta: (down > 0).then(|| (total - progress) / down),
        peers: if live { s.peers } else { 0 },
        pieces: (total / (2 * MB)).max(1) as u32,
    }
}

#[async_trait]
impl Backend for DemoBackend {
    fn set_limits(&self, _: u32, _: u32) {}

    fn snapshot(&self, selected: Option<usize>, want_peers: bool) -> Snapshot {
        let states = self.states.lock().unwrap().clone();
        let torrents: Vec<TorrentInfo> =
            SAMPLES.iter().zip(states).enumerate().map(|(i, (s, st))| info(i + 1, s, st)).collect();
        let detail = selected.and_then(|id| {
            let s = SAMPLES.get(id.checked_sub(1)?)?;
            let t = torrents.iter().find(|t| t.id == id)?;
            let done_files = s.percent as usize * s.files.len() / 100;
            let files = s
                .files
                .iter()
                .enumerate()
                .map(|(i, (path, len))| FileInfo {
                    components: path.iter().map(|c| c.to_string()).collect(),
                    length: *len,
                    included: i != 2 || s.files.len() < 3,
                    progress: if i < done_files { *len } else { len * s.percent / 100 },
                })
                .collect();
            let peers = if want_peers && t.peers > 0 {
                [("77.88.8.8:51413", "qBittorrent 5.1"), ("5.9.0.1:6881", "Transmission 4.1"), ("8.8.8.8:6881", "Deluge 2.2")]
                    .iter()
                    .enumerate()
                    .map(|(i, (addr, client))| PeerInfo {
                        addr: addr.to_string(),
                        client: client.to_string(),
                        state: "live".into(),
                        downloaded: (3 - i as u64) * 40 * MB,
                        uploaded: i as u64 * 5 * MB,
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let filled = PIECE_CELLS * s.percent as usize / 100;
            let cells = (0..PIECE_CELLS)
                .map(|i| if i < filled { 1.0 } else if i == filled { 0.5 } else { 0.0 })
                .collect();
            Some(Detail { id, files, peers, cells })
        });
        Snapshot {
            down_bps: torrents.iter().map(|t| t.down_bps).sum(),
            up_bps: torrents.iter().map(|t| t.up_bps).sum(),
            torrents,
            dht_nodes: Some(412),
            port: Some(51413),
            detail,
        }
    }

    async fn recheck(&self, _: usize, _: bool) -> Result<(), String> {
        Ok(())
    }

    async fn pause(&self, id: usize) -> Result<(), String> {
        if let Some(st) = self.states.lock().unwrap().get_mut(id - 1) {
            *st = if SAMPLES[id - 1].percent == 100 { State::Completed } else { State::Paused };
        }
        Ok(())
    }

    async fn resume(&self, id: usize) -> Result<(), String> {
        if let Some(st) = self.states.lock().unwrap().get_mut(id - 1) {
            *st = if SAMPLES[id - 1].percent == 100 { State::Seeding } else { State::Downloading };
        }
        Ok(())
    }

    async fn remove(&self, _: usize, _: bool) -> Result<(), String> {
        Ok(())
    }

    async fn set_only_files(&self, _: usize, _: Vec<usize>) -> Result<(), String> {
        Ok(())
    }

    async fn preview(&self, _: Vec<u8>) -> Result<Preview, String> {
        let s = &SAMPLES[2];
        Ok(Preview {
            name: s.name.into(),
            files: s.files.iter().map(|(p, l)| (p.iter().map(|c| c.to_string()).collect(), *l)).collect(),
        })
    }

    async fn preview_magnet(&self, _: String) -> Result<MagnetPreview, String> {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let preview = self.preview(Vec::new()).await?;
        Ok(MagnetPreview { preview, bytes: Vec::new(), trackers: Vec::new() })
    }

    async fn add_bytes(&self, _: Vec<u8>, _: AddOpts) -> Result<String, String> {
        Ok(String::new())
    }

    async fn add_magnet(&self, _: String, _: AddOpts) -> Result<String, String> {
        Ok(String::new())
    }

    async fn move_torrent(&self, _: usize, _: PathBuf) -> Result<(), String> {
        Ok(())
    }

    async fn create(&self, _: PathBuf, _: Vec<String>, _: Option<u32>) -> Result<(Vec<u8>, PathBuf), String> {
        Err("demo mode".into())
    }

    async fn shutdown(&self) {}
}
