use anyhow::Result;
use std::fs;
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::CompressionMethod;

use crate::config::BuildConfig;

/// Build a ZIP with `<name>_<version>/` top-level and forward slashes.
pub fn build_zip(mod_root: &Path, out_zip: &Path, top: &str, config: &BuildConfig) -> Result<()> {
    prepare_output_file(out_zip)?;

    let file = fs::File::create(out_zip)?;
    let mut zip = zip::ZipWriter::new(file);

    let dir_opts: FileOptions<()> = FileOptions::default();
    let file_opts: FileOptions<()> = FileOptions::default().compression_method(CompressionMethod::Deflated);

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