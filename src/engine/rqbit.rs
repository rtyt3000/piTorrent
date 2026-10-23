use super::*;
use librqbit::api::{Api, TorrentIdOrHash};
use librqbit::limits::LimitsConfig;
use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, ConnectionOptions, CreateTorrentOptions, Magnet, PeerConnectionOptions, ListenerMode, ListenerOptions, Session,
    SessionOptions, SessionPersistenceConfig, TorrentStatsState, create_torrent,
};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::num::NonZeroU32;
use std::time::Duration;

pub struct RqbitBackend {
    api: Api,
    session: Arc<Session>,
}

/// Where librqbit puts the files of a torrent: multi-file torrents get a sub-folder named
/// after the torrent, single-file ones go straight into `base`.
fn output_folder(base: &str, name: &str, file_count: usize) -> String {
    let base = PathBuf::from(base);
    let out = if file_count >= 2 {
        let leaf = Path::new(name).file_name().map(|n| n.to_owned()).unwrap_or_else(|| name.into());
        base.join(leaf)
    } else {
        base
    };
    out.to_string_lossy().into_owned()
}

fn move_files(from: &Path, to: &Path, files: &[Vec<String>], remove_root: bool) -> Result<(), String> {
    let full = |root: &Path, rel: &Vec<String>| rel.iter().fold(root.to_path_buf(), |p, c| p.join(c));
    for rel in files {
        let (src, dst) = (full(from, rel), full(to, rel));
        if !src.exists() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if std::fs::rename(&src, &dst).is_err() {
            std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
            std::fs::remove_file(&src).map_err(|e| e.to_string())?;
        }
    }
    for rel in files {
        let mut dir = full(from, rel).parent().map(Path::to_path_buf);
        while let Some(d) = dir {
            if !d.starts_with(from) || (d == from && !remove_root) || std::fs::remove_dir(&d).is_err() {
                break;
            }
            dir = d.parent().map(Path::to_path_buf);
        }
    }
    Ok(())
}

/// Carries the hidden `.parts` file (the edges of deselected files, see the vendored storage)
/// along with a moved torrent; best effort, a missing one is simply re-downloaded.
fn move_parts(old_folder: &Path, new_folder: &Path, multi: bool, info_hash: &str) {
    let parts = |folder: &Path| {
        let base = if multi { folder.parent().unwrap_or(folder) } else { folder };
        base.join(".parts")
    };
    let name = format!(".{info_hash}.parts");
    let (from, to) = (parts(old_folder), parts(new_folder));
    if from == to || !from.join(&name).exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&to);
    if std::fs::rename(from.join(&name), to.join(&name)).is_err() {
        if std::fs::copy(from.join(&name), to.join(&name)).is_ok() {
            let _ = std::fs::remove_file(from.join(&name));
        }
    }
    let _ = std::fs::remove_dir(&from);
}

/// Public trackers added to every torrent: many magnet links carry none, and DHT alone
/// is slow to find peers for big or rarely-seeded torrents.
const DEFAULT_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.tiny-vps.com:6969/announce",
    "udp://explodie.org:6969/announce",
    "udp://tracker.dler.org:6969/announce",
];

fn limit(mb: u32) -> Option<NonZeroU32> {
    NonZeroU32::new(mb.saturating_mul(1024 * 1024))
}

