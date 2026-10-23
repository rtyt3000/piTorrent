use std::{
    fs::OpenOptions,
    io::IoSlice,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use tracing::warn;

use crate::{
    storage::{StorageFactoryExt, filesystem::opened_file::OurFileExt},
    torrent_state::{ManagedTorrentShared, TorrentMetadata},
};

use crate::storage::{StorageFactory, TorrentStorage};

use super::opened_file::OpenedFile;

#[derive(Default, Clone, Copy)]
pub struct FilesystemStorageFactory {}

impl StorageFactory for FilesystemStorageFactory {
    type Storage = FilesystemStorage;

    fn create(
        &self,
        shared: &ManagedTorrentShared,
        _metadata: &TorrentMetadata,
    ) -> anyhow::Result<FilesystemStorage> {
        Ok(FilesystemStorage {
            output_folder: shared.options.output_folder.clone(),
            opened_files: Default::default(),
            parts: None,
        })
    }

    fn clone_box(&self) -> crate::storage::BoxStorageFactory {
        self.boxed()
    }
}

pub struct FilesystemStorage {
    pub(crate) output_folder: PathBuf,
    pub(crate) opened_files: Vec<OpenedFile>,
    parts: Option<Parts>,
}

/// A byte range of a file that can be shared with a neighbour in a piece.
#[derive(Clone, Copy)]
struct Window {
    start: u64,
    len: u64,
    /// Where the range lives in the part file.
    base: u64,
}

/// Like qBittorrent's `.parts` folder: the edges of deselected files that have to be downloaded
/// anyway (a piece is verified as a whole) go into one hidden file instead of leaving
/// partial files in the download folder. The layout is our own, not libtorrent's.
struct Parts {
    path: PathBuf,
    file: OpenedFile,
    /// Per file: the leading and trailing range that share a piece with a neighbour.
    windows: Vec<Vec<Window>>,
    /// Per file: whether the file itself exists, and so holds all of its data.
    on_disk: Vec<AtomicBool>,
}

impl Parts {
    fn take(&self) -> anyhow::Result<Self> {
        Ok(Self {
            path: self.path.clone(),
            file: self.file.take_clone()?,
            windows: self.windows.clone(),
            on_disk: self.on_disk.iter().map(|b| AtomicBool::new(b.load(Ordering::Relaxed))).collect(),
        })
    }
}

fn file_windows(offset: u64, len: u64, piece_len: u64, next_base: &mut u64) -> Vec<Window> {
    if len == 0 || piece_len == 0 {
        return Vec::new();
    }
    let first_end = (offset / piece_len + 1) * piece_len;
    let last_start = (offset + len - 1) / piece_len * piece_len;
    let mut ranges = Vec::new();
    if last_start < first_end {
        ranges.push((0, len));
    } else {
        ranges.push((0, first_end - offset));
        ranges.push((last_start - offset, len));
    }
    ranges
        .into_iter()
        .map(|(start, end)| {
            let w = Window { start, len: end - start, base: *next_base };
            *next_base += w.len;
            w
        })
        .collect()
}

/// `.parts` sits next to the torrent's folder (or in the download folder for a single file).
fn parts_path(output_folder: &Path, multi: bool, info_hash: &str) -> PathBuf {
    let base = if multi { output_folder.parent().unwrap_or(output_folder) } else { output_folder };
    base.join(".parts").join(format!(".{info_hash}.parts"))
}

impl FilesystemStorage {
    #[allow(dead_code)]
    pub(crate) fn take_fs(&self) -> anyhow::Result<Self> {
        Ok(Self {
            opened_files: self
                .opened_files
                .iter()
                .map(|f| f.take_clone())
                .collect::<anyhow::Result<Vec<_>>>()?,
            output_folder: self.output_folder.clone(),
            parts: self.parts.as_ref().map(Parts::take).transpose()?,
        })
    }

    /// The offset in the part file, if this range belongs there rather than in the file itself.
    fn in_parts(&self, file_id: usize, offset: u64, len: usize) -> Option<(&Parts, u64)> {
        let p = self.parts.as_ref()?;
        if p.on_disk.get(file_id)?.load(Ordering::Relaxed) {
            return None;
        }
        let end = offset + len as u64;
        p.windows[file_id]
            .iter()
            .find(|w| offset >= w.start && end <= w.start + w.len)
            .map(|w| (p, w.base + offset - w.start))
    }

    /// The file is about to be created: bring over what its edges already have in the part file.
    fn activate(&self, file_id: usize) -> anyhow::Result<()> {
        let Some(p) = &self.parts else { return Ok(()) };
        if p.on_disk[file_id].swap(true, Ordering::Relaxed) {
            return Ok(());
        }
        let Ok(src) = p.file.lock_read() else { return Ok(()) };
        let src_len = src.metadata()?.len();
        let dst = self.opened_files.get(file_id).context("no such file")?;
        let mut buf = vec![0u8; 1 << 20];
        for w in &p.windows[file_id] {
            let mut done = 0;
            while done < w.len && w.base + done < src_len {
                let n = (w.len - done).min(buf.len() as u64).min(src_len - w.base - done) as usize;
                src.pread_exact(w.base + done, &mut buf[..n])?;
                // Zeros are what's not written yet; the real file may already hold data there.
                if buf[..n].iter().any(|&b| b != 0) {
                    dst.lock_writable()?.pwrite_all(w.start + done, &buf[..n])?;
                }
                done += n as u64;
            }
        }
        Ok(())
    }
}

impl TorrentStorage for FilesystemStorage {
    fn pread_exact(&self, file_id: usize, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
        if let Some((p, pos)) = self.in_parts(file_id, offset, buf.len()) {
            return p.file.lock_read()?.pread_exact(pos, buf);
        }
        self.opened_files
            .get(file_id)
            .context("no such file")?
            .lock_read()?
            .pread_exact(offset, buf)
    }

    fn pwrite_all(&self, file_id: usize, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
        if let Some((p, pos)) = self.in_parts(file_id, offset, buf.len()) {
            return p.file.lock_writable()?.pwrite_all(pos, buf);
        }
        self.activate(file_id)?;
        let of = self.opened_files.get(file_id).context("no such file")?;
        of.lock_writable()?.pwrite_all(offset, buf)
    }

    fn pwrite_all_vectored(
        &self,
        file_id: usize,
        offset: u64,
        bufs: [IoSlice<'_>; 2],
    ) -> anyhow::Result<usize> {
        let len = bufs[0].len() + bufs[1].len();
        if let Some((p, pos)) = self.in_parts(file_id, offset, len) {
            return p.file.lock_writable()?.pwrite_all_vectored(pos, bufs);
        }
        self.activate(file_id)?;
        let of = self.opened_files.get(file_id).context("no such file")?;
        of.lock_writable()?.pwrite_all_vectored(offset, bufs)
    }

    fn remove_file(&self, _file_id: usize, filename: &Path) -> anyhow::Result<()> {
        if let Some(p) = &self.parts {
            let _ = std::fs::remove_file(&p.path);
            if let Some(dir) = p.path.parent() {
                let _ = std::fs::remove_dir(dir);
            }
        }
        Ok(std::fs::remove_file(self.output_folder.join(filename))?)
    }

    fn ensure_file_length(&self, file_id: usize, len: u64) -> anyhow::Result<()> {
        self.activate(file_id)?;
        let f = &self.opened_files.get(file_id).context("no such file")?;
        Ok(f.lock_writable()?.set_len(len)?)
    }

    fn take(&self) -> anyhow::Result<Box<dyn TorrentStorage>> {
        Ok(Box::new(Self {
            opened_files: self
                .opened_files
                .iter()
                .map(|f| f.take_clone())
                .collect::<anyhow::Result<Vec<_>>>()?,
            output_folder: self.output_folder.clone(),
            parts: self.parts.as_ref().map(Parts::take).transpose()?,
        }))
    }

    fn remove_directory_if_empty(&self, path: &Path) -> anyhow::Result<()> {
        let path = self.output_folder.join(path);
        if !path.is_dir() {
            anyhow::bail!("cannot remove dir: {path:?} is not a directory")
        }
        if std::fs::read_dir(&path)?.count() == 0 {
            std::fs::remove_dir(&path).with_context(|| format!("error removing {path:?}"))
        } else {
            warn!("did not remove {path:?} as it was not empty");
            Ok(())
        }
    }

    fn init(
        &mut self,
        shared: &ManagedTorrentShared,
        metadata: &TorrentMetadata,
    ) -> anyhow::Result<()> {
        let mut files = Vec::<OpenedFile>::new();
        let mut windows = Vec::new();
        let mut on_disk = Vec::new();
        let (mut next_base, piece_len) = (0, u64::from(metadata.lengths().default_piece_length()));
        for file_details in metadata.file_infos.iter() {
            let mut full_path = self.output_folder.clone();
            let relative_path = &file_details.relative_filename;
            full_path.push(relative_path);

            if file_details.attrs.padding {
                files.push(OpenedFile::new_dummy());
                windows.push(Vec::new());
                on_disk.push(AtomicBool::new(true));
                continue;
            };
            if shared.options.allow_overwrite {
                windows.push(file_windows(file_details.offset_in_torrent, file_details.len, piece_len, &mut next_base));
                on_disk.push(AtomicBool::new(full_path.exists()));
                files.push(OpenedFile::new_lazy(full_path));
                continue;
            }
            windows.push(Vec::new());
            on_disk.push(AtomicBool::new(true));
            std::fs::create_dir_all(full_path.parent().context("bug: no parent")?)?;
            let f = {
                // create_new does not seem to work with read(true), so calling this twice.
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&full_path)
                    .with_context(|| {
                        format!(
                            "error creating a new file (because allow_overwrite = false) {:?}",
                            full_path
                        )
                    })?;
                OpenOptions::new().read(true).write(true).open(&full_path)?
            };
            files.push(OpenedFile::new(full_path.clone(), f));
        }

        if shared.options.allow_overwrite {
            let path = parts_path(&self.output_folder, metadata.file_infos.len() >= 2, &shared.info_hash.as_string());
            self.parts = Some(Parts { file: OpenedFile::new_lazy(path.clone()), path, windows, on_disk });
        }
        self.opened_files = files;
        Ok(())
    }
}
