//! Editable starters shared by CLI and desktop hosts. Generation never plays them.
use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::Path,
};

#[derive(Clone, Copy)]
pub struct Recipe {
    pub id: &'static str,
    pub description: &'static str,
    pub source: &'static str,
}

pub const RECIPES: &[Recipe] = &[
    Recipe {
        id: "basic",
        description: "Start with one wait and build actions using guided forms",
        source: include_str!("../recipes/basic.yaml"),
    },
    Recipe {
        id: "text-entry",
        description: "Type into one field, with an explicit replace-text reset",
        source: include_str!("../recipes/text-entry.yaml"),
    },
    Recipe {
        id: "form-fill",
        description: "Fill two fields with readiness checks and pacing",
        source: include_str!("../recipes/form-fill.yaml"),
    },
    Recipe {
        id: "app-switch",
        description: "Switch between two exact windows and return",
        source: include_str!("../recipes/app-switch.yaml"),
    },
];

impl Recipe {
    /// Create a new editable file. Existing files and symlinks are never overwritten.
    pub fn create(&self, path: &Path) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(self.source.as_bytes())
    }
}

pub fn find(id: &str) -> Option<&'static Recipe> {
    RECIPES.iter().find(|recipe| recipe.id == id)
}
