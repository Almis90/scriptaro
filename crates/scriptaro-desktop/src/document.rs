//! Editable source and disk persistence, independent of native views.
use scriptaro_core::{MAX_SCRIPT_BYTES, Script, Section, yaml};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("script exceeds the {MAX_SCRIPT_BYTES}-byte limit")]
    TooLarge,
    #[error("{0}")]
    Invalid(String),
    #[error(
        "The file changed outside Scriptaro. Save your draft elsewhere or reload before saving."
    )]
    ExternalChange,
}

pub struct Document {
    pub path: PathBuf,
    source: String,
    saved_source: String,
    script: Option<Script>,
    diagnostic: Option<String>,
}

fn read(path: &Path) -> Result<String, DocumentError> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((MAX_SCRIPT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_SCRIPT_BYTES {
        return Err(DocumentError::TooLarge);
    }
    String::from_utf8(bytes)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e).into())
}

impl Document {
    /// Invalid scripts remain editable, but never retain an earlier playable version.
    pub fn load(path: &Path) -> Result<Self, DocumentError> {
        let path = path.canonicalize()?;
        let source = read(&path)?;
        let mut doc = Self {
            path,
            saved_source: source.clone(),
            source: String::new(),
            script: None,
            diagnostic: None,
        };
        doc.update(source);
        Ok(doc)
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn is_dirty(&self) -> bool {
        self.source != self.saved_source
    }
    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }
    pub fn sections(&self) -> &[Section] {
        self.script.as_ref().map_or(&[], |s| &s.sections)
    }
    pub fn update(&mut self, source: String) {
        match yaml::from_str(&source) {
            Ok(script) => {
                self.script = Some(script);
                self.diagnostic = None;
            }
            Err(error) => {
                self.script = None;
                self.diagnostic = Some(error.to_string());
            }
        }
        self.source = source;
    }
    pub fn prepare(&self, section: Option<&str>, retake: bool) -> Result<Script, DocumentError> {
        self.script
            .as_ref()
            .ok_or_else(|| DocumentError::Invalid(self.diagnostic.clone().unwrap_or_default()))?
            .prepare(section, retake)
            .map_err(|e| DocumentError::Invalid(e.to_string()))
    }
    pub fn base_dir(&self) -> PathBuf {
        self.path
            .parent()
            .expect("canonical file has parent")
            .into()
    }

    /// Keep a draft at a new path without replacing an existing file.
    pub fn save_as(&mut self, path: &Path) -> Result<(), DocumentError> {
        if self.source.len() > MAX_SCRIPT_BYTES {
            return Err(DocumentError::TooLarge);
        }
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let result = file
            .write_all(self.source.as_bytes())
            .and_then(|_| file.sync_all());
        if let Err(error) = result {
            let _ = fs::remove_file(path);
            return Err(error.into());
        }
        self.path = path.canonicalize()?;
        self.saved_source.clone_from(&self.source);
        Ok(())
    }

    /// Save raw source (including comments or an invalid draft) by replacing the file.
    /// Check for external edits both before writing and immediately before rename.
    pub fn save(&mut self) -> Result<(), DocumentError> {
        if self.source.len() > MAX_SCRIPT_BYTES {
            return Err(DocumentError::TooLarge);
        }
        self.check_disk()?;
        if !self.is_dirty() {
            return Ok(());
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temp = self.base_dir().join(format!(
            ".scriptaro-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        let result = (|| {
            file.set_permissions(fs::metadata(&self.path)?.permissions())?;
            file.write_all(self.source.as_bytes())?;
            file.sync_all()?;
            self.check_disk()?;
            fs::rename(&temp, &self.path)?;
            Ok::<_, DocumentError>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result?;
        self.saved_source.clone_from(&self.source);
        Ok(())
    }
    fn check_disk(&self) -> Result<(), DocumentError> {
        if fs::symlink_metadata(&self.path)?.file_type().is_symlink()
            || read(&self.path)? != self.saved_source
        {
            return Err(DocumentError::ExternalChange);
        }
        Ok(())
    }
}
