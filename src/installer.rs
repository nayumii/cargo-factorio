use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::BuildConfig;
use crate::mod_info::{resolve_mod_paths, Info};
use crate::platform::factorio_mods_dir;
use crate::zip_builder::build_zip;

/// Main installation function - coordinates the entire process
pub fn install_mods(mod_path: Option<PathBuf>, out_dir: String, config: BuildConfig) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mods = resolve_mod_paths(mod_path, &cwd)?;
    
    if mods.is_empty() {
        bail!("No mods found. Place an info.json in the repo root or in subfolders.");
    }

    let out_dir = PathBuf::from(out_dir);
    fs::create_dir_all(&out_dir)?;

    for mod_path in mods {
        config.log(&format!("🔍 Processing mod at {}", mod_path.display()));
        install_one(&mod_path, &out_dir, &config)?;
    }

    Ok(())
}

/// Install one mod: build zip into out_dir, copy to Factorio/mods, keep the built zip.
fn install_one(mod_root: &Path, out_dir: &Path, config: &BuildConfig) -> Result<()> {
    let info = Info::load_from_dir(mod_root)
        .context("Failed to parse info.json")?;
    
    let zip_name = info.zip_name();
    let zip_path = out_dir.join(format!("{}.zip", zip_name));

    build_zip(mod_root, &zip_path, &zip_name, config)?;
    
    let mods_dir = factorio_mods_dir()?;
    fs::create_dir_all(&mods_dir)?;
    
    let dest = mods_dir.join(zip_path.file_name().unwrap());
    fs::copy(&zip_path, &dest)?;
    
    println!("✅ Installed {} → {}", zip_name, dest.display());
    Ok(())
}