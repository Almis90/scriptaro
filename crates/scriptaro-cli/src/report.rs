//! Reserve the destination before effects; atomically replace the in-progress marker.
use crate::{diagnostic::Diagnostic, output::Outcome};
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub struct ReportFile {
    path: PathBuf,
    marker: Vec<u8>,
}
fn create(path: &Path) -> io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}
fn bytes(value: &impl serde::Serialize) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
impl ReportFile {
    pub fn reserve(path: &Path, initial: &Value) -> Result<Self, Diagnostic> {
        let result = (|| {
            let marker = bytes(initial)?;
            let mut file = create(path)?;
            if let Err(error) = file.write_all(&marker).and_then(|_| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(path);
                return Err(error);
            }
            Ok(Self {
                path: path.canonicalize()?,
                marker,
            })
        })();
        result.map_err(|e| Diagnostic::io(e, "reserve report before playback", path))
    }
    pub fn finish(&self, outcome: &Outcome) -> Result<(), Diagnostic> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temp = self
            .path
            .parent()
            .expect("canonical report has a parent")
            .join(format!(
                ".scriptaro-report-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        let result = (|| {
            let bytes = bytes(outcome)?;
            let mut file = create(&temp)?;
            let result = (|| {
                file.write_all(&bytes)?;
                file.sync_all()?;
                drop(file);
                if fs::symlink_metadata(&self.path)?.file_type().is_symlink()
                    || fs::read(&self.path)? != self.marker
                {
                    return Err(io::Error::other(
                        "reserved report changed outside Scriptaro",
                    ));
                }
                fs::rename(&temp, &self.path)
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temp);
            }
            result
        })();
        result.map_err(|error: io::Error| Diagnostic::new("report_write_failed", format!("Could not finalize run report: {error}"), "Inspect the run result before retrying: desktop effects may already have occurred. The report may still contain its in-progress marker.").at(&self.path))
    }
}
