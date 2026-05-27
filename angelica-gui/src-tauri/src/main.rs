#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "angelica-gui", about = "Angelica — Tauri GUI")]
struct Cli {
    #[arg(long)]
    debug: bool,
    #[arg(long)]
    log_level: Option<String>,
    #[arg(short, long)]
    config: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    angelica_gui::run(cli.debug, cli.log_level.as_deref(), cli.config);
}