impl RqbitBackend {
    pub async fn start(s: &Settings) -> Result<Self, String> {
        let _ = std::fs::create_dir_all(&s.download_dir);
        let opts = SessionOptions {
            dht: s.dht.then(Default::default),
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(app_dir().join("session")),
            }),
            fastresume: true,
            listen: Some(ListenerOptions {
                mode: ListenerMode::TcpAndUtp,
                listen_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, s.port)),
                enable_upnp_port_forwarding: true,
                ..Default::default()
            }),
            trackers: DEFAULT_TRACKERS.iter().filter_map(|t| t.parse().ok()).collect(),
            peer_limit: Some(500),
            // Peers of busy swarms are slow to answer; the 10 s defaults drop many of them
            // while the (large) metadata is still being fetched.
            connect: Some(ConnectionOptions {
                peer_opts: Some(PeerConnectionOptions {
                    connect_timeout: Some(Duration::from_secs(20)),
                    read_write_timeout: Some(Duration::from_secs(60)),
                    keep_alive_interval: None,
                }),
                ..Default::default()
            }),
            ratelimits: LimitsConfig {
                upload_bps: limit(s.up_limit),
                download_bps: limit(s.down_limit),
            },
            ..Default::default()
        };
        let session = Session::new_with_opts(PathBuf::from(&s.download_dir), opts)
            .await
            .map_err(|e| format!("{e:#}"))?;
        Ok(Self {
            api: Api::new(session.clone(), None),
            session,
        })
    }

    fn detail(&self, id: usize, want_peers: bool, torrents: &[TorrentInfo]) -> Option<Detail> {
        let idx = TorrentIdOrHash::Id(id);
        let d = self.api.api_torrent_details(idx).ok()?;
        let stats = self.api.api_stats_v1(idx).ok();
        let progress = stats.as_ref().map(|s| s.file_progress.clone()).unwrap_or_default();
        let files = d
            .files
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, f)| FileInfo {
                components: f.components,
                length: f.length,
                included: f.included,
                progress: progress.get(i).copied().unwrap_or(0),
            })
            .collect();

        let peers = if want_peers {
            self.api
                .api_peer_stats(idx, Default::default())
                .map(|snap| {
                    let mut v: Vec<_> = snap
                        .peers
                        .into_iter()
                        .map(|(addr, p)| PeerInfo {
                            addr,
                            client: p.client_name.unwrap_or_else(|| "—".into()),
                            state: p.state.to_string(),
                            downloaded: p.counters.fetched_bytes,
                            uploaded: p.counters.uploaded_bytes,
                        })
                        .collect();
                    v.sort_by(|a, b| b.downloaded.cmp(&a.downloaded).then(a.addr.cmp(&b.addr)));
                    v
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let cells = match self.api.api_dump_haves(idx) {
            Ok((bf, total)) if total > 0 => {
                // Without the selection every piece counts as wanted.
                let wanted = self.api.api_dump_selected(idx).ok();
                cells_from_bitfield(
                    |k| bf.get(k).map(|x| *x).unwrap_or(false),
                    |k| wanted.as_ref().is_none_or(|w| w.get(k).map(|x| *x).unwrap_or(true)),
                    total as usize,
                )
            }
            _ => {
                let frac = torrents
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| if t.finished { 1.0 } else { t.progress as f32 / t.total.max(1) as f32 })
                    .unwrap_or(0.0);
                vec![frac; PIECE_CELLS]
            }
        };
        Some(Detail { id, files, peers, cells })
    }

    async fn add(&self, t: AddTorrent<'_>, o: AddOpts, output: Option<String>) -> Result<String, String> {
        self.api
            .api_add_torrent(
                t,
                Some(AddTorrentOptions {
                    paused: !o.start,
                    overwrite: true,
                    output_folder: output,
                    only_files: o.only_files,
                    trackers: (!o.trackers.is_empty()).then_some(o.trackers),
                    ..Default::default()
                }),
            )
            .await
            .map(|r| r.details.info_hash)
            .map_err(|e| e.to_string())
    }
}

#[async_trait]
impl Backend for RqbitBackend {
    fn set_limits(&self, down_mb: u32, up_mb: u32) {
        self.session.ratelimits.set_download_bps(limit(down_mb));
        self.session.ratelimits.set_upload_bps(limit(up_mb));
    }

    fn snapshot(&self, selected: Option<usize>, want_peers: bool) -> Snapshot {
        let list = self
            .api
            .api_torrent_list_ext(librqbit::api::ApiTorrentListOpts { with_stats: true });
        let mut out = Snapshot::default();
        for t in list.torrents {
            let (Some(id), Some(st)) = (t.id, t.stats) else {
                continue;
            };
            let (down, up, peers) = match &st.live {
                Some(l) => (
                    l.download_speed.as_bytes(),
                    l.upload_speed.as_bytes(),
                    l.snapshot.peer_stats.live as usize,
                ),
                None => (0, 0, 0),
            };
            let state = match st.state {
                TorrentStatsState::Error => State::Error,
                TorrentStatsState::Paused if st.finished => State::Completed,
                TorrentStatsState::Paused => State::Paused,
                TorrentStatsState::Initializing { .. } => State::Checking,
                TorrentStatsState::Live if st.finished => State::Seeding,
                TorrentStatsState::Live => State::Downloading,
            };
            let left = st.total_bytes.saturating_sub(st.progress_bytes);
            out.down_bps += down;
            out.up_bps += up;
            out.torrents.push(TorrentInfo {
                id,
                priority: 0,
                info_hash: t.info_hash.clone(),
                name: t.name.unwrap_or_else(|| crate::i18n::trf("torrent.fetching", &[t.info_hash.clone()])),
                folder: t.output_folder,
                total: st.total_bytes,
                progress: st.progress_bytes,
                uploaded: st.uploaded_bytes,
                state,
                finished: st.finished,
                error: st.error,
                down_bps: down,
                up_bps: up,
                eta: (down > 0 && left > 0).then(|| left / down),
                peers,
                pieces: t.total_pieces,
            });
        }
        out.torrents.sort_by_key(|t| t.id);
        out.dht_nodes = self.api.api_dht_stats().ok().map(|d| d.routing_table_size);
        out.port = self.session.announce_port().or_else(|| self.session.listen_addr().map(|a| a.port()));
        if let Some(id) = selected {
            out.detail = self.detail(id, want_peers, &out.torrents);
        }
        out
    }

