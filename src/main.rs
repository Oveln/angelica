use clap::Parser;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(name = "angelica", about = "A ReAct-based AI agent with TUI")]
struct Cli {
    /// Config file path (default: ~/.config/angelica/config.toml)
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Enable debug HTTP server
    #[arg(long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let log_file = std::fs::File::create("angelica.log").ok();
    let builder = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
    );
    if let Some(file) = log_file {
        builder.with_writer(file).with_ansi(false).init();
    } else {
        builder.init();
    }

    let cli = Cli::parse();

    let config = match cli.config {
        Some(ref path) => {
            let abs = if path.is_absolute() {
                path.clone()
            } else {
                std::env::current_dir()?.join(path)
            };
            let mut cfg = angelica::config::Config::from_file(&abs)?;
            cfg.resolve_paths();
            cfg
        }
        None => {
            let config_path = angelica::config::config_path();
            if config_path.exists() {
                let mut cfg = angelica::config::Config::from_file(&config_path)?;
                cfg.resolve_paths();
                cfg
            } else {
                if let Some(parent) = config_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut cfg = angelica::config::Config::default();
                std::fs::write(&config_path, toml::to_string_pretty(&cfg)?)?;
                tracing::info!("Created default config at {}", config_path.display());
                cfg.resolve_paths();
                cfg
            }
        }
    };

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
    let conversation_path = config.state.conversation_path.clone();
    let config_clone = config;
    let agent_handle = tokio::spawn(async move {
        angelica::agent::run(config_clone, user_action_rx, app_event_tx, debug_tx).await
    });

    angelica::tui::app::run_tui(app_event_rx, user_action_tx, model_name, conversation_path)
        .await?;

    agent_handle.await??;

    Ok(())
}
