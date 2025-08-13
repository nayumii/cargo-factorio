use std::fs;
use std::path::{Path, PathBuf};

/// Configuration for building mods
pub struct BuildConfig {
    pub verbose: bool,
    pub default_thumbnail: Option<Vec<u8>>,
    pub excludes: &'static [&'static str],
}

impl BuildConfig {
    pub fn new(verbose: bool, default_thumbnail: Option<PathBuf>) -> Self {
        Self {
            verbose,
            default_thumbnail: load_default_thumbnail_bytes(&default_thumbnail),
            excludes: &["build", ".git", ".github", ".idea", ".vscode"],
        }
    }

    pub fn log(&self, message: &str) {
        if self.verbose {
            println!("{}", message);
        }
    }
}

/// Load thumbnail bytes from explicit path or default location
fn load_default_thumbnail_bytes(explicit: &Option<PathBuf>) -> Option<Vec<u8>> {
    if let Some(path) = explicit {
        if let Ok(bytes) = fs::read(path) {
            return Some(bytes);
        }
    }
    
    // Try default location
    let fallback = Path::new("assets").join("default_thumbnail.png");
    fs::read(fallback).ok()
}