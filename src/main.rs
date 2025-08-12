use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::fs;
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::CompressionMethod;

#[derive(Parser)]
#[command(author, version, about = "Factorio mod helper (zip + install)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install a mod (or all detected mods) into your Factorio mods/ folder
    Install {
        /// Optional path to a mod folder containing info.json. If omitted, installs all detected mods in the repo.
        mod_path: Option<PathBuf>,

        /// Output directory for the built .zip(s) before install (default: build)
        #[arg(long, default_value = "build")]
        out_dir: String,

        /// Optional default thumbnail to use when a submod has none.
        #[arg(long, value_name = "PATH")]
        default_thumbnail: Option<PathBuf>,

        /// Print extra information while building.
        #[arg(long)]
        verbose: bool,
    },
}

#[derive(Deserialize)]
struct Info {
    name: String,
    version: String,
}

/// Configuration for building mods
struct BuildConfig {
    verbose: bool,
    default_thumbnail: Option<Vec<u8>>,
    excludes: &'static [&'static str],
}

impl BuildConfig {
    fn new(verbose: bool, default_thumbnail: Option<PathBuf>) -> Self {
        Self {
            verbose,
            default_thumbnail: load_default_thumbnail_bytes(&default_thumbnail),
            excludes: &["build", ".git", ".github", ".idea", ".vscode"],
        }
    }

    fn log(&self, message: &str) {
        if self.verbose {
            println!("{}", message);
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { mod_path, out_dir, default_thumbnail, verbose } => {
            let config = BuildConfig::new(verbose, default_thumbnail);
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
        }
    }

    Ok(())
}

/// Resolve mod paths based on input
fn resolve_mod_paths(mod_path: Option<PathBuf>, cwd: &Path) -> Result<Vec<PathBuf>> {
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

/// Install one mod: build zip into out_dir, copy to Factorio/mods, keep the built zip.
fn install_one(mod_root: &Path, out_dir: &Path, config: &BuildConfig) -> Result<()> {
    let info: Info = serde_json::from_str(&fs::read_to_string(mod_root.join("info.json"))?)
        .context("Failed to parse info.json")?;
    
    let top = format!("{}_{}", info.name, info.version);
    let zip_path = out_dir.join(format!("{top}.zip"));

    build_zip(mod_root, &zip_path, &top, config)?;
    
    let mods_dir = factorio_mods_dir()?;
    fs::create_dir_all(&mods_dir)?;
    
    let dest = mods_dir.join(zip_path.file_name().unwrap());
    fs::copy(&zip_path, &dest)?;
    
    println!("✅ Installed {} → {}", top, dest.display());
    Ok(())
}

/// Build a ZIP with `<name>_<version>/` top-level and forward slashes.
fn build_zip(mod_root: &Path, out_zip: &Path, top: &str, config: &BuildConfig) -> Result<()> {
    prepare_output_file(out_zip)?;

    let file = fs::File::create(out_zip)?;
    let mut zip = zip::ZipWriter::new(file);

    let dir_opts = FileOptions::default();
    let file_opts = FileOptions::default().compression_method(CompressionMethod::Deflated);

    for entry in WalkDir::new(mod_root).follow_links(false).into_iter().filter_map(Result::ok) {
        if entry.path() == mod_root {
            continue;
        }

        let Some(zip_path) = create_zip_path(entry.path(), mod_root, top, config.excludes) else {
            continue;
        };

        if entry.file_type().is_dir() {
            add_directory_to_zip(&mut zip, &zip_path, dir_opts, config)?;
        } else {
            add_file_to_zip(&mut zip, entry.path(), &zip_path, file_opts, config)?;
        }
    }

    add_default_thumbnail_if_missing(&mut zip, mod_root, config.default_thumbnail.as_deref(), top, config)?;

    zip.finish()?;
    println!("📦 Built {}", out_zip.display());
    Ok(())
}

/// Prepare the output file by creating parent directories and removing existing file
fn prepare_output_file(out_zip: &Path) -> Result<()> {
    if let Some(parent) = out_zip.parent() {
        fs::create_dir_all(parent)?;
    }
    if out_zip.exists() {
        fs::remove_file(out_zip)?;
    }
    Ok(())
}

/// Create ZIP path for a file, returning None if it should be excluded
fn create_zip_path(path: &Path, mod_root: &Path, top: &str, excludes: &[&str]) -> Option<String> {
    let rel = path.strip_prefix(mod_root).ok()?;
    
    // Check if this path should be excluded
    if should_exclude_path(rel, excludes) {
        return None;
    }

    let mut zip_path = PathBuf::from(top);
    zip_path.push(rel);
    
    // Convert to forward slashes for ZIP compatibility
    Some(zip_path.to_string_lossy().replace('\\', "/"))
}

/// Check if a relative path should be excluded based on its first component
fn should_exclude_path(rel_path: &Path, excludes: &[&str]) -> bool {
    rel_path
        .components()
        .next()
        .and_then(|c| c.as_os_str().to_str())
        .map(|first| excludes.contains(&first))
        .unwrap_or(false)
}

/// Add a directory to the ZIP archive
fn add_directory_to_zip<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    zip_path: &str,
    opts: FileOptions<()>,
    config: &BuildConfig,
) -> Result<()> {
    zip.add_directory(zip_path, opts)?;
    config.log(&format!("📁 Dir   → {}", zip_path));
    Ok(())
}

/// Add a file to the ZIP archive
fn add_file_to_zip<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    file_path: &Path,
    zip_path: &str,
    opts: FileOptions<()>,
    config: &BuildConfig,
) -> Result<()> {
    zip.start_file(zip_path, opts)?;
    let mut file = fs::File::open(file_path)?;
    io::copy(&mut file, zip)?;
    config.log(&format!("📄 File  {} → {}", file_path.display(), zip_path));
    Ok(())
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

/// Add default thumbnail to ZIP if the mod doesn't have one
fn add_default_thumbnail_if_missing<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    submod_root: &Path,
    default_thumb: Option<&[u8]>,
    top: &str,
    config: &BuildConfig,
) -> Result<()> {
    // If mod already has thumbnail or no default provided, do nothing
    if submod_root.join("thumbnail.png").exists() || default_thumb.is_none() {
        return Ok(());
    }

    let bytes = default_thumb.unwrap();
    let opts: FileOptions<()> = FileOptions::default().compression_method(CompressionMethod::Deflated);
    let thumbnail_path = format!("{}/thumbnail.png", top);
    
    zip.start_file(&thumbnail_path, opts)?;
    zip.write_all(bytes)?;

    config.log(&format!("🔧 Injected default thumbnail into {}", top));
    Ok(())
}

/// Detect mods: include repo root if it has info.json, plus each direct child dir with info.json.
fn detect_all_mod_roots(root: &Path) -> Result<Vec<PathBuf>> {
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

/// Get the platform-specific Factorio mods directory
fn factorio_mods_dir() -> Result<PathBuf> {
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