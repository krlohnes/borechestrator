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
        Commands::Run { config, mode: _ } => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(commands::run::run(&config))
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
