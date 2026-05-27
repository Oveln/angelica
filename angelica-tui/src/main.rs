use clap::Parser;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing_subscriber::prelude::*;

#[derive(Parser)]
#[command(name = "angelica", about = "A ReAct-based AI agent with TUI")]
struct Cli {
    /// Config file path (default: ~/.config/angelica/config.toml)
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Enable debug HTTP server
    #[arg(long)]
    debug: bool,
    /// Log level: trace, debug, info, warn, error (overrides RUST_LOG)
    #[arg(long)]
    log_level: Option<String>,
}

fn init_logging(log_level: Option<&str>) {
    let env_filter = match log_level {
        Some(level) => tracing_subscriber::EnvFilter::new(level),
        None => tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
    };

    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("angelica");

    let file_appender = tracing_appender::rolling::daily(&log_dir, "angelica-tui.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_level(true)
        .with_target(false)
        .with_line_number(false)
        .with_filter(env_filter);

    tracing_subscriber::registry()
        .with(file_layer)
        .init();

    // Leak the guard so the non-blocking writer stays alive.
    std::mem::forget(_guard);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(cli.log_level.as_deref());

    let config = angelica::config::Config::load_or_create(cli.config)?;

    let (app_event_tx, app_event_rx) = mpsc::channel::<angelica::agent::events::AppEvent>(256);
    let (user_action_tx, user_action_rx) =
        mpsc::channel::<angelica::agent::events::UserAction>(256);

    {
        let data_dir = config.state.data_dir();
        if let Err(e) = angelica::data_git::ensure_repo(&data_dir) {
            tracing::warn!("Failed to initialize data git repo: {}", e);
        }
    }

    let debug_tx = if cli.debug {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 9914));
        let default_snapshot = angelica::debug::DebugSnapshot::default();
        let (tx, rx) = tokio::sync::watch::channel(default_snapshot);
        tracing::info!("Starting debug server on http://{addr}");
        angelica::debug::start_debug_server(addr, rx);
        Some(tx)
    } else {
        None
    };

    let model_name = config.llm.default_model_name().to_string();
    let config_clone = config;
    let agent_handle = tokio::spawn(async move {
        angelica::agent::run(config_clone, user_action_rx, app_event_tx, debug_tx).await
    });

    angelica_tui::app::run_tui(app_event_rx, user_action_tx, model_name).await?;

    agent_handle.await??;

    Ok(())
}
