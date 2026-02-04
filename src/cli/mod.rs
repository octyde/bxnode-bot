//! CLI module - Command line interface definitions

pub mod config;
pub mod cron;

use clap::{Parser, Subcommand};

/// BXNode Bot - AI agent gateway with multi-platform messaging support
#[derive(Parser, Debug)]
#[command(name = "bxnode-bot")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the gateway server
    Serve(ServeArgs),

    /// Manage configuration
    Config(ConfigArgs),

    /// Manage cron jobs
    Cron(CronArgs),

    /// Print version information
    Version,
}

/// Arguments for the serve command
#[derive(Parser, Debug, Clone)]
pub struct ServeArgs {
    /// Host to bind to
    #[arg(short = 'H', long, default_value = "0.0.0.0", env = "BXNODE_HOST")]
    pub host: String,

    /// Port to listen on
    #[arg(short, long, default_value = "3000", env = "BXNODE_PORT")]
    pub port: u16,

    /// Path to configuration file
    #[arg(short, long, env = "BXNODE_CONFIG")]
    pub config: Option<String>,

    /// Enable hot-reload of configuration
    #[arg(long, default_value = "true")]
    pub hot_reload: bool,
}

/// Arguments for the config command
#[derive(Parser, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show current configuration
    Show,

    /// Validate configuration file
    Validate {
        /// Path to configuration file
        #[arg(short, long)]
        path: Option<String>,
    },

    /// Initialize a new configuration file
    Init {
        /// Output path
        #[arg(short, long, default_value = "config.yaml")]
        output: String,
    },
}

/// Arguments for the cron command
#[derive(Parser, Debug)]
pub struct CronArgs {
    /// Path to configuration file (to determine cron store location)
    #[arg(short, long, env = "BXNODE_CONFIG")]
    pub config: Option<String>,

    #[command(subcommand)]
    pub action: CronAction,
}

#[derive(Subcommand, Debug)]
pub enum CronAction {
    /// List all cron jobs
    List,

    /// Add a new cron job
    Add {
        /// Unique job ID
        #[arg(short, long)]
        id: String,

        /// Cron schedule (6-field: sec min hour day month weekday)
        #[arg(short, long)]
        schedule: String,

        /// Job payload as JSON
        #[arg(short, long, default_value = "{}")]
        payload: String,

        /// Human-readable description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Remove a cron job
    Remove {
        /// Job ID to remove
        id: String,
    },

    /// Manually trigger a cron job
    Run {
        /// Job ID to run
        id: String,
    },

    /// Show recent runs for a job
    Runs {
        /// Job ID (optional, shows all if not specified)
        id: Option<String>,

        /// Number of runs to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
}
