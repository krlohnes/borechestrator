use std::path::Path;
use std::process::ExitCode;
use boring_proto::config::BoringConfig;
use boring_broker::NatsBroker;
use boring_store::LocalStore;
use boring_runtime::LocalRuntime;
use boring_secrets::{EnvSecretProvider, ChainSecretProvider, FileSecretProvider};
use boring_controller::reconciler::{Reconciler, RunResult};

pub async fn run(config_path: &Path) -> ExitCode {
    let config = match BoringConfig::from_file(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            return ExitCode::from(1);
        }
    };

    if let Err(errors) = config.validate() {
        eprintln!("Validation errors:");
        for e in &errors {
            eprintln!("  - {}", e);
        }
        return ExitCode::from(2);
    }

    // Determine broker URL
    let broker_url = config
        .broker
        .as_ref()
        .map(|b| b.url.as_str())
        .unwrap_or("nats://127.0.0.1:4222");

    let stream_name = config
        .broker
        .as_ref()
        .and_then(|b| b.stream.as_deref())
        .unwrap_or("BORING");

    let broker = match NatsBroker::new(broker_url, stream_name).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to connect to NATS: {}", e);
            return ExitCode::from(1);
        }
    };

    // Local store in .boring/ directory
    let store_dir = std::env::current_dir().unwrap().join(".boring");
    let store = LocalStore::new(&store_dir);

    let runtime = LocalRuntime::new();

    // Secret resolution chain: env vars first, then files in ~/.boring/secrets/
    let secrets_dir = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join(".boring")
        .join("secrets");
    let secrets = ChainSecretProvider::new(vec![
        Box::new(EnvSecretProvider::new()),
        Box::new(FileSecretProvider::new(&secrets_dir)),
    ]);

    let mut reconciler = Reconciler::new(
        config,
        Box::new(broker),
        Box::new(store),
        Box::new(runtime),
        Box::new(secrets),
    );

    match reconciler.run().await {
        Ok(RunResult::Completed) => {
            println!("Run completed successfully.");
            ExitCode::SUCCESS
        }
        Ok(RunResult::MaxIterationsReached) => {
            println!("Run reached maximum iterations.");
            ExitCode::from(1)
        }
        Ok(RunResult::TimedOut) => {
            println!("Run timed out.");
            ExitCode::from(1)
        }
        Ok(RunResult::Failed { reason }) => {
            eprintln!("Run failed: {}", reason);
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}
