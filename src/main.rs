use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use dirs_next::home_dir;
use serde::Deserialize;
use std::fs::{self, File};
use std::io;
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
    },
}

#[derive(Deserialize)]
struct Info {
    name: String,
    version: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { mod_path, out_dir } => {
            let cwd = std::env::current_dir()?;
            let mods = if let Some(p) = mod_path {
                let p = if p.is_absolute() { p } else { cwd.join(p) };
                if !p.join("info.json").exists() {
                    bail!("No info.json found at {}", p.display());
                }
                vec![p]
            } else {
                detect_all_mod_roots(&cwd)?
            };

            if mods.is_empty() {
                bail!("No mods found. Place an info.json in the repo root or in subfolders.");
            }

            let out_dir = PathBuf::from(out_dir);
            fs::create_dir_all(&out_dir)?;

            for m in mods {
                install_one(&m, &out_dir)?;
            }
        }
    }

    Ok(())
}

/// Install one mod: build zip into out_dir, copy to Factorio/mods, keep the built zip.
fn install_one(mod_root: &Path, out_dir: &Path) -> Result<()> {
    let info: Info = serde_json::from_str(&fs::read_to_string(mod_root.join("info.json"))?)
        .context("Failed to parse info.json")?;
    let top = format!("{}_{}", info.name, info.version);
    let zip_path = out_dir.join(format!("{top}.zip"));

    build_zip(mod_root, &zip_path, &top)?;
    let mods_dir = factorio_mods_dir()?;
    fs::create_dir_all(&mods_dir)?;
    let dest = mods_dir.join(zip_path.file_name().unwrap());
    fs::copy(&zip_path, &dest)?;
    println!("✅ Installed {} → {}", top, dest.display());
    Ok(())
}

/// Detect mods: include repo root if it has info.json, plus each direct child dir with info.json.
fn detect_all_mod_roots(root: &Path) -> Result<Vec<PathBuf>> {
    let mut mods = Vec::new();
    if root.join("info.json").exists() {
        mods.push(root.to_path_buf());
    }
    for entry in fs::read_dir(root)? {
        let p = entry?.path();
        if p.is_dir() && p.join("info.json").exists() {
            mods.push(p);
        }
    }
    Ok(mods)
}

fn factorio_mods_dir() -> Result<PathBuf> {
    let home = home_dir().context("No home directory found")?;
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

/// Build a ZIP with `<name>_<version>/` top-level and forward slashes.
fn build_zip(mod_root: &Path, out_zip: &Path, top: &str) -> Result<()> {
    if let Some(parent) = out_zip.parent() { fs::create_dir_all(parent)?; }
    if out_zip.exists() { fs::remove_file(out_zip)?; }

    let file = File::create(out_zip)?;
    let mut zip = zip::ZipWriter::new(file);

    // Use the simple options type: T = ()
    type Opts<'a> = zip::write::FileOptions<'a, ()>;
    let dir_opts: Opts = FileOptions::default();
    let file_opts: Opts = FileOptions::default().compression_method(CompressionMethod::Deflated);

    const EXCLUDES: &[&str] = &["build", ".git", ".github", ".idea", ".vscode"];

    for entry in WalkDir::new(mod_root).follow_links(false).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path == mod_root { continue; }

        if let Ok(rel) = path.strip_prefix(mod_root) {
            if let Some(first) = rel.components().next().and_then(|c| c.as_os_str().to_str()) {
                if EXCLUDES.iter().any(|e| e == &first) && entry.file_type().is_dir() {
                    continue;
                }
            }

            let mut p = PathBuf::from(top);
            p.push(rel);
            let inzip: String = p.to_string_lossy().replace('\\', "/");

            if entry.file_type().is_dir() {
                // Explicit generics: <String, ()>
                zip.add_directory::<String, ()>(inzip, dir_opts)?;
            } else {
                zip.start_file::<String, ()>(inzip, file_opts)?;
                let mut f = File::open(path)?;
                io::copy(&mut f, &mut zip)?;
            }
        }
    }

    zip.finish()?;
    println!("📦 Built {}", out_zip.display());
    Ok(())
}