use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

mod commands;

#[derive(Parser)]
#[command(name = "boring", about = "The world's most boring AI agent orchestrator")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate a borechestrator config file
    Validate {
        /// Path to config file
        #[arg(short, long, default_value = "borechestrator.yml")]
        config: PathBuf,
    },
    /// Run an orchestration
    Run {
        /// Path to config file
        #[arg(short, long, default_value = "borechestrator.yml")]
        config: PathBuf,
        /// Runtime mode override
        #[arg(long)]
        mode: Option<String>,
        /// Inline prompt (overrides prompt_file in config)
        #[arg(short = 'p', long)]
        prompt: Option<String>,
        /// Path to prompt file (overrides prompt_file in config)
        #[arg(short = 'P', long)]
        prompt_file: Option<PathBuf>,
        /// Resume an interrupted run
        #[arg(long)]
        r#continue: bool,
    },
    /// Initialize a new borechestrator.yml from a preset
    Init {
        /// Preset name (e.g., feature, tdd, research, debug, review, minimal)
        preset: Option<String>,
        /// List available presets
        #[arg(long)]
        list: bool,
    },
    /// Show the status of a run
    Status {
        /// Run ID (shows latest if omitted)
        run_id: Option<String>,
    },
    /// Show logs/events for a run
    Logs {
        /// Run ID
        run_id: String,
        /// Filter by hat name
        #[arg(long)]
        hat: Option<String>,
    },
    /// Emit an event into a running orchestration
    Emit {
        /// Run ID
        run_id: String,
        /// Event topic
        topic: String,
        /// Event payload
        payload: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { config } => commands::validate::run(&config),
        Commands::Run { config, mode: _, prompt, prompt_file, r#continue: _ } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::run::run(&config, prompt.as_deref(), prompt_file.as_deref()))
        }
        Commands::Init { preset, list } => {
            commands::init::run(preset.as_deref(), list)
        }
        Commands::Status { run_id } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::status::run(run_id.as_deref()))
        }
        Commands::Logs { run_id, hat } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::logs::run(&run_id, hat.as_deref()))
        }
        Commands::Emit {
            run_id,
            topic,
            payload,
        } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::emit::run(&run_id, &topic, &payload))
        }
    }
}
