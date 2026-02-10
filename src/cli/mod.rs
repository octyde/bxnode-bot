//! CLI module - Command line interface definitions

pub mod config;
pub mod cron;
pub mod memory;
pub mod plugin;
pub mod skill;

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

    /// Manage memory storage
    Memory(MemoryArgs),

    /// Manage plugins
    Plugin(PluginArgs),

    /// Manage agent skills (OpenClaw/Agent Skills)
    Skill(SkillArgs),

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

/// Arguments for the memory command
#[derive(Parser, Debug)]
pub struct MemoryArgs {
    /// Path to configuration file (to determine memory store location)
    #[arg(short, long, env = "BXNODE_CONFIG")]
    pub config: Option<String>,

    #[command(subcommand)]
    pub action: MemoryAction,
}

#[derive(Subcommand, Debug)]
pub enum MemoryAction {
    /// List memories (optionally filtered by scope)
    List {
        /// Filter by agent ID
        #[arg(long)]
        agent: Option<String>,

        /// Filter by channel ID
        #[arg(long)]
        channel: Option<String>,

        /// Filter by user ID
        #[arg(long)]
        user: Option<String>,

        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Search memories by query
    Search {
        /// Search query
        query: String,

        /// Filter by agent ID
        #[arg(long)]
        agent: Option<String>,

        /// Maximum number of results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Show memory statistics
    Stats,

    /// Get a specific memory by ID
    Get {
        /// Memory ID
        id: String,
    },

    /// Delete a memory (soft delete)
    Delete {
        /// Memory ID to delete
        id: String,
    },

    /// Compact the memory store (remove deleted records)
    Compact,
}

/// Arguments for the plugin command
#[derive(Parser, Debug)]
pub struct PluginArgs {
    /// Path to configuration file
    #[arg(short, long, env = "BXNODE_CONFIG")]
    pub config: Option<String>,

    #[command(subcommand)]
    pub action: PluginAction,
}

#[derive(Subcommand, Debug)]
pub enum PluginAction {
    /// List available plugins
    List {
        /// Show all plugins (including disabled)
        #[arg(short, long)]
        all: bool,
    },

    /// Show detailed information about a plugin
    Info {
        /// Plugin ID
        id: String,
    },

    /// Enable a plugin
    Enable {
        /// Plugin ID to enable
        id: String,
    },

    /// Disable a plugin
    Disable {
        /// Plugin ID to disable
        id: String,
    },
}

/// Arguments for the skill command
#[derive(Parser, Debug)]
pub struct SkillArgs {
    /// Path to configuration file
    #[arg(short, long, env = "BXNODE_CONFIG")]
    pub config: Option<String>,

    #[command(subcommand)]
    pub action: SkillAction,
}

#[derive(Subcommand, Debug)]
pub enum SkillAction {
    /// List available skills
    List {
        /// Show all skills (including inactive)
        #[arg(short, long)]
        all: bool,

        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show detailed information about a skill
    Info {
        /// Skill name
        name: String,
    },

    /// Install a skill from GitHub URL or registry
    Install {
        /// GitHub URL or skill slug
        source: String,

        /// Target directory for installation
        #[arg(short, long)]
        dir: Option<String>,

        /// Force reinstall if skill already exists
        #[arg(short, long)]
        force: bool,
    },

    /// Update installed skills
    Update {
        /// Specific skill to update (all if not specified)
        name: Option<String>,

        /// Force update even if skill appears up-to-date
        #[arg(short, long)]
        force: bool,
    },

    /// Enable a skill (shows configuration instructions)
    Enable {
        /// Skill name to enable
        name: String,
    },

    /// Disable a skill (shows configuration instructions)
    Disable {
        /// Skill name to disable
        name: String,
    },

    /// Sync skills from configured sources or awesome-openclaw-skills
    Sync {
        /// Force re-download even if skills exist
        #[arg(short, long)]
        force: bool,
    },

    /// Search for skills by name or description
    Search {
        /// Search query
        query: String,
    },
}
