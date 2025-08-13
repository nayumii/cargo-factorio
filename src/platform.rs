use anyhow::{Context, Result};
use std::path::PathBuf;

/// Get the platform-specific Factorio mods directory
pub fn factorio_mods_dir() -> Result<PathBuf> {
    let home = dirs_next::home_dir().context("No home directory found")?;
    
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .unwrap_or_else(|_| home.join("AppData\\Roaming").to_string_lossy().to_string());
        Ok(PathBuf::from(appdata).join("Factorio\\mods"))
    }
    
    #[cfg(target_os = "macos")]
    {
        Ok(home.join("Library/Application Support/factorio/mods"))
    }
    
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        Ok(home.join(".factorio/mods"))
    }
}