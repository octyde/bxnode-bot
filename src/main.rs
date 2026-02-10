//! BXNode Bot - AI agent gateway with multi-platform messaging support

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use bxnode_bot::cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bxnode_bot=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Execute command
    match cli.command {
        Commands::Serve(args) => {
            tracing::info!("Starting BXNode Bot server on {}:{}", args.host, args.port);
            bxnode_bot::gateway::serve(args).await?;
        }
        Commands::Config(args) => {
            bxnode_bot::cli::config::handle(args)?;
        }
        Commands::Cron(args) => {
            bxnode_bot::cli::cron::execute(args)?;
        }
        Commands::Memory(args) => {
            bxnode_bot::cli::memory::execute(args)?;
        }
        Commands::Plugin(args) => {
            bxnode_bot::cli::plugin::execute(args)?;
        }
        Commands::Skill(args) => {
            bxnode_bot::cli::skill::execute(args)?;
        }
        Commands::Version => {
            println!("bxnode-bot {}", env!("CARGO_PKG_VERSION"));
        }
    }

    Ok(())
}
