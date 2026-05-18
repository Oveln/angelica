use clap::Parser;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(name = "angelica", about = "A ReAct-based AI agent with TUI")]
struct Cli {
    /// Config file path
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    let current_dir = std::env::current_dir()?;

    let config = if cli.config.exists() {
        let config_path = if cli.config.is_absolute() {
            cli.config
        } else {
            current_dir.join(&cli.config)
        };
        let mut cfg = angelica::config::Config::from_file(&config_path)?;
        cfg.resolve_paths(config_path.parent().unwrap_or(&current_dir));
        cfg
    } else {
        let mut cfg = angelica::config::Config::default();
        cfg.resolve_paths(&current_dir);
        cfg
    };

    let (app_event_tx, app_event_rx) = mpsc::channel::<angelica::agent::events::AppEvent>(256);
    let (user_action_tx, user_action_rx) =
        mpsc::channel::<angelica::agent::events::UserAction>(256);

    let model_name = config.llm.model.clone();
    let config_clone = config;
    let agent_handle = tokio::spawn(async move {
        angelica::agent::run(config_clone, user_action_rx, app_event_tx).await;
    });

    angelica::tui::app::run_tui(app_event_rx, user_action_tx, model_name).await?;

    agent_handle.await?;

    Ok(())
}
