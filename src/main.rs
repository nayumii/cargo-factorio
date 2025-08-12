use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::fs;
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::write::SimpleFileOptions;
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { mod_path, out_dir, default_thumbnail, verbose } => {
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

            // Load optional default thumbnail once
            let default_bytes = load_default_thumbnail_bytes(&default_thumbnail);

            for m in mods {
                if verbose {
                    println!("🔍 Processing mod at {}", m.display());
                }
                install_one(&m, &out_dir, &default_bytes, verbose)?;
            }
        }
    }

    Ok(())
}

/// Install one mod: build zip into out_dir, copy to Factorio/mods, keep the built zip.
fn install_one(mod_root: &Path, out_dir: &Path, default_bytes: &Option<Vec<u8>>, verbose: bool) -> Result<()> {
    let info: Info = serde_json::from_str(&fs::read_to_string(mod_root.join("info.json"))?)
        .context("Failed to parse info.json")?;
    let top = format!("{}_{}", info.name, info.version);
    let zip_path = out_dir.join(format!("{top}.zip"));

    build_zip(mod_root, &zip_path, &top, default_bytes, verbose)?;
    let mods_dir = factorio_mods_dir()?;
    fs::create_dir_all(&mods_dir)?;
    let dest = mods_dir.join(zip_path.file_name().unwrap());
    fs::copy(&zip_path, &dest)?;
    println!("✅ Installed {} → {}", top, dest.display());
    Ok(())
}

/// Build a ZIP with `<name>_<version>/` top-level and forward slashes.
fn build_zip(mod_root: &Path, out_zip: &Path, top: &str, default_bytes: &Option<Vec<u8>>, verbose: bool) -> Result<()> {
    if let Some(parent) = out_zip.parent() { fs::create_dir_all(parent)?; }
    if out_zip.exists() { fs::remove_file(out_zip)?; }

    let file = fs::File::create(out_zip)?; // <-- use fs::File
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
                zip.add_directory::<&str, ()>(inzip.as_str(), dir_opts)?;
                if verbose {
                    println!("📁 Dir   {} → {}", path.display(), inzip);
                }
            } else {
                zip.start_file::<&str, ()>(inzip.as_str(), file_opts)?;
                let mut f = fs::File::open(path)?; // <-- use fs::File
                io::copy(&mut f, &mut zip)?;
                if verbose {
                    println!("📄 File  {} → {}", path.display(), inzip);
                }
            }
        }
    }

    // Inject default thumbnail only if missing, and only if we have one to inject
    let default_ref = default_bytes.as_deref();
    add_default_thumbnail_if_missing(&mut zip, mod_root, default_ref, top, verbose)?;

    zip.finish()?;
    println!("📦 Built {}", out_zip.display());
    Ok(())
}

/// load `thumbnail.png` from args or by default from `assets/default_thumbnail.png`
fn load_default_thumbnail_bytes(explicit: &Option<PathBuf>) -> Option<Vec<u8>> {
    if let Some(p) = explicit {
        if let Ok(bytes) = fs::read(p) {
            return Some(bytes);
        }
    }
    let fallback = Path::new("assets").join("default_thumbnail.png");
    fs::read(&fallback).ok()
}

fn add_default_thumbnail_if_missing<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    submod_root: &Path,
    default_thumb: Option<&[u8]>,
    top: &str,
    verbose: bool,
) -> Result<()> {
    // if the mod already has one, do nothing.
    if submod_root.join("thumbnail.png").exists() {
        return Ok(());
    }

    // No user thumbnail and no default provided? Do nothing.
    let Some(bytes) = default_thumb else { return Ok(()) };

    // Inject as thumbnail.png at the ZIP root (inside <name>_<version>/)
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file::<&str, ()>(format!("{}/thumbnail.png", top).as_str(), opts)?;
    zip.write_all(bytes)?;

    if verbose {
        println!("🔧 Injected default thumbnail into {}", top);
    }

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