use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod config;
mod installer;
mod mod_info;
mod platform;
mod zip_builder;

use config::BuildConfig;
use installer::install_mods;

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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install { mod_path, out_dir, default_thumbnail, verbose } => {
            let config = BuildConfig::new(verbose, default_thumbnail);
            install_mods(mod_path, out_dir, config)?;
        }
    }

    Ok(())
}