    async fn pause(&self, id: usize) -> Result<(), String> {
        self.api
            .api_torrent_action_pause(TorrentIdOrHash::Id(id))
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn resume(&self, id: usize) -> Result<(), String> {
        self.api
            .api_torrent_action_start(TorrentIdOrHash::Id(id))
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn remove(&self, id: usize, delete_files: bool) -> Result<(), String> {
        self.session
            .delete(TorrentIdOrHash::Id(id), delete_files)
            .await
            .map_err(|e| format!("{e:#}"))
    }

    async fn set_only_files(&self, id: usize, files: Vec<usize>) -> Result<(), String> {
        let set: HashSet<usize> = files.into_iter().collect();
        self.api
            .api_torrent_action_update_only_files(TorrentIdOrHash::Id(id), &set)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn preview(&self, bytes: Vec<u8>) -> Result<Preview, String> {
        let resp = self
            .api
            .api_add_torrent(
                AddTorrent::from_bytes(bytes),
                Some(AddTorrentOptions {
                    list_only: true,
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(Preview {
            name: resp.details.name.unwrap_or_default(),
            files: resp
                .details
                .files
                .unwrap_or_default()
                .into_iter()
                .map(|f| (f.components, f.length))
                .collect(),
        })
    }

    async fn preview_magnet(&self, link: String) -> Result<MagnetPreview, String> {
        const TIMEOUT: Duration = Duration::from_secs(300);
        let trackers = Magnet::parse(&link).map_err(|e| e.to_string())?.trackers;
        let opts = AddTorrentOptions { list_only: true, ..Default::default() };
        let added = tokio::time::timeout(TIMEOUT, self.session.add_torrent(AddTorrent::from_url(link), Some(opts)))
            .await
            .map_err(|_| "timed out waiting for the metadata".to_string())?
            .map_err(|e| format!("{e:#}"))?;
        let AddTorrentResponse::ListOnly(r) = added else {
            return Err("the torrent is already added".into());
        };
        let files = r
            .info
            .iter_file_details()
            .map(|d| (d.filename.to_vec(), d.len))
            .collect();
        let name = r.info.name().map(|n| n.into_owned()).unwrap_or_default();
        Ok(MagnetPreview {
            preview: Preview { name, files },
            bytes: r.torrent_bytes.to_vec(),
            trackers,
        })
    }

    async fn add_bytes(&self, bytes: Vec<u8>, o: AddOpts) -> Result<String, String> {
        let pv = self.preview(bytes.clone()).await?;
        let out = output_folder(&o.folder, &pv.name, pv.files.len());
        self.add(AddTorrent::from_bytes(bytes), o, Some(out)).await
    }

    /// The name and file count are unknown until the metadata arrives, so the files land in
    /// the session's default folder; `Engine` moves them later when another folder was asked for.
    async fn add_magnet(&self, link: String, o: AddOpts) -> Result<String, String> {
        self.add(AddTorrent::from_url(link), o, None).await
    }

    async fn move_torrent(&self, id: usize, base: PathBuf) -> Result<(), String> {
        let idx = TorrentIdOrHash::Id(id);
        let handle = self.session.get(idx).ok_or("torrent not found")?;
        let bytes = handle
            .with_metadata(|m| m.torrent_bytes.clone())
            .map_err(|e| e.to_string())?;
        drop(handle);
        let details = self.api.api_torrent_details(idx).map_err(|e| e.to_string())?;
        let old_folder = PathBuf::from(&details.output_folder);
        let files = details.files.unwrap_or_default();
        let multi = files.len() >= 2;
        let new_folder = if multi {
            base.join(old_folder.file_name().ok_or("bad torrent folder")?)
        } else {
            base
        };
        if new_folder == old_folder {
            return Ok(());
        }
        let was_paused = self
            .api
            .api_stats_v1(idx)
            .map(|s| matches!(s.state, TorrentStatsState::Paused))
            .unwrap_or(false);
        let included: Vec<usize> = files.iter().enumerate().filter(|(_, f)| f.included).map(|(i, _)| i).collect();
        let only_files = (included.len() != files.len()).then_some(included);
        let rels: Vec<Vec<String>> = files.into_iter().map(|f| f.components).collect();

        self.session.delete(idx, false).await.map_err(|e| format!("{e:#}"))?;
        let (from, to) = (old_folder.clone(), new_folder.clone());
        let moved = tokio::task::spawn_blocking(move || move_files(&from, &to, &rels, multi))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);

        if moved.is_ok() {
            move_parts(&old_folder, &new_folder, multi, &details.info_hash);
        }

        // Re-add at the new place, or back at the old one if the move failed.
        let target = if moved.is_ok() { new_folder } else { old_folder };
        self.api
            .api_add_torrent(
                AddTorrent::from_bytes(bytes),
                Some(AddTorrentOptions {
                    paused: was_paused,
                    overwrite: true,
                    output_folder: Some(target.to_string_lossy().into_owned()),
                    only_files,
                    preferred_id: Some(id),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| e.to_string())?;
        moved
    }


    async fn create(
        &self,
        path: PathBuf,
        trackers: Vec<String>,
        piece_length: Option<u32>,
    ) -> Result<(Vec<u8>, PathBuf), String> {
        let res = create_torrent(
            &path,
            CreateTorrentOptions {
                name: None,
                trackers,
                piece_length,
            },
            &BlockingSpawner::new(4),
        )
        .await
        .map_err(|e| format!("{e:#}"))?;
        let bytes = res.as_bytes().map_err(|e| format!("{e:#}"))?;
        Ok((bytes.to_vec(), res.output_folder))
    }

    async fn shutdown(&self) {
        self.session.stop().await;
    }
}

/// One cell per piece up to `PIECE_CELLS`, then pieces are grouped. A cell holds the share of its
/// wanted pieces that are downloaded, or `SKIPPED` when none of them is wanted.
fn cells_from_bitfield(have_at: impl Fn(usize) -> bool, wanted_at: impl Fn(usize) -> bool, total: usize) -> Vec<f32> {
    let cells = total.min(PIECE_CELLS);
    (0..cells)
        .map(|i| {
            let a = i * total / cells;
            let b = (((i + 1) * total / cells).max(a + 1)).min(total);
            let (mut wanted, mut have) = (0, 0);
            for k in a..b {
                if wanted_at(k) {
                    wanted += 1;
                    have += have_at(k) as usize;
                } else if have_at(k) {
                    // A piece shared with a neighbouring selected file can already be here.
                    wanted += 1;
                    have += 1;
                }
            }
            if wanted == 0 { SKIPPED } else { have as f32 / wanted as f32 }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_cells_are_exact_for_small_torrents_and_mark_skipped_pieces() {
        let cells = cells_from_bitfield(|k| k < 10, |k| k < 20, 40);
        assert_eq!(cells.len(), 40);
        assert_eq!(cells[0], 1.0);
        assert_eq!(cells[15], 0.0);
        assert_eq!(cells[30], SKIPPED);
    }

    #[test]
    fn piece_cells_group_pieces_of_big_torrents() {
        let cells = cells_from_bitfield(|k| k < 5000, |_| true, 10_000);
        assert_eq!(cells.len(), PIECE_CELLS);
        assert_eq!(cells[0], 1.0);
        assert_eq!(cells[PIECE_CELLS - 1], 0.0);
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("piTorrent-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn multi_file_torrents_get_a_subfolder() {
        assert_eq!(output_folder("D:/dl", "Movie", 3), PathBuf::from("D:/dl").join("Movie").to_string_lossy());
        assert_eq!(output_folder("D:/dl", "file.iso", 1), PathBuf::from("D:/dl").to_string_lossy());
    }

    #[test]
    fn moves_files_and_removes_the_emptied_torrent_folder() {
        let root = temp("move-multi");
        let (from, to) = (root.join("a").join("Movie"), root.join("b").join("Movie"));
        std::fs::create_dir_all(from.join("sub")).unwrap();
        std::fs::write(from.join("x.mkv"), b"x").unwrap();
        std::fs::write(from.join("sub").join("y.srt"), b"y").unwrap();
        let rels = vec![vec!["x.mkv".to_string()], vec!["sub".to_string(), "y.srt".to_string()]];

        move_files(&from, &to, &rels, true).unwrap();

        assert_eq!(std::fs::read(to.join("x.mkv")).unwrap(), b"x");
        assert_eq!(std::fs::read(to.join("sub").join("y.srt")).unwrap(), b"y");
        assert!(!from.exists(), "the emptied torrent folder is removed");
        assert!(root.join("a").exists(), "the download folder itself stays");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn single_file_move_keeps_the_download_folder() {
        let root = temp("move-single");
        let (from, to) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&from).unwrap();
        std::fs::write(from.join("f.iso"), b"f").unwrap();
        std::fs::write(from.join("other.txt"), b"o").unwrap();

        move_files(&from, &to, &[vec!["f.iso".to_string()]], false).unwrap();

        assert!(to.join("f.iso").exists());
        assert!(from.join("other.txt").exists(), "unrelated files are untouched");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_files_are_skipped() {
        let root = temp("move-missing");
        let (from, to) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&from).unwrap();
        move_files(&from, &to, &[vec!["never-downloaded.bin".to_string()]], false).unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
}
