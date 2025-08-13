use anyhow::{bail, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct Info {
    pub name: String,
    pub version: String,
}

impl Info {
    /// Load mod info from info.json in the given directory
    pub fn load_from_dir(mod_root: &Path) -> Result<Self> {
        let info_path = mod_root.join("info.json");
        let content = fs::read_to_string(&info_path)?;
        let info: Info = serde_json::from_str(&content)?;
        Ok(info)
    }

    /// Get the mod's zip name in the format "name_version"
    pub fn zip_name(&self) -> String {
        format!("{}_{}", self.name, self.version)
    }
}

/// Resolve mod paths based on input
pub fn resolve_mod_paths(mod_path: Option<PathBuf>, cwd: &Path) -> Result<Vec<PathBuf>> {
    if let Some(p) = mod_path {
        let p = if p.is_absolute() { p } else { cwd.join(p) };
        if !p.join("info.json").exists() {
            bail!("No info.json found at {}", p.display());
        }
        Ok(vec![p])
    } else {
        detect_all_mod_roots(cwd)
    }
}

/// Detect mods: include repo root if it has info.json, plus each direct child dir with info.json.
pub fn detect_all_mod_roots(root: &Path) -> Result<Vec<PathBuf>> {
    let mut mods = Vec::new();
    
    // Check if root itself is a mod
    if root.join("info.json").exists() {
        mods.push(root.to_path_buf());
    }
    
    // Check direct children
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        if path.is_dir() && path.join("info.json").exists() {
            mods.push(path);
        }
    }

    Ok(mods)
}