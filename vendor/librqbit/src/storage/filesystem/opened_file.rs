use std::{
    fs::File,
    io::IoSlice,
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{
        Arc, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Context;
use parking_lot::{Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::Error;

pub trait OurFileExt {
    fn pwrite_all_vectored(&self, offset: u64, bufs: [IoSlice<'_>; 2]) -> anyhow::Result<usize>;
    fn pread_exact(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<()>;
    fn pwrite_all(&self, offset: u64, buf: &[u8]) -> anyhow::Result<()>;
}

impl OurFileExt for File {
    #[cfg(unix)]
    fn pwrite_all_vectored(&self, offset: u64, bufs: [IoSlice<'_>; 2]) -> anyhow::Result<usize> {
        nix::sys::uio::pwritev(self, &bufs, offset.try_into()?).context("error calling pwritev")
    }

    #[cfg(not(unix))]
    fn pwrite_all_vectored(&self, offset: u64, bufs: [IoSlice<'_>; 2]) -> anyhow::Result<usize> {
        match (bufs[0].len(), bufs[1].len()) {
            (len, 0) if len > 0 => {
                self.pwrite_all(offset, &bufs[0])?;
                Ok(len)
            }
            (0, len) if len > 0 => {
                self.pwrite_all(offset, &bufs[1])?;
                Ok(len)
            }
            (0, 0) => Ok(0),
            (l0, l1) => {
                // concatenate the buffers in memory so that we issue one write call instead of 2
                // assumes the message is <= CHUNK_SIZE
                use librqbit_core::constants::CHUNK_SIZE;
                let mut buf = [0u8; CHUNK_SIZE as usize];

                buf.get_mut(..l0)
                    .context("buf too small")?
                    .copy_from_slice(&bufs[0]);
                buf.get_mut(l0..l0 + l1)
                    .context("buf too small")?
                    .copy_from_slice(&bufs[1]);
                self.pwrite_all(offset, &buf[..l0 + l1])?;
                Ok(l0 + l1)
            }
        }
    }

    #[cfg(unix)]
    fn pread_exact(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
        use std::os::unix::fs::FileExt;

        Ok(self.read_exact_at(buf, offset)?)
    }

    #[cfg(windows)]
    fn pread_exact(&self, mut offset: u64, mut buf: &mut [u8]) -> anyhow::Result<()> {
        use std::os::windows::fs::FileExt;
        while !buf.is_empty() {
            let n = self.seek_read(buf, offset)?;
            if n == 0 {
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof").into());
            }
            offset += n as u64;
            buf = &mut buf[n..];
        }
        Ok(())
    }

    #[cfg(not(any(windows, unix)))]
    fn pread_exact(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
        anyhow::bail!("pread_exact not implemented for your platform")
    }

    #[cfg(unix)]
    fn pwrite_all(&self, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
        use std::os::unix::fs::FileExt;
        Ok(self.write_all_at(buf, offset)?)
    }

    #[cfg(windows)]
    fn pwrite_all(&self, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
        use std::os::windows::fs::FileExt;

        let mut remaining = buf.len();
        let mut buf = buf;
        let mut offset = offset;
        while remaining > 0 {
            let written = self.seek_write(&buf[..remaining], offset)?;
            remaining -= written;
            offset += written as u64;
            buf = &buf[written..];
        }
        Ok(())
    }

    #[cfg(not(any(windows, unix)))]
    fn pwrite_all(&self, offset: u64, buf: &[u8]) -> anyhow::Result<()> {
        anyhow::bail!("pwrite_all not implemented for your platform")
    }
}

// piTorrent patch: a file with a handle open for writing can't be executed on Windows
// (and on Linux fails with ETXTBSY), so seeded executables wouldn't start. The writable
// handle is therefore only kept while the file is being written to; after a short idle
// period it is swapped for a read-only one and reopened on the next write.
const WRITER_IDLE: Duration = Duration::from_secs(3);
// A file nobody has read or written for this long is closed altogether (and reopened on demand),
// so an idle torrent holds no handles: its folder can be renamed, moved or deleted in Explorer.
const HANDLE_IDLE: Duration = Duration::from_secs(15);

#[derive(Default, Debug)]
struct OpenedFileLocked {
    path: PathBuf,
    fd: Option<File>,
    writable: bool,
    /// No file on disk until the first write, so deselected files aren't created as empty ones.
    lazy: bool,
    /// The handle was closed for being idle; reopen it from `path` when needed.
    reopen: bool,
    #[cfg(windows)]
    tried_marking_sparse: bool,
}

impl Deref for OpenedFileLocked {
    type Target = Option<File>;

    fn deref(&self) -> &Self::Target {
        &self.fd
    }
}

impl DerefMut for OpenedFileLocked {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.fd
    }
}

#[derive(Debug)]
struct Inner {
    file: RwLock<OpenedFileLocked>,
    /// Millis since `epoch()` of the last write.
    last_write: AtomicU64,
    /// Millis since `epoch()` of the last read or write.
    last_use: AtomicU64,
}

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    epoch().elapsed().as_millis() as u64
}

impl Inner {
    fn release_idle(&self) {
        self.release_writer();
        if now_ms().saturating_sub(self.last_use.load(Ordering::Relaxed)) < HANDLE_IDLE.as_millis() as u64 {
            return;
        }
        let mut g = self.file.write();
        if g.fd.is_some() && !g.writable {
            g.fd = None;
            g.reopen = true;
        }
    }

    fn release_writer(&self) {
        if now_ms().saturating_sub(self.last_write.load(Ordering::Relaxed))
            < WRITER_IDLE.as_millis() as u64
        {
            return;
        }
        let mut g = self.file.write();
        if !g.writable || g.fd.is_none() {
            return;
        }
        // Keep the writable handle if the read-only one can't be opened (e.g. file removed).
        if let Ok(ro) = File::open(&g.path) {
            g.fd = Some(ro);
            g.writable = false;
        }
    }
}

fn registry() -> &'static Mutex<Vec<Weak<Inner>>> {
    static REGISTRY: OnceLock<Mutex<Vec<Weak<Inner>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        std::thread::Builder::new()
            .name("fs-writer-janitor".into())
            .spawn(|| loop {
                std::thread::sleep(Duration::from_secs(1));
                let live: Vec<Arc<Inner>> = {
                    let mut r = registry().lock();
                    r.retain(|w| w.strong_count() > 0);
                    r.iter().filter_map(Weak::upgrade).collect()
                };
                for f in live {
                    f.release_idle();
                }
            })
            .ok();
        Mutex::new(Vec::new())
    })
}

#[derive(Debug)]
pub(crate) struct OpenedFile {
    inner: Arc<Inner>,
}

impl OpenedFile {
    fn from_locked(l: OpenedFileLocked) -> Self {
        let inner = Arc::new(Inner {
            file: RwLock::new(l),
            last_write: AtomicU64::new(now_ms()),
            last_use: AtomicU64::new(now_ms()),
        });
        registry().lock().push(Arc::downgrade(&inner));
        Self { inner }
    }

    /// `f` must be opened for reading and writing.
    pub fn new(path: PathBuf, f: File) -> Self {
        Self::from_locked(OpenedFileLocked {
            path,
            fd: Some(f),
            writable: true,
            lazy: false,
            reopen: false,
            #[cfg(windows)]
            tried_marking_sparse: false,
        })
    }

    /// The file is created (or an existing one opened) on the first write.
    pub fn new_lazy(path: PathBuf) -> Self {
        Self::from_locked(OpenedFileLocked {
            path,
            fd: None,
            writable: false,
            lazy: true,
            reopen: false,
            #[cfg(windows)]
            tried_marking_sparse: false,
        })
    }

    pub fn new_dummy() -> Self {
        Self {
            inner: Arc::new(Inner {
                file: RwLock::new(Default::default()),
                last_write: AtomicU64::new(0),
                last_use: AtomicU64::new(0),
            }),
        }
    }

    pub fn take_clone(&self) -> anyhow::Result<Self> {
        let f = std::mem::take(&mut *self.inner.file.write());
        Ok(Self::from_locked(f))
    }

    /// Read-only access (the handle may or may not be writable).
    pub fn lock_read(&self) -> crate::Result<impl Deref<Target = File>> {
        self.inner.last_use.store(now_ms(), Ordering::Relaxed);
        {
            let g = self.inner.file.read();
            if g.fd.is_none() && (g.lazy || g.reopen) {
                drop(g);
                let mut g = self.inner.file.write();
                if g.fd.is_none() {
                    // A file that doesn't exist yet simply has nothing to read.
                    let f = File::open(&g.path).map_err(|_| Error::FsFileIsNone)?;
                    g.fd = Some(f);
                    g.writable = false;
                    g.reopen = false;
                }
            }
        }
        RwLockReadGuard::try_map(self.inner.file.read(), |f| f.as_ref())
            .ok()
            .ok_or(Error::FsFileIsNone)
    }

    #[allow(dead_code)]
    pub fn lock_write(&self) -> crate::Result<impl DerefMut<Target = File>> {
        RwLockWriteGuard::try_map(self.inner.file.write(), |f| f.as_mut())
            .ok()
            .ok_or(Error::FsFileIsNone)
    }

    /// A handle that is open for writing; reopens it if it was downgraded to read-only.
    /// On Windows also marks the file sparse once.
    pub fn lock_writable(&self) -> crate::Result<impl Deref<Target = File>> {
        self.inner.last_write.store(now_ms(), Ordering::Relaxed);
        self.inner.last_use.store(now_ms(), Ordering::Relaxed);
        {
            let g = self.inner.file.read();
            if g.writable && g.fd.is_some() {
                #[cfg(windows)]
                let ready = g.tried_marking_sparse;
                #[cfg(not(windows))]
                let ready = true;
                if ready {
                    return RwLockReadGuard::try_map(g, |f| f.fd.as_ref())
                        .ok()
                        .ok_or(Error::FsFileIsNone);
                }
            }
        }
        let mut g = self.inner.file.write();
        if g.fd.is_none() && !g.lazy && !g.reopen {
            return Err(Error::FsFileIsNone);
        }
        if !g.writable {
            if let Some(dir) = g.path.parent().filter(|_| g.lazy) {
                std::fs::create_dir_all(dir)
                    .map_err(|e| Error::Anyhow(anyhow::anyhow!("error creating {dir:?}: {e}")))?;
            }
            let f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(g.lazy)
                .truncate(false)
                .open(&g.path)
                .map_err(|e| Error::Anyhow(anyhow::anyhow!("error reopening {:?}: {e}", g.path)))?;
            g.fd = Some(f);
            g.writable = true;
            g.reopen = false;
        }
        #[cfg(windows)]
        if !g.tried_marking_sparse {
            g.tried_marking_sparse = true;
            let f = g.fd.as_ref().ok_or(Error::FsFileIsNone)?;
            tracing::debug!(path=?g.path, marked=super::sparse::mark_file_sparse(f), "marking sparse");
        }
        let g = parking_lot::RwLockWriteGuard::downgrade(g);
        Ok(RwLockReadGuard::try_map(g, |f| f.fd.as_ref()).ok().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use librqbit_core::constants::CHUNK_SIZE;
    use peer_binary_protocol::DoubleBufHelper;
    use tempfile::TempDir;

    use crate::storage::filesystem::opened_file::OurFileExt;

    #[test]
    fn test_pwrite_all_vectored() {
        let td = TempDir::with_prefix("test_pwrite_all_vectored").unwrap();
        let mut tmp_buf = [0u8; CHUNK_SIZE as usize];
        for bufsize in [10000usize, CHUNK_SIZE as usize] {
            let mut buf = vec![0u8; bufsize];
            rand::fill(&mut buf[..]);
            for split_point in [0, bufsize / 2, bufsize] {
                let path = td.path().join(format!("file_{bufsize}_{split_point}"));
                let file = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .unwrap();
                let (first, second) = buf.split_at(split_point);
                let bufs = DoubleBufHelper::new(first, second).as_ioslices(bufsize);
                file.pwrite_all_vectored(0, bufs).unwrap();

                let mut file = std::fs::File::open(&path).unwrap();
                assert_eq!(file.metadata().unwrap().len(), bufsize as u64, "{path:?}");
                file.read_exact(&mut tmp_buf[..bufsize]).unwrap();
                assert_eq!(&tmp_buf[..bufsize], buf);
            }
        }
    }
}
