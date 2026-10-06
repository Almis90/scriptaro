//! Stage beside the destination; publish with an atomic no-clobber hard link.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// Resolve existing parents, reject any existing entry (including dangling links).
pub(crate) fn destination(path: &Path) -> io::Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing output parent"))?
        .canonicalize()?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("missing output filename"))?;
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "screenshot destination already exists",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(path),
        Err(e) => Err(e),
    }
}

pub(crate) struct Output {
    destination: PathBuf,
    directory: PathBuf,
    file: Option<File>,
}
impl Output {
    pub(crate) fn prepare(path: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let destination = destination(path)?;
        let parent = destination.parent().expect("resolved parent");
        let directory = loop {
            let directory = parent.join(format!(
                ".scriptaro-capture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match builder.create(&directory) {
                Ok(()) => break directory,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        };
        let mut output = Self {
            destination,
            directory,
            file: None,
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        output.file = Some(options.open(output.directory.join("capture.png"))?);
        Ok(output)
    }
    pub(crate) fn finish(mut self, png: &[u8]) -> io::Result<()> {
        // Bound backend output and reject an obviously broken backend contract.
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") || png.len() > 256 * 1024 * 1024 {
            return Err(io::Error::other(
                "backend returned invalid or oversized PNG data",
            ));
        }
        let mut file = self.file.take().expect("prepared output");
        file.write_all(png)?;
        file.sync_all()?;
        drop(file);
        // Same filesystem, no replacement even if another writer won the race.
        // Filesystems without hard links fail explicitly without a partial PNG.
        fs::hard_link(self.directory.join("capture.png"), &self.destination)
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(self.directory.join("capture.png"));
        let _ = fs::remove_dir(&self.directory);
    }
}
